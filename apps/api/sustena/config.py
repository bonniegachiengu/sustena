"""
sustena/config.py
Loads and validates all environment configuration for Sustena XII.
Raises a clear error on startup if any required variable is missing.
"""

from typing import Literal
from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore",
    )

    # ── AI ────────────────────────────────────────────────────────────────
    # Set to "mock" to use MockClaudeClient (zero spend, full stack runs locally)
    anthropic_api_key: str = Field(default="mock", description="Anthropic API key — 'mock' for local dev")
    claude_haiku_model: str = Field(
        default="claude-haiku-4-5-20251001",
        description="Claude Haiku model string for fast classification calls",
    )
    claude_sonnet_model: str = Field(
        default="claude-sonnet-4-6",
        description="Claude Sonnet model string for strategic reasoning",
    )

    # ── Database ──────────────────────────────────────────────────────────
    database_url: str = Field(
        default="sqlite+aiosqlite:///./sustena.db",
        description="SQLAlchemy async database URL",
    )

    # ── WhatsApp Business API ─────────────────────────────────────────────
    # Leave as "mock" to use MockWhatsAppSender — messages print to console only
    whatsapp_token: str = Field(default="mock", description="Meta WhatsApp token — 'mock' for local dev")
    whatsapp_phone_id: str = Field(default="mock", description="Meta WhatsApp Phone ID — 'mock' for local dev")
    whatsapp_verify_token: str = Field(
        default="sustena_webhook_verify_2026",
        description="Webhook verification token set in Meta dashboard",
    )

    # ── Security ──────────────────────────────────────────────────────────
    secret_key: str = Field(default="dev-secret-change-in-production", description="JWT signing secret")
    admin_token: str = Field(default="dev-admin-token", description="Bearer token for admin-only endpoints")

    # ── Environment ───────────────────────────────────────────────────────
    environment: Literal["development", "production"] = Field(
        default="development",
        description="Runtime environment — controls debug features, CORS, OpenAPI docs",
    )

    # ── Google Cloud ──────────────────────────────────────────────────────
    google_cloud_project: str = Field(
        default="sustena-xii",
        description="GCP project ID for Firestore sync",
    )

    # ── Cockpit surface ───────────────────────────────────────────────────
    # SEPARATE from `environment`, deliberately.
    #
    # `/devui` began as a dev-only surface but is no longer one: Orchie (the
    # phone app), Studio (desktop) and Mycelium all depend on it for the
    # sustain list, holon children/parent, and roll-up. Every one of its ~51
    # endpoints requires a real JWT session — it is an authenticated API, not
    # an open debug hatch (verified: an unauthenticated request gets 401).
    #
    # While it was mounted under `if is_development`, hardening the host would
    # have amputated all three apps — which is precisely why this deployment
    # stayed in development mode. Splitting the two concerns is what lets the
    # host be hardened without breaking what depends on it.
    #
    # `/seed` and `/dev` are deliberately NOT covered by this flag; they remain
    # strictly development-only. /seed writes rows directly and bypasses the
    # engine (it can half-create a sustain with no genesis event), and /dev is
    # a debug surface for the legacy WhatsApp stub. Neither belongs on a live
    # host at any setting.
    enable_cockpit: bool = Field(
        default=True,
        description="Mount the authenticated /devui cockpit API (Orchie, Studio and Mycelium need it)",
    )

    @property
    def is_development(self) -> bool:
        return self.environment == "development"

    @property
    def is_production(self) -> bool:
        return self.environment == "production"


# Values that ship in this repository and are therefore public knowledge.
# Fine for local development; never acceptable on anything reachable.
_PUBLIC_DEFAULT_SECRETS = {
    "secret_key": "dev-secret-change-in-production",
    "admin_token": "dev-admin-token",
}


def _assert_production_secrets(cfg: "Settings") -> None:
    """Refuse to boot in production with the repository's public defaults.

    The module docstring has always promised this ("Raises a clear error on
    startup if any required variable is missing") but nothing enforced it —
    every field had a default, so a misconfigured production deploy would come
    up silently signing JWTs with a secret anyone can read in this repo.

    Fails loudly at import, listing every offending variable at once rather
    than one per restart.
    """
    if not cfg.is_production:
        return

    offenders = [
        name for name, public_default in _PUBLIC_DEFAULT_SECRETS.items()
        if getattr(cfg, name) == public_default
    ]
    if offenders:
        listed = ", ".join(v.upper() for v in offenders)
        raise RuntimeError(
            f"Refusing to start: ENVIRONMENT=production but {listed} still "
            f"hold the public default value(s) published in this repository. "
            f"Anyone could forge a session. Set real values in the environment "
            f"— generate one with: "
            f"python -c \"import secrets; print(secrets.token_urlsafe(32))\""
        )


# Singleton — import this everywhere
settings = Settings()
_assert_production_secrets(settings)
