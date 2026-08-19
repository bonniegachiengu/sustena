"""
Cross-holon transfer vector generation — R1 PARITY, recorded from the reference.

Drives the reference's real `_execute_holon_transfer` against a live in-memory
`SustainEngine` with two genuinely linked habitats. That function IS the
keystone: it runs every guard, applies the move, checks conservation, and
commits both legs in one sqlite transaction.

Each case records the decision AND both balances **after** — because for a
transfer the interesting property is not only "was it refused" but "did anything
move when it was". A refusal that left one side changed would be the exact
failure this slice exists to rule out, so the recorded state is part of the
contract.

`total_before` / `total_after` are summed here from the two real states, so the
conservation law is recorded as an observation rather than as the reference's
own claim about itself.
"""

from __future__ import annotations

import asyncio

import sustena.operators  # noqa: F401 -- importing registers every operator
from sustena.core.state import StateAccessor
from sustena.core.sustain_engine import SustainEngine
from sustena.operators.holon import _execute_holon_transfer

LIQUID = "finances.liquid.balance"

# name, from_balance, to_balance, amount, direction, linked
#   direction "up"   = child -> parent
#   direction "down" = parent -> child
#   direction "self" = the same holon on both sides
#   linked False     = two holons with no direct edge
TRANSFER_CASES = [
    ("conserved_child_to_parent", 1000.0, 250.0, 400.0, "up", True),
    ("conserved_parent_to_child", 1000.0, 250.0, 400.0, "down", True),
    ("moving_the_whole_balance_conserves", 700.0, 0.0, 700.0, "up", True),
    ("insufficient_balance_moves_nothing", 10.0, 5.0, 999.0, "up", True),
    ("exactly_the_balance_is_allowed", 300.0, 0.0, 300.0, "up", True),
    ("one_minor_unit_over_is_refused", 300.0, 0.0, 301.0, "up", True),
    ("zero_amount_refused", 100.0, 0.0, 0.0, "up", True),
    ("negative_amount_refused", 100.0, 0.0, -50.0, "up", True),
    ("self_transfer_refused", 100.0, 0.0, 10.0, "self", True),
    ("unlinked_pair_refused", 100.0, 0.0, 10.0, "up", False),
    # ★ Recorded because the two engines DIVERGE here, deliberately: the
    #   reference conserves to six decimal places, this core conserves in
    #   integer minor units. Both answers are in the file so the divergence is
    #   measured rather than asserted.
    ("fractional_amount", 100.0, 0.0, 0.5, "up", True),
]


def _seed(engine: SustainEngine, sustain_id: str, balance: float) -> None:
    acc = StateAccessor(engine.get_state(sustain_id))
    acc.set(LIQUID, balance)
    engine.commit_external_mutation(sustain_id, acc, "event.test.seed", {})


def run_transfer_case(
    from_balance: float, to_balance: float, amount: float, direction: str, linked: bool
) -> dict:
    """Record what the reference decided, and what both holons hold after."""
    engine = SustainEngine(db_path=":memory:")
    params = {"name": "A", "role_in_family": "x", "owner_ids": ["u1"]}
    parent = engine.instantiate("habitat", "u1", dict(params, name="Parent"))
    child = engine.instantiate("habitat", "u1", dict(params, name="Child"))
    if linked:
        engine.link_child(parent, child, slot="slot_1", member="Child")

    if direction == "down":
        src, dst = parent, child
    else:
        src, dst = child, parent
    if direction == "self":
        dst = src

    _seed(engine, src, from_balance)
    if dst != src:
        _seed(engine, dst, to_balance)

    def liquid(sid: str) -> float:
        return StateAccessor(engine.get_state(sid)).get(LIQUID)

    before = (liquid(src), liquid(dst) if dst != src else liquid(src))
    total_before = before[0] + before[1] if dst != src else before[0]

    result = asyncio.run(_execute_holon_transfer(engine, src, dst, amount))

    after = (liquid(src), liquid(dst) if dst != src else liquid(src))
    total_after = after[0] + after[1] if dst != src else after[0]

    return {
        "status": result.status,
        "constraint_violated": getattr(result, "constraint_violated", None),
        "from_before": before[0],
        "to_before": before[1],
        "from_after": after[0],
        "to_after": after[1],
        "total_before": total_before,
        "total_after": total_after,
        "conserved": round(total_before, 9) == round(total_after, 9),
    }
