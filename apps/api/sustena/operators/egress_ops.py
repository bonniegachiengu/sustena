"""
sustena/operators/egress_ops.py

egress.* — the only operators permitted to queue an OUTBOUND effect
(Slice 11). Egress operators never send anything themselves — they call
ctx.egress.queue(...), which SustainEngine.execute_operator() turns into a
'prepared' row in egress_outbox after the gate passes. The actual send only
happens later, from a human-confirmed SustainEngine.confirm_egress() call —
never from inside an operator, never automatically.

HARD SAFETY BOUNDARY: no egress operator in this module (or any future one)
may touch money, a payment rail, or a third-party messaging API. The only
implemented send target is a local file export (see
SustainEngine._send_egress). Anything financial stays prepare/draft-only
forever per the standing rule — Sustena does not fire money actions
autonomously, full stop.
"""

from sustena.core.operator import OperatorContext, OperatorResult, sustena_operator


@sustena_operator(
    name="egress.prepare_household_summary",
    description=(
        "Prepare a household/personal finance summary for export. Queues it "
        "in the outbox as 'prepared' — nothing leaves the system until a "
        "human explicitly confirms it."
    ),
    side_effects=["event.egress.prepared"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
    protocol="rpc",
    ui_schema={
        "widget_type": "egress_prepared_card",
        "fields": [
            {"label": "Label", "source": "inputs.label", "display": "text"},
            {"label": "Liquid balance", "source": "state.finances.liquid.balance", "display": "currency"},
        ],
        "ctas": ["Review outbox"],
    },
)
async def prepare_household_summary(ctx: OperatorContext, label: str = "") -> OperatorResult:
    """
    Builds a summary payload from CURRENT state (liquid balance + pocket
    overview) and queues it for export. Mutates no sustain state at all —
    a pure read + queue, so the S2 gate has nothing to object to and the
    S3 fold records an event with empty mutations (a real, legitimate
    "this happened but changed no state" record, same as any purely
    informational event).

    idempotency_key defaults to a per-day key so preparing "the daily
    summary" twice on the same day is a no-op at the engine level (see
    SustainEngine._queue_egress's INSERT OR IGNORE on
    (sustain_id, idempotency_key)); an explicit label lets a human prepare
    more than one distinct summary in a day, and — deliberately not
    sanitized here — a label containing characters illegal in a filename
    will make the eventual send genuinely fail rather than silently
    succeed with mangled output; see confirm_egress()'s honest failure
    handling.
    """
    snapshot = ctx.state.snapshot()
    finances = snapshot.get("finances") if isinstance(snapshot, dict) else None
    finances = finances if isinstance(finances, dict) else {}
    liquid_balance = (finances.get("liquid") or {}).get("balance", 0)
    pockets = finances.get("pockets") or {}
    pocket_summary = {
        name: {"allocated": p.get("allocated", 0), "spent": p.get("spent", 0)}
        for name, p in pockets.items() if isinstance(p, dict)
    }

    payload = {
        "sustain_id": ctx.sustain_id,
        "generated_at": ctx.timestamp.isoformat(),
        "label": label,
        "liquid_balance": liquid_balance,
        "pockets": pocket_summary,
    }

    # "--" not ":" as the separator: a raw ":" in a Windows filename doesn't
    # error, it silently redirects everything after it into an NTFS
    # Alternate Data Stream — confirmed empirically while building this
    # operator. Keeping ":" out of the key means confirm_egress's file
    # write either produces the real, visible file the human expects, or
    # genuinely fails (e.g. on a label containing "?", "*", "<", ">", "|")
    # — never a silent, invisible stream nobody will ever look at.
    idempotency_key = (
        f"household_summary--{label}" if label
        else f"household_summary--{ctx.timestamp.date().isoformat()}"
    )

    ctx.egress.queue(
        kind="household_summary_export",
        target="local_file",
        payload=payload,
        idempotency_key=idempotency_key,
    )

    await ctx.events.publish("event.egress.prepared", {
        "kind": "household_summary_export",
        "idempotency_key": idempotency_key,
    })

    return OperatorResult.ok({"idempotency_key": idempotency_key, "queued": True})
