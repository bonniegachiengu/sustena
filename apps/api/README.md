# Sustena XII — Backend
![CI](https://github.com/bonniegachiengu/sustena/actions/workflows/ci.yml/badge.svg)
> Human-agent reality interface. 7 primitives. Any describable system.

## Quick start (Windows / PowerShell)

```powershell
cd backend\

# 1. Create and activate a virtual environment
python -m venv venv
.\venv\Scripts\Activate.ps1

# 2. Copy env config — all defaults work locally, no credentials needed
copy .env.example .env

# 3. Install dependencies
pip install -e ".[dev]"

# 4. Start the server (creates DB tables automatically on first run)
uvicorn sustena.api.main:app --reload --host 0.0.0.0 --port 9000
```

Then open `http://localhost:9000/docs` for the interactive API.

### PyCharm setup
- Open the `backend\` folder as a PyCharm project
- Set the Python interpreter to `backend\venv\Scripts\python.exe`
- Add a **FastAPI** run config: module `uvicorn`, parameters `sustena.api.main:app --reload --port 9000`
- Tests: right-click `tests\` → Run with pytest

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
