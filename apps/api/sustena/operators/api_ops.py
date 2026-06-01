"""
sustena/operators/api_ops.py

api.* operators — generic HTTP adapters.

Any external REST endpoint becomes a Sustena operator.

Operators:
  api.get            — HTTP GET against an external URL (rpc)
  api.post           — HTTP POST with a JSON body (rpc)
  api.webhook_listen — Register a webhook event listener config (event_driven)
"""

import httpx

from sustena.core.operator import OperatorContext, OperatorResult, sustena_operator


# ── api.get ────────────────────────────────────────────────────────────────────

@sustena_operator(
    name="api.get",
    protocol="rpc",
    description="HTTP GET against an external URL and return the response body.",
    side_effects=[],
    pawa_cost=1,
    license_tier="per_use",
    author="sustena_core",
)
async def api_get(
    ctx: OperatorContext,
    url: str,
    headers: dict | None = None,
    timeout_s: int = 10,
) -> OperatorResult:
    """
    params:
      url       -- target URL
      headers   -- optional request headers
      timeout_s -- timeout in seconds
    """
    try:
        async with httpx.AsyncClient() as client:
            response = await client.get(url, headers=headers or {}, timeout=timeout_s)
        return OperatorResult.ok({
            "status_code": response.status_code,
            "body": response.text,
            "headers": dict(response.headers),
        })
    except httpx.TimeoutException:
        return OperatorResult.fail(reason=f"GET {url} timed out after {timeout_s}s")
    except httpx.RequestError as exc:
        return OperatorResult.fail(reason=f"GET {url} failed: {exc}")


# ── api.post ───────────────────────────────────────────────────────────────────

@sustena_operator(
    name="api.post",
    protocol="rpc",
    description="HTTP POST with a JSON body to an external URL and return the response.",
    side_effects=[],
    pawa_cost=1,
    license_tier="per_use",
    author="sustena_core",
)
async def api_post(
    ctx: OperatorContext,
    url: str,
    body: dict | None = None,
    headers: dict | None = None,
    timeout_s: int = 10,
) -> OperatorResult:
    """
    params:
      url       -- target URL
      body      -- JSON body to send
      headers   -- optional request headers
      timeout_s -- timeout in seconds
    """
    try:
        async with httpx.AsyncClient() as client:
            response = await client.post(
                url, json=body or {}, headers=headers or {}, timeout=timeout_s
            )
        return OperatorResult.ok({
            "status_code": response.status_code,
            "body": response.text,
            "headers": dict(response.headers),
        })
    except httpx.TimeoutException:
        return OperatorResult.fail(reason=f"POST {url} timed out after {timeout_s}s")
    except httpx.RequestError as exc:
        return OperatorResult.fail(reason=f"POST {url} failed: {exc}")


# ── api.webhook_listen ─────────────────────────────────────────────────────────

@sustena_operator(
    name="api.webhook_listen",
    protocol="event_driven",
    description="Register a webhook endpoint as an EventBus source for incoming external events.",
    side_effects=["event.api.webhook_registered"],
    pawa_cost=0,
    license_tier="free",
    author="sustena_core",
)
async def api_webhook_listen(
    ctx: OperatorContext,
    event_type: str,
    endpoint_path: str = "/webhook",
) -> OperatorResult:
    """
    Persist a webhook listener config in sustain state so the HTTP router can
    register a handler. Actual delivery is handled by the API layer.

    params:
      event_type    -- sustain event name to emit on incoming webhook
      endpoint_path -- URL path to listen on (default /webhook)
    """
    webhook_config = {
        "event_type": event_type,
        "endpoint_path": endpoint_path,
        "sustain_id": ctx.sustain_id,
        "registered_at": ctx.timestamp.isoformat(),
    }

    if not ctx.state.exists("webhooks"):
        ctx.state.set("webhooks", {"listeners": []})
    elif not ctx.state.exists("webhooks.listeners"):
        ctx.state.set("webhooks.listeners", [])

    ctx.state.append("webhooks.listeners", webhook_config)

    await ctx.events.publish("event.api.webhook_registered", webhook_config)

    return OperatorResult.ok({
        "event_type": event_type,
        "endpoint_path": endpoint_path,
        "registered": True,
    })
