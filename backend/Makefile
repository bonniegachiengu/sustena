.PHONY: run test migrate lint install clean

# ── Dev server ────────────────────────────────────
run:
	uvicorn sustena.api.main:app --reload --host 0.0.0.0 --port 9000

# ── Tests ─────────────────────────────────────────
test:
	pytest tests/ -v

test-cov:
	pytest tests/ --cov=sustena --cov-report=term-missing

# ── Database ──────────────────────────────────────
migrate:
	alembic upgrade head

migrate-new:
	alembic revision --autogenerate -m "$(msg)"

migrate-down:
	alembic downgrade -1

# ── Code quality ──────────────────────────────────
lint:
	ruff check sustena/ tests/
	ruff format --check sustena/ tests/

format:
	ruff format sustena/ tests/

# ── Install ───────────────────────────────────────
install:
	pip install -e ".[dev]"

# ── Cloud Run ─────────────────────────────────────
deploy:
	gcloud builds submit --config cloudbuild.yaml

# ── Clean ─────────────────────────────────────────
clean:
	find . -type d -name __pycache__ -exec rm -rf {} +
	find . -name "*.pyc" -delete
	find . -name ".pytest_cache" -exec rm -rf {} +
	rm -f sustena.db
