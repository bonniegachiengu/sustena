# Sustena XII — Backend

> Human-agent reality interface. 7 primitives. Any describable system.

## Quick start

```bash
cd backend/
cp .env.example .env        # fill in your real keys
make install                # pip install -e ".[dev]"
make migrate                # create SQLite tables
make run                    # start dev server on :8000
```

Then open `http://localhost:8000/docs` for the interactive API.

## Directory structure

```
backend/
├── sustena/
│   ├── api/            # FastAPI routes and app factory
│   │   └── routes/     # One file per resource (sustains, users, webhook, ...)
│   ├── core/           # The 7 primitives: State, Operator, Constraint, Event, Pawa, Council
│   ├── db/             # SQLAlchemy schema + Alembic migrations + Firestore sync
│   ├── operators/      # Operator implementations (budget.*, chama.*, vyyb.*, ...)
│   ├── operatives/     # Operative classes (Mentor, Protégé, Chama Secretary, ...)
│   └── sustains/       # Sustain JSON specs (homestead.json, vyyb_biashara.json, ...)
├── tests/
├── alembic/            # Database migration scripts
├── Dockerfile
├── cloudbuild.yaml     # Google Cloud Build → Cloud Run CI/CD
├── Makefile
└── pyproject.toml
```

## Testing

```bash
make test               # run all tests
make test-cov           # run tests with coverage report
```

## Deploy to Cloud Run

```bash
make deploy             # runs gcloud builds submit
```

Requires `gcloud` CLI configured with the `sustena-xii` project.

## Architecture

See `docs/Sustena_XII_Master_Strategy.md` (project root) and `sustena/Sustena_XII_Feature_Spec.md` for full architecture docs.

The 7 primitives: **State · Operator · Constraint · Event · Time · Consensus · Operative**
