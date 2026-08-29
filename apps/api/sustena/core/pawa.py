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
  Pawa charge (usage):  70 / 15 / 5 / 5 / 5
  Licence sale (access): 80 / 10 / 3 / 2 / 5
  across contributor / treasury / validator / proposer / referrer
  5%  → referrer (if any)
  5%  → validator

All Phase 1 ledger entries use the same structure as the blockchain will —
making Phase 3 a bulk transaction issuance from the SQLite log, not a schema change.
"""

import logging
import uuid
from datetime import datetime

logger = logging.getLogger(__name__)

# Network treasury — receives the treasury share of every settlement
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

    # The ratified two-revenue-type schedule (2026-08-04), as
    # (contributor, treasury, validator, proposer, referrer) in per cent.
    #
    # ★★★ The earlier four-way 70/20/5/5 was not merely the wrong numbers — the
    #     SIGNATURE could not hold the right ones. It had no proposer parameter,
    #     so the ratified split was not expressible here at all, and a caller
    #     wiring it up would have inherited a schedule that silently dropped a
    #     role. Adding the parameter is the substance of this fix; changing the
    #     percentages is the easy half.
    #
    # ★★★ Paying to RUN something is not paying to HAVE it, and one schedule
    #     could not tell them apart. `revenue` is required rather than defaulted
    #     for that reason: a caller that has not decided which of the two
    #     happened has not decided what it is settling.
    _SCHEDULES = {
        "usage":  (70, 15, 5, 5, 5),
        "access": (80, 10, 3, 2, 5),
    }

    async def charge(
        self,
        caller_user_id: str,
        caller_sustain_id: str | None,
        pawa_cost: int,
        contributor_id: str,
        revenue: str,
        referrer_id: str | None = None,
        proposer_id: str | None = None,
        validator_id: str | None = None,
    ) -> bool:
        """
        Charge a component call or a licence sale and distribute royalties.

        `revenue` is "usage" (a pawa charge — someone ran it) or "access"
        (a licence sale — someone bought it). See `_SCHEDULES`.

        ★★★ Integer-floor arithmetic with the remainder assigned to the
        validator share, so the split is EXACTLY conserving by construction.
        A rounding leak here is not a cosmetic bug — it is a violation of the
        conservation invariant, which is the one property this method must not
        break.

        ★★ An absent role's share folds into the treasury and is never dropped,
        for the same reason: dropping it would break conservation, and the
        treasury is the right destination because it *is* the commons.

        Returns False if caller has insufficient balance (the call is blocked).
        """
        if revenue not in self._SCHEDULES:
            raise ValueError(
                f"revenue must be one of {sorted(self._SCHEDULES)}, got {revenue!r} — "
                "paying to run something is not paying to have it, and one schedule "
                "cannot tell them apart"
            )
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

        # Distribute royalties on the ratified schedule.
        c_pct, t_pct, v_pct, p_pct, r_pct = self._SCHEDULES[revenue]
        contributor_share = pawa_cost * c_pct // 100
        treasury_share    = pawa_cost * t_pct // 100
        validator_share   = pawa_cost * v_pct // 100
        proposer_share    = pawa_cost * p_pct // 100
        referrer_share    = pawa_cost * r_pct // 100

        # ★★★ The remainder is not discarded — it IS the validator's last unit.
        #     This is the line that makes conservation a theorem rather than a
        #     hope, and it mirrors sustena-core's `royalty::split` exactly.
        assigned = (contributor_share + treasury_share + validator_share
                    + proposer_share + referrer_share)
        validator_share += pawa_cost - assigned

        await self.credit(contributor_id, None, contributor_share,
                          f"royalty:caller={caller_user_id}")

        # ★★ An absent role folds into the treasury; never dropped.
        to_treasury = treasury_share
        for share, who, label in (
            (validator_share, validator_id, "validator"),
            (proposer_share, proposer_id, "proposer"),
            (referrer_share, referrer_id, "referrer"),
        ):
            if who:
                await self.credit(who, None, share, f"{label}:caller={caller_user_id}")
            else:
                to_treasury += share
        await self.credit(NETWORK_TREASURY_ID, None, to_treasury,
                          f"treasury:caller={caller_user_id}")

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
