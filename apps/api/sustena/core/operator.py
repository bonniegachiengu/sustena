"""
sustena/core/operator.py

OperatorContext, OperatorResult, and the @sustena_operator decorator.

Every operator function receives an OperatorContext and returns an OperatorResult.
The @sustena_operator decorator registers functions in the global OPERATOR_REGISTRY
and validates their metadata.

Usage:
    from sustena.core.operator import sustena_operator, OperatorContext, OperatorResult

    @sustena_operator(
        name="budget.allocate",
        description="Allocate an amount to a budget pocket",
        constraints=["params.amount > 0", "finances.liquid.balance >= params.amount"],
        side_effects=["event.finances.pocket_allocated"],
        pawa_cost=0,
        license_tier="free",
        author="sustena_core",
    )
    async def budget_allocate(ctx: OperatorContext, pocket_name: str, amount: float) -> OperatorResult:
        ctx.state.decrement("finances.liquid.balance", amount)
        ctx.state.increment(f"finances.pockets.{pocket_name}.allocated", amount)
        await ctx.events.publish("event.finances.pocket_allocated", {
            "pocket": pocket_name, "amount": amount
        })
        return OperatorResult.ok({"pocket": pocket_name, "allocated": amount})
"""

import functools
import logging
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Callable

logger = logging.getLogger(__name__)

# ── Global operator registry ───────────────────────────────────────────────────
# All operators decorated with @sustena_operator are registered here.
# The SustainEngine loads operators from this registry by name.
OPERATOR_REGISTRY: dict[str, "OperatorMeta"] = {}


# ── Operator metadata ─────────────────────────────────────────────────────────

@dataclass
class OperatorMeta:
    """Metadata for a registered operator."""
    name: str                           # e.g. "budget.allocate"
    description: str
    fn: Callable                        # The actual async function
    constraints: list[str] = field(default_factory=list)  # Pre-condition predicates
    post_constraints: list[str] = field(default_factory=list)  # Post-condition predicates
    side_effects: list[str] = field(default_factory=list)   # Event names it may publish
    pawa_cost: int = 0
    license_tier: str = "free"          # free | per_use | subscription | one_time
    author: str = "sustena_core"
    min_privilege: int = 1              # Minimum sustain access tier required (0=owner, 1=member, ...)
    ui_schema: dict = field(default_factory=dict)
    protocol: str = "rpc"               # rpc | event_driven | polling | streaming


# ── Operator context ──────────────────────────────────────────────────────────

@dataclass
class OperatorContext:
    """
    Everything an operator needs to execute, passed as first argument.

    - state: read/write access to the sustain's live state
    - events: publish events to the event log
    - pawa: charge pawa tokens
    - sustain_id, user_id: execution context
    - operative_id: which operative called this (None = user direct call)
    - timestamp: execution timestamp (use this, not datetime.utcnow())
    """
    state: Any              # StateAccessor
    events: Any             # EventBus
    pawa: Any               # PawaLedger
    sustain_id: str
    user_id: str
    operative_id: str | None = None
    timestamp: datetime = field(default_factory=datetime.utcnow)
    logger: logging.Logger = field(default_factory=lambda: logging.getLogger("sustena.operator"))


# ── Operator result ───────────────────────────────────────────────────────────

class OperatorResult:
    """
    Standardised return type from every operator.

    status: "ok" | "failed" | "deferred"
    data: output data (on success)
    reason: failure message (on failure)
    constraint_violated: which constraint string failed (on failure)
    proposal_id: Council proposal ID (on deferred — awaiting Council vote)
    """

    def __init__(
        self,
        status: str,
        data: dict | None = None,
        reason: str | None = None,
        constraint_violated: str | None = None,
        proposal_id: str | None = None,
    ) -> None:
        self.status = status
        self.data = data or {}
        self.reason = reason
        self.constraint_violated = constraint_violated
        self.proposal_id = proposal_id

    @classmethod
    def ok(cls, data: dict | None = None) -> "OperatorResult":
        return cls(status="ok", data=data or {})

    @classmethod
    def fail(
        cls,
        reason: str,
        constraint_violated: str | None = None,
    ) -> "OperatorResult":
        return cls(status="failed", reason=reason, constraint_violated=constraint_violated)

    @classmethod
    def deferred(cls, proposal_id: str) -> "OperatorResult":
        return cls(status="deferred", proposal_id=proposal_id)

    @property
    def succeeded(self) -> bool:
        return self.status == "ok"

    # Alias kept for backwards compatibility (stale bytecode in __pycache__)
    @property
    def success(self) -> bool:
        return self.status == "ok"

    @property
    def failed(self) -> bool:
        return self.status == "failed"

    @property
    def is_deferred(self) -> bool:
        return self.status == "deferred"

    def to_response(self) -> dict:
        """Serialisable dict — safe to return from a FastAPI endpoint."""
        result: dict[str, Any] = {"status": self.status, "data": self.data}
        if self.reason:
            result["reason"] = self.reason
        if self.constraint_violated:
            result["constraint_violated"] = self.constraint_violated
        if self.proposal_id:
            result["proposal_id"] = self.proposal_id
        return result

    def __repr__(self) -> str:
        if self.status == "ok":
            return f"OperatorResult.ok({self.data})"
        if self.status == "failed":
            return f"OperatorResult.fail({self.reason!r})"
        return f"OperatorResult.deferred({self.proposal_id!r})"


# ── @sustena_operator decorator ───────────────────────────────────────────────

def sustena_operator(
    name: str,
    description: str,
    constraints: list[str] | None = None,
    post_constraints: list[str] | None = None,
    side_effects: list[str] | None = None,
    pawa_cost: int = 0,
    license_tier: str = "free",
    author: str = "sustena_core",
    min_privilege: int = 1,
    ui_schema: dict | None = None,
    protocol: str = "rpc",
) -> Callable:
    """
    Decorator that registers an async operator function in OPERATOR_REGISTRY.

    The decorated function must:
      - Be async
      - Accept (ctx: OperatorContext, **kwargs) as its signature
      - Return an OperatorResult

    Metadata fields are validated at decoration time (import time), not at call time.
    This means a missing required field causes an ImportError, not a runtime error.
    """
    def decorator(fn: Callable) -> Callable:
        # Validate required fields
        if not name or "." not in name:
            raise ValueError(
                f"Operator name '{name}' must be namespaced with a dot: 'domain.action'"
            )
        if not description:
            raise ValueError(f"Operator '{name}' must have a description")

        meta = OperatorMeta(
            name=name,
            description=description,
            fn=fn,
            constraints=constraints or [],
            post_constraints=post_constraints or [],
            side_effects=side_effects or [],
            pawa_cost=pawa_cost,
            license_tier=license_tier,
            author=author,
            min_privilege=min_privilege,
            ui_schema=ui_schema or {},
            protocol=protocol,
        )

        if name in OPERATOR_REGISTRY:
            logger.warning("Operator '%s' is being re-registered — overwriting.", name)

        OPERATOR_REGISTRY[name] = meta
        logger.debug("Operator registered: %s (pawa=%d, tier=%s)", name, pawa_cost, license_tier)

        @functools.wraps(fn)
        async def wrapper(ctx: OperatorContext, **kwargs: Any) -> OperatorResult:
            return await fn(ctx, **kwargs)

        wrapper._operator_meta = meta  # type: ignore[attr-defined]
        return wrapper

    return decorator
