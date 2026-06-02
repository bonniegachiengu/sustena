# Sustena XII — Claude Code Context

> Read this before touching any code. It tells you where we are, how things are built,
> and how Bonnie works. Everything here is current as of 2 June 2026.

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
Current roadmap: `docs/Sustena_XII_Roadmap_Jun2026.md` ← read this for sprint detail

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
  core/           7 primitives + UIParser:
                    state.py, constraints.py, events.py, pawa.py,
                    operator.py, sustain_engine.py, council.py,
                    uiparser.py, widget_registry.py          ← added Sprint 2
  operators/      budget.py, chama.py, procurement.py, vyyb.py,
                  biashara.py, calendar.py,
                  ui_render.py                              ← added Sprint 2
  operatives/     base.py, mentor.py, protege.py, attache.py, navigator.py,
                  curator.py, chama_secretary.py
  sustains/       homestead.json, vyyb.json, chama.json, biashara.json, colosso.json
  api/
    main.py       FastAPI app, lifespan (init_db + Claude client mode log), CORS
    routes/       sustains.py, devui.py, orchie.py, council.py, whatsapp.py, ...
  db/schema.py    SQLAlchemy tables + async engine singleton (_engine)
  config.py       Pydantic BaseSettings (extra="ignore") — reads .env
```

### Running
```bash
# From apps/api/
pip install -r requirements.txt
uvicorn sustena.api.main:app --reload --port 8000
```

### Testing
```bash
# From apps/api/
python -m pytest tests/ -q --tb=short     # 1162 tests, all pass

# Run specific file
python -m pytest tests/test_uiparser.py -v
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

**Current state:** Sprint 1 + Sprint 2 complete. All panels wired to real API.
Controller panel has CONSOLE / UI PREVIEW tab toggle.

---

## Sprint state

### Sprint 0 ✅ — Pre-flight
865 tests pass. DB schema live. CI green.

### Sprint 1 ✅ — "Sustena Runs"
All 5 tasks done and committed:
- [x] 1.1 Monitor panel → `GET /devui/state` + `WS /devui/state-stream`
- [x] 1.2 Sustain selector → `GET /devui/sustains`
- [x] 1.3 Operator Console → `POST /devui/console/execute` (sustain_id dynamic, response parsing fixed)
- [x] 1.4 Orchie chat → `POST /orchie/message` (uses `api.post`, not hardcoded fetch)
- [x] 1.5 Startup log: `"Claude client: mock mode (zero API calls)"` in lifespan

### Sprint 2 ✅ — UIParser
All 5 tasks done and committed:
- [x] 2.1+2.2 `UISchema` dataclasses + `UISchemaParser` (`uiparser.py`)
  - `parse(ui_schema_dict) → UISchema`
  - `resolve_source(expr, inputs, state)` — handles `inputs.*`, `state.*`, `state.a - state.b`
  - No dynamic code execution anywhere
- [x] 2.3 `ui.render.*` operators (`ui_render.py`) — operator_card, operative_dashboard, sustain_home, preview
  - All pawa_cost=0, no side_effects, registered in OPERATOR_REGISTRY
- [x] 2.4 `WidgetTypeRegistry` (`widget_registry.py`) — 9 built-in types, Jinja2 + generic fallback
  - `GET /devui/widgets` — list all widget types
  - `POST /devui/preview-widget` — parse spec + mock state → ResponseWidget
- [x] 2.5 UI Preview tab in Controller panel (`other.jsx`)
  - Tab toggle: CONSOLE / UI PREVIEW
  - Left: editable spec JSON + mock state; Right: live rendered widget (500ms debounce)

### Sprint 3 ✅ — Operators + Protocols
All 7 tasks done and committed:
- [x] 3.1 `api.*` operators — `api.get`, `api.post`, `api.webhook_listen`
- [x] 3.2 `monitor.*` operators — `monitor.state_path`, `monitor.constraint`
- [x] 3.3 `visualize.*` operators — `visualize.pocket_ring`, `visualize.event_feed`, `visualize.constraint_health`
- [x] 3.4 `simulate.*` operators — `simulate.fork`, `simulate.run_path`, `simulate.score`
- [x] 3.5 `edit.*` operators — `edit.state_patch`, `edit.operator_spec`
- [x] 3.6 `control.*` operators — `control.execute_approved`, `control.rollback`
- [x] 3.7 `protocol` field required on `OperatorMeta` (3rd arg in `@sustena_operator`); all 44 existing operators back-filled with `protocol="rpc"`

### Sprint 4 ✅ — Operator UIs
All 4 tasks done and committed (1162 tests):
- [x] 4.1 Protocol-aware Operator Console — `GET /devui/registry/operators` includes `protocol`; terminal shows colour-coded badge per execution; streaming panel + event-driven/polling indicators
- [x] 4.2 Monitor Panel widget grid — `GET /devui/monitor-widgets` calls the three `visualize.*` operators; `PocketRingWidget`, `EventFeedWidget`, `ConstraintHealthWidget` rendered live
- [x] 4.3 Simulate Panel — `POST /devui/simulate-pipeline` chains `simulate.fork → run_path → score`; editable proposal textarea, goal-metric selector, per-step accordion
- [x] 4.4 Control Panel — `ProposalCard` calls `control.execute_approved`; `RollbackPanel` calls `control.rollback`; both confirm before executing

### Sprint 5 — NEXT: Operative Networks (LLM-Optional Base Layer)
See `docs/Sustena_XII_Roadmap_Jun2026.md` for full task list.
Tasks 5.1–5.7 cover: `OperativeGraph` base layer, refactor `BaseOperative` to use graphs,
rewrite Mentor as graph-of-operators, protocol-aware operative runtime, JSON graph specs,
graph designs for all Council operatives, and `operative.spawn` with `{{placeholder}}` calibration.

---

## UIParser — key facts (Sprint 2)

### `ui_schema` block format (already on all existing operators)
```python
ui_schema={
    "widget_type": "budget_allocation_card",
    "fields": [
        {"label": "Pocket",  "source": "inputs.pocket_name",           "display": "text"},
        {"label": "Amount",  "source": "inputs.amount",                "display": "currency"},
        {"label": "Balance", "source": "state.finances.liquid.balance", "display": "currency",
         "colour_rule": "amber_if_below_20pct"},
    ],
    "ctas": ["View Budget"],
    "chart": "donut",   # optional
}
```

### `resolve_source` rules (Phase 1, no eval)
- `"inputs.<key>"` → `operator_inputs[key]`
- `"state.<dot.path>"` → `state.get(path)` (StateAccessor) or plain dict walk
- `"state.<a> - state.<b>"` → numeric subtraction of two state paths

### New devui endpoints (Sprints 2 + 4)
- `GET  /devui/widgets` — list all registered widget types
- `POST /devui/preview-widget` — body: `{spec_json, mock_state}` → `{widget: ResponseWidget}`
- `GET  /devui/monitor-widgets?sustain_id=` — calls `visualize.*` operators, returns `{pocket_ring, event_feed, constraint_health}` widgets
- `POST /devui/simulate-pipeline` — body: `{sustain_id, proposal, goal_metric}` → `{fork_id, steps, score, interpretation}`
- `GET  /devui/registry/operators` — now includes `protocol` field on every operator entry

---

## Bonnie's patterns — follow these exactly

### Commits
Conventional commits, **direct to main** (no feature branches, no PRs).
**Commit each sprint task before starting the next one.**
**Push to GitHub (`git push origin main`) immediately after every commit — no exceptions.**
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
- `test_operator_registry.py` has `ALL_KNOWN_OPERATORS` — add new operators there when registering.

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
Not implemented yet — add to `OperatorMeta` in Sprint 3.7.

### UIParser (Sprint 2 — done)
`uiparser.py` owns `UISchema`, `UISchemaField`, `ResponseWidget`, `UISchemaParser`.
`widget_registry.py` owns `WidgetTypeRegistry` (singleton: `widget_registry`).
`ui_render.py` owns the four `ui.render.*` operators.
All existing operators already have `ui_schema` blocks — the spec was established
before UIParser was built.

---

## Key commands cheatsheet

```bash
# Backend
cd apps/api
python -m pytest tests/ -q              # run all tests (929)
uvicorn sustena.api.main:app --reload   # start server

# Frontend
cd apps/web
npm run dev                             # start dev server
npm run build                           # production build

# Useful API calls (dev)
curl -H "Authorization: Bearer dev-admin-token" http://localhost:8000/devui/sustains
curl -H "Authorization: Bearer dev-admin-token" http://localhost:8000/devui/widgets
curl -H "Authorization: Bearer dev-admin-token" \
  "http://localhost:8000/devui/state?sustain_id=<id>"
curl -X POST http://localhost:8000/orchie/message \
  -H "Content-Type: application/json" \
  -d '{"sustain_id":"<id>","message":"habari"}'
curl -X POST http://localhost:8000/devui/preview-widget \
  -H "Authorization: Bearer dev-admin-token" \
  -H "Content-Type: application/json" \
  -d '{"spec_json":{"widget_type":"budget_allocation_card","fields":[{"label":"Pocket","source":"inputs.pocket_name"}],"ctas":[]},"mock_state":{"inputs":{"pocket_name":"food"}}}'
```

---

## What NOT to do

- Do not add async/await to `SustainEngine` methods — it uses synchronous `sqlite3`,
  not aiosqlite. The async layer is only in the FastAPI routes and `init_db()`.
- Do not add `rootdir` to `pyproject.toml [tool.pytest.ini_options]` — it's not a valid key.
- Do not write to `sustena.db` from tests — conftest.py uses `:memory:`.
- Do not start Epics 1.12 (Monte Carlo), 1.13 (Mkulima), or 1.14 (Daraja) yet.
  Sequence is: Sprint 3 operators+protocols → Sprint 4 operator UIs → Sprint 5 operative graphs.
- Do not add a new operator without adding it to `ALL_KNOWN_OPERATORS` in `test_operator_registry.py`.
