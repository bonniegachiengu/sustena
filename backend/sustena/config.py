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
    )

    # ── AI ────────────────────────────────────────────────────────────────
    anthropic_api_key: str = Field(..., description="Anthropic API key")
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
    whatsapp_token: str = Field(..., description="Meta WhatsApp Business API bearer token")
    whatsapp_phone_id: str = Field(..., description="Meta WhatsApp Business Phone ID")
    whatsapp_verify_token: str = Field(
        default="sustena_webhook_verify_2026",
        description="Webhook verification token set in Meta dashboard",
    )

    # ── Security ──────────────────────────────────────────────────────────
    secret_key: str = Field(..., description="JWT signing secret — must be long and random")
    admin_token: str = Field(..., description="Bearer token for admin-only endpoints")

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

    @property
    def is_development(self) -> bool:
        return self.environment == "development"

    @property
    def is_production(self) -> bool:
        return self.environment == "production"


# Singleton — import this everywhere
settings = Settings()
