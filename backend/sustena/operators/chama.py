"""
sustena/operators/chama.py

Chama domain operators — group savings/lending mechanics for any Chama sustain.

State schema these operators expect under state.chama and state.rules:
{
  "chama": {
    "members": {
      "<member_id>": {
        "contributions": [
          {"id": "uuid", "amount": 500.0, "period": "2026-05",
           "recorded_at": "2026-05-25T10:00:00"}
        ],
        "balance": 1500.0,     -- cumulative contributions minus fines minus loan principal
        "fines_total": 0.0,
        "loans": []            -- [{loan_id: "uuid"}] back-references
      }
    },
    "loans": [
      {
        "id": "uuid",
        "member_id": "m001",
        "amount": 5000.0,
        "purpose": "school fees",
        "status": "IN_VOTING",  -- IN_VOTING | PASSED | FAILED | DISBURSED | REPAID
        "repaid": 0.0,
        "proposal_id": "uuid",
        "created_at": "ISO",
        "disbursed_at": null
      }
    ],
    "fines": [],               -- global fines log
    "meetings": [],
    "fund": {
      "total": 0.0             -- total pooled cash (sum of contributions minus disbursements)
    }
  },
  "rules": {
    "fine_reasons": ["late_contribution", "absenteeism", "misconduct"]
  }
}

NOTE: loans are stored as a LIST (not a dict) because StateAccessor.set() uses
a regex that only allows word characters in path segments — UUIDs contain dashes
and cannot be used as dot-path keys.

Operators:
  chama.contribution.record  — logs a member contribution, credits member balance
  chama.loan.request         — creates a loan proposal, fires council event, returns deferred
  chama.loan.disburse        — disburses a PASSED loan (runtime constraint: status == PASSED)
  chama.loan.repay           — records a repayment against a loan
  chama.fine.record          — records a fine; constraint: reason IN rules.fine_reasons
  chama.meeting.schedule     — schedules a meeting
  chama.dividend.calculate   — computes per-member dividend split, returns ResponseWidget

All four primitives are wired in every operator:
  1. @sustena_operator   — registers in OPERATOR_REGISTRY with metadata + constraints
  2. ConstraintEngine    — evaluates pre-conditions at the top of every operator body
  3. StateAccessor       — all reads and mutations go through ctx.state
  4. EventBus            — ctx.events.publish() fires domain events after mutations
  5. PawaLedger          — ctx.pawa.deduct() charges pawa after constraints pass
"""

import uuid
from datetime import datetime

from sustena.core.constraints import ConstraintEngine
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext, OperatorResult, sustena_operator

# Shared engine instance — stateless, safe to reuse across calls
_engine = ConstraintEngine()


def _check_constraints(operator_name: str, ctx: OperatorContext, params: dict) -> "OperatorResult | None":
    """
    Run the operator's declared pre-condition constraints via ConstraintEngine.
    Returns OperatorResult.fail() on the first violated constraint, else None.

    Identical pattern to budget.py — called at the top of every operator body.
    """
    meta = OPERATOR_REGISTRY.get(operator_name)
    if meta is None or not meta.constraints:
        return None
    ok, reason = _engine.evaluate_all(meta.constraints, ctx.state, params)
    if not ok:
        return OperatorResult.fail(reason=reason, constraint_violated=reason)
    return None


async def _charge_pawa(operator_name: str, ctx: OperatorContext) -> "OperatorResult | None":
    """
    Deduct pawa for this operator call via PawaLedger.
    Returns OperatorResult.fail() if the user has insufficient balance, else None.
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


def _find_loan(ctx: OperatorContext, loan_id: str) -> dict | None:
    """
    Look up a loan record in chama.loans list by id.
    Returns the loan dict (a live reference into state) or None.
    """
    loans: list = ctx.state.get("chama.loans", [])
    return next((ln for ln in loans if ln.get("id") == loan_id), None)


# ── chama.contribution.record ──────────────────────────────────────────────────

@sustena_operator(
    name="chama.contribution.record",
    description="Record a member's chama contribution and credit their balance.",
    constraints=[
        "params.amount > 0",
    ],
    side_effects=["event.chama.contribution_recorded"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "transaction_confirmation",
        "fields": [
            {"label": "Member", "source": "inputs.member_id", "display": "text"},
            {"label": "Amount", "source": "inputs.amount", "display": "currency"},
            {"label": "Period", "source": "inputs.period", "display": "text"},
        ],
        "ctas": ["View Chama", "Record Another"],
    },
)
async def chama_contribution_record(
    ctx: OperatorContext,
    member_id: str,
    amount: float,
    period: str = "",
) -> OperatorResult:
    """
    Log a member's contribution and credit their chama balance.
    Also increments the chama fund total.

    Primitives used:
      ConstraintEngine  — enforces amount > 0
      PawaLedger        — deducts 0 pawa (free operator; pattern established)
      StateAccessor     — creates/updates member record and fund total
      EventBus          — fires event.chama.contribution_recorded

    params:
      member_id -- chama member identifier (alphanumeric/underscore only)
      amount    -- KES contributed
      period    -- contribution period label, e.g. "2026-05"
    """
    params = {"member_id": member_id, "amount": amount, "period": period}

    # 1. ConstraintEngine — pre-condition check
    fail = _check_constraints("chama.contribution.record", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger — deduct operator cost
    fail = await _charge_pawa("chama.contribution.record", ctx)
    if fail:
        return fail

    # 3. StateAccessor — create member record if needed, then mutate
    member_path = f"chama.members.{member_id}"

    if not ctx.state.exists(f"{member_path}.balance"):
        ctx.state.set(member_path, {
            "contributions": [],
            "balance": 0.0,
            "fines_total": 0.0,
            "loans": [],
        })

    contribution = {
        "id": str(uuid.uuid4()),
        "amount": amount,
        "period": period,
        "recorded_at": ctx.timestamp.isoformat(),
    }
    ctx.state.append(f"{member_path}.contributions", contribution)
    ctx.state.increment(f"{member_path}.balance", amount)

    # Ensure fund.total exists, then increment
    if not ctx.state.exists("chama.fund.total"):
        ctx.state.set("chama.fund", {"total": 0.0})
    ctx.state.increment("chama.fund.total", amount)

    # 4. EventBus — fire domain event after successful mutation
    await ctx.events.publish(
        "event.chama.contribution_recorded",
        {"member_id": member_id, "amount": amount, "period": period},
    )

    new_balance = ctx.state.get(f"{member_path}.balance")
    fund_total = ctx.state.get("chama.fund.total")
    return OperatorResult.ok({
        "member_id": member_id,
        "amount_credited": amount,
        "period": period,
        "member_balance": new_balance,
        "fund_total": fund_total,
    })


# ── chama.loan.request ─────────────────────────────────────────────────────────

@sustena_operator(
    name="chama.loan.request",
    description="Create a loan proposal and send it to the Council for approval.",
    constraints=[
        "params.amount > 0",
    ],
    side_effects=["event.chama.loan_requested", "event.council.proposal_created"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "proposal_card",
        "fields": [
            {"label": "Member", "source": "inputs.member_id", "display": "text"},
            {"label": "Amount", "source": "inputs.amount", "display": "currency"},
            {"label": "Purpose", "source": "inputs.purpose", "display": "text"},
            {"label": "Status", "source": "deferred.proposal_id", "display": "badge"},
        ],
        "ctas": ["View Proposal", "Check Status"],
    },
)
async def chama_loan_request(
    ctx: OperatorContext,
    member_id: str,
    amount: float,
    purpose: str = "",
) -> OperatorResult:
    """
    Create a loan record with IN_VOTING status and fire two events:
    one for the chama domain log, one to trigger Council processing.
    Returns OperatorResult.deferred(proposal_id) — execution pauses until Council votes.

    Loans are stored as a LIST under chama.loans (not a dict keyed by UUID) because
    StateAccessor path segments must be word characters only (no hyphens).

    Primitives used:
      ConstraintEngine  — enforces amount > 0
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — appends loan record to chama.loans list
      EventBus          — fires event.chama.loan_requested + event.council.proposal_created

    params:
      member_id -- requesting member
      amount    -- KES loan amount
      purpose   -- reason for loan (e.g. "school fees", "medical")
    """
    params = {"member_id": member_id, "amount": amount, "purpose": purpose}

    # 1. ConstraintEngine
    fail = _check_constraints("chama.loan.request", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("chama.loan.request", ctx)
    if fail:
        return fail

    # 3. StateAccessor — append loan to list
    loan_id = str(uuid.uuid4())
    proposal_id = str(uuid.uuid4())

    loan_record = {
        "id": loan_id,
        "member_id": member_id,
        "amount": amount,
        "purpose": purpose,
        "status": "IN_VOTING",
        "repaid": 0.0,
        "proposal_id": proposal_id,
        "created_at": ctx.timestamp.isoformat(),
        "disbursed_at": None,
    }

    if not ctx.state.exists("chama.loans"):
        ctx.state.set("chama.loans", [])
    ctx.state.append("chama.loans", loan_record)

    # Cross-reference on member record (create member if missing)
    member_path = f"chama.members.{member_id}"
    if not ctx.state.exists(f"{member_path}.balance"):
        ctx.state.set(member_path, {
            "contributions": [],
            "balance": 0.0,
            "fines_total": 0.0,
            "loans": [],
        })
    ctx.state.append(f"{member_path}.loans", {"loan_id": loan_id})

    # 4. EventBus — two events: chama domain log + council trigger
    await ctx.events.publish(
        "event.chama.loan_requested",
        {"loan_id": loan_id, "member_id": member_id, "amount": amount, "purpose": purpose},
    )
    await ctx.events.publish(
        "event.council.proposal_created",
        {
            "proposal_id": proposal_id,
            "operator_name": "chama.loan.disburse",
            "input": {"loan_id": loan_id},
            "requested_by": member_id,
        },
    )

    return OperatorResult.deferred(proposal_id=proposal_id)


# ── chama.loan.disburse ────────────────────────────────────────────────────────

@sustena_operator(
    name="chama.loan.disburse",
    description="Disburse a loan that has received Council PASSED status.",
    constraints=[],   # runtime constraint: loan status must be PASSED (checked in body)
    side_effects=["event.chama.loan_disbursed"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "transaction_confirmation",
        "fields": [
            {"label": "Loan ID", "source": "inputs.loan_id", "display": "text"},
            {"label": "Amount", "source": "state.chama.loans.amount", "display": "currency"},
            {"label": "Member", "source": "state.chama.loans.member_id", "display": "text"},
        ],
        "ctas": ["View Loan", "View Chama"],
    },
)
async def chama_loan_disburse(
    ctx: OperatorContext,
    loan_id: str,
) -> OperatorResult:
    """
    Disburse funds for a Council-approved loan.

    Pre-condition (runtime): loan must exist AND status must be "PASSED".
    The constraint is dynamic (depends on loan_id param), so it is checked
    in the operator body rather than declared in the constraints list —
    same pattern as budget.spend's pocket-balance check.

    Primitives used:
      ConstraintEngine  — no static constraints (runtime-only)
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — updates loan status → DISBURSED, decrements fund
      EventBus          — fires event.chama.loan_disbursed

    params:
      loan_id -- the loan to disburse (must have status "PASSED")
    """
    params = {"loan_id": loan_id}

    # 1. ConstraintEngine — no static constraints for this operator
    fail = _check_constraints("chama.loan.disburse", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("chama.loan.disburse", ctx)
    if fail:
        return fail

    # 3. StateAccessor — runtime constraint: loan must exist and be PASSED
    loan = _find_loan(ctx, loan_id)

    if loan is None:
        return OperatorResult.fail(
            reason=f"Loan '{loan_id}' does not exist.",
            constraint_violated="loan_exists",
        )

    loan_status = loan["status"]
    if loan_status != "PASSED":
        return OperatorResult.fail(
            reason=(
                f"Loan '{loan_id}' cannot be disbursed — status is '{loan_status}', "
                f"must be 'PASSED'."
            ),
            constraint_violated="loan_status_is_passed",
        )

    loan_amount = loan["amount"]
    fund_total = ctx.state.get("chama.fund.total", 0.0)

    if loan_amount > fund_total:
        return OperatorResult.fail(
            reason=(
                f"Fund total (KES {fund_total:,.0f}) is insufficient for loan "
                f"of KES {loan_amount:,.0f}."
            ),
            constraint_violated="fund_sufficient_for_loan",
        )

    # Mutate loan in-place (the list item is a live reference into state._data)
    loan["status"] = "DISBURSED"
    loan["disbursed_at"] = ctx.timestamp.isoformat()

    ctx.state.decrement("chama.fund.total", loan_amount)

    # 4. EventBus
    member_id = loan["member_id"]
    await ctx.events.publish(
        "event.chama.loan_disbursed",
        {"loan_id": loan_id, "member_id": member_id, "amount": loan_amount},
    )

    return OperatorResult.ok({
        "loan_id": loan_id,
        "member_id": member_id,
        "amount_disbursed": loan_amount,
        "fund_total_remaining": ctx.state.get("chama.fund.total"),
    })


# ── chama.loan.repay ───────────────────────────────────────────────────────────

@sustena_operator(
    name="chama.loan.repay",
    description="Record a repayment against an outstanding chama loan.",
    constraints=[
        "params.amount > 0",
    ],
    side_effects=["event.chama.loan_repayment_recorded"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "transaction_confirmation",
        "fields": [
            {"label": "Loan ID", "source": "inputs.loan_id", "display": "text"},
            {"label": "Amount", "source": "inputs.amount", "display": "currency"},
        ],
        "ctas": ["View Loan", "View Chama"],
    },
)
async def chama_loan_repay(
    ctx: OperatorContext,
    loan_id: str,
    amount: float,
) -> OperatorResult:
    """
    Record a loan repayment. Increments loan.repaid and fund.total.
    Marks loan as REPAID when repaid >= loan amount.

    Primitives used:
      ConstraintEngine  — enforces amount > 0
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — increments loan.repaid; marks REPAID when fully settled
      EventBus          — fires event.chama.loan_repayment_recorded

    params:
      loan_id -- loan being repaid
      amount  -- KES repayment amount
    """
    params = {"loan_id": loan_id, "amount": amount}

    # 1. ConstraintEngine
    fail = _check_constraints("chama.loan.repay", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("chama.loan.repay", ctx)
    if fail:
        return fail

    # 3. StateAccessor — runtime check: loan must exist and be active
    loan = _find_loan(ctx, loan_id)

    if loan is None:
        return OperatorResult.fail(
            reason=f"Loan '{loan_id}' does not exist.",
            constraint_violated="loan_exists",
        )

    loan_status = loan["status"]
    if loan_status not in ("DISBURSED", "PASSED"):
        return OperatorResult.fail(
            reason=f"Loan '{loan_id}' cannot accept repayment — status is '{loan_status}'.",
            constraint_violated="loan_is_active",
        )

    # Mutate loan in-place
    loan["repaid"] = loan.get("repaid", 0.0) + amount

    # Return principal to fund
    if not ctx.state.exists("chama.fund.total"):
        ctx.state.set("chama.fund", {"total": 0.0})
    ctx.state.increment("chama.fund.total", amount)

    loan_amount = loan["amount"]
    total_repaid = loan["repaid"]
    is_fully_repaid = total_repaid >= loan_amount

    if is_fully_repaid:
        loan["status"] = "REPAID"

    member_id = loan["member_id"]

    # 4. EventBus
    await ctx.events.publish(
        "event.chama.loan_repayment_recorded",
        {
            "loan_id": loan_id,
            "member_id": member_id,
            "amount": amount,
            "total_repaid": total_repaid,
            "fully_repaid": is_fully_repaid,
        },
    )

    return OperatorResult.ok({
        "loan_id": loan_id,
        "member_id": member_id,
        "amount_repaid": amount,
        "total_repaid": total_repaid,
        "loan_amount": loan_amount,
        "outstanding": max(0.0, loan_amount - total_repaid),
        "fully_repaid": is_fully_repaid,
    })


# ── chama.fine.record ──────────────────────────────────────────────────────────

@sustena_operator(
    name="chama.fine.record",
    description="Record a fine against a member. Reason must be in the chama's approved fine reasons.",
    constraints=[
        "params.amount > 0",
        "params.reason IN rules.fine_reasons",
    ],
    side_effects=["event.chama.fine_recorded"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "transaction_confirmation",
        "fields": [
            {"label": "Member", "source": "inputs.member_id", "display": "text"},
            {"label": "Amount", "source": "inputs.amount", "display": "currency"},
            {"label": "Reason", "source": "inputs.reason", "display": "text"},
        ],
        "ctas": ["View Member", "View Chama"],
    },
)
async def chama_fine_record(
    ctx: OperatorContext,
    member_id: str,
    amount: float,
    reason: str,
) -> OperatorResult:
    """
    Record a fine against a member.

    The reason must appear in rules.fine_reasons — enforced by ConstraintEngine
    using the DSL: params.reason IN rules.fine_reasons.
    Deducts from member balance and appends to the global fines log.

    Primitives used:
      ConstraintEngine  — enforces amount > 0 AND reason IN rules.fine_reasons
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — decrements member.balance, appends fine record
      EventBus          — fires event.chama.fine_recorded

    params:
      member_id -- member being fined
      amount    -- KES fine amount
      reason    -- must be an approved reason from rules.fine_reasons
    """
    params = {"member_id": member_id, "amount": amount, "reason": reason}

    # 1. ConstraintEngine — enforces amount > 0 AND reason IN rules.fine_reasons
    fail = _check_constraints("chama.fine.record", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("chama.fine.record", ctx)
    if fail:
        return fail

    # 3. StateAccessor
    member_path = f"chama.members.{member_id}"

    if not ctx.state.exists(f"{member_path}.balance"):
        ctx.state.set(member_path, {
            "contributions": [],
            "balance": 0.0,
            "fines_total": 0.0,
            "loans": [],
        })

    ctx.state.decrement(f"{member_path}.balance", amount, allow_negative=True)
    ctx.state.increment(f"{member_path}.fines_total", amount)

    fine_record = {
        "id": str(uuid.uuid4()),
        "member_id": member_id,
        "amount": amount,
        "reason": reason,
        "recorded_at": ctx.timestamp.isoformat(),
    }

    if not ctx.state.exists("chama.fines"):
        ctx.state.set("chama.fines", [])
    ctx.state.append("chama.fines", fine_record)

    # 4. EventBus
    await ctx.events.publish(
        "event.chama.fine_recorded",
        {"member_id": member_id, "amount": amount, "reason": reason},
    )

    return OperatorResult.ok({
        "member_id": member_id,
        "amount_fined": amount,
        "reason": reason,
        "member_balance": ctx.state.get(f"{member_path}.balance"),
        "member_fines_total": ctx.state.get(f"{member_path}.fines_total"),
    })


# ── chama.meeting.schedule ─────────────────────────────────────────────────────

@sustena_operator(
    name="chama.meeting.schedule",
    description="Schedule a chama meeting with a date and agenda.",
    constraints=[],
    side_effects=["event.chama.meeting_scheduled"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "meeting_card",
        "fields": [
            {"label": "Date", "source": "inputs.date", "display": "date"},
            {"label": "Agenda", "source": "inputs.agenda", "display": "list"},
        ],
        "ctas": ["View Meetings", "Notify Members"],
    },
)
async def chama_meeting_schedule(
    ctx: OperatorContext,
    date: str,
    agenda: list | None = None,
) -> OperatorResult:
    """
    Schedule a chama meeting, appending it to chama.meetings.

    Primitives used:
      ConstraintEngine  — no constraints (any date/agenda is valid)
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — appends meeting to chama.meetings list
      EventBus          — fires event.chama.meeting_scheduled

    params:
      date   -- ISO date or human-readable date string (e.g. "2026-06-01")
      agenda -- list of agenda item strings
    """
    if agenda is None:
        agenda = []

    params = {"date": date, "agenda": agenda}

    # 1. ConstraintEngine — no static constraints
    fail = _check_constraints("chama.meeting.schedule", ctx, params)
    if fail:
        return fail

    # 2. PawaLedger
    fail = await _charge_pawa("chama.meeting.schedule", ctx)
    if fail:
        return fail

    # 3. StateAccessor
    if not ctx.state.exists("chama.meetings"):
        ctx.state.set("chama.meetings", [])

    meeting = {
        "id": str(uuid.uuid4()),
        "date": date,
        "agenda": agenda,
        "scheduled_at": ctx.timestamp.isoformat(),
        "status": "scheduled",
    }
    meeting_id = ctx.state.append("chama.meetings", meeting)

    # 4. EventBus
    await ctx.events.publish(
        "event.chama.meeting_scheduled",
        {"meeting_id": meeting_id, "date": date, "agenda": agenda},
    )

    return OperatorResult.ok({
        "meeting_id": meeting_id,
        "date": date,
        "agenda": agenda,
        "status": "scheduled",
    })


# ── chama.dividend.calculate ───────────────────────────────────────────────────

@sustena_operator(
    name="chama.dividend.calculate",
    description="Compute per-member dividend split based on contribution balances. Returns a ResponseWidget.",
    constraints=[],
    side_effects=[],   # read-only — no state mutations, no events
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    ui_schema={
        "widget_type": "dividend_table",
        "fields": [
            {"label": "Fund Total", "source": "state.chama.fund.total", "display": "currency"},
            {"label": "Members", "source": "state.chama.members", "display": "count"},
        ],
        "chart": "bar",
        "ctas": ["Approve Payout", "Export CSV"],
    },
)
async def chama_dividend_calculate(ctx: OperatorContext) -> OperatorResult:
    """
    Read all member balances and compute each member's proportional dividend.

    Dividend share = member_balance / sum_of_all_positive_balances.
    Returns a dividend_table ResponseWidget. No state mutations, no events fired.

    Primitives used:
      ConstraintEngine  — no constraints (read-only, always permitted)
      PawaLedger        — deducts 0 pawa (free operator)
      StateAccessor     — reads chama.members and chama.fund.total
      EventBus          — no events (read-only)
    """
    # 1. ConstraintEngine — no constraints
    # 2. PawaLedger
    fail = await _charge_pawa("chama.dividend.calculate", ctx)
    if fail:
        return fail

    # 3. StateAccessor — read all member balances
    members_raw: dict = ctx.state.get("chama.members", {})
    fund_total: float = ctx.state.get("chama.fund.total", 0.0)

    member_rows = []
    total_positive_balance = 0.0

    for member_id, data in members_raw.items():
        balance = data.get("balance", 0.0)
        if balance > 0:
            total_positive_balance += balance
        member_rows.append({
            "member_id": member_id,
            "balance": balance,
            "contributions_count": len(data.get("contributions", [])),
            "fines_total": data.get("fines_total", 0.0),
        })

    for row in member_rows:
        balance = row["balance"]
        if total_positive_balance > 0 and balance > 0:
            row["dividend_share_pct"] = round(balance / total_positive_balance * 100, 2)
            row["dividend_amount"] = round(balance / total_positive_balance * fund_total, 2)
        else:
            row["dividend_share_pct"] = 0.0
            row["dividend_amount"] = 0.0

    member_rows.sort(key=lambda r: r["balance"], reverse=True)

    widget = {
        "type": "dividend_table",
        "data": {
            "fund_total": fund_total,
            "member_count": len(member_rows),
            "total_positive_balance": total_positive_balance,
            "members": member_rows,
        },
        "summary": (
            f"KES {fund_total:,.0f} pool across {len(member_rows)} member(s)."
        ),
    }

    # 4. EventBus — no events for read-only operator
    return OperatorResult.ok(widget)
