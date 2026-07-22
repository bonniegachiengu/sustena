"""
sustena/operators/budget.py

Budget domain operators — the core financial primitives for any Homestead sustain.

State schema these operators expect under state.finances:
{
  "finances": {
    "liquid": {
      "balance": 50000.0       -- money not yet allocated to any pocket
    },
    "pockets": {
      "food": {
        "allocated": 5000.0,   -- amount moved from liquid into this pocket this period
        "spent":     1200.0,   -- amount deducted from this pocket via budget.spend
        "limit":     5000.0    -- optional hard cap (0 = no limit)
      }
    },
    "income": {
      "sources": [
        {"id": "uuid", "label": "Salary", "amount": 45000.0,
         "frequency": "monthly", "last_received": "2026-05-25"}
      ],
      "monthly_total": 45000.0
    }
  }
}

Operators:
  budget.record_income  — credit liquid balance, log income source
  budget.allocate       — move money from liquid → pocket
  budget.spend          — deduct from pocket.spent (enforces allocated limit)
  budget.transfer       — move money between two pockets
  budget.summary        — return budget ring ResponseWidget (read-only)

All four primitives are wired together in every operator:
  1. @sustena_operator   — registers in OPERATOR_REGISTRY with metadata + constraints
  2. ConstraintEngine    — self-checks pre-conditions at the start of each operator body
  3. StateAccessor       — all state reads and mutations go through ctx.state
  4. EventBus            — ctx.events.publish() fires domain events after mutations
  5. PawaLedger          — ctx.pawa.deduct() charges pawa after constraints pass
"""

import uuid
from datetime import datetime

from sustena.core.constraints import ConstraintEngine
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult, sustena_operator

# Shared engine instance — stateless, safe to reuse across calls
_engine = ConstraintEngine()


def _check_constraints(operator_name: str, ctx: OperatorContext, params: dict) -> OperatorResult | None:
    """
    Run the operator's declared pre-condition constraints via ConstraintEngine.
    Returns an OperatorResult.fail() if any constraint fails, else None (proceed).

    Called at the top of every operator body — makes operators self-contained
    without needing the SustainEngine runner to enforce pre-conditions.
    """
    meta = OPERATOR_REGISTRY.get(operator_name)
    if meta is None or not meta.constraints:
        return None
    ok, reason = _engine.evaluate_all(meta.constraints, ctx.state, params)
    if not ok:
        return OperatorResult.fail(reason=reason, constraint_violated=reason)
    return None


async def _charge_pawa(operator_name: str, ctx: OperatorContext) -> OperatorResult | None:
    """
    Deduct pawa for this operator call via PawaLedger.
    Returns an OperatorResult.fail() if the user has insufficient balance, else None.

    For budget operators (pawa_cost=0) this is always a no-op — PawaLedger.deduct()
    returns True immediately for zero-cost calls. The call is still made explicitly
    so the pattern is established for paid operators in later epics.
    """
    meta = OPERATOR_REGISTRY.get(operator_name)
    if meta is None:
        return None
    charged = await ctx.pawa.deduct(
        user_id=ctx.user_id,
        sustain_id=ctx.sustain_id,
        amount=meta.pawa_cost,
        reason=f"op:{operator_name}",
    )
    if not charged:
        return OperatorResult.fail(
            reason=(
                f"Insufficient pawa: '{operator_name}' costs {meta.pawa_cost} pawa "
                f"but your balance is too low."
            ),
            constraint_violated="pawa_balance_sufficient",
        )
    return None


# ── budget.record_income ───────────────────────────────────────────────────────

@sustena_operator(
    name="budget.record_income",
    description="Record an income transaction and credit the liquid balance.",
    constraints=[
        "params.amount > 0",
    ],
    side_effects=["event.finances.income_received"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "transaction_confirmation",
        "fields": [
            {"label": "Amount", "source": "inputs.amount", "display": "currency"},
            {"label": "Source", "source": "inputs.source", "display": "text"},
            {"label": "New Balance", "source": "state.finances.liquid.balance", "display": "currency"},
        ],
        "ctas": ["View Budget"],
    },
)
async def budget_record_income(
    ctx: OperatorContext,
    amount: float,
    source: str = "income",
    frequency: str = "once",
) -> OperatorResult:
    """
    Credit liquid balance and append to income.sources.

    Primitives used:
      ConstraintEngine  — enforces amount > 0
      PawaLedger        — deducts 0 pawa (free operator; pattern established)
      StateAccessor     — increments liquid balance, appends income source
      EventBus          — fires event.finances.income_received

    params:
      amount    -- KES amount received
      source    -- label e.g. "Salary", "Side hustle", "Chama payout"
      frequency -- once | monthly | weekly | daily
    """
    params = {"amount": amount, "source": source, "frequency": frequency}

    # 1. ConstraintEngine — pre-condition check
    fail = _check_constraints("budget.record_income", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger — deduct operator cost (0 pawa for free operators)
    fail = await _charge_pawa("budget.record_income", ctx)
    if fail:
        return fail

    # 3. StateAccessor — mutate state
    ctx.state.increment("finances.liquid.balance", amount)

    income_entry = {
        "id": str(uuid.uuid4()),
        "label": source,
        "amount": amount,
        "frequency": frequency,
        "received_at": ctx.timestamp.isoformat(),
    }
    ctx.state.append("finances.income.sources", income_entry)

    # Update monthly_total (simple add — proper period calc in Epic 1.2 Mentor operative)
    ctx.state.increment("finances.income.monthly_total", amount)

    # 4. EventBus — fire domain event after successful mutation
    await ctx.events.publish(
        "event.finances.income_received",
        {"amount": amount, "source": source, "frequency": frequency},
    )

    new_balance = ctx.state.get("finances.liquid.balance")
    return OperatorResult.ok({
        "amount_credited": amount,
        "source": source,
        "liquid_balance": new_balance,
    })


# ── budget.allocate ────────────────────────────────────────────────────────────

@sustena_operator(
    name="budget.allocate",
    description="Move an amount from liquid balance into a named budget pocket.",
    constraints=[
        "params.amount > 0",
        "finances.liquid.balance >= params.amount",
    ],
    side_effects=["event.finances.pocket_allocated"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "budget_allocation_card",
        "fields": [
            {"label": "Pocket", "source": "inputs.pocket_name", "display": "text"},
            {"label": "Amount", "source": "inputs.amount", "display": "currency"},
            {"label": "Liquid Remaining", "source": "state.finances.liquid.balance", "display": "currency"},
        ],
        "ctas": ["View Budget", "Allocate Another"],
    },
)
async def budget_allocate(
    ctx: OperatorContext,
    pocket_name: str,
    amount: float,
    period: str = "monthly",
) -> OperatorResult:
    """
    Allocate an amount from liquid balance into a named pocket.

    Creates the pocket if it doesn't exist.
    Does NOT allow over-drawing liquid — enforced by ConstraintEngine.

    Primitives used:
      ConstraintEngine  — enforces amount > 0 AND liquid >= amount
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — decrements liquid, increments pocket.allocated
      EventBus          — fires event.finances.pocket_allocated

    params:
      pocket_name -- e.g. "food", "rent", "emergency", "chama"
      amount      -- KES to allocate
      period      -- monthly | weekly | daily (metadata only for now)
    """
    params = {"pocket_name": pocket_name, "amount": amount, "period": period}

    # 1. ConstraintEngine — pre-condition check (amount > 0 AND liquid >= amount)
    fail = _check_constraints("budget.allocate", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger — deduct operator cost
    fail = await _charge_pawa("budget.allocate", ctx)
    if fail:
        return fail

    # 3. StateAccessor — mutate state
    pocket_path = f"finances.pockets.{pocket_name}"

    # Create pocket if it doesn't exist
    if not ctx.state.exists(f"{pocket_path}.allocated"):
        ctx.state.set(pocket_path, {"allocated": 0.0, "spent": 0.0, "limit": 0.0})

    # Move from liquid → pocket
    ctx.state.decrement("finances.liquid.balance", amount)
    ctx.state.increment(f"{pocket_path}.allocated", amount)

    # 4. EventBus — fire domain event
    await ctx.events.publish(
        "event.finances.pocket_allocated",
        {"pocket": pocket_name, "amount": amount, "period": period},
    )

    allocated = ctx.state.get(f"{pocket_path}.allocated")
    liquid = ctx.state.get("finances.liquid.balance")
    return OperatorResult.ok({
        "pocket": pocket_name,
        "amount_allocated": amount,
        "pocket_total_allocated": allocated,
        "liquid_remaining": liquid,
    })


# ── budget.spend ───────────────────────────────────────────────────────────────

@sustena_operator(
    name="budget.spend",
    description="Record a spend against a pocket. Fails if it would exceed the pocket's allocated amount.",
    constraints=[
        "params.amount > 0",
    ],
    side_effects=["event.finances.pocket_spent"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "transaction_confirmation",
        "fields": [
            {"label": "Pocket", "source": "inputs.pocket_name", "display": "text"},
            {"label": "Amount", "source": "inputs.amount", "display": "currency"},
            {"label": "Description", "source": "inputs.description", "display": "text"},
            {"label": "Pocket Remaining", "source": "inputs.pocket_name", "display": "currency"},
        ],
        "ctas": ["View Budget"],
    },
)
async def budget_spend(
    ctx: OperatorContext,
    pocket_name: str,
    amount: float,
    description: str = "",
    category: str = "",
) -> OperatorResult:
    """
    Deduct an amount from a pocket's remaining balance (allocated - spent).

    Enforces: pocket must exist AND amount <= (allocated - spent).
    The pocket-balance constraint is checked at runtime (depends on pocket state,
    not expressible as a static constraint string). ConstraintEngine still runs
    the static pre-conditions (amount > 0).

    Primitives used:
      ConstraintEngine  — enforces amount > 0
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — increments pocket.spent; reads allocated
      EventBus          — fires event.finances.pocket_spent

    params:
      pocket_name -- pocket to deduct from
      amount      -- KES spent
      description -- what was bought
      category    -- sub-category e.g. "groceries", "transport"
    """
    params = {"pocket_name": pocket_name, "amount": amount, "description": description, "category": category}

    # 1. ConstraintEngine — static pre-condition check (amount > 0)
    fail = _check_constraints("budget.spend", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger — deduct operator cost
    fail = await _charge_pawa("budget.spend", ctx)
    if fail:
        return fail

    # 3. StateAccessor — runtime constraint: pocket must exist with sufficient balance
    pocket_path = f"finances.pockets.{pocket_name}"

    if not ctx.state.exists(f"{pocket_path}.allocated"):
        return OperatorResult.fail(
            reason=f"Pocket '{pocket_name}' does not exist. Create it with budget.allocate first.",
            constraint_violated="pocket_exists",
        )

    allocated = ctx.state.get(f"{pocket_path}.allocated", 0.0)
    spent = ctx.state.get(f"{pocket_path}.spent", 0.0)
    remaining = allocated - spent

    if amount > remaining:
        return OperatorResult.fail(
            reason=(
                f"Spend of KES {amount:,.0f} exceeds remaining balance "
                f"in '{pocket_name}' pocket (KES {remaining:,.0f} left)."
            ),
            constraint_violated="pocket_balance_sufficient",
        )

    ctx.state.increment(f"{pocket_path}.spent", amount)

    spend_record = {
        "id": str(uuid.uuid4()),
        "pocket": pocket_name,
        "amount": amount,
        "description": description,
        "category": category,
        "timestamp": ctx.timestamp.isoformat(),
    }

    # 4. EventBus — fire domain event
    await ctx.events.publish(
        "event.finances.pocket_spent",
        {**spend_record},
    )

    new_spent = ctx.state.get(f"{pocket_path}.spent")
    new_remaining = allocated - new_spent
    return OperatorResult.ok({
        "pocket": pocket_name,
        "amount_spent": amount,
        "description": description,
        "pocket_remaining": new_remaining,
        "pocket_spent_total": new_spent,
    })


# ── budget.transfer ────────────────────────────────────────────────────────────

@sustena_operator(
    name="budget.transfer",
    description="Move an amount from one pocket to another.",
    constraints=[
        "params.amount > 0",
    ],
    side_effects=["event.finances.pocket_transfer"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "transaction_confirmation",
        "fields": [
            {"label": "From", "source": "inputs.from_pocket", "display": "text"},
            {"label": "To", "source": "inputs.to_pocket", "display": "text"},
            {"label": "Amount", "source": "inputs.amount", "display": "currency"},
        ],
        "ctas": ["View Budget"],
    },
)
async def budget_transfer(
    ctx: OperatorContext,
    from_pocket: str,
    to_pocket: str,
    amount: float,
) -> OperatorResult:
    """
    Transfer allocation from one pocket to another.

    The 'from' pocket loses allocated amount (not spent — this is a reallocation).
    The 'to' pocket gains the same amount. Liquid balance is unchanged.

    Primitives used:
      ConstraintEngine  — enforces amount > 0
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — adjusts allocated on both pockets
      EventBus          — fires event.finances.pocket_transfer
    """
    params = {"from_pocket": from_pocket, "to_pocket": to_pocket, "amount": amount}

    # 1. ConstraintEngine
    fail = _check_constraints("budget.transfer", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("budget.transfer", ctx)
    if fail:
        return fail

    # 3. StateAccessor — runtime constraints + mutations
    from_path = f"finances.pockets.{from_pocket}"
    to_path   = f"finances.pockets.{to_pocket}"

    if not ctx.state.exists(f"{from_path}.allocated"):
        return OperatorResult.fail(
            reason=f"Source pocket '{from_pocket}' does not exist.",
            constraint_violated="from_pocket_exists",
        )

    from_allocated = ctx.state.get(f"{from_path}.allocated", 0.0)
    from_spent     = ctx.state.get(f"{from_path}.spent", 0.0)
    from_remaining = from_allocated - from_spent

    if amount > from_remaining:
        return OperatorResult.fail(
            reason=(
                f"Transfer of KES {amount:,.0f} exceeds available allocation "
                f"in '{from_pocket}' (KES {from_remaining:,.0f} available)."
            ),
            constraint_violated="from_pocket_balance_sufficient",
        )

    if not ctx.state.exists(f"{to_path}.allocated"):
        ctx.state.set(to_path, {"allocated": 0.0, "spent": 0.0, "limit": 0.0})

    ctx.state.decrement(f"{from_path}.allocated", amount)
    ctx.state.increment(f"{to_path}.allocated", amount)

    # 4. EventBus
    await ctx.events.publish(
        "event.finances.pocket_transfer",
        {"from": from_pocket, "to": to_pocket, "amount": amount},
    )

    return OperatorResult.ok({
        "from_pocket": from_pocket,
        "to_pocket": to_pocket,
        "amount_transferred": amount,
        "from_remaining": ctx.state.get(f"{from_path}.allocated") - from_spent,
        "to_allocated": ctx.state.get(f"{to_path}.allocated"),
    })


# ── budget.summary ─────────────────────────────────────────────────────────────

@sustena_operator(
    name="budget.summary",
    description="Return a budget ring ResponseWidget showing all pockets and key metrics.",
    constraints=[],
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "budget_ring",
        "fields": [
            {"label": "Unallocated", "source": "state.finances.liquid.balance", "display": "currency"},
            {"label": "Total Allocated", "source": "state.finances.pockets", "display": "currency"},
            {"label": "Total Spent", "source": "state.finances.pockets", "display": "currency"},
        ],
        "chart": "donut",
        "ctas": ["Allocate", "Record Spend", "Ask Orchie"],
    },
)
async def budget_summary(ctx: OperatorContext) -> OperatorResult:
    """
    Read-only operator — returns the full budget state as a ResponseWidget.
    No state mutations, no events fired.

    Primitives used:
      ConstraintEngine  — no constraints declared (read-only, always permitted)
      PawaLedger — deducts 0 pawa (free operator)
      StateAccessor — reads liquid balance and all pockets
      EventBus — no events (read-only)
    """
    # 1+2. No constraints; deduct 0 pawa
    fail = await _charge_pawa("budget.summary", ctx)
    if fail:
        return fail

    # 3. StateAccessor
    liquid = ctx.state.get("finances.liquid.balance", 0.0)
    pockets_raw: dict = ctx.state.get("finances.pockets", {})

    pocket_list = []
    total_allocated = 0.0
    total_spent = 0.0

    for name, data in pockets_raw.items():
        allocated = data.get("allocated", 0.0)
        spent = data.get("spent", 0.0)
        remaining = allocated - spent
        pct = round((spent / allocated * 100) if allocated > 0 else 0, 1)
        total_allocated += allocated
        total_spent += spent
        pocket_list.append({
            "name": name,
            "allocated": allocated,
            "spent": spent,
            "remaining": remaining,
            "pct_spent": pct,
            "status": "ok" if pct < 80 else ("warn" if pct < 100 else "over"),
        })

    pocket_list.sort(key=lambda p: p["pct_spent"], reverse=True)

    pct_total = round((total_spent / total_allocated * 100) if total_allocated > 0 else 0, 1)
    burn_rate = round(total_spent / total_allocated, 4) if total_allocated > 0 else 0.0

    widget = {
        "type": "budget_ring",
        "data": {
            "liquid": liquid,
            "total_allocated": total_allocated,
            "total_spent": total_spent,
            "pct_spent": pct_total,
            "burn_rate": burn_rate,
            "safe_to_spend": liquid,
            "pockets": pocket_list,
        },
        "summary": (
            f"KES {liquid:,.0f} unallocated. "
            f"{pct_total:.0f}% of allocated budget spent."
        ),
    }

    # 4. EventBus — no events for read-only operator
    return OperatorResult.ok(widget)
