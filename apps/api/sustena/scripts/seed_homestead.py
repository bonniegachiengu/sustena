"""
sustena/scripts/seed_homestead.py

Seeds Bonnie's Homestead sustain into sustena.db with realistic pocket data.

Run from apps/api/:
    python -m sustena.scripts.seed_homestead

Safe to re-run -- skips creation if a Homestead sustain for user 'bonnie'
already exists. Delete sustena.db to reseed from scratch.

Pockets seeded (KES):
  food         8,000/month
  transport    6,000/month
  rent        25,000/month
  savings     15,000/month
  emergency   10,000/month
  discretionary 5,000/month

Monthly income: 80,000 KES
Goal: Emergency fund -- 240,000 KES by Dec 2026
"""

import asyncio
import sys
from pathlib import Path

_ROOT = Path(__file__).resolve().parent.parent.parent  # apps/api/
if str(_ROOT) not in sys.path:
    sys.path.insert(0, str(_ROOT))

import sustena.operators  # noqa: F401 -- register all operators
from sustena.core.sustain_engine import SustainEngine

DB_PATH    = str(_ROOT / "sustena.db")
USER_ID    = "bonnie"
TEMPLATE   = "homestead"
INCOME_KES = 80_000.0

POCKETS = {
    "food":           8_000.0,
    "transport":      6_000.0,
    "rent":          25_000.0,
    "savings":       15_000.0,
    "emergency":     10_000.0,
    "discretionary":  5_000.0,
}


def _already_seeded(engine: SustainEngine) -> str | None:
    rows = engine._db.execute(
        "SELECT id FROM sustains WHERE user_id = ? AND template_id = ?",
        (USER_ID, TEMPLATE),
    ).fetchall()
    return rows[0]["id"] if rows else None


async def _seed() -> str:
    engine = SustainEngine(db_path=DB_PATH)

    existing = _already_seeded(engine)
    if existing:
        print(f"[seed] Homestead already exists -- sustain_id={existing}")
        print("[seed] Delete sustena.db to re-seed from scratch.")
        return existing

    # 1. Instantiate
    sustain_id = engine.instantiate(
        template_id=TEMPLATE,
        user_id=USER_ID,
        parameters={
            "owner_ids":             [USER_ID],
            "initial_income":        INCOME_KES,
            "initial_goal_name":     "Emergency fund",
            "initial_goal_amount":   240_000.0,
            "initial_goal_deadline": "2026-12-31",
        },
    )
    print(f"[seed] Instantiated Homestead -- sustain_id={sustain_id}")

    # 2. Record income (uses frequency, not period)
    result = await engine.execute_operator(
        sustain_id=sustain_id,
        operator_name="budget.record_income",
        params={"source": "salary", "amount": INCOME_KES, "frequency": "monthly"},
    )
    if result.succeeded:
        print(f"[seed] Income recorded: KES {INCOME_KES:,.0f}")
    else:
        print(f"[seed] WARNING -- income failed: {result.reason}")
        return sustain_id

    # 3. Allocate pockets
    for pocket_name, amount in POCKETS.items():
        r = await engine.execute_operator(
            sustain_id=sustain_id,
            operator_name="budget.allocate",
            params={"pocket_name": pocket_name, "amount": amount, "period": "monthly"},
        )
        status = "ok" if r.succeeded else f"FAILED ({r.reason})"
        print(f"[seed]   {pocket_name:<15} {amount:>8,.0f} KES  {status}")

    # 4. Verify
    state = engine.get_state(sustain_id)
    liquid = state.get("finances", {}).get("liquid", {}).get("balance", 0)
    print(f"[seed] Liquid balance after allocation: KES {liquid:,.0f}")

    print(f"\n[seed] Done.")
    print(f"  sustain_id = {sustain_id}")
    print(f"  Use in devui: GET /devui/state?sustain_id={sustain_id}")
    return sustain_id


if __name__ == "__main__":
    asyncio.run(_seed())
