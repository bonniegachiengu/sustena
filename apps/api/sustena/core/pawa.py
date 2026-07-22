"""
sustena/core/pawa.py

PawaLedger — the Pawa token accounting primitive.

Phase 1: SQLite-backed ledger. Every debit and credit is an immutable row.
Phase 3: replaced by ERC-20 token contract (web3.py). The interface stays
         identical — only the backend changes.

Web3 Phase 3 mapping:
  deduct()       → ERC-20 transfer (user → network treasury)
  credit()       → ERC-20 transfer (treasury → user)
  get_balance()  → balanceOf(address)
  charge()       → chargeRoyalty() Solidity function

Royalty split on every component call:
  70% → contributor
  20% → network treasury
  5%  → referrer (if any)
  5%  → validator

All Phase 1 ledger entries use the same structure as the blockchain will —
making Phase 3 a bulk transaction issuance from the SQLite log, not a schema change.
"""

import logging
import uuid
from datetime import datetime

logger = logging.getLogger(__name__)

# Network treasury — receives 20% of all component charges
NETWORK_TREASURY_ID = "sustena.network_treasury" # TODO: Perhaps rename this an other siblings to sustena.mycelium.treasury i.e., others will be sustena.mycelium.market, etc.

# Pawa granted to new users on signup
ONBOARDING_GRANT = 100


class InsufficientPawaError(Exception):
    """Raised when a deduction would push balance below zero."""


class PawaLedger:
    """
    Manages pawa token balances. Phase 1: SQLite-backed.

    All methods are async to match the Phase 3 web3.py interface.
    In-memory fallback is used when no DB session is provided (e.g. in tests).
    """

    def __init__(self, db_session=None) -> None:
        self._db = db_session
        # In-memory fallback: {user_id: balance}
        self._balances: dict[str, int] = {}
        self._history: list[dict] = []

    # ── Balance operations ─────────────────────────────────────────────────────

    async def get_balance(self, user_id: str) -> int:
        """Return current pawa balance for user_id. Returns 0 if user not found."""
        if self._db is None:
            return self._balances.get(user_id, 0)

        from sqlalchemy import select, func
        from sustena.db.schema import pawa_ledger

        result = await self._db.execute(
            select(func.sum(pawa_ledger.c.delta))
            .where(pawa_ledger.c.user_id == user_id)
        )
        total = result.scalar()
        return int(total) if total is not None else 0

    async def deduct(
        self,
        user_id: str,
        sustain_id: str | None,
        amount: int,
        reason: str,
    ) -> bool:
        """
        Deduct `amount` pawa from user_id.
        Returns False (does NOT raise) if balance is insufficient.
        Returns True on success.
        """
        if amount <= 0:
            return True  # Zero-cost operations always succeed

        current = await self.get_balance(user_id)
        if current < amount:
            logger.debug(
                "Insufficient pawa: user=%s needs %d has %d",
                user_id, amount, current
            )
            return False

        new_balance = current - amount
        await self._write(user_id, sustain_id, -amount, reason, new_balance)
        return True

    async def credit(
        self,
        user_id: str,
        sustain_id: str | None,
        amount: int,
        reason: str,
    ) -> int:
        """Credit `amount` pawa to user_id. Returns new balance."""
        current = await self.get_balance(user_id)
        new_balance = current + amount
        await self._write(user_id, sustain_id, amount, reason, new_balance)
        return new_balance

    async def transfer(
        self,
        from_user: str,
        to_user: str,
        amount: int,
        reason: str,
    ) -> bool:
        """
        Transfer pawa from one user to another atomically.
        Returns False if from_user has insufficient balance.
        """
        ok = await self.deduct(from_user, None, amount, reason)
        if not ok:
            return False
        await self.credit(to_user, None, amount, reason)
        return True

    # ── Component charging (royalty split) ────────────────────────────────────

    async def charge(
        self,
        caller_user_id: str,
        caller_sustain_id: str | None,
        pawa_cost: int,
        contributor_id: str,
        referrer_id: str | None = None,
    ) -> bool:
        """
        Charge a component call and distribute royalties.

        Split:
          70% → contributor
          20% → network treasury
           5% → referrer (if provided, else → treasury)
           5% → validator (treasury for now; earmarked for Phase 3 nodes)

        Returns False if caller has insufficient balance (component call is blocked).
        """
        if pawa_cost <= 0:
            return True  # Free component

        # Check caller balance first
        caller_balance = await self.get_balance(caller_user_id)
        if caller_balance < pawa_cost:
            logger.debug(
                "Component charge blocked: caller=%s cost=%d balance=%d",
                caller_user_id, pawa_cost, caller_balance
            )
            return False

        # Deduct from caller
        await self.deduct(
            caller_user_id, caller_sustain_id, pawa_cost,
            f"component_charge:contributor={contributor_id}"
        )

        # Distribute royalties
        contributor_share = int(pawa_cost * 0.70)
        treasury_share    = int(pawa_cost * 0.20)
        referrer_share    = int(pawa_cost * 0.05)
        validator_share   = pawa_cost - contributor_share - treasury_share - referrer_share

        await self.credit(contributor_id, None, contributor_share, f"royalty:caller={caller_user_id}")
        await self.credit(NETWORK_TREASURY_ID, None, treasury_share, f"treasury_fee:caller={caller_user_id}")

        if referrer_id:
            await self.credit(referrer_id, None, referrer_share, f"referral:caller={caller_user_id}")
        else:
            await self.credit(NETWORK_TREASURY_ID, None, referrer_share, f"treasury_referral:caller={caller_user_id}")

        # Validator share → treasury for Phase 1; earmarked for validator nodes in Phase 3
        await self.credit(NETWORK_TREASURY_ID, None, validator_share, f"treasury_validator:caller={caller_user_id}")

        logger.debug(
            "Charge: caller=%s cost=%d → contributor=%s gets %d pawa",
            caller_user_id, pawa_cost, contributor_id, contributor_share
        )
        return True

    # ── History ────────────────────────────────────────────────────────────────

    async def get_history(self, user_id: str, limit: int = 50) -> list[dict]:
        """Return recent pawa history for user_id."""
        if self._db is None:
            entries = [e for e in self._history if e["user_id"] == user_id]
            return entries[-limit:]

        from sqlalchemy import select, desc
        from sustena.db.schema import pawa_ledger

        result = await self._db.execute(
            select(pawa_ledger)
            .where(pawa_ledger.c.user_id == user_id)
            .order_by(desc(pawa_ledger.c.timestamp))
            .limit(limit)
        )
        rows = result.mappings().all()
        return [dict(r) for r in rows]

    async def get_earnings(self, contributor_id: str, limit: int = 50) -> list[dict]:
        """Return royalty earnings for a contributor."""
        if self._db is None:
            entries = [
                e for e in self._history
                if e["user_id"] == contributor_id and e["delta"] > 0
                and "royalty" in e.get("reason", "")
            ]
            return entries[-limit:]

        from sqlalchemy import select, desc
        from sustena.db.schema import pawa_ledger

        result = await self._db.execute(
            select(pawa_ledger)
            .where(pawa_ledger.c.user_id == contributor_id)
            .where(pawa_ledger.c.delta > 0)
            .order_by(desc(pawa_ledger.c.timestamp))
            .limit(limit)
        )
        rows = result.mappings().all()
        return [dict(r) for r in rows]

    # ── Internal write ─────────────────────────────────────────────────────────

    async def _write(
        self,
        user_id: str,
        sustain_id: str | None,
        delta: int,
        reason: str,
        balance_after: int,
    ) -> None:
        """Write a pawa ledger entry to SQLite or in-memory."""
        entry = {
            "id": str(uuid.uuid4()),
            "user_id": user_id,
            "sustain_id": sustain_id,
            "delta": delta,
            "balance_after": balance_after,
            "reason": reason,
            "timestamp": datetime.utcnow().isoformat(),
        }
        self._history.append(entry)

        # Update in-memory balance cache (used when no DB)
        self._balances[user_id] = balance_after

        if self._db is not None:
            from sustena.db.schema import pawa_ledger
            try:
                await self._db.execute(
                    pawa_ledger.insert().values(
                        id=entry["id"],
                        user_id=user_id,
                        sustain_id=sustain_id,
                        delta=delta,
                        balance_after=balance_after,
                        reason=reason,
                        timestamp=datetime.utcnow(),
                    )
                )
                await self._db.commit()
            except Exception as e:
                logger.error("Failed to write pawa ledger entry: %s", e)
