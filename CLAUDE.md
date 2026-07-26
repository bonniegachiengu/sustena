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
  operators/      budget.py, chama.py, procurement.py, calendar.py,
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
  sustains/       homestead.json, habitat.json, chama.json     ← pruned to homestead-only + habitat, see below
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
uvicorn sustena.api.main:app --reload --port 9000
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
VITE_API_BASE_URL=http://localhost:9000
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
Sprint 8.5  Editor real graph ✅          ← 8.2
Sprint 8.6  Auth/Users (register/login) ✅ ← 8.2; UNBLOCKS 8.7–8.10
Sprint 8.7  Lore CMS + Journal API ✅     ← 8.6
Sprint 8.8  Library + Spore flow ✅       ← 8.2 + 8.6
Sprint 8.9  Arena products + publishing ✅ ← 8.6 + 8.8
Sprint 8.10 Profile telemetry + Ctrl ✅   ← 8.6 + 8.2
Sprint 8.11 Council panel polish + WS ✅  ← 8.3 + 8.10
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

**Sustains schema migration fix (2 Jun 2026, post-8.11):**
- ✅ Root cause of "empty platform / can't run a sustain": DBs created before `template_id` existed have a `sustains` table without it. `init_db()` uses `CREATE TABLE IF NOT EXISTS`, so `metadata.create_all` never adds the column; `SustainEngine.list_all()` / `instantiate()` fail with `no such column: s.template_id`, the startup seed is swallowed as non-fatal, and `/devui/sustains` returns `[]`. The earlier CI fix only corrected the *schema definition* — existing DBs were never migrated.
- ✅ Added `_migrate_sustains_schema(sync_conn)` in `schema.py` (mirrors `_migrate_users_auth`): rename → recreate with `template_id` + nullable seed columns → copy → drop. Detection uses `PRAGMA table_info` — NOT a failing `SELECT`, which would invalidate the shared transaction and silently abort the recreate. Called in `init_db()` after `_migrate_users_auth`.
- ✅ 5 new tests in `test_sustains_migration.py` (adds column, preserves rows, NULL template_id after copy, idempotent, no-op when present). Expected total **1590 tests** — confirm with a local run.
- ⚠️ The on-disk `apps/api/sustena.db` was found corrupted ("database disk image is malformed") — classic OneDrive-syncing-a-live-SQLite hazard. It held only 1 user + 0 sustains. Fix: delete it and restart the backend; `init_db()` recreates a clean DB with the correct schema. Recommend moving the dev DB outside OneDrive.

**Create-sustain via UI (2 Jun 2026, post-8.11):**
- ✅ `devui.py` — `GET /devui/templates` (lists homestead/vyyb/chama/biashara/colosso with display_name, description, operatives, parameters; handles `operatives` as **dict OR list** — only homestead.json uses a dict) and `POST /devui/sustains` → `SustainEngine.instantiate(template_id, user_id, params)`; `owner_ids` defaults to `[user_id]`; unknown template → 422. Returns `{sustain_id, sustain}`.
- ✅ `shell.jsx` TopBar selector — a `CREATE SUSTAIN` section in the dropdown: loads `/devui/templates` on open; clicking a template POSTs `/devui/sustains`, refreshes the list and auto-selects the new sustain; empty-state "no sustains yet · create one below". `createSustain()`/`refreshSustains()` helpers in `App()`.
- ✅ 10 new backend tests (`TestListTemplates`, `TestCreateSustain`). **1600 tests.**

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

**Sprint 8.5 status (2 Jun 2026):**
- ✅ `EditorPanel` — GRAPH/STATE/OP SPEC tab toggle; STATE tab fetches `GET /devui/state`, displays live JSON, applies patches via `edit.state_patch`; OP SPEC tab calls `edit.operator_spec` with field selector and result diff.
- ✅ Node palette OPERATORS section wired to `GET /devui/sustain/{id}/operators`; clicking an operator opens OP SPEC mode pre-filled.
- ✅ 1554 tests pass (no new backend tests — purely frontend).

**Sprint 8.9 status (2 Jun 2026):**
- ✅ `schema.py` — added `arena_products` (name, seller_type, price, unit, emoji, tags) and `arena_orders` (ref, items_json, product_total, licenses_json) tables.
- ✅ `routes/arena.py` — `seed_demo_products()` inserts 8 Kenyan products (4 Vyyb food + 4 Mkulima farm) at startup if table empty; real `GET /api/v1/arena/products`; real `POST /api/v1/arena/orders` (returns ref + per-package licenses); real `GET /api/v1/arena/orders` (user-scoped, formatted for OrderCard).
- ✅ `main.py` — `await seed_demo_products()` in lifespan.
- ✅ `CartDrawer` in `ArenaPage.jsx` — async `handleConfirm` POSTs to real orders endpoint when token present; shows loading state; falls back to local-only when no token.
- ✅ `LibraryPanel` in `other.jsx` — PUBLISH button opens inline form (name, kind, description, tags); submits to `POST /api/v1/arena/packages`; refreshes library on success.
- ✅ 6 new tests. **1574 tests total.**

**Sprint 8.8 status (2 Jun 2026):**
- ✅ `schema.py` — added `arena_packages` table (kind, trust_score, download_count, pawa_cost, is_free, tags, description, version).
- ✅ `routes/arena.py` — `GET /api/v1/arena/packages` (public, ?kind= filter), `POST /api/v1/arena/packages` (auth), `GET /api/v1/arena/packages/{id}` (public), `/products` + `/orders` stubs.
- ✅ `devui.py` — `GET /devui/library` groups arena_packages by kind for LibraryPanel.
- ✅ `LibraryPanel` wired to `GET /devui/library`; tab counts show real API data; empty state "mycelium is quiet · no packages yet".
- ✅ `ProfilePage.jsx` — `SignInPanel` now supports SIGN IN / REGISTER mode toggle with display name field; REGISTER + SIGN IN buttons in header and unauthenticated body.
- ✅ 14 new tests in `test_arena.py`. **1568 tests total.**

**Sprint 8.11 status (2 Jun 2026):**
- ✅ `council.py` — added `GET /` root route (fixes `test_council_stub_route_reachable`).
- ✅ `ControllerPanel` — replaced hardcoded `PROPOSALS` with real `GET /api/v1/council/{id}/proposals?status=PASSED`; 15s polling; empty state "council is quiet · no proposals in motion".
- ✅ `_transformProposal()` — maps API response shape to `ProposalCard`/`ProposalModal` props (id, sustain, title, summary, autonomy, cta, cost, sim, council).
- ✅ `ProposalCard` — per-councillor vote tally bar + expandable breakdown (operative_id, vote, reasoning truncated). Expand toggle stops card click propagation.
- ✅ `ProposalModal` — null-safe sim metrics (2-col fallback when no simulation); `votes.breakdown` renders COUNCILLOR BREAKDOWN section; EXECUTE button wired to `control.execute_approved` via API (not just flash).
- ✅ 8 new tests in `test_council_routes.py`. **1585 tests total.**

**New routes added in Sprints 8–11:**
- `POST /api/v1/users/register`, `POST /api/v1/users/login`, `GET /api/v1/users/me`, `GET /api/v1/users/me/stats`, `GET /api/v1/users/me/activity`
- `GET|POST|PUT /api/v1/lore/entries`, `POST /api/v1/lore/entries/{id}/publish`
- `GET|POST|PUT|DELETE /api/v1/journal/entries`
- `GET /api/v1/arena/packages`, `POST /api/v1/arena/packages`, `GET /api/v1/arena/packages/{id}`, `GET /api/v1/arena/products`, `POST|GET /api/v1/arena/orders`
- `GET /devui/library`, `GET /devui/sustain/{id}/graph`, `GET /devui/sustain/{id}/operators`
- `GET / | GET|POST /{sustain_id}/proposals | GET|POST /{sustain_id}/proposals/{id}/vote` (council)

---

### Slice 0 ✅ — Make the Monitor tell the truth (21 Jul 2026)

Audit-driven pass: every value the Monitor panel displays must trace to a real writer; racing/duplicate fetches that let cards disagree must collapse to one refresh path. 1628 tests pass (5 new).

**A1 — Seeded data reaching the Monitor:**
- `seed_pockets`/`seed_event` were already bridged into engine state in a prior session (`SustainEngine.seed_pocket()`, `seed_event()`). Verified still correct.
- `seed_operatives` was still an orphaned table — enabling/disabling an operative via the Seed panel had zero effect on the Monitor. Fixed: new `operative_overrides` table in `SustainEngine` (`_ensure_tables`), `set_operative_enabled(sustain_id, operative_id, enabled)`, and `get_operative_statuses()` now filters out explicitly-disabled operatives. `POST /seed/operative` calls the bridge and returns `bridged_to_monitor`, mirroring `seed_pocket`'s pattern.

**A2 + A3 — One computation, one refresh path:**
- `GET /devui/state` now also returns `widgets` (pocket_ring/event_feed/constraint_health), computed via a shared `_compute_monitor_widgets()` helper in `devui.py` used by both `/devui/state` and `/devui/monitor-widgets` — same computation, not two independently-fetched ones. `/devui/monitor-widgets` is kept standalone (used by the shell's event-count heartbeat) but delegates to the same helper.
- `MonitorPanel` (`monitor.jsx`) now makes **one** fetch per tick (`/devui/state`) driving state, events, operatives, constraints, and widgets together. Removed the separate `/devui/monitor-widgets` polling effect and the WS `liveState` partial-state merge (which only updated `state`, the actual source of card disagreement). `VisualizeWidgetGrid` is now presentational — receives `widgets` as a prop instead of self-fetching. `liveState` prop dropped from the `<MonitorPanel>` call in `shell.jsx` (shell's own WS connection/Footer/pawa-balance fallback untouched).

**B — Honest metrics, sweep results:**
- Removed `system.pawa_balance` and `system.api_p95_ms` rows from `apiStateToStateTree` (`monitor.jsx`) — neither is ever written by any operator; the real pawa balance was already correctly wired into the lower-left sidebar (`LeftNav` in `shell.jsx`, falling back to `GET /api/v1/users/me`) in a prior session.
- Removed the dead `pantry.*` row-generation block — `pantry` isn't in any sustain's `default_state` or written by any operator; it was inert (never populated) but had no real writer, so per the audit rule it's deleted rather than left as speculative code.
- Removed dead unused `pawaBalance`/`opsPerMin` variables in `monitor.jsx`.
- `ops_per_min` (Footer "bottom margin belt", `shell.jsx`) — confirmed no writer exists anywhere. Left as the existing honest `—` (no fabricated computation added).
- Orchie's operative card hardcoded `status:'active', pawa:0`. Orchie has no backing engine operative object (not in `_OPERATIVE_MAP` or any sustain spec), so there's nothing real to bind to — changed to `status:'idle', pawa:null, confidence:null` (same honest-idle treatment already used for operatives with no task/confidence data).
- Sweep also caught: the Event Log's offline/initial fallback used `pickEvent()` against `LOG_EVENTS` which had already been emptied to `[]` in a prior de-mocking pass — instead of showing mock data it rendered blank junk rows (timestamp + empty fields). Replaced with the design-language empty state `"no events recorded yet"`; same treatment added to LIVE STATE STREAM (`"no state signals yet · sustain is fresh"`) for consistency.

**C — Ordering:**
- Urgency ranking **was** possible: pockets have `allocated`/`spent`, so `pct = spent/allocated` (already computed for the existing red/amber/ok color coding) is a real "distance from the pocket's own limit" signal — no new schema/threshold data needed. Default sort is now urgency (highest pct first); a `URGENCY`/`BALANCE` toggle in the LIVE STATE STREAM header switches to descending remaining balance. Verified live: seeded a 90%-spent pocket next to a 0%-spent one, confirmed both sort orders.

**D — Layout:** Active Sustains big-digit-plus-list and the 6-operative 3×2 grid were already in place from a prior session (commit `18d1109`) — verified live via browser, no changes needed.

**Not fixed / flagged for a later slice:**
- The shell-level WS connection to `/devui/state-stream` (`shell.jsx`) currently errors in local dev (`[api.ws] error: Event`) and, even when connected, only ever pushes `state.system` fields that no operator writes — so it contributes nothing the REST poll + real user-balance fallback don't already cover. Not touched in this slice (out of the Monitor-panel-specific scope); worth a dedicated look.
- Event-log timestamp timezone bug (naive `datetime.utcnow().isoformat()`) — confirmed still present, per the brief this is Slice 1, not touched here.

---

### Slice 1 ✅ — Public access + real per-user auth (21 Jul 2026)

- Exposed the app publicly at `sustena.vyybandasky.online` via the existing WSL-hosted `vyyb-tunnel` cloudflared tunnel (new ingress entry + DNS route only — VOS/`vyyb-os` untouched). FastAPI now serves the built `apps/web/dist` directly (`StaticFiles` mount in `main.py`), so frontend + API share one hostname.
- Replaced the shared `ADMIN_TOKEN` model with real per-user JWT auth: `users` table gained `email`/`password_hash` (PBKDF2-SHA256) + `token_version` (for genuine logout revocation, not just client-side token deletion). All `devui.py`/`seed.py`/`dev.py`/`council.py` routes converted from `verify_admin` to `get_current_user`. Full-page login gate in `shell.jsx` (`AuthGate` + `SignInPanel`, reused from `ProfilePage.jsx`); `api.js` reads the session token from `localStorage.getItem('sustena_token')` per request, `VITE_ADMIN_TOKEN` removed entirely.
- Accounts created/reset for Bonnie; a keepalive watchdog (PowerShell + hidden VBS launcher) keeps the local dev stack running so the public hostname stays reachable.

---

### Slice 2 ✅ — Needs-attention + real mobile responsive layout (22 Jul 2026)

The first deliberately-visible slice after two invisible ones (Slice 0, Slice 1's login). Two parts, built as one mechanism: the same urgency-ranking logic that drives mobile's attention budget also drives the needs-attention block.

**Part A — Needs-attention block:**
- `GET /devui/state` extended with `proposals_in_voting` (new `_get_proposals_in_voting()` helper in `devui.py`, reads the already-existing `council_proposals` table — no new backend state, added to the same unified per-tick fetch rather than a second poll).
- `computeNeedsAttention()` in `monitor.jsx` ranks three real, already-computed signals: pockets ≥80% spent (reuses Slice 0's urgency `pct`), failing constraints (from `evaluate_constraints`), proposals awaiting a vote. Each row states what/why/how urgent (e.g. "90% spent · 100 of 1,000 left") and is tappable to the relevant panel where one exists. Empty categories render nothing; genuinely-clear state shows one plain line, never a zero-filled list.
- Deliberately excluded the WhatsApp support-queue count despite technically fitting "a total already computed and discarded" — CLAUDE.md's no-new-WhatsApp-development rule stands even for read-only surfacing.

**Part B — Responsive layout, not a shrunk desktop:**
- `useViewportWidth()` hook (640px breakpoint) drives `isMobile` in `monitor.jsx`; single-column stack, `compact` variants of `OperativeCard`/`StateRow`/`LogRow` (genuinely different layouts, not narrower copies of the fixed-pixel-grid desktop versions), progressive disclosure (4-row cap + "SHOW N MORE" toggle) on the state stream and event log.
- Root cause of the reported "BUDGETvisualize.pocket_ring" label collision: each `visualize.*` widget card showed its technical operator-name subtitle via `justify-content:space-between` with no wrap protection — hidden on mobile as decorative/developer-facing detail.
- **The actual blocker for "no horizontal scroll at any width" turned out to live in `shell.jsx`, not `monitor.jsx`.** The app-shell grid (`200px sidebar | 1fr main`) had no `minWidth:0` on the `main` grid track, so unconstrained child content forced the track wider than the viewport; `TopBar`'s right cluster (PROD·LIVE badge, OP identity, clock) didn't shrink or hide anything below its natural width either. Fixed: `LeftNav` auto-collapses to a 56px icon rail below the breakpoint (`useShellIsMobile()`, same 640px cutoff), `TopBar` trims to a single presence dot + bare clock + truncated sustain label, `Footer` becomes internally horizontally-scrollable (`overflowX:auto` + `.no-scrollbar`) with non-essential ticks hidden, and `main` got `minWidth:0`.
- Verified at 360/390/414px and 1440px desktop via direct DOM measurement (`getBoundingClientRect`, `scrollLeft` manipulation to confirm the page **cannot** actually scroll horizontally, `gridTemplateColumns` to confirm the correct layout branch is live) rather than screenshots — the in-app browser tool's screenshot/zoom actions reliably time out in this environment; this was confirmed as a tool limitation, not an app issue, before falling back. Confirmed live over the public hostname (`sustena.vyybandasky.online`) at both phone and desktop widths, authenticated as Bonnie, showing his real session data.
- 1649 backend tests pass (5 new: `proposals_in_voting` shape/empty/real-data cases). Frontend rebuilt (`npm run build`) and confirmed the public hostname is serving the new asset hashes without a backend restart (`StaticFiles` reads from disk per request).

---

### Slice 2 ✅ — Typed predicate invariants + the enforcing gate (the keystone) (22 Jul 2026)

Backend correctness — mostly invisible by design. The one visible payoff is honest *refusals* replacing silent bad state, nothing more was built to make it look bigger.

**What the audit had established:** `ConstraintEngine.evaluate_constraints()` existed but was purely advisory — called only from `devui.py` diagnostic routes, never gating a mutation. `ConstraintEngine.evaluate()` was single-state (no typed AST, no schema binding). Sustain `invariants` were flat strings. Operator `post_constraints` (and `min_privilege`) were declared on `OperatorMeta` but only ever stored, never evaluated.

**Move 1 — typed predicate invariants (`sustena/core/predicates.py`, new):**
- A full AST (`Comparison`, `Membership`, `LogicalAnd/Or/Not`, `Quantifier`, `Aggregate`, ...) with its own hand-written recursive-descent parser — no `eval()`/`exec()`/`compile()`, same discipline as the existing `constraints.py`.
- Grammar matches what the *shipped* sustain specs actually use (`path[*].field`, brackets after the path) rather than the older `Sustena_DSL_Dot_Protocol.md` reference doc's `[path].field` form — the running specs are the contract this follows, the doc is stale and worth reconciling separately.
- Added the `SUM`/`COUNT`/`AVG`/`MIN`/`MAX` aggregate-function grammar that was previously flagged as an explicit gap (`ConstraintEngine`'s old quantifier code literally rejected `SUM(...)` with "unsupported aggregate/block expression") — needed for the `journal_balanced` invariant (`ALL accounts.journal_entries[*]: SUM(lines[*].debit) == SUM(lines[*].credit)`) used by biashara/vyyb/colosso.
- `validate_against_schema(node, state_schema)` walks every state-path reference in the AST against the sustain's declared `state_schema` (handling `properties`, `items`, `additionalProperties` for the dict-of-objects pattern pockets/inventory use) — a predicate referencing an undeclared dimension is a **load-time finding**, not a silent runtime `None`-comparison.
- Real semantic subtlety caught and fixed during build: inside a quantifier body, a bare name must resolve against the current *item*'s own fields first, but still fall through to the top-level state for anything not shadowed (e.g. `rules.fine_reasons` referenced from inside `ALL fines[*]...`) — mirrors the old engine's `{**state.snapshot(), **item}` merge; both the evaluator and the schema validator thread a constant `global_root`/`root_schema` alongside the per-quantifier `scope`/`scope_schema` to get this right.
- `sustena/core/sustain_engine.py._compile_spec_invariants()` compiles + schema-binds every invariant once at spec load (idempotent, cached on the spec dict as `_compiled_invariants` / `_invariant_compile_errors`), non-fatal — an invalid invariant is logged and skipped from enforcement, it does not block the sustain from loading.

**Move 2 — the enforcing gate (`SustainEngine.execute_operator` + `simulate`):**
- After the operator function returns and `result.succeeded`, **before persisting**, `_check_enforcement_gate()` evaluates every compiled sustain invariant and the operator's declared `post_constraints` against the mutated `StateAccessor`. Any failure → `OperatorResult.fail(reason="would violate invariant '<id>' (<expr>): <legible reason>", constraint_violated="enforcement_gate")` — live state is **not** persisted, no exception thrown. Wired identically into `simulate()`'s per-step loop so a forked simulation can never advance past a state the live path would refuse (matches the DSL doc's "Simulator runs all constraints before Council deliberates").
- Only the `refuse` strategy from §4E's three (refuse / clamp / defer) is implemented, as scoped. Clamp/defer are not stubbed — no declared-but-unwired hooks were cheap enough to add honestly in this pass.
- `min_privilege` enforcement (the separate `privilege.check` mechanism from Master Strategy §4.5) remains unbuilt — out of scope for this slice, needs `access_policy` tier resolution that doesn't exist yet.

**Rollout — opt-in per sustain, not default-on:** a sustain spec must declare `"enforcement": {"enabled": true}` for the gate to do anything; everything else behaves exactly as before. Enabled explicitly on:
- **`homestead.json`** — both invariants (`liquid_non_negative`, `pocket_allocated_non_negative`) compile clean; every current homestead operator already self-guards to the same effect (`budget.allocate`'s own pre-check `finances.liquid.balance >= params.amount` is mathematically equivalent to the invariant), so this is a last-line-of-defense gate, not expected to trip in Bonnie's normal use — confirmed via the full homestead/budget/sustain_engine/sustain_specs test subset (272 tests) passing unchanged with the gate live.
- **`biashara.json`** — both invariants (`journal_balanced` using the new `SUM` aggregate, `inventory_qty_non_negative`) compile clean against its schema. No existing test currently exercises a biashara-templated sustain end-to-end through the engine, so this is validated statically (schema-correct, parses+binds) rather than empirically exercised — lower confidence than homestead, but zero risk to Bonnie's live usage since he isn't on a biashara sustain.

**Left disabled — real findings, not silently patched over:**
- **`vyyb.json`** — `journal_balanced` and `inventory_qty_non_negative` reference `accounts.*`/`inventory.*`, both present in `default_state` but **missing from `state_schema` entirely**. Pre-existing spec inconsistency; `state_schema` needs the same fields biashara's already has.
- **`colosso.json`** — same `accounts`/`inventory` schema gap (inherited invariants copied from Biashara without updating `state_schema`); `kyc_before_loan` additionally uses dynamic bracket indexing (`members[loan.member_id]`) which is outside the documented DSL grammar (`'[' number ']'` or `'[' '*' ']'` only) — deliberately not invented to accommodate it.
- **`chama.json`** — `loan_within_max_ratio` uses arithmetic (`rules.max_loan_ratio * pool.balance`) — the DSL grammar has no arithmetic operators, not invented for this slice. `contribution_within_range` is missing its `[*]` bracket entirely (`ALL contribution:` — not even valid under either bracket convention). `pool_balance_non_negative` and `fine_reason_valid` **do** compile clean, but the sustain as a whole is left off pending the other two being fixed.

All four non-homestead/biashara findings are pre-existing spec bugs that predate this slice — none were introduced by it, and none block enforcement anywhere it's actually enabled.

**Tests:** `tests/test_predicates.py` (39 tests — parser, schema binder, evaluator, plus a dedicated class that compiles every real invariant string from all 5 shipped specs and asserts the expected pass/fail per the findings above) + 8 new tests in `tests/test_sustain_engine.py::TestEnforcementGate` (refuses a bad transition via a monkeypatched operator that bypasses `StateAccessor.decrement()`'s own guard, confirms live state is untouched on refusal, confirms the reason names the failing dimension, confirms a real good transition still commits — including the `>= 0` boundary landing exactly on zero, confirms `simulate()` won't advance a forked state past a refused step, confirms a non-enforced sustain like vyyb is a true no-op). **1696 tests pass** (1688 + 8).

*(Note: the "vyyb is a true no-op" test above references a sustain removed in the very next slice — see below. Its assertions were rewritten to use `chama` instead; this historical paragraph is left as-written since it was accurate at the time.)*

---

### Slice 3 ✅ — Prune to homestead-only + introduce `habitat` (27 Jul 2026)

Scope confirmed with Bonnie before starting: do only the "do now" part below — auto-provisioning a habitat on signup and real parent/child roll-up are explicitly deferred to later slices (5 and 8 respectively), not built here even partially.

**Pre-flight:** git confirmed clean and in sync with `origin/main` before any deletion; `apps/api/sustena.db` (the live DB) copied to `backups/sustena_pre_prune_<timestamp>.db`; confirmed via direct query that the live DB holds exactly 2 sustain rows, both `template_id="homestead"` — zero biashara/vyyb/colosso instances existed anywhere, so the deletion carried zero data risk.

**1 — Deleted biashara/vyyb/colosso everywhere:**
- Removed `sustena/sustains/{biashara,vyyb,colosso}.json`, `sustena/sustains/seeds/vyyb_seed.json` (already-orphaned, referenced by nothing), `sustena/operators/{biashara,vyyb}.py` (no `colosso.py` ever existed — colosso's spec declared 6 `colosso.*` operators that were never implemented anywhere, a pre-existing dead end now moot), and the three dedicated test files (`test_biashara_operators.py`, `test_vyyb_operators.py`, `test_sustain_specs.py` — the last was 100% biashara/vyyb/colosso content, nothing homestead/chama-specific despite the generic filename).
- Updated `operators/__init__.py` (dropped the two now-dead imports), `test_operator_registry.py` (dropped `BIASHARA_OPERATORS`/`VYYB_OPERATORS` from `ALL_KNOWN_OPERATORS`), `test_predicates.py::TestRealSustainSpecs` (dropped the vyyb/colosso/biashara compile-check methods; homestead's stays, chama's stays since chama is untouched, added habitat's), `test_sustain_engine.py::test_non_enforced_sustain_is_unaffected` (rewritten against `chama` instead of `vyyb`, with chama's real 2-valid/2-error invariant profile).
- Minor cosmetic cleanup: `seed.py`'s legacy `type_map` dropped its `"business"→"vyyb"` and `"farm"→"mkulima"` entries (the latter already pointed at a template that's never existed — not part of this slice's ask, but equally dead either way, fixed while there); `claude_client.py`'s mock Orchie responses dropped the `"vyyb"` keyed reply and the Vyyb-specific clauses from `"burn"`/`"status"`.
- Deliberately left untouched (verified each is non-functional / out of scope, not silently missed): `procurement.py`'s `event.biashara.signal_evaluation_requested` event name and doc comments (a string literal / forward-looking hook, never coupled to biashara.json — its own tests still pass unchanged); `navigator.py`'s domain-list entry and docstring mentioning Vyyb dispatch (same — a domain-string match, not a file dependency); `chama_secretary.py`'s "this is a Colosso product" comment (business/brand context, not the deleted sustain template); ~8 incidental `"vyyb.hive"`/`"event.vyyb.order_placed"` example strings across `test_devui_routes.py`/`test_council_operatives.py`/`test_sustains_migration.py` (arbitrary example IDs, not real instantiation); `arena.py`'s Vyyb/Mkulima marketplace product listings and `profile.jsx`'s "Vyyb"/"Colosso" company-affiliation entries (real-world business names in demo/marketplace content, unrelated to the sustain-template system); `whatsapp_handler.py`'s Biashara onboarding copy — **not touched, per the standing no-`whatsapp_*`-files rule**, even though it now references a removed sustain.
- Chama was evaluated and left completely alone as instructed — it has real, independent code (`operators/chama.py`, `operatives/chama_secretary.py`, its own spec) and was never a fork of or dependency on biashara.
- **Verified:** app imports cleanly, `OPERATOR_REGISTRY` has zero biashara/vyyb/colosso entries (47 operators total), `sustena/sustains/*.json` glob returns exactly `chama`, `habitat`, `homestead`. Full suite: **1409 tests pass** (down from 1696 — the difference is the deleted dedicated test files, not a regression).

**2 — Introduced `habitat` (declared as a template only, per the explicit scope cut):**
- New `sustena/sustains/habitat.json` — the person-level primary Sustain: `identity` (name + free-text role) and a scaled-down `finances` block (liquid/pockets/income) identical in shape to homestead's own. Reuses the existing, already-tested `budget.record_income/allocate/spend/summary` operators verbatim — **no new operator code was written**. Deliberately excludes `homestead.tasks.*` to avoid using a `homestead.`-namespaced operator inside a non-homestead sustain (a real dot-protocol mismatch, not papered over).
- Ships its own `liquid_non_negative` invariant with `enforcement.enabled: true` — verified via `predicates.compile_invariant()` before enabling, exactly like homestead's and biashara's opt-ins in Slice 2.
- End-to-end verified live (not just compiled): instantiated a habitat, ran `budget.record_income` then `budget.allocate` — both succeeded and mutated state correctly. Then monkeypatched an operator to push the balance negative directly — the enforcement gate refused it, `constraint_violated="enforcement_gate"`, state unchanged. Caught and fixed a real bug in the process: the initial `default_state` was missing `finances.income.monthly_total`/`sources`, which `budget.record_income` needs internally — homestead's own spec carries these two fields in `default_state` despite them not being declared in `state_schema`; habitat's spec now matches that same pattern.
- **Auto-provisioning a habitat per new user on signup is NOT wired** — confirmed explicitly out of scope, belongs to Slice 5 (create/definition). Nothing currently creates a habitat instance automatically; it only exists as an instantiable template today.

**3 — Homestead recomposed as a declared holon of 6 habitats — honest answer: this is SCAFFOLDING, not real composition:**
- The earlier Slice 2 audit is correct and still stands: child-sustain composition (⊕) and state roll-up (ρ) are **not built**. `homestead.json` gained a new `"habitats"` block (structural spec metadata, not `state_schema`/`default_state`) declaring 6 member slots — **Bonnie, Cira, Epha, Mum, Kui, Frankie**, exactly as given, no names invented or expanded — each with `template: "habitat"`, `sustain_id: null`, `status: "declared"`, `composition_status: "declared"`, and `roll_up_status: "PENDING — not computed; requires Slice 8 (composition/coordination)"`.
- This block is **inert**: no code anywhere in `sustena/core` reads or acts on a `"habitats"` key. It cannot affect homestead's own state, its invariants, or the enforcement gate — confirmed by re-running the full suite and a direct spec-load check after adding it. There is no live linkage (`sustain_id` stays `null` for all 6 — no habitat instances were created for any Gachiengu member), and homestead's totals do **not** include, and cannot be made to include, any habitat's numbers until Slice 8 actually builds ⊕/ρ.
- What's real vs scaffolded, stated plainly: the *habitat template* is real and functional (proven above). The *declaration* that homestead is composed of 6 named habitats is real (it's genuine, correctly-typed JSON, not a lie). The *composition* — actually linking 6 live habitat instances as homestead's children and rolling their state up — does **not exist** and this slice does not pretend it does.

**What Bonnie still needs to provide:** nothing further on the member list — all 6 names (Bonnie, Cira, Epha, Mum, Kui, Frankie) were supplied directly in this slice's scope confirmation, so there's no fillable placeholder left in `homestead.json`. What's genuinely still needed is a decision on *when* to greenlight Slice 5 (auto-provisioning) and Slice 8 (real composition/roll-up) — both are scoped and understood, neither is scheduled.

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
curl -H "Authorization: Bearer dev-admin-token" http://localhost:9000/devui/sustains
curl -H "Authorization: Bearer dev-admin-token" http://localhost:9000/devui/widgets
curl -H "Authorization: Bearer dev-admin-token" \
  "http://localhost:9000/devui/state?sustain_id=<id>"
curl -X POST http://localhost:9000/orchie/message \
  -H "Content-Type: application/json" \
  -d '{"sustain_id":"<id>","message":"habari"}'
curl -X POST http://localhost:9000/devui/preview-widget \
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
