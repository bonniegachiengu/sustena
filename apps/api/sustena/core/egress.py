"""
sustena/core/egress.py

EgressQueue — the in-memory queuing primitive an operator uses to signal
"I want an outbound effect prepared", mirroring EventBus's
published_this_context() pattern.

HARD SAFETY BOUNDARY: EgressQueue has no send capability whatsoever. It is
a plain in-memory list an operator appends to; nothing about it can reach
a network, a file, or a payment rail. The actual outbound send only ever
happens in SustainEngine._send_egress(), called from confirm_egress() —
a human-triggered action, never from inside an operator call or from
evaluate_operatives(). Queuing (this class) and sending (SustainEngine)
are deliberately different code paths so there is no way for "an operator
ran" to imply "something left the system."

During SustainEngine.simulate() (the sandboxed fork), an operator can call
ctx.egress.queue() exactly like in a real call — it's harmless: the
EgressQueue instance is discarded with the rest of the forked context and
SustainEngine never reads simulate()'s queued items into the outbox table.
Only execute_operator() (the real path) drains a context's queued items
into egress_outbox.
"""

from __future__ import annotations

from typing import Any


class EgressQueue:
    """Per-call, in-memory only. Never touches a DB or the network itself."""

    def __init__(self, sustain_id: str) -> None:
        self.sustain_id = sustain_id
        self._queued: list[dict[str, Any]] = []

    def queue(
        self,
        kind: str,
        target: str,
        payload: dict,
        idempotency_key: str,
    ) -> dict:
        """
        Record an outbound intent for this operator call. Returns the queued
        entry (not yet persisted — SustainEngine.execute_operator() persists
        it to egress_outbox after this operator call succeeds and passes the
        gate, exactly like it persists events).
        """
        entry = {
            "kind": kind,
            "target": target,
            "payload": payload,
            "idempotency_key": idempotency_key,
        }
        self._queued.append(entry)
        return entry

    def queued_this_context(self) -> list[dict]:
        return list(self._queued)
