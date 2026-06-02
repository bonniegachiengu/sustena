# Sustena XII — Claude Code Context

> Read this before touching any code. It tells you where we are, how things are built,
> and how Bonnie works. Everything here is current as of 2 June 2026 (updated Sprint 8.3 + CI schema fix).

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
  core/           7 primitives + UIParser + operative graph layer:
                    state.py, constraints.py, events.py, pawa.py,
                    operator.py, sustain_engine.py, council.py,
                    uiparser.py, widget_registry.py,         ← added Sprint 2
                    operative_graph.py,                      ← added Sprint 5
                    operative_runtime.py                     ← added Sprint 5
  operators/      budget.py, chama.py, procurement.py, vyyb.py,
                  biashara.py, calendar.py,
                  ui_render.py,                             ← added Sprint 2
                  api_ops.py, monitor.py, visualize.py,     ← added Sprint 3
                  simulate_ops.py, edit_ops.py, control_ops.py,
                  mentor_ops.py,                            ← added Sprint 5
                  operative_ops.py                          ← added Sprint 5
  operatives/     base.py, mentor.py, protege.py, attache.py, navigator.py,
                  curator.py, chama_secretary.py
  operatives/graphs/   JSON graph specs for all Council operatives ← Sprint 5
                    mentor_evaluation.json, mentor_deliberation.json,
                    protege_*.json, attache_*.json, curator_*.json,
                    navigator_*.json, orchie_evaluation.json
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
python -m pytest tests/ -q --tb=short     # 1245 tests, all pass

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

**Current state:** Sprints 1–4 complete. All panels wired to real API.
Controller panel has CONSOLE / UI PREVIEW tab toggle. Monitor/Simulate/Control panels
all use real Sprint 3 operators.

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

### Sprint 5 ✅ — Operative Networks (LLM-Optional Base Layer)
All 7 tasks done and committed (1245 tests — 1269 after Sprint 6.1):
- [x] 5.1 `OperativeGraph` base layer — `CalibrationError`, `Condition`, `OperativeNode`, `OperativeEdge`, `OperativeGraph` in `core/operative_graph.py`. `from_spec()` with `{{placeholder}}` resolution and inline defaults. `$dot.path` dynamic kwargs for runtime value injection.
- [x] 5.2 `BaseOperative` uses `OperativeGraph` — optional `evaluation_graph`/`deliberation_graph` attributes. Default `evaluate()`/`deliberate()` dispatch to graphs. `_build_operator_context()` helper added.
- [x] 5.3 Mentor rewritten as graph-of-operators — `mentor.evaluate_budget` + `mentor.deliberate_budget` operators replace all `_call_claude()` calls. Zero Anthropic API calls.
- [x] 5.4 Protocol-aware operative runtime — `OperativeRuntime` in `core/operative_runtime.py`. `event_driven` → EventBus subscriber; `polling` → `start_polling()` loop; `rpc` → `run_once()`.
- [x] 5.5 Graph spec in JSON — `OperativeGraph.from_spec_file(path)`. Mentor graph files in `operatives/graphs/`. `homestead.json` operatives upgraded to dict with `class` + graph path keys. `GRAPHS_DIR` constant.
- [x] 5.6 Graph specs for all Council operatives — 11 JSON files in `operatives/graphs/` covering Mentor, Protégé, Attaché, Curator, Navigator, Orchie (evaluation + deliberation each).
- [x] 5.7 `operative.spawn` operator — loads template, resolves `{{placeholder}}` from `calibration_data` + live state, returns `instantiated_spec`. `CalibrationError` propagates as `OperatorResult.fail`.

### Sprint 7 🔄 — Council Enrichment
- [x] 7.1 `CouncillorConfig` dataclass + `load_councillor_configs()` in `core/council.py`. `homestead.json` operatives extended with `domain` + `sub_operatives` (2 per councillor). 10 sub-operative graph JSON stubs created. 39 new tests. (1336 total)
- [x] 7.2 Domain relevance check — `CouncillorConfig.is_relevant()`, `OPERATOR_DOMAIN_MAP`, `get_proposal_domains()`. `create_proposal()` auto-tags with domains. `collect_votes()` accepts optional `councillor_configs` — irrelevant councillors get immediate ABSTAIN with `abstain_reason: "no_domain_overlap"`, `deliberate()` never called. 34 new tests. (1370 total)
- [x] 7.3 Sandbox simulation per councillor — `CouncilSession._create_sandbox()` calls `simulate.fork()` once per relevant councillor (unique fork_id per councillor, none shared). Sub-operative graphs run against the fork's isolated state. `get_fork_state()` added to `simulate_ops.py`. `fork_id` + `sandbox_results` recorded on vote records and passed to `deliberate()`. Graceful degradation on fork failure. 19 new tests. (1389 total)
- [x] 7.4 `DelegatedVote` dataclass — sub-operative vote aggregation at councillor level. Each sub-operative result in `sandbox_results` is parsed into a `DelegatedVote(position: YES|NO|ABSTAIN, confidence: float, reasoning: str)`. Councillor aggregates them (weighted by confidence) to form its final `OperativeVote`. The aggregation logic lives in `core/council.py` (`aggregate_delegated_votes()`). 31 new tests. (1420 total)
- [x] 7.5 Enriched `CouncilSession.collect_votes()` — wire the full flow end-to-end: relevance check → sandbox fork → sub-operative execution → `DelegatedVote` aggregation → councillor's final `OperativeVote`. The existing `deliberate()` interface on `BaseOperative` is still honoured as fallback when no sub-operatives produce votes. `delegated_votes` recorded on vote records. 8 new tests, 2 updated. (1428 total)

### Sprint 7.6 — Council Panel UI (next after Sprint 7)
Part of Sprint 11 below. ProposalCard in `other.jsx` needs per-councillor vote bars + expandable delegated_votes. Data available in `council_votes` (fork_id, sandbox_results, delegated_votes added 7.3–7.5).

---

### Sprints 8–11 🗓️ — Full Product Wiring (approved plan, 2 June 2026)

**Goal:** Make every panel and page fully usable — real data, no stubs, scrollable.

**Dependency order:**
```
Sprint 8.1  GitHub cleanup + scroll        — no deps
Sprint 8.2  Engine singleton               — UNBLOCKS all panels
Sprint 8.3  Monitor real widgets           ← 8.2
Sprint 8.4  Simulator DAG ✅              ← 8.2
Sprint 8.5  Editor real graph             ← 8.2  ⚠️ SKIPPED — do next
Sprint 8.6  Auth/Users (register/login) ✅ ← 8.2; UNBLOCKS 8.7–8.10
Sprint 8.7  Lore CMS + Journal API ✅     ← 8.6
Sprint 8.8  Library + Spore flow          ← 8.2 + 8.6  ⚠️ do next
Sprint 8.9  Arena products + publishing   ← 8.6 + 8.8
Sprint 8.10 Profile telemetry + Ctrl      ← 8.6 + 8.2
Sprint 8.11 Council panel polish + WS     ← 8.3 + 8.10
```

**Key new DB tables (to add to `schema.py`):**
- `lore_entries` — public blog (draft/published, body_json blocks)
- `journal_entries` — private user journal (DECISION/NOTE/REFLECTION/UPDATE, linked to sustain_id + operators_log.id)
- `arena_packages` — operative/operator/spore/widget packages (kind, spec_json, trust_score)
- `orders` — arena product orders

**Journal vs Lore — they are different APIs:**
- Journal (`/api/v1/journal/`): private, always authenticated, linked to sustains, types = DECISION/NOTE/REFLECTION/UPDATE, never published publicly
- Lore (`/api/v1/lore/`): public-read, authenticated write, editorial content, types = VISION/TECHNICAL/REFLECTION/UPDATE, draft → published lifecycle

**Sprint 8.1 status (2 Jun 2026):**
- ✅ Vertical scroll fixed: removed `overflow:hidden` + `height:100vh` from `globals.css` (`html/body/#root`); `ProfilePage`, `ArenaPage`, `LoreLayout` changed to `minHeight:100vh`.
- ✅ ControllerPanel: switched from `gridTemplateRows: 'auto auto 1fr'` to flex-column + `overflowY:auto`; Terminal/IoT/Rollback section given `minHeight:440` so it's never squished.
- ✅ MonitorPanel: added `overflowY:auto`; moved State Stream + Event Log to the bottom (after operative cards + widget grid); replaced `flex:1,minHeight:0` with `minHeight:380`.
- ✅ GitHub folder cleanup: `brand/`, `vyyb/`, `homestead/`, `biashara/`, `mkulima/`, `colosso/`, `scripts/`, `lore/`, `archive/`, `Briefs/` removed from git tracking. `.gitignore` updated.

**Sprint 8.2 status (2 Jun 2026):**
- ✅ `engine_singleton.py` — `get_shared_engine()` backed by `sustena.db`; `reset_shared_engine()` for tests.
- ✅ `sustain_engine.py` — fixed truncated `_get_spec`; added `get_spec()`, `get_operative_statuses()`, `evaluate_constraints()`.
- ✅ `devui.py` — replaced all 8 `SustainEngine()` calls with `get_shared_engine()`; wired `get_state()` to real engine; wired proposals to `council_proposals` table; wired operative registry to `_OPERATIVE_MAP`.
- ✅ `main.py` lifespan — seeds one homestead sustain on startup if DB is empty.
- ✅ `conftest.py` — resets engine singleton between test modules.
- ✅ 18 new tests in `test_engine_singleton.py`. 1446 tests pass.

**Sprint 8.3 status (2 Jun 2026):**
- ✅ Removed `_MONITOR_STUB_STATE` from `devui.py` — `get_monitor_widgets` and `simulate_pipeline` use `{}` when engine has no state.
- ✅ `get_monitor_widgets` passes real constraint expressions (from `evaluate_constraints`) to `visualize.constraint_health`.
- ✅ `GET /devui/sustain/{id}/graph` — Orchie + council operative nodes and delegation edges for Orchie panel graph tree.
- ✅ `MonitorPanel` (`monitor.jsx`) — empty-state "no operatives reporting · all thresholds nominal" when operatives list is empty.
- ✅ 9 new tests in `test_devui_routes.py`. 1455 tests pass.

**Sprint 8.4 status (2 Jun 2026):**
- ✅ `GET /devui/sustain/{id}/operators` — returns spec-allowed operators with name, description, params, pawa_cost, and protocol from OPERATOR_REGISTRY.
- ✅ `SimulatorPanel` proposal textarea starts empty (no hardcoded `budget.allocate` stub step).
- ✅ ADD STEP section — clickable operator chips fetched from the new endpoint, wired to the live `sustain_id` from the TopBar selector.
- ✅ `ProposalDag` component — sequential DAG view using `DagNode` + `DagEdges`; shows steps with completed highlighting after pipeline run.
- ✅ `sustain_id` wired throughout `StateDiff` via the `sustain` prop (no more hardcoded fallback in API call).
- ✅ 9 new tests in `TestSustainOperators`. 1464 tests pass.

**CI schema fix (2 Jun 2026, post-8.3):**
- ✅ `schema.py` `sustains` table was missing `template_id` column used by `SustainEngine`. In CI (file-based `test.db`), SQLAlchemy's `init_db()` runs first and creates the table without that column; `SustainEngine._ensure_tables()` then skips `CREATE TABLE IF NOT EXISTS`, leaving `template_id` absent. Fix: added `template_id` (nullable) and made seed-only columns (`sustain_type`, `name`, `template_version`, `is_active`) nullable so both writers can INSERT their respective column subsets without conflict. 1455 tests pass locally and in CI. Root cause: `conftest.py` uses `os.environ.setdefault(...)` which is a no-op when CI already has `DATABASE_URL` set.

**Sprint 8.6 status (2 Jun 2026):**
- ✅ `schema.py` `users` table — added `email` (unique, nullable) + `password_hash`; made `phone_number` nullable; `_migrate_users_auth` sync migration recreates the table on old DBs using rename→recreate→copy→drop (SQLite cannot ALTER COLUMN).
- ✅ `users.py` full rewrite — PBKDF2-SHA256 password hashing via stdlib `hashlib` (avoids passlib/bcrypt 5.x incompatibility); HS256 JWT with 30-day expiry via `python-jose`; `register`, `login`, `/me`, `/me/stats`, `/me/activity`.
- ✅ 33 new tests in `test_users.py` using `AsyncClient` + explicit `init_db()` per function. 1497 tests total.
- ✅ `test_api.py` stub test updated to reflect real register endpoint.
- ✅ CI fix: `test_users.py` `client` fixture creates its own `sqlite+aiosqlite:///:memory:` engine and injects it into `_schema._engine` directly, bypassing `DATABASE_URL`. CI sets `DATABASE_URL=sqlite+aiosqlite:///./test.db`; `conftest.py` `setdefault` is a no-op there, so tests shared a file-based DB and accumulated duplicate users across functions. Fix ensures each test gets a clean isolated DB.

**Sprint 8.7 status (2 Jun 2026):**
- ✅ `schema.py` — added `lore_entries` (public editorial, draft/published, VISION/TECHNICAL/REFLECTION/UPDATE) and `journal_entries` (private user notes, DECISION/NOTE/REFLECTION/UPDATE, links to sustain_id + operator_log_id).
- ✅ `routes/lore.py` — `GET /api/v1/lore/entries` (public), `POST /entries` (auth), `PUT /entries/{id}` (auth, own only), `POST /entries/{id}/publish` (auth, own, draft→published, 409 on double-publish).
- ✅ `routes/journal.py` — `GET/POST/PUT/DELETE /api/v1/journal/entries` (all auth-required, user-scoped, 403 on cross-user access).
- ✅ `main.py` — both routers mounted. 57 new tests (test_lore.py + test_journal.py). **1554 tests total.**

**Sprint 8.7 UI wiring (2 Jun 2026, same session as 8.7):**
- ✅ `LorePage.jsx` — `GET /api/v1/lore/entries` on mount; `SignInPanel` (JWT → localStorage `sustena_token`); `ComposePanel` (POST + publish in one flow); `textToBlocks` converts textarea to `[{type,text}]` blocks; `+ WRITE` / `sign in to write` footer; SIGN OUT.
- ✅ `JournalPage.jsx` — `GET /api/v1/journal/entries` on mount (auth required); `ComposePanel` wired to `POST /api/v1/journal/entries`; shared JWT pattern; entry list refreshes after save; proper empty states.
- ✅ `ProfilePage.jsx` — fetches `/me` + `/me/stats` + `/me/activity`; stat pills and contribution timeline from live API; `SignInPanel` + SIGN OUT in settings; "sign in to view your profile" when unauthenticated.
- ✅ `ArenaPage.jsx` — converted 7 empty module-level constants to `useState`; `useEffect` scaffolds for `GET /api/v1/arena/packages`, `/products`, `/orders` (silent-fail until Sprint 8.9); TABS counts and filtered pool use live state; "Waiting for API" strings replaced with Sustena empty states.
- ✅ `data.jsx` — "Waiting for API" in `OrchieActivityViz` CURRENT TASKS → "no active tasks · sustain is clear".
- ✅ `monitor.jsx` — "use MOCK data" comment cleaned up.
- ✅ `CLAUDE.md` — added "UI + backend must always be fully wired" permanent principle.

**Controller panel UI fixes (2 Jun 2026, post-8.2):**
- ✅ Universal Controls instrument frames were clipped at the bottom — added `flexShrink:0` to the Card and removed `minHeight:0` from `Instrument` (`other.jsx`).
- ✅ [E-HLD] EXEC MODE column too narrow (130px) — LIVE button overflowed the instrument frame; widened to 165px.

**New routes added in Sprints 8–11:**
- `POST /api/v1/users/register`, `POST /api/v1/users/login`, `GET /api/v1/users/me`, `GET /api/v1/users/me/stats`, `GET /api/v1/users/me/activity`
- `GET|POST|PUT /api/v1/lore/entries`, `POST /api/v1/lore/entries/{id}/publish`
- `GET|POST|PUT|DELETE /api/v1/journal/entries`
- `GET|POST /api/v1/arena/packages`, `GET /api/v1/arena/products`, `POST /api/v1/arena/orders`
- `GET /devui/library`, `GET /devui/sustain/{id}/graph`, `GET /devui/sustain/{id}/operators`
- `GET|POST /api/v1/council/{sustain_id}/proposals`

---

### Sprint 6 ✅ — Today List + Morning Brief
All 4 tasks done and committed (1297 tests):
Tasks 6.1–6.4:
- [x] 6.1 `homestead.tasks.*` operators — add, complete, list, carryover. State at `tasks.items`. homestead.json updated.
- [x] 6.2 `orchie.morning_brief` operator — aggregates today's tasks, calendar events, passed proposals, liquid balance into `morning_brief_card` ResponseWidget + `brief_text`. `morning_brief_card` registered in WidgetTypeRegistry. 22 new tests.
- [x] 6.3 Orchie session init — `useStreamedConversation` in `chat.jsx` fires `orchie.morning_brief` via `/devui/console/execute` on mount (when script is empty). Renders `brief_text` as first Orchie message; falls back to empty-state line on error or null.
- [x] 6.4 Orchie Panel — full-page view at `/orchie-panel?sustain=`. Layout:
      - Top-left **TODAY** card (replaces "MOST ASKED"): renders `morning_brief_card` — tasks due today, calendar events, approved strategies.
      - Bottom-left **OPEN EDITS**: recent operator executions awaiting council review.
      - Middle **PINNED MONITORING**: the three `visualize.*` widgets (pocket ring, constraint health, event feed).
      - Middle-bottom **RECENT ORCHIE REPLIES**: last 3 Orchie chat messages.
      - Right sidebar: TASK/SUBTASK active graph tree, DELEGATION/COUNCIL network (Orchie at centre), CURRENT TASKS checklist, DELIBERATION council vote bars (Mentor/Curator/Navigator/Protégé), LISTEN + Ask Orchie input.

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

## Operative graph — key facts (Sprint 5)

### New operators added in Sprint 5
```
mentor.evaluate_budget   — scans pockets, returns proposal dict; no LLM
mentor.deliberate_budget — rule-based budget vote (YES/NO/ABSTAIN); no LLM
operative.spawn          — loads graph template, resolves {{placeholders}}, returns instantiated_spec
```

### OperativeGraph spec format
```python
{
  "entry": "node_id",
  "exit":  "node_id",
  "nodes": {
    "node_id": {
      "operator": "operator.name",
      "kwargs": {
        "static_param": "value",
        "calibrated_param": "{{user.income}}",        # resolved at from_spec() time
        "dynamic_param": "$prior_node.field"          # resolved at run() time
      }
    }
  },
  "edges": [
    {"from": "a", "to": "b"},
    {"from": "b", "to": "c", "condition": {"field": "score", "op": ">", "value": 0.7}}
  ]
}
```

### Graph file locations
```
sustena/operatives/graphs/
  mentor_evaluation.json       deliberation.json
  protege_*.json               attache_*.json
  curator_*.json               navigator_*.json
  orchie_evaluation.json       (Orchie does not deliberate)
```

### operative.spawn flow
```
operative.spawn(template_id, calibration_data)
  → load graph from operatives/graphs/{template_id}.json
  → merge ctx.state snapshot into calibration_data under "state" key
  → OperativeGraph.from_spec(spec, calibration_data) — resolves {{tokens}}
  → return OperatorResult.ok({"instantiated_spec": resolved_spec, ...})
  → caller does: OperativeGraph.from_spec(result["instantiated_spec"]).run(ctx, trigger)
```

### New devui endpoints (Sprint 5 — none; purely backend/operator layer)

---

## Bonnie's patterns — follow these exactly

### Sprint subtask cadence — MANDATORY gate between every task
After completing each numbered sprint subtask (e.g. 6.1, 6.2, …):
1. `python -m pytest tests/ -q --tb=short` — all pass, zero failures.
2. `git commit` with conventional commit message.
3. `git push origin main`.
4. Update CLAUDE.md: mark the task `[x]` in the sprint state, update the test count.
5. **STOP. Tell Bonnie what was built and what comes next. Wait for explicit go-ahead.**

Do not begin the next subtask until Bonnie replies. One subtask per conversation turn.

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

### UI + backend must always be fully wired — PERMANENT rule
**The goal at every stage is zero hanging surfaces.** When a backend endpoint is built,
the UI that uses it must be wired in the same sprint or immediately after. When a UI
task is started, every data source it touches must either call a real API or show a
designed empty state — never a stub, "Waiting for API" string, or hardcoded constant.

If Bonnie asks about any UI surface: assume she also wants every related surface wired
in the same session. Do not stop at the one thing asked. Check siblings and complete the
job. If in doubt about scope, ask — but always default to wiring everything that can be
wired with the APIs that exist, not just the one explicitly mentioned.

**"Waiting for API" is never acceptable in committed code.** If the API doesn't exist yet,
the section must show a designed empty state and have a `useEffect` scaffold ready for
when the API lands.

### Frontend data honesty — PERMANENT rule for all UI work
Every UI component renders real data from the API or shows a designed empty state.
**Never invent, hardcode, or leave placeholder mock data in any component.**

When building or touching any UI panel, widget, or section:
1. Wire it to the real API call. If the endpoint doesn't exist yet, leave a `useEffect`
   scaffold and show the designed empty state — do not fill with made-up data.
2. If the API returns an empty array, null, or zero — render an **empty state** in
   Sustena's design language (see below). Do not hide the section.
3. If you encounter existing hardcoded/mock data anywhere in the frontend while working
   on a task, remove it in the same commit and replace with the empty-state pattern.

**Sustena empty-state design language**
The UI is a terminal-adjacent monitoring surface — not a consumer app. Empty states
should feel like a system at rest, not a friendly onboarding screen.

Use short, lowercase, operative-voiced lines. Examples by context:
```
Tasks          — "no active tasks · sustain is clear"
Council        — "council is quiet · no proposals in motion"
Operatives     — "no operatives reporting · all thresholds nominal"
Deliberation   — "no vote in progress"
Events         — "no events recorded yet"
Morning brief  — "nothing scheduled · sustain state nominal"
Monitoring     — "no constraints breached · all within bounds"
Proposals      — "no edits pending review"
Recent replies — "orchie hasn't spoken yet"
```
Style: dim text (text-gray-500 or equivalent), no icons unless a single subtle
dot/ring indicator fits. No illustrations. No "Get started" CTAs. One line maximum.
The component frame should still be visible so the layout doesn't collapse.

### No WhatsApp work
WhatsApp stubs exist in the codebase and stay. No new WhatsApp development until
explicitly reactivated. Do not touch `whatsapp_*.py` files or the `/webhook` routes.

### LLM in development
`ANTHROPIC_API_KEY=mock` is always set in `.env`. Never make real Anthropic API calls
during development or testing. The MockClaudeClient in `core/claude_client.py` handles
all operative calls locally.

---

## Architecture decisions

### Operative graphs (Sprint 5 — done)
Operatives are directed graphs of operator calls — no LLM required at the base layer.
`OperativeGraph` (core/operative_graph.py) runs the graph. `OperativeRuntime`
(core/operative_runtime.py) dispatches by protocol (event_driven/polling/rpc).
`operative.spawn(template_id, calibration_data)` calibrates a library template with
user context, resolving `{{placeholder}}` tokens. `CalibrationError` is raised on
unresolved required placeholders. Graph JSON files live in `operatives/graphs/`.
Circular import between `operative_graph.py` and `operatives/base.py` is broken with
a `TYPE_CHECKING` guard and a lazy import inside `_build_proposal`.

### Operative graph kwargs — two resolution passes
1. **`from_spec()` time**: `{{placeholder}}` tokens are resolved from `calibration_data`.
2. **`run()` time**: `$dot.path` string kwargs are resolved from `accumulated` node results
   (e.g. `"$trigger_event"` injects the trigger event dict; `"$fork.fork_id"` injects
   the fork ID from a prior node).
Never use `eval()` for either resolution.

### devui endpoints
`/devui/*` endpoints exist and have stub fallbacks. They try to call `SustainEngine`
methods first, fall back to hardcoded stub data if the engine isn't initialised.
After seeding, the real data flows. All stubs have `# TODO: wire real` comments.

### Protocol types (Sprint 3 — done)
Every operator declares `protocol`: `rpc | event_driven | polling | streaming` on
`OperatorMeta`. `OperativeRuntime` uses the entry node's protocol to decide dispatch.

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
python -m pytest tests/ -q              # run all tests (1245)
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
  Sequence is: Sprint 5 operative graphs → Sprint 6 today list → Sprint 7 council enrichment.
- Do not add a new operator without adding it to `ALL_KNOWN_OPERATORS` in `test_operator_registry.py`.
- Do not import `OperativeProposal` at the top level of `operative_graph.py` — it creates
  a circular import. Use the lazy import inside `_build_proposal()` that is already there.
- Do not use `eval()` anywhere in UIParser, OperativeGraph, or placeholder resolution.
  Phase 1 is strictly pattern-matching and dict-walking only.
