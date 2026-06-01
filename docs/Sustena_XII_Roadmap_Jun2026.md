# Sustena XII — Revised Roadmap
**Date:** 1 June 2026  
**Author:** Claude (Cowork) — full codebase read + brain dump integration  
**Prepared for:** Bonventure Gachiengu — Lead Architect  
**Supersedes:** `Bonnie_Master_Roadmap.md` (still canonical for long-horizon epics; this doc governs sprint sequence)

---

## Strategic Pivot — 1 June 2026

> **No more WhatsApp development.** Existing stubs remain but receive no new work until explicitly reactivated. Sustena XII is building its own app — not a WhatsApp-dependent product.

---

## State Snapshot — 1 June 2026

```
Backend core:   ██████████████████░░░░  ~75% Phase 1 — primitives + operators + engine solid
API routes:     ████████████░░░░░░░░░░  Exist but serve stub data — not wired to engine
Frontend:       ████████████░░░░░░░░░░  Shell + pages built; all mock data cleared; not connected
Tests:          ████████████████░░░░░░  614 pass / 58 error (fixture bug, not logic bug)
CI:             ████░░░░░░░░░░░░░░░░░░  Workflow exists; will fail on the 58 errors
Wiring:         ░░░░░░░░░░░░░░░░░░░░░░  ZERO — frontend has no API calls; engine not seeded
```

---

## Pre-Flight Issues — Fix Before Any Sprint

### P1 — 58 Test Errors (aiosqlite fixture incompatibility) 🔴

**Root cause:** `metadata.create_all()` called synchronously on an async `aiosqlite` engine in test fixtures. The logic is correct — the setup is wrong.

**Fix:** Use `asyncio.run(async_engine.begin(conn: await conn.run_sync(metadata.create_all)))` in all 9 affected test files' fixtures (`test_api.py`, `test_health.py`, `test_devui_routes.py`, `test_orchie_routes.py`, `test_whatsapp_handler.py`, `test_chama_secretary.py`, `test_council_operatives.py`, `test_operative_runtime.py`, `test_sustain_engine.py`).

**Commit:** `fix(tests): async metadata.create_all in all conftest fixtures`

---

### P2 — `backend/` Stale Directory 🟡

Pre-monorepo copy at repo root. Missing `sustena/scripts/`. Delete it.

**Commit:** `chore(repo): remove stale backend/ directory`

---

### P3 — `SustainEngine.list_all()` Not Implemented 🟡

`/devui/sustains` catches the exception and returns hardcoded stubs. Until this runs, the sustain selector never shows real data.

**Fix:** `SELECT id, sustain_type, name FROM sustains` in `sustain_engine.py`.

**Commit:** `feat(engine): implement SustainEngine.list_all()`

---

### P4 — No Seeded Sustain in DB 🔴

`sustena.db` exists but is empty. Every API call falls back to stubs.

**Fix:** `apps/api/sustena/scripts/seed_homestead.py` — calls `engine.instantiate("homestead", user_id="bonnie", ...)`, runs `budget.record_income`, outputs the sustain ID.

**Commit:** `feat(seed): seed_homestead.py — Bonnie's Homestead sustain with realistic pockets`

---

### P5 — Frontend Has Zero API Calls 🔴

June 1 commit cleared all mock data but added no fetch calls. The UI is a shell with empty panels.

**Fix:** Sprint 1 (below).

---

## Sprint 0 — Pre-Flight
**Goal:** CI green, DB seeded, API returns real data  
**Definition of done:** `pytest tests/ -q` → 0 errors, 670+ pass; `GET /devui/sustains` returns Homestead

| # | Task | Commit |
|---|------|--------|
| 0.1 | Fix async fixtures in all 9 test files | `fix(tests): async create_all in fixtures` |
| 0.2 | Delete `backend/` | `chore(repo): remove stale backend/` |
| 0.3 | Implement `SustainEngine.list_all()` | `feat(engine): list_all() — query sustains table` |
| 0.4 | Write + run `seed_homestead.py` | `feat(seed): Bonnie Homestead with realistic pockets` |
| 0.5 | Add `SUSTAIN_ID=homestead.bonnie` to `.env.example` | `chore(config): add default sustain ID to env` |

---

## Sprint 1 — "Sustena Runs"
**Goal:** Web UI shows real Homestead data from DB. Orchie responds in mock mode. No LLM calls.  
**Definition of done:** Every panel shows real state; Orchie chat replies; nothing is mocked in the frontend

### 1.1 — Wire Monitor Panel to `/devui/state` + WebSocket

On mount: `GET /devui/state?sustain_id={activeSustain}`. Then connect `WS /devui/state-stream?sustain_id=...&token=...` for live updates. Render operative cards, constraint health, event feed from live data.

**Commit:** `feat(web): wire Monitor panel to /devui/state + WebSocket`

### 1.2 — Wire Sustain Selector to `/devui/sustains`

Fetch real sustain list on mount. Populate selector. Switching re-fetches state.

**Commit:** `feat(web): wire TopBar selector to GET /devui/sustains`

### 1.3 — Wire Operator Console to `/devui/console/execute`

POST on submit, display result delta inline.

**Commit:** `feat(web): wire Operator Console to POST /devui/console/execute`

### 1.4 — Wire Orchie Chat to `/orchie/message`

POST user message, render `reply`. Endpoint already uses `MockClaudeClient` — no LLM key needed.

**Commit:** `feat(web): wire Orchie chat to POST /orchie/message`

### 1.5 — Confirm mock mode at startup

Log `"Claude client: mock mode (zero API calls)"` when `ANTHROPIC_API_KEY=mock`.

**Commit:** `chore(config): log mock/real client mode at startup`

**[TEST] Sprint 1 criteria:**
- [ ] `http://localhost:5173` → Monitor panel shows real Homestead pocket data
- [ ] Sustain selector lists "Homestead"; switching re-fetches
- [ ] Operator Console: `budget.allocate pocket=food amount=500` → result + DB updated
- [ ] Orchie: "habari" → reply rendered; server logs show zero Anthropic API calls

---

## Sprint 2 — UIParser
**Goal:** A spec-driven rendering system so any operator, operative, or sustain can declare its own UI  
**Dependency:** Sprint 1 complete (frontend makes real API calls)  
**Maps to:** Epic 1.11 (ui_schema) from `Bonnie_Master_Roadmap.md`

### Why UIParser is second

Everything that follows — operator UIs, operative dashboards, the Today list, Council proposal cards — needs a rendering layer that can turn a JSON spec into a widget without writing new frontend code each time. UIParser is that layer.

### 2.1 — `ui_schema` block spec

Define the `ui_schema` block format in operator/operative/sustain specs:

```json
"ui_schema": {
  "widget_type": "budget_summary_card",
  "fields": [
    { "label": "Allocated", "source": "inputs.amount", "display": "currency_kes" },
    { "label": "Remaining",  "source": "state.finances.pockets.food.allocated - state.finances.pockets.food.spent", "display": "currency_kes", "colour_rule": "amber_if_below_20pct" }
  ],
  "ctas": ["Approve", "Modify", "Defer"]
}
```

**Commit:** `feat(uiparser): define ui_schema block spec — fields, sources, colour_rules, ctas`

### 2.2 — `UISchemaParser` class

`parse(operator_spec: dict) -> UISchema` — reads `ui_schema` block, validates, returns typed `UISchema`.  
`resolve_source(source_expr, operator_inputs, state) -> Any` — evaluates dot-path expressions and subtraction formulae (Phase 1 only; no `eval()`).

**Commit:** `feat(uiparser): UISchemaParser — parse + resolve_source, no eval()`

### 2.3 — `ui.render.*` sub-operators

`ui.render.operator_card(ui_schema, inputs, state) -> ResponseWidget`  
`ui.render.operative_dashboard(ui_schema, operative_state) -> ResponseWidget`  
`ui.render.sustain_home(ui_schema, full_state, active_operatives) -> list[ResponseWidget]`  
`ui.render.preview(spec_json, mock_state) -> ResponseWidget` — for the dev console live preview

Pawa cost: 0. No side effects.

**Commit:** `feat(uiparser): ui.render.* sub-operators — operator_card, operative_dashboard, sustain_home, preview`

### 2.4 — `WidgetTypeRegistry`

`register(widget_type, template_path, schema)` — local registry of widget types.  
`render(widget: ResponseWidget) -> str` — Jinja2 HTML fragment.  
`GET /devui/widgets` — returns all registered widget types.

**Commit:** `feat(uiparser): WidgetTypeRegistry — register, render, GET /devui/widgets`

### 2.5 — Dev console UI Preview tab

Left: paste operator spec JSON. Right: live rendered widget preview (debounced 500ms, POSTs to `POST /devui/preview-widget`).

**Commit:** `feat(web): UIParser Preview tab in Dev Console`

**[TEST] Sprint 2 criteria:**
- [ ] Add `ui_schema` to `budget.allocate`; `UISchemaParser.parse()` returns correct `UISchema`
- [ ] `ui.render.operator_card()` with live Homestead state returns correct `ResponseWidget`
- [ ] Dev Console → UI Preview: paste `budget.allocate` spec → widget renders in right pane within 1s
- [ ] `GET /devui/widgets` lists all registered widget types
- [ ] `resolve_source()` handles subtraction formula correctly; no `eval()` call in the codebase

---

## Sprint 3 — Operators + Protocols
**Goal:** New operator types covering the full execution surface; protocol patterns as first-class concepts  
**Dependency:** UIParser complete (new operators should include their `ui_schema` blocks)

### What a Protocol is

A **protocol** is the communication pattern governing how an operator interacts with the world:

| Protocol | Pattern | Example |
|----------|---------|---------|
| `event_driven` | Subscribes to EventBus; fires when event matches filter | Mentor fires on `event.finances.pocket_spent` at >80% |
| `polling` | Runs on a timer; reads state, compares to threshold | Pantry monitor checks stock every 6h |
| `streaming` | Maintains open connection; pushes deltas as they arrive | WebSocket state-stream to the Monitor panel |
| `rpc` | Synchronous call-response; blocks until result | `budget.allocate` — call, get result, continue |

Every operator in the registry declares its `protocol` in metadata. The operative runtime uses this to decide how to call it.

### New Operator Types

### 3.1 — `api.*` operators (external RPC)

Generic HTTP adapter. Any external REST endpoint becomes a Sustena operator.

```python
@sustena_operator(
    name="api.get",
    protocol="rpc",
    description="HTTP GET against an external URL"
)
async def api_get(ctx, url: str, headers: dict = {}, timeout_s: int = 10) -> OperatorResult:
    ...
```

Also: `api.post`, `api.webhook_listen` (event_driven — registers as EventBus listener for incoming webhook events).

**Commit:** `feat(operators): api.* operators — get, post, webhook_listen`

### 3.2 — `monitor.*` operators (event_driven + polling)

Watch a state path or constraint expression; emit an alert event when it breaches.

```python
@sustena_operator(name="monitor.state_path", protocol="event_driven")
async def monitor_state_path(ctx, path: str, condition: str, alert_event: str) -> OperatorResult:
    # Checks condition against state; publishes alert_event if breached
    ...

@sustena_operator(name="monitor.constraint", protocol="polling")
async def monitor_constraint(ctx, constraint_expr: str, interval_s: int = 3600) -> OperatorResult:
    # Re-evaluates constraint on interval; publishes event.monitor.constraint_breached if failing
    ...
```

**Commit:** `feat(operators): monitor.* operators — state_path, constraint`

### 3.3 — `visualize.*` operators (produce ResponseWidget outputs)

Wires UIParser into operator outputs. Every operator that produces a display should use `visualize.*`.

```python
@sustena_operator(name="visualize.pocket_ring", protocol="rpc")
async def visualize_pocket_ring(ctx, sustain_id: str) -> OperatorResult:
    # Returns a ResponseWidget of type budget_ring_chart
    ...

@sustena_operator(name="visualize.event_feed", protocol="streaming")
async def visualize_event_feed(ctx, sustain_id: str, limit: int = 20) -> OperatorResult:
    ...

@sustena_operator(name="visualize.constraint_health", protocol="rpc")
async def visualize_constraint_health(ctx, sustain_id: str) -> OperatorResult:
    ...
```

**Commit:** `feat(operators): visualize.* operators — pocket_ring, event_feed, constraint_health`

### 3.4 — `simulate.*` operators (fork + run)

Exposes the simulation engine as operators (not just an internal SustainEngine method).

```python
@sustena_operator(name="simulate.fork", protocol="rpc")
async def simulate_fork(ctx, sustain_id: str) -> OperatorResult:
    # Returns a fork_id (in-memory state copy)

@sustena_operator(name="simulate.run_path", protocol="rpc")
async def simulate_run_path(ctx, fork_id: str, operator_sequence: list) -> OperatorResult:
    # Runs sequence on fork; returns SimulationPath

@sustena_operator(name="simulate.score", protocol="rpc")
async def simulate_score(ctx, fork_id: str, goal_metric: str) -> OperatorResult:
    # Scores the fork's final state against a goal metric
```

**Commit:** `feat(operators): simulate.* operators — fork, run_path, score`

### 3.5 — `edit.*` operators (spec/state/config mutations)

```python
@sustena_operator(name="edit.state_patch", protocol="rpc")
async def edit_state_patch(ctx, sustain_id: str, patch: dict) -> OperatorResult:
    # JSON Patch RFC 6902 against live state — requires Council approval if above threshold

@sustena_operator(name="edit.operator_spec", protocol="rpc")
async def edit_operator_spec(ctx, operator_name: str, field: str, value) -> OperatorResult:
    # Modifies an operator's metadata in the registry (description, constraints, pawa_cost)
```

**Commit:** `feat(operators): edit.* operators — state_patch, operator_spec`

### 3.6 — `control.*` operators (execute + commit decisions)

```python
@sustena_operator(name="control.execute_approved", protocol="rpc")
async def control_execute_approved(ctx, proposal_id: str) -> OperatorResult:
    # Executes a PASSED Council proposal against live state

@sustena_operator(name="control.rollback", protocol="rpc")
async def control_rollback(ctx, sustain_id: str, to_version: int) -> OperatorResult:
    # Restores sustain state to a prior snapshot version
```

**Commit:** `feat(operators): control.* operators — execute_approved, rollback`

### 3.7 — Protocol registration in `@sustena_operator`

Extend `OperatorMeta` with `protocol: str` field. Update `@sustena_operator` decorator to require it. Update `OPERATOR_REGISTRY` listing to include protocol. The operative runtime (Sprint 5) uses protocol to determine how to call each operator.

**Commit:** `feat(operators): add protocol field to OperatorMeta + all existing operators`

**[TEST] Sprint 3 criteria:**
- [ ] `monitor.constraint` on `pantry.cooking_oil_L >= 1.0` fires `event.monitor.constraint_breached` when oil is 0.4
- [ ] `simulate.fork` + `simulate.run_path` with 3-step budget sequence returns correct `SimulationPath`; DB unchanged
- [ ] `control.execute_approved` runs a PASSED proposal and updates state
- [ ] `control.rollback` restores state to prior snapshot
- [ ] All operators in registry have `protocol` field; `pytest tests/test_operator_registry.py` passes with protocol assertion
- [ ] `visualize.pocket_ring` returns a `ResponseWidget` that UIParser can render

---

## Sprint 4 — Operator UIs
**Goal:** Every new operator type has a corresponding widget and panel in the frontend  
**Dependency:** UIParser + Operators+Protocols complete

### 4.1 — Protocol-aware operator console

The dev console already accepts `operator_name params`. Extend it to:
- Show protocol badge next to operator name (`event_driven`, `polling`, `rpc`, `streaming`)
- For `streaming` operators: open a live panel below the console that receives the stream
- For `event_driven` operators: show "waiting for trigger..." with the event filter

**Commit:** `feat(web): protocol-aware Operator Console — badges + stream panel + event trigger UI`

### 4.2 — Monitor Panel widget grid

The Monitor Panel should render live `visualize.*` operator outputs as a widget grid:
- Budget ring (`visualize.pocket_ring`)
- Event feed (`visualize.event_feed`)
- Constraint health grid (`visualize.constraint_health`)
- Each widget uses UIParser to render — no hardcoded layouts

**Commit:** `feat(web): Monitor Panel renders visualize.* widgets via UIParser`

### 4.3 — Simulate Panel wired to `simulate.*` operators

Replace the current `POST /devui/simulate` call with a sequence of: `simulate.fork` → `simulate.run_path` → `simulate.score`. Show per-step state diff in the accordion.

**Commit:** `feat(web): Simulator uses simulate.* operators — fork/run/score pipeline`

### 4.4 — Control Panel proposal execution

The proposal queue calls `control.execute_approved`. Rollback calls `control.rollback`. Both show a confirmation modal with the state diff preview before executing.

**Commit:** `feat(web): Control Panel uses control.execute_approved + control.rollback`

**[TEST] Sprint 4 criteria:**
- [ ] Monitor Panel renders all three `visualize.*` widgets with real Homestead data
- [ ] Streaming widget in console shows live event feed as events are published
- [ ] Simulator: fork → run 3 steps → score → accordion renders step diffs
- [ ] Control Panel: execute proposal → state updated; rollback → state restored

---

## Sprint 5 — Operative Networks (LLM-Optional Base Layer)
**Goal:** Operative reasoning defined as a directed graph of operator calls. No LLM required. LLM is a pluggable optional node.  
**Dependency:** All operator types built (Sprint 3)

### The Architecture

An operative's intelligence is currently: `deliberate(proposal) → call_claude() → parse_response → OperativeVote`. This makes all operatives LLM-dependent and untestable without API calls.

The new architecture:

```
OperativeGraph
  ├── nodes: dict[str, OperatorRef]   — each node is an operator call
  ├── edges: list[(from, to, Condition)]  — conditional routing
  ├── entry_node: str
  └── exit_node: str

Execution: entry → [run operator] → [route by result] → ... → exit → OperativeProposal
```

The LLM is just another node type — `llm.haiku_call`, `llm.sonnet_call`. When `ANTHROPIC_API_KEY=mock`, these nodes return mock responses. The graph still executes.

### 5.1 — `OperativeGraph` base layer

Define `OperativeGraph`, `OperativeNode`, `OperativeEdge`, `Condition` in `sustena/core/operative_graph.py`.  
`OperativeGraph.run(context, trigger_event) -> OperativeProposal` — executes the graph.  
`OperativeGraph.from_spec(spec_dict) -> OperativeGraph` — deserialise from JSON spec.

**Commit:** `feat(operatives): OperativeGraph base layer — nodes, edges, conditional routing`

### 5.2 — Refactor `BaseOperative` to use `OperativeGraph`

`BaseOperative.evaluate(trigger_event)` → runs `self.graph.run(ctx, trigger_event)`.  
`BaseOperative.deliberate(proposal)` → runs `self.deliberation_graph.run(ctx, proposal)`.  
Each operative has two graphs: an `evaluation_graph` (triggered by events) and a `deliberation_graph` (triggered by council proposals).

**Commit:** `feat(operatives): BaseOperative uses OperativeGraph for evaluate + deliberate`

### 5.3 — Rewrite Mentor as graph-of-operators

Replace Mentor's LLM call with a graph:

```
entry: monitor.state_path(path="finances.pockets.*.spent_pct", condition=">80%")
  → evaluate: simulate.run_path([budget.reallocate, ...])
  → score: simulate.score(goal="minimize_budget_deviation")
  → propose: if score > threshold → council.create_proposal(...)
  → exit
```

No LLM call unless the `llm.haiku_call` node is explicitly in the graph.

**Commit:** `feat(operatives): rewrite Mentor as OperativeGraph — no LLM dependency`

### 5.4 — Protocol-aware operative runtime

The operative runtime respects each operator's protocol when building the graph:
- `event_driven` nodes: registered as EventBus subscribers (fire when event arrives)
- `polling` nodes: scheduled by the runtime at the declared interval
- `rpc` nodes: called synchronously when the graph reaches them
- `streaming` nodes: open a connection; subsequent graph steps receive stream events

**Commit:** `feat(operatives): protocol-aware runtime — event/polling/rpc/streaming dispatch`

### 5.5 — Graph spec in JSON (sustain spec integration)

Operatives can now be fully specified in JSON sustain specs, not just Python classes:

```json
"operatives": {
  "mentor": {
    "class": "MentorOperative",
    "evaluation_graph": "graphs/mentor_evaluation.json",
    "deliberation_graph": "graphs/mentor_deliberation.json"
  }
}
```

**Commit:** `feat(sustains): operative graph specs in JSON — evaluation + deliberation graphs`

### 5.6 — Design graph specs for all Council operatives

Each of the five Council operatives and Orchie gets a designed `evaluation_graph` and `deliberation_graph` spec before implementation begins. These live as JSON files in `apps/api/sustena/operatives/graphs/`.

| Operative | Trigger protocol | Evaluation graph | Deliberation graph |
|-----------|-----------------|------------------|--------------------|
| **Mentor** | `event_driven` — `event.finances.pocket_spent` >threshold | `monitor.state_path` → `simulate.run_path([budget.reallocate])` → `simulate.score(goal=minimize_budget_deviation)` → `council.create_proposal` if score warrants | `simulate.run_path(proposal)` → `simulate.score` → YES if improves budget position |
| **Protégé** | `polling` hourly + `event_driven` on `event.task.added` | `calendar.upcoming_events` → `homestead.tasks.list` → `monitor.deadline_proximity` → surface overdue items | `simulate.run_path(proposal)` → check time conflicts → score on time_optimization |
| **Attaché** | `event_driven` — any M-Pesa event or contact interaction | `attache.profile_lookup` → `attache.trust_score_update` → `visualize.relationship_summary` → propose if trust threshold crossed | `simulate.run_path(proposal)` → score on social_capital impact |
| **Curator** | `polling` every 6h + `event_driven` on `event.inventory.consumption` | `monitor.state_path(inventory.*.level, condition=below_reorder_threshold)` → `visualize.restock_alert` → `council.create_proposal(procurement.raise_po)` | `simulate.run_path(proposal)` → score on value_per_kes + quality |
| **Navigator** | `event_driven` — logistics and delivery events | `monitor.order_status` → `api.get(routing_data)` → `simulate.run_path(route_sequence)` → `visualize.route_summary` | `simulate.run_path(proposal)` → score on time_cost_efficiency |
| **Orchie** | User input (session) + scheduled `morning_brief` | Intent parse → route to operator OR `operative.spawn` OR `council.create_proposal` | *Orchie does not vote — it proposes and executes* |

Each graph is stored as a JSON spec (`mentor_evaluation.json`, `mentor_deliberation.json`, etc.) and loaded by `OperativeGraph.from_spec()`. The table above is the design reference; the JSON files are the implementation.

**Commit:** `feat(operatives): graph spec designs for Mentor, Protégé, Attaché, Curator, Navigator, Orchie`

---

### 5.7 — `operative.spawn` operator + calibration mechanism

The calibration mechanism is how a generic template operative from the Mycelium Library becomes a context-specific instance. It is the implementation of the Master Strategy's "mating" concept: **developer template + caller context + user state = personalised operative instance**.

#### The spawn flow

```
Caller (Orchie OR a Councillor)
  │
  ├── operative.spawn(template_id, calibration_data)
  │     │
  │     ├── load template graph from library (JSON spec with {{placeholder}} tokens)
  │     ├── resolve placeholders using calibration_data:
  │     │     {{ user.income }}          → state.finances.income.amount
  │     │     {{ user.goal }}            → state.finances.goals[0].name
  │     │     {{ caller.domain }}        → councillor's domain list
  │     │     {{ proposal.scenario }}    → current proposal being evaluated
  │     │     {{ sustain.context }}      → summary of current sustain state
  │     └── return instantiated OperativeGraph (placeholders resolved, ready to run)
  │
  └── run instantiated graph → OperativeProposal OR DelegatedVote
```

#### `operative.spawn` as an operator

```python
@sustena_operator(name="operative.spawn", protocol="rpc")
async def operative_spawn(
    ctx,
    template_id: str,          # e.g. "investment_advisor" from library
    calibration_data: dict,    # user/caller context injected into {{placeholders}}
    caller_id: str,            # who is spawning — Orchie or a councillor name
    scope: str = "session",    # "session" (ephemeral) | "persistent" (added to sustain)
) -> OperatorResult:
    # Loads template from sustain/operatives/graphs/{template_id}.json
    # Resolves {{placeholder}} tokens using calibration_data + live state
    # Returns an instantiated OperativeGraph, not yet run
```

#### Placeholder resolution in `OperativeGraph.from_spec()`

Extend `from_spec(spec_dict, calibration_data={})` to resolve `{{dot.path}}` tokens before building the graph. Resolution order:
1. `calibration_data` dict (caller-injected context — highest priority)
2. `state.*` paths from live sustain state
3. Default values declared in the template spec

Any unresolved placeholder at instantiation time raises `CalibrationError` — not a runtime error, a spec error. The template must declare all required calibration fields.

#### Orchie-spawned vs. councillor-spawned

| Spawn context | Calibration data source | Output |
|---------------|------------------------|--------|
| Orchie spawning a specialist for a user task | User intent + full sustain state | `OperativeProposal` → surfaced to user |
| Councillor spawning a sub-operative for deliberation | Councillor's domain + current proposal + forked sustain state | `DelegatedVote` → returned to councillor |

The `operative.spawn` operator is unaware of which context it's in — that distinction is determined by who calls it and what `calibration_data` they pass. The operator itself is generic.

**Commit:** `feat(operatives): operative.spawn operator + {{placeholder}} calibration in OperativeGraph.from_spec()`

**[TEST] Sprint 5 criteria:**
- [ ] `OperativeGraph.run()` on a 3-node graph returns correct `OperativeProposal`; zero LLM calls
- [ ] Mentor `evaluate()` triggered by `event.finances.pocket_spent` at >80% → proposal created; no Anthropic calls in logs
- [ ] `OperativeGraph.from_spec()` loads a graph from JSON and runs correctly
- [ ] `OperativeGraph.from_spec(spec, calibration_data)` resolves `{{user.income}}` from state; raises `CalibrationError` on missing required placeholder
- [ ] `operative.spawn("investment_advisor", calibration_data={...})` → instantiated graph with all placeholders resolved
- [ ] Orchie-spawned graph returns `OperativeProposal`; councillor-spawned graph returns `DelegatedVote`
- [ ] Protocol dispatch: `polling` node fires on schedule; `event_driven` node fires on matching event
- [ ] `pytest tests/test_operative_runtime.py` — all pass; mock mode confirmed

---

## Sprint 6 — Today List + Morning Brief
**Goal:** Orchie opens each session with a structured daily brief — tasks, events, expected transactions, unfinished items, approved strategies — narrated in Orchie's voice  
**Dependency:** Operative networks (Orchie's brief is generated by an operative graph)

### What the Today list aggregates

| Source | What it pulls |
|--------|--------------|
| `calendar.events` | Events today from sustain calendar |
| `council_proposals` | Strategies with status=PASSED that are due or in-progress today |
| `tasks` | Unresolved tasks (from a new `homestead.tasks.*` operator) |
| `finances.expected` | Expected income / recurring expenses for today (auto-detected patterns) |
| `operators_log` | Unfinished operator sequences from previous sessions |
| `operatives.alerts` | Pending alerts from any active operative |

### 6.1 — `homestead.tasks.*` operators

```python
@sustena_operator(name="homestead.tasks.add",    protocol="rpc")
@sustena_operator(name="homestead.tasks.complete", protocol="rpc")
@sustena_operator(name="homestead.tasks.list",    protocol="rpc")  # returns today's task list
@sustena_operator(name="homestead.tasks.carryover", protocol="polling")  # marks overdue tasks
```

**Commit:** `feat(operators): homestead.tasks.* — add, complete, list, carryover`

### 6.2 — `orchie.morning_brief` operator

Aggregates all sources above, formats into a `ResponseWidget` of type `morning_brief_card`:

```python
@sustena_operator(name="orchie.morning_brief", protocol="rpc")
async def morning_brief(ctx, sustain_id: str, date: str) -> OperatorResult:
    # Reads calendar, proposals, tasks, expected, alerts
    # Formats into structured widget + brief_text narration
    # brief_text: "Habari Bonnie, leo una..." (Orchie voice)
```

**Commit:** `feat(operators): orchie.morning_brief — aggregates today's full picture`

### 6.3 — Orchie session init

When the web app loads, Orchie automatically runs `orchie.morning_brief` and renders it as the first message in the chat thread. No user input needed.

**Commit:** `feat(web): Orchie auto-sends morning brief on session start`

### 6.4 — Today panel (new UI panel)

Add a "Today" panel alongside the existing Monitor/Simulator/Editor/Controller/Library panels. Renders the `morning_brief_card` widget plus an interactive task list (check-off tasks, approve strategy proposals inline).

**Commit:** `feat(web): Today panel — morning brief card + interactive task list`

**[TEST] Sprint 6 criteria:**
- [ ] `orchie.morning_brief` with a seeded Homestead state returns a `ResponseWidget` with all 5 sections populated
- [ ] Opening the web app → Orchie chat shows the morning brief automatically
- [ ] Today panel renders the brief card + task list; checking off a task calls `homestead.tasks.complete`
- [ ] Approved Council strategies with due dates appear in the brief

---

## Sprint 7 — Council Enrichment
**Goal:** Councillors orchestrate sub-operative networks to evaluate proposals in sandbox simulations before voting  
**Dependency:** Operative networks (Sprint 5) + simulation operators (Sprint 3)

### The Architecture

```
Orchie.propose(scenario, simulation_evidence)
    ↓
Council.design_exploration_space(scenario)
  Each councillor:
    1. Evaluates scenario relevance to their domain
    2. If relevant: spawns domain-specific sub-operatives into a sandbox
    3. Sub-operatives run simulate.* operators on a forked state
    4. Sub-operatives vote → delegate vote up to councillor
    5. Councillor forms its position based on sub-operative results + own utility function
    6. If not relevant: ABSTAIN (no sub-operatives spawned)
    ↓
Council.collect_votes() — Nash bargaining resolution
    ↓
User approval (51% weight) → PASSED / FAILED / DEFERRED
```

### 7.1 — `CouncillorConfig` with sub-operative roster

Each councillor declares its own `domain` list and sub-operative roster in its spec. Domain overlap is evaluated at runtime — which councillors abstain and which sub-operatives get spawned is entirely determined by the spec, not hardcoded anywhere in the engine.

```json
{
  "mentor": {
    "domain": ["finances", "budget", "savings"],
    "sub_operatives": {
      "financial_risk_assessor": "graphs/financial_risk_assessment.json",
      "burn_rate_analyst": "graphs/burn_rate_analysis.json"
    }
  },
  "curator": {
    "domain": ["assets", "inventory", "procurement", "investment"],
    "sub_operatives": {
      "investment_advisor": "graphs/investment_advisory.json",
      "asset_valuator": "graphs/asset_valuation.json"
    }
  }
}
```

**Commit:** `feat(council): CouncillorConfig — domain fields, sub-operative roster`

### 7.2 — Domain relevance check (abstain logic)

Before spawning sub-operatives, each councillor runs a relevance check against its `domain` list and the proposal's tagged domains. If overlap is zero: `ABSTAIN` immediately, zero sub-operatives spawned, zero LLM calls.

**Commit:** `feat(council): domain relevance check — auto-ABSTAIN when no domain overlap`

### 7.3 — Sandbox simulation per councillor

Each relevant councillor:
1. Calls `simulate.fork()` to get its own independent state fork
2. Runs its sub-operatives against the fork
3. Sub-operatives use `simulate.run_path()` and `simulate.score()` to evaluate the proposal
4. Sub-operative results flow back to the councillor (via EventBus on the fork's scope)

No two councillors share a fork — each has an isolated sandbox.

**Commit:** `feat(council): per-councillor sandbox — fork per councillor, isolated sub-operative execution`

### 7.4 — Vote delegation from sub-operatives to councillor

Sub-operative returns `DelegatedVote(position: YES|NO|ABSTAIN, confidence: float, reasoning: str)`. Councillor aggregates delegated votes (weighted by sub-operative confidence) then forms its final vote.

**Commit:** `feat(council): DelegatedVote — sub-operative vote aggregation at councillor level`

### 7.5 — Enriched Council in CouncilSession

Update `CouncilSession.collect_votes()` to run the full enriched flow: relevance check → sandbox fork → sub-operative execution → vote aggregation → councillor vote. The existing `deliberate()` interface on `BaseOperative` is still honoured — it now routes to the graph-based evaluation.

**Commit:** `feat(council): CouncilSession enriched flow — sandbox simulation + sub-operative delegation`

### 7.6 — Council panel in web UI

The Controller panel's proposal queue shows:
- Per-councillor vote with reasoning summary
- Sub-operatives spawned per councillor (expand to see their individual delegated votes)
- Sandbox simulation summary (what forked state looked like after running the proposal)

**Commit:** `feat(web): enriched Council panel — sub-operative votes, sandbox summary`

**[TEST] Sprint 7 criteria:**
- [ ] Proposal with domains that don't overlap a councillor's `domain` list → that councillor auto-ABSTAINs; none of its sub-operatives are instantiated
- [ ] Councillor with domain overlap spawns its sub-operatives; each sub-operative's `DelegatedVote` is aggregated before the councillor casts its final vote
- [ ] Each councillor gets an independent fork; mutations in one fork don't appear in another
- [ ] `CouncilSession.collect_votes()` returns enriched vote record with sub-operative detail
- [ ] Council panel renders per-councillor detail with expand/collapse on sub-operatives

---

## Sprint 8 — "Lore Goes Live" (Epic 1.7b)
**Goal:** `lore.dev` live with 3 journal entries, docs stub, /about, /contact  
**Duration:** 1–2 sessions

Journal entry 1: "What is a Sustain?" — the 7-primitive claim, plain language  
Journal entry 2: "Why the ConstraintEngine can't use eval()" — honest technical postmortem  
Journal entry 3: "Building Orchie LLM-free first" — the mock client decision and operative graph architecture

---

## Sprint 9 — GitHub Hygiene (Epic 1.8)
**Goal:** CI passes on every push; issue + PR templates exist; README investor-ready  

After Sprint 0 fixes tests, Sprint 9 verifies CI is green end-to-end, adds issue/PR templates, and updates README.

---

## Sprint 10+ — Continue Bonnie_Master_Roadmap.md
Once Sprints 0–7 complete:

| Epic | Description |
|------|-------------|
| 1.12 | Monte Carlo simulation engine (builds on `simulate.*` operators from Sprint 3) |
| 1.13 | Mkulima sustain (fork from Biashara) |
| 1.14 | Daraja production (WhatsApp stubs stay; Daraja wired when triggered) |
| 0.0.x | Domains, legal, banking (Brian's lane — parallel) |

---

## What to Hold Off On

- **WhatsApp integration** — existing stubs remain. No new work until explicitly reactivated.
- **Epic 1.12 (Monte Carlo)** — built on `simulate.*` operators (Sprint 3) + needs wired frontend first
- **Blockchain layer** — mocked, correct, no action until Phase 3
- **Mkulima** — after Biashara is dogfooded

---

## Your Confirmed Patterns

| Pattern | How |
|---------|-----|
| **Commits** | Conventional commits, direct to `main`. `feat(scope):`, `fix(scope):`, `chore(scope):`, `ci:`, `docs:`, `test(scope):` |
| **Tests** | Fix all failures before moving on. `pytest tests/ --tb=short -q`. Core coverage >90% |
| **Branches** | None — direct to main |
| **LLM in dev** | `ANTHROPIC_API_KEY=mock` → MockClaudeClient. Zero spend until prod |
| **Briefs** | Daily, action codes (DEEP/CAL/TODO/SKIP/FOL/ANA/EXP/SUM), sci-fi culture section only |
| **Cadence** | Rapid iteration — multiple epics per session |

---

## Sprint Dependency Map

```
Sprint 0 (pre-flight)
    ↓
Sprint 1 (web UI wiring)
    ↓
Sprint 2 (UIParser)
    ↓
Sprint 3 (operators + protocols)
    ↓
Sprint 4 (operator UIs)   ←─── Sprint 3
    ↓                          
Sprint 5 (operative networks) ←─── Sprint 3 (all operator types needed)
    ↓
Sprint 6 (today list + morning brief) ←─── Sprint 5
    ↓
Sprint 7 (council enrichment) ←─── Sprint 5 + Sprint 3
    ↓
Sprint 8 (Lore) — can run in parallel with Sprint 7
Sprint 9 (GitHub hygiene) — can run after Sprint 0
```

---

*Updated 1 June 2026 — integrates: UIParser, operators+protocols, operative networks (LLM-optional graph layer), today list + morning brief, enriched Council with sub-operative delegation + sandbox simulation. WhatsApp development suspended.*
