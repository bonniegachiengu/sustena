"""
sustena/core/operative_runtime.py

OperativeRuntime — protocol-aware dispatcher for OperativeGraph instances.

Determines how a graph is triggered based on the entry node's operator protocol:

  rpc          — immediate call when run_once() is invoked (default)
  event_driven — graph runs when a matching event fires on the trigger EventBus
  polling      — graph runs on a recurring asyncio schedule via start_polling()
  streaming    — graph runs with a persistent connection context (future work)

Usage:
    runtime = OperativeRuntime(graph, trigger_bus, context_factory)
    runtime.register(event_filter="event.finances.pocket_spent")
    # Now the graph fires automatically whenever that event is published.

    # OR for polling:
    runtime.register(interval_s=3600)
    asyncio.create_task(runtime.start_polling())
"""

import asyncio
import logging
from typing import TYPE_CHECKING, Any, Callable

from sustena.core.events import EventBus
from sustena.core.operator import OPERATOR_REGISTRY, OperatorContext

if TYPE_CHECKING:
    from sustena.operatives.base import OperativeProposal

logger = logging.getLogger(__name__)


class OperativeRuntime:
    """
    Protocol-aware dispatcher for an OperativeGraph.

    The entry node's operator protocol governs how the graph is triggered.
    All non-entry nodes execute synchronously within a single graph run.

    Attributes:
        graph            : the OperativeGraph to dispatch
        trigger_event_bus: shared EventBus where trigger events arrive
        context_factory  : callable returning a fresh OperatorContext per run
    """

    def __init__(
        self,
        graph: Any,          # OperativeGraph — avoid circular import
        trigger_event_bus: EventBus,
        context_factory: Callable[[], OperatorContext],
    ) -> None:
        self.graph = graph
        self.trigger_event_bus = trigger_event_bus
        self.context_factory = context_factory
        self._polling_interval: int = 3600
        self._last_proposals: list = []  # list[OperativeProposal] — avoid circular import
        self._registered: bool = False

    # ── Registration ──────────────────────────────────────────────────────────

    def register(
        self,
        event_filter: str | None = None,
        interval_s: int | None = None,
    ) -> None:
        """
        Register this graph with the appropriate dispatch mechanism based on the
        entry node's operator protocol.

        params:
          event_filter : event name / wildcard for event_driven dispatch
                         (default: entry node's side_effects or "event.*.*")
          interval_s   : polling interval in seconds for polling dispatch (default 3600)
        """
        entry_node = self.graph.nodes.get(self.graph.entry_node)
        if entry_node is None:
            logger.warning("[OperativeRuntime] entry node '%s' not found", self.graph.entry_node)
            return

        op_meta = OPERATOR_REGISTRY.get(entry_node.operator_name)
        if op_meta is None:
            logger.warning(
                "[OperativeRuntime] entry operator '%s' not in registry",
                entry_node.operator_name,
            )
            return

        protocol = op_meta.protocol
        logger.debug(
            "[OperativeRuntime] registering graph with protocol='%s' entry='%s'",
            protocol, entry_node.operator_name,
        )

        if protocol == "event_driven":
            self._register_event_subscriber(event_filter or "event.*.*")
        elif protocol == "polling":
            self._polling_interval = interval_s if interval_s is not None else 3600
        elif protocol in ("rpc", "streaming"):
            pass  # rpc/streaming triggered explicitly via run_once()
        else:
            logger.warning("[OperativeRuntime] unknown protocol '%s' — treated as rpc", protocol)

        self._registered = True

    # ── Event-driven dispatch ─────────────────────────────────────────────────

    def _register_event_subscriber(self, event_filter: str) -> None:
        """Subscribe to the trigger EventBus; run the graph when the event fires."""
        async def _handler(payload: dict) -> None:
            ctx = self.context_factory()
            try:
                proposal = await self.graph.run(ctx, payload)
                self._last_proposals.append(proposal)
                logger.info(
                    "[OperativeRuntime] event_driven graph ran: operator='%s'",
                    proposal.operator_name,
                )
            except Exception as exc:
                logger.error(
                    "[OperativeRuntime] event_driven graph failed for '%s': %s",
                    event_filter, exc,
                )

        self.trigger_event_bus.subscribe(event_filter, _handler)
        logger.debug("[OperativeRuntime] subscribed to '%s'", event_filter)

    # ── Polling dispatch ──────────────────────────────────────────────────────

    async def start_polling(self) -> None:
        """
        Start the polling loop. Runs indefinitely at self._polling_interval seconds.
        Designed to be run as an asyncio task; cancel the task to stop it.
        """
        logger.info(
            "[OperativeRuntime] polling started (interval=%ds)", self._polling_interval
        )
        while True:
            try:
                proposal = await self.run_once({"trigger": "poll"})
                self._last_proposals.append(proposal)
                logger.debug(
                    "[OperativeRuntime] polling graph ran: operator='%s'",
                    proposal.operator_name,
                )
            except Exception as exc:
                logger.error("[OperativeRuntime] polling graph failed: %s", exc)
            await asyncio.sleep(self._polling_interval)

    # ── Direct execution ──────────────────────────────────────────────────────

    async def run_once(self, trigger_event: dict) -> "OperativeProposal":
        """
        Run the graph immediately with the given trigger_event.
        Used for rpc protocol and one-shot testing.
        """
        ctx = self.context_factory()
        return await self.graph.run(ctx, trigger_event)

    # ── Result collection ─────────────────────────────────────────────────────

    def collect_proposals(self) -> list:
        """Return and clear all proposals accumulated by event-driven/polling dispatch."""
        proposals = list(self._last_proposals)
        self._last_proposals.clear()
        return proposals
