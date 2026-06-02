"""
sustena/operators/mentor_ops.py

mentor.* operators — deterministic budget evaluation and deliberation logic.

These operators replace MentorOperative's LLM calls with rule-based computation.
No Anthropic API calls. Safe to run in ANTHROPIC_API_KEY=mock environments.

Operators:
  mentor.evaluate_budget  — scan pockets for over-threshold utilisation; return proposal
  mentor.deliberate_budget — vote on a budget proposal; check liquid floor constraint
"""

from sustena.core.operator import OperatorContext, OperatorResult, sustena_operator


# ── mentor.evaluate_budget ─────────────────────────────────────────────────────

@sustena_operator(
    name="mentor.evaluate_budget",
    protocol="rpc",
    description=(
        "Scan budget pockets for over-threshold utilisation. "
        "Returns a proposal (reallocate or alert) when any pocket exceeds action_threshold. "
        "Returns has_action=False when all pockets are healthy. No LLM calls."
    ),
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "budget_evaluation_card",
        "fields": [
            {"label": "Action", "source": "inputs.action_threshold", "display": "percentage"},
        ],
        "ctas": [],
    },
)
async def mentor_evaluate_budget(
    ctx: OperatorContext,
    action_threshold: float = 0.80,
    liquid_floor: float = 0.10,
) -> OperatorResult:
    """
    Scan all budget pockets and return a proposal if any exceeds action_threshold.

    Decision logic (no LLM):
      1. Find pockets where spent / allocated > action_threshold.
      2. If none found → has_action=False (caller should return None proposal).
      3. Pick the most over-spent pocket.
      4. If liquid_balance - top_up_amount >= floor → reallocate (top up by 20% of allocation).
      5. Otherwise → alert (no liquid available).

    params:
      action_threshold : fraction at which a pocket is considered "at risk" (default 0.80)
      liquid_floor     : fraction of monthly income to preserve as liquid floor (default 0.10)
    """
    pockets: dict = ctx.state.get("finances.pockets", {})
    monthly_income: float = ctx.state.get("finances.income.monthly_total", 0.0)
    liquid_balance: float = ctx.state.get("finances.liquid.balance", 0.0)
    floor_amount = monthly_income * liquid_floor

    over_threshold = []
    for name, data in pockets.items():
        if not isinstance(data, dict):
            continue
        allocated = data.get("allocated", 0.0)
        spent = data.get("spent", 0.0)
        if allocated <= 0:
            continue
        pct = spent / allocated
        if pct > action_threshold:
            over_threshold.append({
                "pocket":    name,
                "allocated": allocated,
                "spent":     spent,
                "pct":       round(pct * 100, 1),
                "remaining": allocated - spent,
            })

    if not over_threshold:
        return OperatorResult.ok({
            "has_action": False,
            "rationale": "All pockets are within threshold.",
            "operator_name": "sustena.no_action",
            "input_params": {},
        })

    # Pick the most over-spent pocket
    worst = max(over_threshold, key=lambda x: x["pct"])
    pocket_name = worst["pocket"]
    top_up = worst["allocated"] * 0.20  # suggest 20% of allocation as top-up

    if liquid_balance - top_up >= floor_amount:
        return OperatorResult.ok({
            "has_action": True,
            "operator_name": "budget.reallocate",
            "input_params": {
                "pocket_name": pocket_name,
                "amount": round(top_up, 2),
                "reason": f"MentorOperative graph: pocket at {worst['pct']}%",
            },
            "rationale": (
                f"Pocket '{pocket_name}' at {worst['pct']}% — "
                f"topping up {top_up:.0f} from liquid balance {liquid_balance:.0f}."
            ),
            "pocket": pocket_name,
            "amount": round(top_up, 2),
            "pockets_over_threshold": over_threshold,
            "liquid_before": liquid_balance,
            "liquid_after": round(liquid_balance - top_up, 2),
        })
    else:
        return OperatorResult.ok({
            "has_action": True,
            "operator_name": "sustena.alert",
            "input_params": {
                "message": f"Pocket '{pocket_name}' at {worst['pct']}% — no liquid to reallocate.",
                "pockets": over_threshold,
                "severity": "warn",
            },
            "rationale": (
                f"Pocket '{pocket_name}' at {worst['pct']}% but liquid {liquid_balance:.0f} "
                f"would breach floor {floor_amount:.0f}."
            ),
            "pocket": pocket_name,
            "pockets_over_threshold": over_threshold,
            "liquid_balance": liquid_balance,
        })


# ── mentor.deliberate_budget ───────────────────────────────────────────────────

@sustena_operator(
    name="mentor.deliberate_budget",
    protocol="rpc",
    description=(
        "Vote on a budget-related Council proposal. "
        "Hard NO if proposal would breach liquid floor. "
        "ABSTAIN if unrelated to finances. YES otherwise. No LLM calls."
    ),
    side_effects=[],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def mentor_deliberate_budget(
    ctx: OperatorContext,
    proposal_dict: dict = None,
    liquid_floor: float = 0.10,
) -> OperatorResult:
    """
    Vote on a Council proposal using rule-based logic (no LLM).

    Decision rules:
      1. Proposal operator not in budget.* / sustena.alert domain → ABSTAIN
      2. Proposed amount would push liquid below floor → NO
      3. Otherwise → YES

    params:
      proposal_dict : the proposal dict (operator_name, input_params, etc.)
      liquid_floor  : fraction of monthly income to preserve as liquid (default 0.10)
    """
    if proposal_dict is None:
        proposal_dict = {}

    operator_name = proposal_dict.get("operator_name", "")

    if not operator_name.startswith(("budget.", "sustena.alert")):
        return OperatorResult.ok({
            "vote": "ABSTAIN",
            "utility": 0.5,
            "reasoning": "Proposal is outside MentorOperative's domain (budget/finance).",
            "operator_name": "deliberation_result",
            "rationale": "ABSTAIN: outside budget domain.",
        })

    monthly_income: float = ctx.state.get("finances.income.monthly_total", 0.0)
    liquid_balance: float = ctx.state.get("finances.liquid.balance", 0.0)
    floor_amount = monthly_income * liquid_floor

    input_params = proposal_dict.get("input_params", {})
    proposed_amount = float(input_params.get("amount", 0) or 0)

    if proposed_amount > 0 and (liquid_balance - proposed_amount) < floor_amount:
        reasoning = (
            f"NO: executing this proposal would reduce liquid from "
            f"{liquid_balance:,.0f} to {liquid_balance - proposed_amount:,.0f}, "
            f"below the 10% income floor of {floor_amount:,.0f}."
        )
        return OperatorResult.ok({
            "vote": "NO",
            "utility": 0.1,
            "reasoning": reasoning,
            "operator_name": "deliberation_result",
            "rationale": reasoning,
        })

    reasoning = (
        "YES: proposal is finance-related and does not breach the liquid floor. "
        "Expected to improve budget position."
    )
    return OperatorResult.ok({
        "vote": "YES",
        "utility": 0.8,
        "reasoning": reasoning,
        "operator_name": "deliberation_result",
        "rationale": reasoning,
    })
