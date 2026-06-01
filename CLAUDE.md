# Sustena XII — Claude Code Context

> Read this before touching any code. It tells you where we are, how things are built,
> and how Bonnie works. Everything here is current as of 1 June 2026.

---

## What this project is

Sustena XII is a human-agent reality interface — a platform where any describable system
(household, business, farm, chama) can be modelled, simulated, and governed through
7 primitives: State, Operators, Constraints, Events, Time, Consensus, Operatives.

The founding insight: operatives do not require LLMs. Their reasoning is defined as a
directed graph of operator calls. LLMs are optional, pluggable nodes in that graph.
`ANTHROPIC_API_KEY=mock` in `.env` activates MockClaudeClient — the full stack runs
with zero API spend.

Full strategy: `docs/Sustena_XII_Master_Strategy.md`  
Current roadmap: `docs/Sustena_XII_Roadmap_Jun2026.md` ← read this for sprint state

---

## Repo layout

```
apps/
  api/          FastAPI backend (Python) — the source of truth
  web/          React + Vite frontend
docs/           Strategy docs, roadmap, analysis
brand/          UI design components and inspirations
vyyb/           Vyyb business sustain docs
lore/           Public knowledge surface (journal, docs)
```

---

## Backend — `apps/api/`

### Key directories
```
sustena/
  core/           7 primitives: state.py, constraints.py, events.py, pawa.py,
                  operator.py, sustain_engine.py, council.py
  operators/      budget.py, chama.py, procurement.py, vyyb.py, biashara.py, calendar.py
  operatives/     base.py, mentor.py, protege.py, attache.py, navigator.py,
                  curator.py, chama_secretary.py
  sustains/       homestead.json, vyyb.json, chama.json, biashara.json, colosso.json
  api/
    main.py       FastAPI app, lifespan (init_db), CORS, logging
    routes/       sustains.py, devui.py, orchie.py, council.py, whatsapp.py, ...
  db/schema.py    SQLAlchemy tables + async engine singleton (_engine)
  config.py       Pydantic BaseSettings (extra="ignore") — reads .env
  scripts/
    seed_homestead.py   Seeds Bonnie's Homestead sustain into sustena.db
```

### Running
```bash
# From apps/api/
pip install -r requirements.txt
python -m sustena.scripts.seed_homestead   # first time only — delete sustena.db first
uvicorn sustena.api.main:app --reload --port 8000
```

### Testing
```bash
# From apps/api/
python -m pytest tests/ -q --tb=short     # 864 tests, all pass

# Run specific file
python -m pytest tests/test_sustain_engine.py -v
```

**Never use `pytest` directly** — always `python -m pytest` so the package path resolves.

### Test setup (important)
`tests/conftest.py` resets the SQLAlchemy `_engine` singleton before each module and
each async function. This prevents aiosqlite event-loop cross-contamination. Tests use
`sqlite+aiosqlite:///:memory:` — never the real `sustena.db`. Do not remove conftest.py.

### Environment
`.env` defaults work out of the box for local dev:
- `ANTHROPIC_API_KEY=mock` → MockClaudeClient (zero spend)
- `DATABASE_URL=sqlite+aiosqlite:///./sustena.db`
- `ADMIN_TOKEN=dev-admin-token`

---

## Frontend — `apps/web/`

React 18 + Vite + TypeScript. Components loaded as window globals (side-effect imports
in App.tsx) — fragile but functional in Vite's bundled output. Do not enable code-splitting.

```bash
# From apps/web/
npm install
npm run dev      # dev server on :5173
npm run build    # production build
```

Needs `apps/web/.env` with:
```
VITE_API_BASE_URL=http://localhost:8000
VITE_ADMIN_TOKEN=dev-admin-token
```

**Current state:** Shell + all panels built. Mock data was cleared on 1 June 2026.
Frontend makes no API calls yet — Sprint 1 wires it.

---

## Current sprint: Sprint 1 — "Sustena Runs"

Sprint 0 is complete (864/864 tests pass, seed script works, list_all() implemented).

**Sprint 1 goal:** Web UI shows real Homestead data. Orchie replies in mock mode. No LLM.

Tasks:
- [ ] Wire Monitor panel → `GET /devui/state?sustain_id=...` + `WS /devui/state-stream`
- [ ] Wire sustain selector → `GET /devui/sustains`
- [ ] Wire Operator Console → `POST /devui/console/execute`
- [ ] Wire Orchie chat → `POST /orchie/message`
- [ ] Verify `ANTHROPIC_API_KEY=mock` logs "mock mode" at startup

**Definition of done:** Open localhost:5173, see real Homestead pocket data, type in
Orchie chat, get a reply, check server logs show zero Anthropic API calls.

---

## Bonnie's patterns — follow these exactly

### Commits
Conventional commits, **direct to main** (no feature branches, no PRs):
```
feat(scope): description
fix(scope): description
chore(scope): description
test(scope): description
ci: description
docs: description
```

### Tests
- Fix ALL failures before moving to the next task. Never defer test fixes.
- `python -m pytest tests/ -q --tb=short` must pass clean before every commit.
- Core coverage target: >90%.

### No WhatsApp work
WhatsApp stubs exist in the codebase and stay. No new WhatsApp development until
explicitly reactivated. Do not touch `whatsapp_*.py` files or the `/webhook` routes.

### LLM in development
`ANTHROPIC_API_KEY=mock` is always set in `.env`. Never make real Anthropic API calls
during development or testing. The MockClaudeClient in `core/claude_client.py` handles
all operative calls locally.

---

## Architecture decisions

### Operative graphs (Sprint 5 target)
Operatives will be rewritten as directed graphs of operator calls — no LLM required
at the base layer. The graph is defined in JSON. `operative.spawn(template_id, calibration_data)`
calibrates a library template with user context. See Sprint 5 in the roadmap.

### devui endpoints
`/devui/*` endpoints exist and have stub fallbacks. They try to call `SustainEngine`
methods first, fall back to hardcoded stub data if the engine isn't initialised.
After seeding, the real data flows. All stubs have `# TODO: wire real` comments.

### Protocol types (Sprint 3 target)
Every operator will declare `protocol`: `rpc | event_driven | polling | streaming`.
This isn't implemented yet — it's on the roadmap.

---

## Key commands cheatsheet

```bash
# Backend
cd apps/api
python -m pytest tests/ -q              # run all tests
uvicorn sustena.api.main:app --reload   # start server
python -m sustena.scripts.seed_homestead  # seed Homestead sustain

# Frontend
cd apps/web
npm run dev                             # start dev server
npm run build                           # production build

# Useful API calls (dev)
curl -H "Authorization: Bearer dev-admin-token" http://localhost:8000/devui/sustains
curl -H "Authorization: Bearer dev-admin-token" \
  "http://localhost:8000/devui/state?sustain_id=<id>"
curl -X POST http://localhost:8000/orchie/message \
  -H "Content-Type: application/json" \
  -d '{"sustain_id":"<id>","message":"habari"}'
```

---

## What NOT to do

- Do not add async/await to `SustainEngine` methods — it uses synchronous `sqlite3`,
  not aiosqlite. The async layer is only in the FastAPI routes and `init_db()`.
- Do not add `rootdir` to `pyproject.toml [tool.pytest.ini_options]` — it's not a valid key.
- Do not write to `sustena.db` from tests — conftest.py uses `:memory:`.
- Do not start Epics 1.12 (Monte Carlo), 1.13 (Mkulima), or 1.14 (Daraja) yet.
  Sequence is: Sprint 1 wiring → Sprint 2 UIParser → Sprint 3 operators+protocols.
