# Bonnie — Technical Master Roadmap
### Sustena XII Platform Build — Phase 0 through Phase 3
**Personal document — Bonventure Gachiengu**
**Version 1.0 — May 2026**
**Cross-reference:** Sustena_XII_Master_Strategy.md, Sustena_DSL_Dot_Protocol.md

---

## How to Use This Roadmap

This is your personal task sequence as lead architect and principal engineer. Each task has:
- A checkbox `[ ]` (mark `[x]` when done)
- A **priority** (🔴 critical path | 🟡 important | 🟢 nice-to-have)
- A **Claude prompt** labelled by tool: `[CODE]`, `[DISPATCH]`, or `[DESIGN]`
- Subtasks broken to the function/file level where precision matters

**Ground rule:** If a task has a Claude Code prompt, run it. Do not improvise the architecture from memory — always run the prompt and refine the output. Your job is to review, correct, and integrate — not to write boilerplate from scratch.

---

## Phase 0 — Foundation (Days 1–7)

### Epic 0.1 — Repository and Environment Setup

- [ ] 🔴 **0.1.1** Create the fresh Sustena XII repository
  - [ ] `git init sustena-xii` (local) + push to `https://github.com/bonniegachiengu/sustena.git`
  - [ ] Create branch structure: `main` (production), `dev` (active development), `feat/*` (feature branches)
  - [ ] Add `.gitignore`: Python (`__pycache__`, `*.pyc`, `.env`, `venv/`), SQLite (`*.db`), secrets
  - [ ] Add `README.md` skeleton (project purpose, setup instructions, architecture pointer)

  > **[CODE]** "Initialise a Python FastAPI project called `sustena-xii`. Create the directory structure: `sustena/core/` (primitives), `sustena/operators/` (operator registry), `sustena/operatives/` (operative templates), `sustena/sustains/` (sustain specs), `sustena/api/` (FastAPI routes), `sustena/db/` (SQLite schema and migrations), `sustena/tests/`. Add a `pyproject.toml` with dependencies: fastapi, uvicorn, pydantic, anthropic, sqlalchemy, aiosqlite, python-dotenv, pytest, httpx. Add a `Makefile` with targets: `run`, `test`, `migrate`, `lint`."

- [ ] 🔴 **0.1.2** Configure environment variables
  - [ ] Create `.env.example` with: `ANTHROPIC_API_KEY`, `DATABASE_URL`, `WHATSAPP_TOKEN`, `WHATSAPP_PHONE_ID`, `SECRET_KEY`, `ENVIRONMENT` (development/production)
  - [ ] Create `sustena/config.py` — loads `.env` via `python-dotenv`; validates required vars; exposes typed config object

  > **[CODE]** "Create `sustena/config.py` that loads environment variables using `python-dotenv` and exposes a `Settings` class using Pydantic `BaseSettings`. Required fields: `ANTHROPIC_API_KEY` (str), `DATABASE_URL` (str, default sqlite+aiosqlite:///./sustena.db), `WHATSAPP_TOKEN` (str), `WHATSAPP_PHONE_ID` (str), `SECRET_KEY` (str), `ENVIRONMENT` (Literal['development', 'production'], default 'development'), `CLAUDE_HAIKU_MODEL` (str, default 'claude-haiku-4-5-20251001'), `CLAUDE_SONNET_MODEL` (str, default 'claude-sonnet-4-6'). Raise clear error on missing required vars."

- [ ] 🔴 **0.1.3** SQLite database schema — v1
  - [ ] Tables: `sustains`, `states`, `operators_log`, `events`, `pawa_ledger`, `users`, `operatives`, `council_proposals`, `council_votes`
  - [ ] Use SQLAlchemy Core (not ORM) — explicit table definitions; async engine

  > **[CODE]** "Create `sustena/db/schema.py` using SQLAlchemy Core with async SQLite. Define the following tables: `users` (id, phone_number, created_at, pawa_balance), `sustains` (id, user_id, sustain_type, name, version, created_at), `sustain_states` (id, sustain_id, state_json, updated_at, version_number), `operators_log` (id, sustain_id, operative_id, operator_name, input_json, output_json, status, pawa_cost, timestamp), `events` (id, sustain_id, event_name, payload_json, timestamp), `pawa_ledger` (id, user_id, sustain_id, delta, reason, balance_after, timestamp), `council_proposals` (id, sustain_id, proposed_by, operator_name, input_json, status, created_at, resolved_at), `council_votes` (id, proposal_id, operative_id, vote, reasoning, timestamp). Include SQLAlchemy migration helper using Alembic."

---

### Epic 0.2 — Core Primitives Layer

This is the most important code in the entire platform. Get it right before writing any operator.

- [ ] 🔴 **0.2.1** `StateAccessor` — the state read/write primitive

  > **[CODE]** "Implement `sustena/core/state.py`. The `StateAccessor` class wraps a JSON dict (the sustain's state) and provides: `get(path: str) -> Any` (dot-path traversal, raises `StatePathError` if path doesn't exist), `set(path: str, value: Any) -> None`, `increment(path: str, delta: float) -> float` (atomic; validates path resolves to numeric), `decrement(path: str, delta: float) -> float` (validates result >= 0 by default; accepts `allow_negative=True` flag), `append(path: str, item: dict) -> str` (appends to list at path; returns generated UUID for item['id']), `remove(path: str, item_id: str) -> None` (removes by item['id']), `exists(path: str) -> bool`, `snapshot() -> dict` (deep copy; read-only). Paths use dot notation (e.g., 'finances.liquid.balance'). Array indexing via brackets: 'staff.roster[0].name'. Include comprehensive unit tests in `tests/test_state.py`."

- [ ] 🔴 **0.2.2** `ConstraintEngine` — the predicate evaluator

  > **[CODE]** "Implement `sustena/core/constraints.py`. The `ConstraintEngine` class evaluates constraint predicate strings as defined in the Sustena DSL. It must NOT use Python `eval()`. Instead, implement a small recursive descent parser that handles: comparison operators (>, >=, <, <=, ==, !=), membership operators (IN, NOT IN), logical operators (AND, OR, NOT), quantifiers (ALL [path].field op value, EXISTS [path].field op value), temporal constraints (WITHIN n hours|days constraint_expr). The engine takes a `StateAccessor`, a dict of operator call parameters, and the constraint string. Returns `(bool, str)` — result and human-readable reason for failure. Include 20+ test cases covering each operator type, including edge cases (null values, empty arrays, nested paths). Tests in `tests/test_constraints.py`."

- [ ] 🔴 **0.2.3** `EventBus` — the event publishing primitive

  > **[CODE]** "Implement `sustena/core/events.py`. The `EventBus` class handles event publishing within a sustain's execution context. Methods: `publish(name: str, payload: dict, timestamp: datetime) -> str` (returns event_id; writes to SQLite `events` table asynchronously; validates event name follows dot-protocol format `event.domain.type`), `subscribe(event_name: str, handler_fn: Callable) -> None` (registers in-memory handler; called synchronously during same execution context), `get_history(sustain_id: str, event_name: str | None, limit: int) -> list[dict]` (queries event log). Event name validation: must start with 'event.', contain at least 3 dot-separated segments, use lowercase snake_case only."

- [ ] 🔴 **0.2.4** `PawaLedger` — token accounting

  > **[CODE]** "Implement `sustena/core/pawa.py`. The `PawaLedger` class manages pawa token balances. Phase 1 implementation: SQLite-backed (no blockchain). Methods: `get_balance(user_id: str) -> int`, `deduct(user_id: str, sustain_id: str, amount: int, reason: str) -> bool` (returns False and does NOT deduct if balance insufficient; atomic), `credit(user_id: str, sustain_id: str, amount: int, reason: str) -> int` (returns new balance), `transfer(from_user: str, to_user: str, amount: int, reason: str) -> bool`, `get_history(user_id: str, limit: int) -> list[dict]`. All operations write to `pawa_ledger` table. Include a `ComponentLicense` class that takes (component_id, contributor_id, license_tier, pawa_cost) and implements a `charge(caller_user_id: str, ledger: PawaLedger) -> dict` method that splits: contributor 70%, network treasury 20%, referrer 5%, validator 5%. Tests in `tests/test_pawa.py`."

  **Note on web3.py mapping (Phase 3):** The `PawaLedger` class will be replaced by a web3.py wrapper in Phase 3. The mapping is:
  - `deduct()` → `contract.functions.transfer(TREASURY_ADDR, amount).transact({"from": user_wallet})`
  - `credit()` → `contract.functions.mint(user_wallet, amount).transact({"from": MINTER_ADDR})`
  - `get_balance()` → `contract.functions.balanceOf(user_wallet).call()`
  - `ComponentLicense.charge()` → `contract.functions.chargeRoyalty(component_id, caller_addr).transact()`
  - The split percentages become Solidity mapping: `royaltySplits[componentId] = [contributor: 70, treasury: 20, referrer: 5, validator: 5]`
  - All Phase 1 SQLite writes produce identical logs to what the blockchain would record — making the Phase 3 migration a drop-in replacement, not a rewrite

- [ ] 🔴 **0.2.5** `OperatorContext` and `OperatorResult`

  > **[CODE]** "Implement `sustena/core/operator.py`. Define: `OperatorContext` dataclass with fields: `state: StateAccessor`, `timestamp: datetime`, `sustain_id: str`, `operative_id: str | None`, `user_id: str`, `pawa_ledger: PawaLedger`, `event_bus: EventBus`, `logger: logging.Logger`. Define `OperatorResult` with class methods: `ok(data: dict) -> OperatorResult`, `fail(reason: str, constraint_violated: str | None) -> OperatorResult`, `deferred(proposal_id: str) -> OperatorResult` (for operators that require Council approval). `OperatorResult.to_response() -> dict` returns serialisable dict. Define the `@sustena_operator` decorator that registers the function in a global `OPERATOR_REGISTRY: dict[str, OperatorMeta]` and validates that all metadata fields are present."

- [ ] 🔴 **0.2.6** Write integration test: full operator execution cycle

  > **[CODE]** "Write an integration test in `tests/test_operator_cycle.py`. The test should: (1) Create an in-memory SQLite database with the full schema. (2) Create a sustain with a simple state: `{'finances': {'liquid': {'balance': 50000}}, 'finances.pockets': {'food': {'allocated': 0, 'spent': 0}}}`. (3) Register a mock `budget.allocate` operator using `@sustena_operator`. (4) Execute the operator via the operator registry. (5) Assert state mutations applied correctly. (6) Assert event published. (7) Assert pawa deducted. (8) Attempt execution with a failing constraint and assert OperatorResult.fail() returned. (9) Assert state unchanged after failed execution."

---

### Epic 0.3 — WhatsApp Validation Gateway

- [ ] 🔴 **0.3.1** WhatsApp webhook endpoint (FastAPI)

  > **[CODE]** "Create `sustena/api/whatsapp.py`. Implement two FastAPI endpoints: (1) `GET /webhook` — Meta webhook verification (checks hub.verify_token against env var, returns hub.challenge). (2) `POST /webhook` — Receives incoming WhatsApp messages. Parse the Meta payload to extract: sender phone number, message body, message timestamp, message type (text/interactive/button_reply). Route to `sustena/core/whatsapp_handler.py`. Return 200 OK immediately (WhatsApp requires sub-2s response). Process asynchronously. Include request signature verification using X-Hub-Signature-256 header."

- [ ] 🔴 **0.3.2** WhatsApp message sender

  > **[CODE]** "Create `sustena/core/whatsapp_sender.py`. Implement a `WhatsAppSender` class with methods: `send_text(to: str, message: str) -> bool`, `send_interactive_buttons(to: str, body: str, buttons: list[dict]) -> bool` (each button: {'id': str, 'title': str, max 3 buttons per message}, `send_list_message(to: str, header: str, body: str, footer: str, sections: list[dict]) -> bool`, `send_template(to: str, template_name: str, params: list[str]) -> bool`. All methods call Meta Graph API v20.0. Include retry logic (3 attempts, exponential backoff). Log all sends to operators_log."

- [ ] 🔴 **0.3.3** Initial user onboarding flow (WhatsApp only, no sustain yet)

  > **[CODE]** "Create `sustena/core/onboarding.py`. Implement a state-machine-based onboarding flow for new WhatsApp users. States: WELCOMED → COLLECTED_NAME → COLLECTED_INCOME → COLLECTED_GOAL → SUSTAIN_CREATED. The flow asks: (1) 'Habari! Mimi ni Orchie, msaidizi wako wa fedha. Jina lako ni nani?' (2) 'Mapato yako ya kila mwezi ni kiasi gani? (KES)' (3) 'Lengo lako kuu la fedha ni nani? (e.g., kulipa deni, kujenga akiba, kukua biashara)'. After 3 inputs, create a Homestead sustain spec with the user's data and persist to SQLite. Send confirmation with their opening balance ring summary."

---

## Phase 1 — Foundation Build (Weeks 1–10)

### Epic 1.1 — Operator Library (Core Operators)

- [ ] 🔴 **1.1.1** Budget domain operators

  > **[CODE]** "Implement the following operators in `sustena/operators/budget.py` using the `@sustena_operator` decorator: `budget.record_income(source: str, amount: float, period: str)` — credits liquid balance, emits `event.finances.income_received`. `budget.allocate(pocket_name: str, amount: float, period: str)` — deducts from liquid, allocates to pocket; constraints: amount > 0, amount <= liquid balance. `budget.spend(pocket_name: str, amount: float, description: str, category: str)` — deducts from pocket.allocated, increments pocket.spent; constraint: amount <= pocket.allocated. `budget.transfer(from_pocket: str, to_pocket: str, amount: float)` — moves between pockets. `budget.summary() -> ResponseWidget` — returns budget_ring widget populated from state. Each operator must include: correct `side_effects` declaration, constraint list, license_tier='free', pawa_cost=0, author='sustena_core', ui_schema where relevant."

- [ ] 🔴 **1.1.2** Chama domain operators

  > **[CODE]** "Implement `sustena/operators/chama.py`. Operators: `chama.contribution.record(member_id: str, amount: float, period: str)` — records monthly contribution; increments member balance; checks amount matches rules.contribution_amount. `chama.loan.request(member_id: str, amount: float, purpose: str)` — creates loan proposal; sends to Council. `chama.loan.disburse(loan_id: str)` — disburses approved loan; requires Council PASSED status. `chama.loan.repay(loan_id: str, amount: float)` — records repayment; updates loan balance. `chama.fine.record(member_id: str, amount: float, reason: str)` — adds fine; constraint: reason IN rules.fine_reasons. `chama.meeting.schedule(date: str, agenda: str)` — adds meeting to calendar state. `chama.dividend.calculate()` — reads all member balances, computes dividend split, returns ResponseWidget with breakdown."

- [ ] 🟡 **1.1.3** Procurement domain operators (cross-sustain)

  > **[CODE]** "Implement `sustena/operators/procurement.py`. Operators: `mkulima.broadcast_supply_signal(produce: str, quantity_kg: float, price_per_kg: float, harvest_window_days: int)` — validates produce exists in inventory, publishes signal to Mycelium event bus (Phase 1: writes to shared SQLite events table with destination='mycelium.public'). `mkulima.receive_signal(signal_id: str, produce: str, quantity_kg: float, price_per_kg: float, farm_id: str, harvest_window_days: int)` — appends to procurement.mkulima_supply_signals; triggers Biashara operative evaluation. `procurement.raise_po(signal_id: str, quantity_kg: float, total_kes: float)` — requires Council proposal status == PASSED; creates PO in state; emits event.procurement.po_raised. `procurement.confirm_delivery(po_id: str, actual_quantity_kg: float)` — marks PO delivered; triggers payment scheduling."

---

### Epic 1.2 — Operative Runtime

- [ ] 🔴 **1.2.1** Operative base class and LLM call wrapper

  > **[CODE]** "Create `sustena/operatives/base.py`. Define `BaseOperative` abstract class with: `__init__(self, config: dict, state_accessor: StateAccessor, claude_client: anthropic.Anthropic)`. Abstract methods: `deliberate(context: str, proposal: dict) -> OperativeVote` (returns YES/NO/ABSTAIN + reasoning), `evaluate(trigger_event: dict) -> OperativeProposal | None` (evaluates whether to propose action). Implement `_call_claude(system_prompt: str, user_message: str, model: str = HAIKU) -> str` — wraps `claude_client.messages.create()` with retry logic, token counting, and cost logging to `operators_log`. Add `_build_state_context(paths: list[str]) -> str` — reads specified state paths and formats them as a readable context string for the LLM prompt."

- [ ] 🔴 **1.2.2** Mentor Operative — Budget Watchdog

  > **[CODE]** "Implement `sustena/operatives/mentor.py`. `MentorOperative(BaseOperative)` specialises in budget analysis. System prompt: include the user's income, pocket allocations, and spending history from state. `evaluate()` method: triggered by `event.finances.pocket_spent`; checks if any pocket is >80% spent; if yes, proposes `budget.reallocate` or generates alert. `deliberate()` for council proposals: vote YES if proposal improves budget position or is within constraints; vote NO if it would push liquid balance below 10% of income. Implement `HOESuboperative` as a nested class: `onboard(mpesa_message: str) -> dict` — parses an M-Pesa SMS message (regex-based + Claude Haiku classification) to extract: transaction type, amount, counterparty, date. `ask_clarifying_question(history: list[dict]) -> str | None` — returns one question per day for 7 days based on parsed history gaps; returns None after day 7."

- [ ] 🔴 **1.2.3** Council DAO runtime

  > **[CODE]** "Create `sustena/core/council.py`. Implement `CouncilSession` class. `create_proposal(sustain_id: str, proposed_by: str, operator_name: str, input_params: dict, simulation_results: dict) -> str` — writes to `council_proposals` table; returns proposal_id. `collect_votes(proposal_id: str, operatives: list[BaseOperative]) -> dict` — calls `deliberate()` on each operative; records each vote to `council_votes` table. `resolve(proposal_id: str, user_vote: str | None) -> str` — applies Nash bargaining: if 2+ YES and user hasn't voted NO → status = 'IN_VOTING'; notify user. If user votes YES → status = 'PASSED'. If user votes NO → status = 'OVERRIDDEN_BY_USER'. If no user response in 48h → status = 'DEFERRED'. Returns final status. `nash_utility_score(votes: list[dict]) -> float` — computes geometric mean of operative utility improvements per Nash bargaining solution."

---

### Epic 1.3 — Homestead Sustain (First Complete Sustain)

- [ ] 🔴 **1.3.1** Homestead sustain spec (JSON)

  > **[CODE]** "Create `sustena/sustains/homestead.json`. The complete Homestead Sustain spec following the format in Appendix A of the master strategy. State schema: members (Colo-style generic placeholders), finances (liquid, pockets, income, goals), calendar (events array), alerts (array). Operators: all budget.* operators, homestead.calendar.add_event, homestead.calendar.upcoming_events. Operatives: Mentor, Protégé. Invariants: finances.liquid.balance >= 0, ALL [finances.pockets].allocated >= 0. UI schema: sustain_home widget. Access policy: owner_ids template."

- [ ] 🔴 **1.3.2** Sustain instantiation engine

  > **[CODE]** "Create `sustena/core/sustain_engine.py`. Implement `SustainEngine` class. `instantiate(template_id: str, user_id: str, parameters: dict) -> str` — loads sustain JSON template, resolves all {{placeholder}} tokens with `parameters` dict, validates all required parameters are provided, writes to `sustains` and `sustain_states` tables, initialises operative instances. `execute_operator(sustain_id: str, operator_name: str, params: dict, operative_id: str | None) -> OperatorResult` — loads sustain state, validates operator exists in sustain spec, runs constraint engine, executes operator function from OPERATOR_REGISTRY, persists state mutations, returns result. `get_state(sustain_id: str) -> dict` — returns current state JSON. `simulate(sustain_id: str, operator_sequence: list[dict]) -> list[dict]` — runs operator sequence on a state fork without persisting (for Council pre-deliberation)."

---

### Epic 1.4 — API Layer

- [ ] 🔴 **1.4.1** FastAPI application structure

  > **[CODE]** "Create `sustena/api/main.py`. Set up FastAPI application with: CORS middleware (restrict to known origins in production), request logging middleware (logs method, path, response time, status), health check endpoint `GET /health` (returns version, db status, claude status). Mount routers: `/api/v1/sustains`, `/api/v1/operators`, `/api/v1/council`, `/api/v1/users`, `/webhook` (WhatsApp). Use dependency injection for database sessions and claude client. Add OpenAPI docs (enabled in development only, disabled in production)."

- [ ] 🔴 **1.4.2** Sustain API routes

  > **[CODE]** "Create `sustena/api/routes/sustains.py`. Endpoints: `POST /sustains` — instantiate new sustain from template (body: {template_id, name, parameters}); `GET /sustains/{sustain_id}` — get sustain metadata and current state; `POST /sustains/{sustain_id}/operators/{operator_name}` — execute an operator on a sustain (body: operator params); `GET /sustains/{sustain_id}/events` — get event history; `GET /sustains/{sustain_id}/proposals` — get Council proposals and their status; `POST /sustains/{sustain_id}/proposals/{proposal_id}/vote` — user casts their 51% vote (body: {vote: 'YES' | 'NO'}). All endpoints require auth (JWT from user session). Return standardised response envelope: {status, data, error, timestamp}."

---

### Epic 1.5 — Vyyb Biashara Sustain

- [ ] 🟡 **1.5.1** Vyyb state schema migration from old SQLite

  > **[CODE]** "Create `sustena/sustains/vyyb_biashara.json` — the Vyyb Biashara Sustain spec following Section 2.1 of the Vyyb OS Dev Roadmap. Include all state domains: inventory, production, orders, recipes, accounts, assets, staff, tax, procurement, analytics. Include the `vyyb.*` operator namespace references. Create `sustena/scripts/migrate_vyyb.py` — reads from the old Vyyb OS SQLite database (passed as --source-db argument) and writes seed data into the new Sustena XII SQLite as a Vyyb Biashara sustain instance. Map: old `products` table → new `state.inventory.items`. Old `recipes` table → new `state.recipes.library` (Vyyb IP, not published to Mycelium). Old `orders` table → new `state.orders.order_history`. Old `accounts`/`journal_entries` → new `state.accounts`."

- [ ] 🟡 **1.5.2** `vyyb.*` operator implementations

  > **[CODE]** "Implement `sustena/operators/vyyb.py`. Operators: `vyyb.inventory.restock(item_id: str, quantity: float, unit_cost: float, supplier: str)` — updates inventory item, creates journal entry (debit: inventory, credit: cash/payable). `vyyb.production.start_batch(recipe_id: str, quantity_units: int, outlet_id: str)` — validates ingredients available, deducts from inventory, creates batch record. `vyyb.production.complete_batch(batch_id: str, actual_yield: int)` — closes batch, records yield variance. `vyyb.orders.place(items: list[dict], outlet_id: str, customer_ref: str | None)` — validates items exist, calculates total + VAT, creates order. `vyyb.orders.fulfill(order_id: str)` — marks fulfilled, records to daily_revenue, deducts from production batches. `vyyb.tax.calculate_vat(period: str)` — sums VAT collected, computes payable, emits event. All operators follow vyyb.* namespace convention."

---

## Phase 2 — Platformisation (Months 3–6)

### Epic 2.1 — Mycelium Control Panel (Developer UI)

- [ ] 🟡 **2.1.1** DAG graph editor (React + ReactFlow)

  > **[DESIGN]** "Design the Sustena Developer UI Editor Panel following The Expanse ship operations aesthetic. The panel shows a DAG of a sustain's operators and operatives as nodes, connected by event edges (event published → operator triggered). Node types: operator (rectangular, blue-border), operative (hexagonal, amber-border), event (circular, green). Edges: solid for direct calls, dashed for event subscriptions. Controls: drag to add operator node from left panel; double-click to edit operator config; right-click for context menu (simulate, view source, delete). Dark background (#1a1a2e), high-contrast data labels, Expanse-style panel borders. Mock 3 screens: empty canvas, populated Homestead DAG, populated Vyyb Biashara DAG with cross-sustain Mkulima edge."

  > **[CODE]** "Create `frontend/src/components/DeveloperUI/EditorPanel.jsx`. Use ReactFlow for the DAG canvas. Node types: OperatorNode (shows dot-name, pawa_cost, constraint count), OperativeNode (shows operative name, temperament setting, event subscription count), EventNode (shows event dot-name, subscriber count). Custom edge: EventEdge (dashed animated line with event name label). Left panel: searchable operator registry (fetched from GET /api/v1/operators). Drag-and-drop operator from registry to canvas. On drop: POST /api/v1/sustains/{id}/operators to add to sustain spec. Persist canvas layout (node positions) to localStorage keyed by sustain_id."

- [ ] 🟡 **2.1.2** Simulator Panel (Monte Carlo)

  > **[CODE]** "Create `sustena/core/simulator.py`. Implement `MonteCarloSimulator` class. `fork_state(sustain_id: str) -> dict` — deep copies current sustain state (no persistence). `simulate_path(forked_state: dict, operator_sequence: list[dict], n_iterations: int = 1000) -> SimulationResult` — runs each operator in sequence on the forked state n times with randomised inputs within the operator's input schema bounds; records final state distribution. `enumerate_paths(forked_state: dict, max_depth: int = 5) -> list[Path]` — generates all valid operator sequences up to depth, respecting constraints. `score_path(path: Path, goal_state: dict) -> float` — implements the Feynman Path Integral analogue: P(strategy_k | state_0, constraints, goal) ∝ exp(-C[strategy_k] / T), where C is the path cost (pawa spent + constraint violations) and T is the temperature parameter (exploration factor). Return top-K paths by score."

---

### Epic 2.2 — React Native Mobile App

- [ ] 🟡 **2.2.1** App architecture setup

  > **[CODE]** "Set up a React Native Expo project called `sustena-mobile`. Configure: Expo Router for file-based navigation, NativeWind for Tailwind-compatible styling, react-native-async-storage for local state, axios for API calls with JWT interceptor, react-native-reanimated for animations. Screen structure: `app/(auth)/login.tsx` (phone number + OTP), `app/(app)/(tabs)/home.tsx` (sustain home widget), `app/(app)/(tabs)/orchie.tsx` (chat interface), `app/(app)/(tabs)/council.tsx` (pending proposals), `app/(app)/(tabs)/arena.tsx` (Mycelium Arena). Configure Expo EAS for build pipeline. Dark theme matching The Expanse / Sustena design system."

- [ ] 🟡 **2.2.2** Orchie chat interface

  > **[DESIGN]** "Design the Orchie mobile chat interface. The screen shows a conversation history (Orchie messages + user replies) at the top, with a text input at the bottom. Orchie messages render ResponseWidgets inline — a budget ring, a council proposal card, a supply signal summary — not just text. Widget cards have a dark background, amber-border accent, data-forward layout (numbers large, labels small). User messages are right-aligned, muted blue-grey. The input area has: text field (primary), quick action buttons (👍 Approve, 🗳️ Vote, 📊 View Details). Notification badge on Orchie tab when Council proposals are pending. Design for dark mode only."

---

### Epic 2.3 — web3.py Integration Preparation (Phase 3 Groundwork)

This is done in Phase 2 so the Phase 3 migration is mechanical, not architectural.

- [ ] 🟢 **2.3.1** Map PawaLedger → ERC-20 contract interface

  > **[CODE]** "Create `sustena/core/pawa_web3.py`. Implement `PawaLedgerWeb3` as a drop-in replacement for `PawaLedger` that uses web3.py instead of SQLite. The interface must be identical: same method signatures, same return types. Implementation: `web3 = Web3(Web3.HTTPProvider(os.getenv('ETH_RPC_URL')))`. Connect to a deployed ERC-20 contract at `PAWA_CONTRACT_ADDRESS`. `get_balance(user_id: str) -> int` → maps user_id to wallet address via `users` table → `contract.functions.balanceOf(wallet_addr).call()`. `deduct(user_id, sustain_id, amount, reason)` → `contract.functions.transfer(TREASURY_ADDR, amount).build_transaction({...})` → sign with user's private key (Phase 3: MPC wallet; Phase 2: test with hardcoded dev key). `credit(user_id, sustain_id, amount, reason)` → `contract.functions.mint(wallet_addr, amount).transact({"from": MINTER_ADDR})`. Include unit tests using pytest-web3 with a local Ganache or Anvil fork. Document the exact Solidity ABI functions this Python code expects."

- [ ] 🟢 **2.3.2** ERC-20 pawa token Solidity stub

  > **[CODE]** "Write `contracts/PawaToken.sol`. This is a standard OpenZeppelin ERC-20 contract with: `mint(address to, uint256 amount) onlyMinter` — restricted to platform minter address. `burn(address from, uint256 amount) onlyMinter` — for pawa consumption events. `chargeRoyalty(bytes32 componentId, address caller, uint256 amount)` — reads royalty splits from `royaltySplits[componentId]` mapping; distributes: 70% to contributor address, 20% to treasury, 5% to referrer, 5% to validator. `royaltySplits` mapping: `bytes32 => RoyaltySplit` struct. Constructor sets MINTER_ADDR from deployment param. Include NatDoc comments. Do NOT deploy — this is Phase 3. Write Hardhat tests for chargeRoyalty distribution arithmetic."

---

## Phase 3 — Decentralisation (Month 12+)

### Epic 3.1 — Mycelium Live Network

- [ ] 🟢 **3.1.1** Smart contract deployment (Ethereum L2)

  - [ ] Choose L2: Optimism vs. Base vs. Arbitrum (evaluate gas costs for chargeRoyalty at 10,000 calls/day)
  - [ ] Audit `PawaToken.sol` with Trail of Bits or equivalent ($5,000–15,000)
  - [ ] Deploy to Sepolia testnet first; run 30-day production simulation
  - [ ] Deploy to mainnet L2 after audit sign-off

  > **[CODE]** "Create `scripts/deploy.py` using web3.py + compiled contract ABI + bytecode. Deploy sequence: (1) Deploy PawaToken with MINTER_ADDR = platform multisig. (2) Deploy RoyaltyRegistry — maps componentId → contributor address + license tier. (3) Deploy MyceliumIndex — on-chain registry of published operator/operative hashes (IPFS CID → componentId mapping). Return deployed addresses, write to `contracts/deployed_addresses.json`. Verify on Etherscan (use etherscan-python library)."

- [ ] 🟢 **3.1.2** STC token design and governance

  - [ ] Define STC as ERC-20 with governance extension (OpenZeppelin `ERC20Votes`)
  - [ ] Define pawa → STC conversion function: `f(accumulated_pawa, network_maturity_factor)`
  - [ ] Write DAO governance contract: proposals, voting, quorum, timelock
  - [ ] Legal opinion: CMA Sandbox application

---

## Appendix — Claude Code Cheat Sheet

### When to use which Claude tool:

| Task | Tool | Why |
|---|---|---|
| Write Python functions, tests, APIs | Claude Code | Direct file creation + iteration |
| Write Solidity, SQL migrations | Claude Code | Domain-specific code |
| Design UI screens (mockups) | Claude Design | Visual output, not code |
| Explore architecture options | Claude Dispatch | Multi-step thinking |
| Debug a specific error | Claude Code | Has access to file context |
| Write long documents (this roadmap, strategy) | Claude Dispatch | Better long-form |
| Set up new environments | Claude Code | Shell + file access |

### Prompt patterns that work well:

**For operator implementations:**
> "Implement the `[operator_name]` operator in `sustena/operators/[domain].py` following the `@sustena_operator` decorator pattern. The operator must: (list behaviours). Include these constraints: (list). Write unit tests in `tests/test_[domain]_operators.py` covering: (1) happy path, (2) constraint violation, (3) state mutation assertions, (4) event published."

**For state path debugging:**
> "I have a StateAccessor with this state: [paste state JSON]. The path `[path]` is returning `StatePathError`. What is wrong with my path, and what is the correct path to [describe what you want to access]?"

**For constraint DSL questions:**
> "Write a Sustena constraint predicate that expresses: [describe the business rule in plain English]. The relevant state paths are: [list paths]. The operator input params are: [list params]."

---

*End of Bonnie Master Roadmap v1.0*
*Next update: add Phase 2 subtasks for IoT integration (MQTT + Raspberry Pi) after Mkulima operative Phase 1 is complete.*
