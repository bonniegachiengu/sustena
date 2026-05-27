# Bonnie — Technical & Operational Master Roadmap
### Sustena XII Platform Build — Phase 0 through Phase 3
**Personal document — Bonventure Gachiengu, Lead Architect & Principal Engineer**
**Version 2.0 — May 2026**
**Cross-reference:** Sustena_XII_Master_Strategy.md | Sustena_DSL_Dot_Protocol.md | Sustena_Document_Analysis.md | Brian_Master_Roadmap.md | Investor_Challenges.md

---

## How to Use This Roadmap

You are the sole developer and engineer. In an AI-era startup with one technical co-founder, you are simultaneously: systems architect, backend engineer, frontend engineer, DevOps engineer, data engineer, AI engineer, security engineer, content creator, community builder, and media strategist. This roadmap reflects that reality.

Each task has:
- A checkbox `[ ]` (mark `[x]` when done)
- A **priority** (🔴 critical path | 🟡 important | 🟢 nice-to-have)
- A **Claude prompt** labelled by tool: `[CODE]`, `[DISPATCH]`, or `[DESIGN]`
- Subtasks broken to the function/file level where precision matters
- **Test criteria** at the end of each milestone epic — mark these with `[TEST]`

**Ground rules:**
1. If a task has a Claude Code prompt, run it. Do not improvise the architecture from memory — always run the prompt and refine the output. Your job is to review, correct, and integrate — not to write boilerplate from scratch.
2. After every milestone, run the test criteria before moving on. A milestone is not complete until its tests pass.
3. You own customer support, the blog, Substack, documentation, and media. Brian owns investor relations, partnerships, legal logistics, and business development content. These are not overlapping lanes.
4. Always check this document, the Master Strategy, and the Document Analysis when something feels unclear — the answer is probably already there.

---

## Phase 0 — Foundation (Days 1–14)

### Epic 0.1 — Repository and Environment Setup

- [ ] 🔴 **0.1.1** Create the fresh Sustena XII repository
  - [ ] `git init sustena-xii` (local) + push to `https://github.com/bonniegachiengu/sustena.git`
  - [ ] Create branch structure: `main` (production), `dev` (active development), `feat/*` (feature branches)
  - [ ] Add `.gitignore`: Python (`__pycache__`, `*.pyc`, `.env`, `venv/`), SQLite (`*.db`), secrets
  - [ ] Add `README.md` skeleton: project purpose, architecture overview, setup instructions, pointer to ARCHITECTURE.md
  - [ ] Add `CONTRIBUTING.md`: contribution guidelines, PR format, code review standards — establish this from Day 1 even when you are the only contributor

  > **[CODE]** "Initialise a Python FastAPI project called `sustena-xii`. Create the directory structure: `sustena/core/` (primitives), `sustena/operators/` (operator registry), `sustena/operatives/` (operative templates), `sustena/sustains/` (sustain specs), `sustena/api/` (FastAPI routes), `sustena/db/` (SQLite schema and migrations), `sustena/tests/`. Add a `pyproject.toml` with dependencies: fastapi, uvicorn, pydantic, anthropic, sqlalchemy, aiosqlite, python-dotenv, pytest, httpx. Add a `Makefile` with targets: `run`, `test`, `migrate`, `lint`."

- [ ] 🔴 **0.1.2** Configure environment variables
  - [ ] Create `.env.example`: `ANTHROPIC_API_KEY`, `DATABASE_URL`, `WHATSAPP_TOKEN`, `WHATSAPP_PHONE_ID`, `SECRET_KEY`, `ENVIRONMENT`
  - [ ] Create `sustena/config.py` — loads `.env` via `python-dotenv`; validates required vars; exposes typed config object
  - [ ] Add secret rotation notes to `ARCHITECTURE.md` — document that keys rotate monthly and where they're stored

  > **[CODE]** "Create `sustena/config.py` that loads environment variables using `python-dotenv` and exposes a `Settings` class using Pydantic `BaseSettings`. Required fields: `ANTHROPIC_API_KEY` (str), `DATABASE_URL` (str, default sqlite+aiosqlite:///./sustena.db), `WHATSAPP_TOKEN` (str), `WHATSAPP_PHONE_ID` (str), `SECRET_KEY` (str), `ENVIRONMENT` (Literal['development', 'production'], default 'development'), `CLAUDE_HAIKU_MODEL` (str, default 'claude-haiku-4-5-20251001'), `CLAUDE_SONNET_MODEL` (str, default 'claude-sonnet-4-6'). Raise clear error on missing required vars."

- [ ] 🔴 **0.1.3** SQLite database schema — v1
  - [ ] Tables: `sustains`, `states`, `operators_log`, `events`, `pawa_ledger`, `users`, `operatives`, `council_proposals`, `council_votes`
  - [ ] Use SQLAlchemy Core (not ORM) — explicit table definitions; async engine

  > **[CODE]** "Create `sustena/db/schema.py` using SQLAlchemy Core with async SQLite. Define tables: `users` (id, phone_number, created_at, pawa_balance), `sustains` (id, user_id, sustain_type, name, version, created_at), `sustain_states` (id, sustain_id, state_json, updated_at, version_number), `operators_log` (id, sustain_id, operative_id, operator_name, input_json, output_json, status, pawa_cost, timestamp), `events` (id, sustain_id, event_name, payload_json, timestamp), `pawa_ledger` (id, user_id, sustain_id, delta, reason, balance_after, timestamp), `council_proposals` (id, sustain_id, proposed_by, operator_name, input_json, status, created_at, resolved_at), `council_votes` (id, proposal_id, operative_id, vote, reasoning, timestamp). Include SQLAlchemy migration helper using Alembic."

**[TEST] Epic 0.1 — Milestone Criteria:**
- [ ] `make test` passes with zero errors on a clean clone
- [ ] `make run` starts the FastAPI server without crashing
- [ ] `make migrate` runs Alembic migration and creates all tables without error
- [ ] `git log --oneline` shows at least one commit per epic completed
- [ ] `.env.example` covers every variable referenced in `config.py` — confirm no `KeyError` on startup

---

### Epic 0.2 — Core Primitives Layer

This is the most important code in the entire platform. Get it right before writing any operator.

- [ ] 🔴 **0.2.1** `StateAccessor` — the state read/write primitive

  > **[CODE]** "Implement `sustena/core/state.py`. The `StateAccessor` class wraps a JSON dict (the sustain's state) and provides: `get(path: str) -> Any` (dot-path traversal, raises `StatePathError` if path doesn't exist), `set(path: str, value: Any) -> None`, `increment(path: str, delta: float) -> float` (atomic; validates path resolves to numeric), `decrement(path: str, delta: float) -> float` (validates result >= 0 by default; accepts `allow_negative=True` flag), `append(path: str, item: dict) -> str` (appends to list at path; returns generated UUID for item['id']), `remove(path: str, item_id: str) -> None` (removes by item['id']), `exists(path: str) -> bool`, `snapshot() -> dict` (deep copy; read-only). Paths use dot notation (e.g., 'finances.liquid.balance'). Array indexing via brackets: 'staff.roster[0].name'. Include comprehensive unit tests in `tests/test_state.py`."

- [ ] 🔴 **0.2.2** `ConstraintEngine` — the predicate evaluator

  > **[CODE]** "Implement `sustena/core/constraints.py`. The `ConstraintEngine` evaluates constraint predicate strings as defined in the Sustena DSL. It must NOT use Python `eval()`. Implement a recursive descent parser handling: comparison operators (>, >=, <, <=, ==, !=), membership operators (IN, NOT IN), logical operators (AND, OR, NOT), quantifiers (ALL [path].field op value, EXISTS [path].field op value), temporal constraints (WITHIN n hours|days constraint_expr). Takes a `StateAccessor`, a dict of operator call parameters, and the constraint string. Returns `(bool, str)` — result and human-readable failure reason. Include 20+ test cases in `tests/test_constraints.py`."

- [ ] 🔴 **0.2.3** `EventBus` — event publishing primitive

  > **[CODE]** "Implement `sustena/core/events.py`. The `EventBus` class handles event publishing within a sustain's execution context. Methods: `publish(name: str, payload: dict, timestamp: datetime) -> str` (returns event_id; writes to SQLite `events` table asynchronously; validates event name follows dot-protocol format `event.domain.type`), `subscribe(event_name: str, handler_fn: Callable) -> None`, `get_history(sustain_id: str, event_name: str | None, limit: int) -> list[dict]`. Event name validation: must start with 'event.', contain at least 3 dot-separated segments, use lowercase snake_case only."

- [ ] 🔴 **0.2.4** `PawaLedger` — token accounting

  > **[CODE]** "Implement `sustena/core/pawa.py`. The `PawaLedger` class manages pawa token balances. Phase 1: SQLite-backed. Methods: `get_balance(user_id: str) -> int`, `deduct(user_id, sustain_id, amount, reason) -> bool` (returns False if insufficient; atomic), `credit(user_id, sustain_id, amount, reason) -> int` (returns new balance), `transfer(from_user, to_user, amount, reason) -> bool`, `get_history(user_id, limit) -> list[dict]`. Include a `ComponentLicense` class: `charge(caller_user_id, ledger) -> dict` splits royalties: contributor 70%, network treasury 20%, referrer 5%, validator 5%. Tests in `tests/test_pawa.py`."

  **Web3 Phase 3 mapping note:** `deduct()` → ERC-20 transfer; `credit()` → mint; `get_balance()` → balanceOf; `ComponentLicense.charge()` → `chargeRoyalty()` Solidity function. All Phase 1 SQLite writes produce identical logs to what the blockchain will record — making Phase 3 a drop-in replacement.

- [ ] 🔴 **0.2.5** `OperatorContext` and `OperatorResult`

  > **[CODE]** "Implement `sustena/core/operator.py`. Define `OperatorContext` dataclass: `state: StateAccessor`, `timestamp: datetime`, `sustain_id: str`, `operative_id: str | None`, `user_id: str`, `pawa_ledger: PawaLedger`, `event_bus: EventBus`, `logger: logging.Logger`. Define `OperatorResult` with class methods: `ok(data: dict)`, `fail(reason: str, constraint_violated: str | None)`, `deferred(proposal_id: str)`. `OperatorResult.to_response() -> dict` returns serialisable dict. Define `@sustena_operator` decorator that registers functions in a global `OPERATOR_REGISTRY: dict[str, OperatorMeta]` and validates all metadata fields are present."

- [ ] 🔴 **0.2.6** Write integration test: full operator execution cycle

  > **[CODE]** "Write an integration test in `tests/test_operator_cycle.py`: (1) Create in-memory SQLite with full schema. (2) Create a sustain with state: `{'finances': {'liquid': {'balance': 50000}}, 'finances.pockets': {'food': {'allocated': 0, 'spent': 0}}}`. (3) Register mock `budget.allocate` operator using `@sustena_operator`. (4) Execute via operator registry. (5) Assert state mutations applied. (6) Assert event published. (7) Assert pawa deducted. (8) Attempt with failing constraint — assert `OperatorResult.fail()` returned. (9) Assert state unchanged after failure."

**[TEST] Epic 0.2 — Milestone Criteria:**
- [ ] `pytest tests/test_state.py` — all 20+ cases pass including edge cases (null values, empty arrays, nested paths)
- [ ] `pytest tests/test_constraints.py` — all 20+ cases pass including temporal constraints
- [ ] `pytest tests/test_pawa.py` — deduct with insufficient balance returns False and makes no state change
- [ ] `pytest tests/test_operator_cycle.py` — state unchanged after failed constraint; event fired after success
- [ ] Run `pytest --cov=sustena/core` — core coverage > 90%

---

### Epic 0.3 — WhatsApp Validation Gateway

- [ ] 🔴 **0.3.1** WhatsApp webhook endpoint (FastAPI)

  > **[CODE]** "Create `sustena/api/whatsapp.py`. Two FastAPI endpoints: (1) `GET /webhook` — Meta webhook verification (checks hub.verify_token, returns hub.challenge). (2) `POST /webhook` — Receives incoming WhatsApp messages. Parse Meta payload: sender phone, message body, timestamp, message type (text/interactive/button_reply). Route to `sustena/core/whatsapp_handler.py`. Return 200 OK immediately. Process asynchronously. Include X-Hub-Signature-256 request signature verification."

- [ ] 🔴 **0.3.2** WhatsApp message sender

  > **[CODE]** "Create `sustena/core/whatsapp_sender.py`. Implement `WhatsAppSender` with: `send_text(to, message) -> bool`, `send_interactive_buttons(to, body, buttons) -> bool` (max 3 buttons per message), `send_list_message(to, header, body, footer, sections) -> bool`, `send_template(to, template_name, params) -> bool`. All methods call Meta Graph API v20.0. Retry logic: 3 attempts, exponential backoff. Log all sends to `operators_log`."

- [ ] 🔴 **0.3.3** Initial user onboarding flow (WhatsApp only)

  > **[CODE]** "Create `sustena/core/onboarding.py`. State-machine onboarding for new WhatsApp users. States: WELCOMED → COLLECTED_NAME → COLLECTED_INCOME → COLLECTED_GOAL → SUSTAIN_CREATED. Flow asks: (1) name, (2) monthly income (KES), (3) main financial goal. After 3 inputs, create a Homestead sustain spec with user data and persist to SQLite. Send confirmation with opening balance ring summary."

- [ ] 🔴 **0.3.4** Customer support handler
  - [ ] All inbound messages that don't match an operator pattern route to a support queue
  - [ ] Support queue writes to `events` table with event name `event.support.user_message`
  - [ ] You (Bonnie) review the support queue daily — build a simple admin endpoint: `GET /admin/support-queue?date=2026-05-25`
  - [ ] Respond to support queries within 24h; maintain a `support_log.md` documenting recurring issues

  > **[CODE]** "Add `GET /admin/support-queue` endpoint to the FastAPI app. Authentication: Bearer token via env var `ADMIN_TOKEN`. Returns all `event.support.user_message` events from the last 24h (default) or a specified date range (query params: `from_date`, `to_date`). Include: event_id, user phone (masked), message_text, timestamp, has_been_addressed (bool, default false). Add `PATCH /admin/support-queue/{event_id}` to mark as addressed."

**[TEST] Epic 0.3 — Milestone Criteria:**
- [ ] WhatsApp webhook GET verification returns 200 + challenge correctly
- [ ] Webhook POST processes a sample Meta payload without error — check `operators_log` table has an entry
- [ ] Send a message from your test phone number → onboarding flow reaches SUSTAIN_CREATED state
- [ ] Admin support queue returns data; marking addressed works
- [ ] Signature verification rejects a message with a tampered X-Hub-Signature-256 header

---

## Phase 1 — Foundation Build (Weeks 1–10)

### Epic 1.1 — Operator Library (Core Operators)

- [ ] 🔴 **1.1.1** Budget domain operators

  > **[CODE]** "Implement the following operators in `sustena/operators/budget.py` using `@sustena_operator`: `budget.record_income(source, amount, period)` — credits liquid balance, emits `event.finances.income_received`. `budget.allocate(pocket_name, amount, period)` — deducts from liquid, allocates to pocket; constraints: amount > 0, amount <= liquid balance. `budget.spend(pocket_name, amount, description, category)` — deducts from pocket.allocated, increments pocket.spent; constraint: amount <= pocket.allocated. `budget.transfer(from_pocket, to_pocket, amount)`. `budget.summary() -> ResponseWidget` — returns budget_ring widget from state. Each operator: correct `side_effects` declaration, constraint list, license_tier='free', pawa_cost=0, author='sustena_core', ui_schema."

- [ ] 🔴 **1.1.2** Chama domain operators

  > **[CODE]** "Implement `sustena/operators/chama.py`. Operators: `chama.contribution.record(member_id, amount, period)`, `chama.loan.request(member_id, amount, purpose)` — creates loan proposal, sends to Council, `chama.loan.disburse(loan_id)` — requires Council PASSED status, `chama.loan.repay(loan_id, amount)`, `chama.fine.record(member_id, amount, reason)` — constraint: reason IN rules.fine_reasons, `chama.meeting.schedule(date, agenda)`, `chama.dividend.calculate()` — reads all member balances, computes dividend split, returns ResponseWidget."

- [ ] 🟡 **1.1.3** Procurement domain operators (cross-sustain)

  > **[CODE]** "Implement `sustena/operators/procurement.py`. Operators: `mkulima.broadcast_supply_signal(produce, quantity_kg, price_per_kg, harvest_window_days)` — validates produce exists in inventory, publishes to Mycelium event bus (Phase 1: writes to shared SQLite events table with destination='mycelium.public'). `mkulima.receive_signal(signal_id, produce, quantity_kg, price_per_kg, farm_id, harvest_window_days)` — appends to procurement.mkulima_supply_signals, triggers Biashara operative evaluation. `procurement.raise_po(signal_id, quantity_kg, total_kes)` — requires Council proposal PASSED; creates PO; emits event.procurement.po_raised. `procurement.confirm_delivery(po_id, actual_quantity_kg)`."

**[TEST] Epic 1.1 — Milestone Criteria:**
- [ ] `pytest tests/test_budget_operators.py` — happy path, constraint violation, state mutation assertions, event published
- [ ] `pytest tests/test_chama_operators.py` — loan request creates Council proposal; disburse fails without PASSED status
- [ ] `pytest tests/test_procurement_operators.py` — broadcast signal writes to events table with correct destination
- [ ] Run a manual end-to-end test: onboard a test user via WhatsApp → allocate a budget pocket → spend from it → check `budget.summary()` reflects correct numbers
- [ ] All operators have `pawa_cost`, `license_tier`, `author`, `side_effects` metadata — `pytest tests/test_operator_registry.py` validates this

---

### Epic 1.2 — Operative Runtime

- [ ] 🔴 **1.2.1** Operative base class and LLM call wrapper

  > **[CODE]** "Create `sustena/operatives/base.py`. Define `BaseOperative` abstract class: `__init__(self, config, state_accessor, claude_client)`. Abstract methods: `deliberate(context, proposal) -> OperativeVote` (returns YES/NO/ABSTAIN + reasoning), `evaluate(trigger_event) -> OperativeProposal | None`. Implement `_call_claude(system_prompt, user_message, model=HAIKU) -> str` — wraps `claude_client.messages.create()` with retry logic, token counting, and cost logging to `operators_log`. Add `_build_state_context(paths) -> str` — reads specified state paths and formats them as readable context for the LLM prompt. Per Document Analysis §2.1: Orchie's background mode must be threshold-triggered (not continuous polling). Add `should_evaluate(state: StateAccessor) -> bool` method that runs the threshold check with zero LLM calls — only calls `evaluate()` when a threshold is breached."

- [ ] 🔴 **1.2.2** Mentor Operative — Budget Watchdog

  > **[CODE]** "Implement `sustena/operatives/mentor.py`. `MentorOperative(BaseOperative)` specialises in budget analysis. System prompt: include user's income, pocket allocations, and spending history from state. `evaluate()`: triggered by `event.finances.pocket_spent`; checks if any pocket is >80% spent; if yes, proposes `budget.reallocate` or generates alert. `deliberate()` for council proposals: vote YES if proposal improves budget position or is within constraints; vote NO if it would push liquid balance below 10% of income. Per Document Analysis §2.2: Mentor's utility function = minimise budget deviation from plan; disagreement point = status quo (no operator executes); utility improvement measure = expected budget variance reduction."

- [ ] 🔴 **1.2.3** Council DAO runtime

  > **[CODE]** "Create `sustena/core/council.py`. Implement `CouncilSession` class. `create_proposal(sustain_id, proposed_by, operator_name, input_params, simulation_results) -> str`. `collect_votes(proposal_id, operatives) -> dict` — calls `deliberate()` on each operative; records each vote to `council_votes`. `resolve(proposal_id, user_vote=None) -> str` — applies Nash bargaining: if 2+ YES and user hasn't voted NO → IN_VOTING; notify user. User YES → PASSED. User NO → OVERRIDDEN_BY_USER. No response in 48h → DEFERRED. `nash_utility_score(votes) -> float` — geometric mean of operative utility improvements per Document Analysis §2.2 definitions."

**[TEST] Epic 1.2 — Milestone Criteria:**
- [ ] Create a test sustain with a Mentor operative; trigger `event.finances.pocket_spent` at 85% — confirm proposal created in `council_proposals` table
- [ ] Run `council.collect_votes()` — confirm vote records created for each operative
- [ ] Simulate 48h timeout (mock datetime) — confirm status moves to DEFERRED
- [ ] Confirm `should_evaluate()` returns False for state below threshold (no LLM call made)
- [ ] Log shows Haiku model used for `deliberate()` calls; token count logged to `operators_log`

---

### Epic 1.3 — Homestead Sustain (First Complete Sustain)

- [ ] 🔴 **1.3.1** Homestead sustain spec (JSON)

  > **[CODE]** "Create `sustena/sustains/homestead.json`. Complete Homestead Sustain spec following Master Strategy Appendix A. State schema: members, finances (liquid, pockets, income, goals), calendar (events array), alerts (array). Operators: all budget.* operators, homestead.calendar.add_event, homestead.calendar.upcoming_events. Operatives: Mentor, Protégé. Invariants: finances.liquid.balance >= 0, ALL [finances.pockets].allocated >= 0. UI schema: sustain_home widget. Access policy: owner_ids template."

- [ ] 🔴 **1.3.2** Sustain instantiation engine

  > **[CODE]** "Create `sustena/core/sustain_engine.py`. Implement `SustainEngine` class. `instantiate(template_id, user_id, parameters) -> str` — loads sustain JSON, resolves {{placeholder}} tokens, validates all required parameters provided, writes to `sustains` and `sustain_states`, initialises operative instances. `execute_operator(sustain_id, operator_name, params, operative_id=None) -> OperatorResult` — loads state, validates operator exists in spec, runs constraint engine, executes from OPERATOR_REGISTRY, persists state mutations, returns result. `get_state(sustain_id) -> dict`. `simulate(sustain_id, operator_sequence) -> list[dict]` — runs sequence on state fork without persisting (for Council pre-deliberation)."

**[TEST] Epic 1.3 — Milestone Criteria:**
- [ ] Instantiate a Homestead sustain from spec; confirm all tables populated correctly
- [ ] Execute `budget.allocate` on the instantiated sustain; check state updated, event fired, pawa deducted
- [ ] Execute `budget.spend` exceeding allocated amount — confirm OperatorResult.fail() returned, state unchanged
- [ ] Run `simulate()` with a 3-step operator sequence — confirm simulation result returned without writing to database
- [ ] End-to-end WhatsApp flow: new user → onboarding → Homestead created → send "allocate 2000 to food" → Orchie executes operator and replies with budget summary

---

### Epic 1.4 — API Layer

- [ ] 🔴 **1.4.1** FastAPI application structure

  > **[CODE]** "Create `sustena/api/main.py`. FastAPI application with: CORS middleware (restrict to known origins in production), request logging middleware (method, path, response time, status), health check `GET /health` (version, db status, claude status). Mount routers: `/api/v1/sustains`, `/api/v1/operators`, `/api/v1/council`, `/api/v1/users`, `/webhook` (WhatsApp). Dependency injection for database sessions and claude client. OpenAPI docs enabled in development only."

- [ ] 🔴 **1.4.2** Sustain API routes

  > **[CODE]** "Create `sustena/api/routes/sustains.py`. Endpoints: `POST /sustains`, `GET /sustains/{sustain_id}`, `POST /sustains/{sustain_id}/operators/{operator_name}`, `GET /sustains/{sustain_id}/events`, `GET /sustains/{sustain_id}/proposals`, `POST /sustains/{sustain_id}/proposals/{proposal_id}/vote` (body: {vote: 'YES' | 'NO'}). Per Document Analysis §4.3: add `GET /sustains/{id}/export` — returns full sustain spec + current state as JSON download. All endpoints require JWT auth. Return standardised response envelope: {status, data, error, timestamp}."

**[TEST] Epic 1.4 — Milestone Criteria:**
- [ ] `GET /health` returns 200 with db_status: "ok" and claude_status: "ok"
- [ ] `POST /sustains` — create a Homestead sustain via API; check 201 returned with sustain_id
- [ ] `POST /sustains/{id}/operators/budget.allocate` — execute operator via API; check state mutation in database
- [ ] `GET /sustains/{id}/export` — returns valid JSON containing sustain spec and current state
- [ ] Unauthenticated request to any `/api/v1/*` endpoint returns 401
- [ ] Run `pytest tests/test_api.py` — all routes covered with at least happy path + auth failure test

---

### Epic 1.5 — Vyyb Biashara Sustain

- [ ] 🟡 **1.5.1** Vyyb state schema migration from old SQLite

  > **[CODE]** "Create `sustena/sustains/vyyb_biashara.json` — Vyyb Biashara Sustain spec following Vyyb OS Dev Roadmap §2.1. Include all state domains: inventory, production, orders, recipes, accounts, assets, staff, tax, procurement, analytics. Mark KDS widget and production batch widget as `'override': true`; standard financial widgets as `'override': false` (per Document Analysis §1.6). Create `sustena/scripts/migrate_vyyb.py` — reads old Vyyb OS SQLite and writes seed data into Sustena XII as a Vyyb Biashara sustain instance. Map: old `products` table → `state.inventory.items`, old `recipes` → `state.recipes.library`, old `orders` → `state.orders.order_history`, old `accounts` → `state.accounts`."

- [ ] 🟡 **1.5.2** `vyyb.*` operator implementations

  > **[CODE]** "Implement `sustena/operators/vyyb.py`. Operators: `vyyb.inventory.restock(item_id, quantity, unit_cost, supplier)`, `vyyb.production.start_batch(recipe_id, quantity_units, outlet_id)`, `vyyb.production.complete_batch(batch_id, actual_yield)`, `vyyb.orders.place(items, outlet_id, customer_ref)`, `vyyb.orders.fulfill(order_id)`, `vyyb.tax.calculate_vat(period)` — sums VAT collected, computes payable, emits event. All operators follow vyyb.* namespace convention. Include journal entry creation (debit/credit) for inventory and fulfillment operators."

---

### Epic 1.6 — Customer Support System

- [ ] 🔴 **1.6.1** Build and maintain the support workflow
  - You manage all product support in Phase 0–1. This is a competitive advantage — first-hand knowledge of what breaks and what confuses users.
  - Daily routine: check `GET /admin/support-queue` each morning; respond via WhatsApp within 24h
  - Log recurring issues in `support_log.md` in the repo; this becomes the bug backlog

- [ ] 🔴 **1.6.2** Support log to GitHub issues pipeline
  - Weekly: convert `support_log.md` entries into GitHub issues labelled `user-reported`
  - Use GitHub Projects to track: `Backlog` → `In Progress` → `Shipped` → `Verified`
  - This keeps the GitHub repo active and your issue tracker reflects real user pain

- [ ] 🔴 **1.6.3** Known issues page
  - Add `sustena.io/status` — a simple HTML page listing known issues and their status (open/resolved/investigating)
  - Update it every time a new bug is confirmed; this builds user trust

---

### Epic 1.7 — Media Strategy: Blogging, Vlogging, and Docs

Your content lane: technical writing, developer documentation, how-to guides, "building in public" posts, and community education. This is distinct from Brian's business storytelling lane.

- [ ] 🔴 **1.7.1** Substack — "The Sustena Build"
  - Publication name: **The Sustena Build** — a technical founder's newsletter for builders interested in AI systems, African tech infrastructure, and building in public
  - Cadence: fortnightly (every 2 weeks)
  - Post types: (a) **Architecture decisions** — why you built it this way; (b) **What broke and what I learned** — honest postmortems; (c) **Concept deep dives** — e.g., "What is a Sustain?" (d) **Progress updates** — honest phase reports
  - Audience: developers, technical founders, AI builders, potential contributors to Mycelium

  > **[DISPATCH]** "Write the first post for 'The Sustena Build' Substack referencing the Sustena XII Master Strategy §1.1 (the thesis) and §4 (the primitives). The post title: 'I'm building a system that can describe any describable system. Here's the first problem I ran into.' Structure: (1) state the ambition clearly (Sustena's thesis — 7 primitives that describe any system), (2) the first real implementation problem: the ConstraintEngine cannot use Python eval() for security reasons, which means writing a recursive descent parser — describe what that is in plain English, why it matters, and what you learned, (3) a brief look at what's next. Tone: honest, technical, builder-to-builder. Length: 600–900 words. No marketing. This is the technical founder's journal."

- [ ] 🟡 **1.7.2** Sustena Blog — milestones, community, and platform transparency
  - Publication: `sustena.io/blog`
  - Post types: (a) **Monthly platform milestones** (MAU, active sustains, operators executed, pawa consumed); (b) **New sustain templates** announced; (c) **Community contributor spotlights** when Mycelium gets its first third-party contributors
  - Cadence: monthly milestone post + ad hoc announcements
  - You write and publish; Brian provides the business narrative (partnership story, client story) as raw input

  > **[DISPATCH]** "Write a template for the Sustena monthly milestone blog post. Structure: (1) header with month and key headline metric (e.g., 'May 2026: 312 active sustains, 4,200 Orchie conversations'), (2) 3 metrics: MAU, new sustain templates published, total operators executed, (3) one user story (anonymised), (4) product update: what shipped (2–3 bullet points), (5) what's coming next month (2–3 bullet points), (6) event calendar: upcoming workshops, webinars, community calls + sign-up links. Length: 400–500 words. Tone: transparent, data-forward, community-first. Reference the brand voice in Master Strategy §12.1."

- [ ] 🟡 **1.7.3** Technical documentation site
  - Build a `docs.sustena.io` site (or `/docs` subdirectory) using MkDocs or Docusaurus
  - Core documentation sections: (a) **Getting Started** — what is Sustena, the 7 primitives explained; (b) **Operator Reference** — auto-generated from operator metadata; (c) **DSL Guide** — constraint language reference; (d) **API Reference** — auto-generated from OpenAPI spec; (e) **Sustain Templates** — community gallery; (f) **Contributing to Mycelium** — how to publish an operator
  - This is the developer onboarding surface for community contributors

  > **[CODE]** "Set up a MkDocs documentation site at `docs/`. Configure `mkdocs.yml` with: site name 'Sustena Developer Docs', theme material (dark mode, Sustena palette: nav background #1a1a2e, accent #f5a623), plugins: mkdocstrings (for auto-generated Python API docs from docstrings), search. Create initial doc pages: `docs/index.md` (what is Sustena), `docs/primitives.md` (the 7 primitives explained with examples), `docs/dsl.md` (constraint DSL reference), `docs/operators.md` (how to write an operator), `docs/contributing.md` (how to contribute to Mycelium). Wire up `make docs` target to `mkdocs serve`."

- [ ] 🟢 **1.7.4** YouTube / Video content — "Building Sustena"
  - Channel: **Sustena Dev** — technical screencasts and system walkthrough videos
  - Video types: (a) **Architecture walkthroughs** — screen recording of you explaining a design decision; (b) **Live coding sessions** — building an operator from scratch; (c) **Demo videos** — platform capability demonstrations for marketing
  - Cadence: monthly once platform is live (Phase 1+)
  - Tools: OBS for recording, DaVinci Resolve (free) for editing
  - YouTube videos become embeds in the Substack and blog

---

### Epic 1.8 — GitHub Strategy

A live, well-maintained GitHub repo is a credibility signal to developers, investors, and future contributors.

- [ ] 🔴 **1.8.1** Repository hygiene
  - [ ] Commit every day you code — even a 5-line fix. Frequency signals activity.
  - [ ] Meaningful commit messages: `feat(operators): add budget.transfer operator`, `fix(council): resolve vote timeout logic`, `docs: add constraint DSL reference`
  - [ ] Use conventional commits format: `feat`, `fix`, `docs`, `test`, `refactor`, `chore`

- [ ] 🔴 **1.8.2** README quality — investor and contributor facing
  - [ ] README must include: what Sustena is (one paragraph), architecture diagram (even ASCII), quick start instructions, link to full docs, build status badge
  - [ ] Keep README updated at every phase transition

  > **[DISPATCH]** "Write a comprehensive README.md for the Sustena XII GitHub repository. Include: (1) one-paragraph project description ('Sustena is a human-agent reality interface — a system of 7 primitives that can model, simulate, and govern any describable system'), (2) ASCII architecture diagram showing: WhatsApp Bot → FastAPI → Core Engine [State|Operators|Constraints|Events] → Sustain Engine → Operatives [Orchie|Mentor|Protégé|Chama Secretary] → Mycelium, (3) quick start instructions (git clone, cp .env.example .env, make migrate, make run), (4) link to docs.sustena.io, (5) badge placeholders for CI/build status, (6) 'Built by' section with Sustena XII founding context. Tone: direct, technically confident, inviting to contributors."

- [ ] 🔴 **1.8.3** Issue templates and PR templates
  - [ ] Add `.github/ISSUE_TEMPLATE/bug_report.md` — structured fields: description, reproduction steps, expected vs actual, environment (OS, Python version, commit hash)
  - [ ] Add `.github/ISSUE_TEMPLATE/feature_request.md` — structured fields: problem statement, proposed solution, sustain spec impact, constraints affected
  - [ ] Add `.github/PULL_REQUEST_TEMPLATE.md` — checklist: tests added, docs updated, breaking change flag, pawa cost reviewed if operator changed
  - [ ] Add GitHub Actions workflow: `ci.yml` — triggers on PR to `main`; runs `make test`; blocks merge if tests fail

  > **[CODE]** "Create `.github/ISSUE_TEMPLATE/bug_report.md`, `.github/ISSUE_TEMPLATE/feature_request.md`, `.github/PULL_REQUEST_TEMPLATE.md`, and `.github/workflows/ci.yml` for a Python/FastAPI project. CI workflow: checkout code, set up Python 3.11, install dependencies via pip, run `pytest tests/ --tb=short -q`. Fail loudly if tests fail. Keep all templates Sustena-specific — reference the operator/sustain model in field labels."

- [ ] 🟡 **1.8.4** GitHub Projects — public roadmap board
  - [ ] Create a GitHub Projects board mirroring this roadmap's phase structure: columns = Phase 0 / Phase 1 / Phase 2 / Phase 3
  - [ ] Populate with issues from the support log and open milestones
  - [ ] Pin the board to the repo; link from README
  - [ ] This is the public face of the roadmap — investors, contributors, and users can see progress

**[TEST] Epic 1.8 — Milestone Criteria:**
- [ ] `git log --oneline` shows consistent daily commits with conventional commit format
- [ ] README renders correctly on GitHub — architecture diagram legible; all links functional
- [ ] `ci.yml` passes on a clean clone in GitHub Actions
- [ ] At least 5 GitHub issues created from the support log, labelled `user-reported`
- [ ] GitHub Projects board shows at least 10 tasks across phases with status tracking

---

### Epic 1.9 — Developer UI: Mycelium Control Panel

**Reference:** Master Strategy §12.3 — "The Sustena Developer UI is not a CRUD admin panel. It is a professional-grade development environment that visualises the live behaviour of the Sustena engine the same way a game engine editor visualises a game world."

The Mycelium Control Panel is your most important internal tool. You build Sustena on Sustena from Day 1. Build this early — it makes every subsequent epic faster to build and debug.

- [ ] 🔴 **1.9.1** Control Panel scaffold — FastAPI + WebSocket backend

  > **[CODE]** "Create `sustena/api/routes/devui.py`. FastAPI WebSocket endpoint: `WS /devui/state-stream/{sustain_id}` — on connect, sends current full sustain state as JSON, then sends incremental delta patches every time the sustain state changes (using JSON Patch RFC 6902 format). REST endpoints: `GET /devui/sustains` (list all sustains with their last-modified state summary), `GET /devui/sustain/{id}/events?limit=100` (event log paginated), `GET /devui/sustain/{id}/proposals` (Council proposals with vote tally), `GET /devui/registry/operators` (full operator registry with metadata), `GET /devui/registry/operatives`. All endpoints: staff auth required (Bearer `ADMIN_TOKEN`). Return standardised JSON envelope."

- [ ] 🔴 **1.9.2** Monitor Panel — Live state stream viewer (HTMX)

  > **[CODE]** "Create `sustena/templates/devui/monitor.html`. HTMX page with WebSocket connection to `/devui/state-stream/{sustain_id}`. Display: (1) State tree — expandable JSON tree view of all state fields, colour-coded by type (number=blue, string=grey, bool=amber, array=teal). Values animate on change using CSS transition. (2) Event log stream — `<div hx-ws='connect:/devui/state-stream/{id}'>` scrolling feed of events in format `[T+Ns] operator_name → delta_summary`. (3) Operative activity cards — one card per active operative showing: name, current task, last action timestamp, pawa consumed. Colour theme: bg #0f0f0f, text #e8e4dc, accent #E8A020. Font: DM Mono for data, Inter Tight for labels. Reference Master Strategy §12.3.3 Monitor Panel spec."

- [ ] 🔴 **1.9.3** Editor Panel — Spec viewer and inline operator console (HTMX)

  > **[CODE]** "Create `sustena/templates/devui/editor.html`. HTMX page. Two sections: (1) **Sustain Spec Viewer** — renders the current sustain's JSON spec as a collapsible tree (operators, constraints, operatives, state schema) with syntax highlighting. Each operator node has a 'Run' button. (2) **Operator Console** — a `<textarea>` that accepts operator invocations in format `operator.name param1=value1 param2=value2`. On submit, POSTs to `POST /devui/console/execute` — validates against operator registry, runs constraint check, executes if valid, returns OperatorResult. Display result inline with state diff (before/after fields that changed). Console has autocomplete: as you type operator name, matching operators appear below the textarea with their parameter signatures. Reference Master Strategy §12.3.4 Controller Panel — Operator Console spec."

- [ ] 🟡 **1.9.4** Simulator Panel — Fork state and run scenarios

  > **[CODE]** "Create `sustena/api/routes/simulator.py` and `sustena/templates/devui/simulator.html`. Backend: `POST /devui/simulate` — body: `{sustain_id, operator_sequence: [{name, params}]}`. Forks current sustain state into memory (does not write to DB). Runs each operator in sequence on the forked state. Returns: `{steps: [{operator, input, constraint_result, state_delta, events_fired, pawa_cost}], final_state, total_pawa}`. Frontend: HTMX page with a form to build an operator sequence (add/remove operators, set params). Submit runs simulation. Display result as a step-by-step accordion: each step shows operator name, constraint pass/fail, state fields changed (diff view), events fired. Total pawa cost shown at bottom. Add 'Compare' button: run same sequence twice with different params to see diverging state outcomes side by side."

- [ ] 🟡 **1.9.5** Controller Panel — Proposal queue and execution authority

  > **[CODE]** "Create `sustena/templates/devui/controller.html`. HTMX page with two sections: (1) **Proposal Queue** — `hx-get='/devui/proposals?status=PASSED' hx-poll-interval='10s'` — live-polled table of proposals with PASSED status. Each row: sustain name, proposed operator, simulation summary, vote tally, pawa cost, two buttons: EXECUTE (green) and REJECT (grey). EXECUTE POSTs to `POST /devui/proposals/{id}/execute` — runs the approved operator on the live sustain. REJECT POSTs to `PATCH /devui/proposals/{id}` with status=REJECTED_BY_STAFF. (2) **Rollback Log** — table of recently executed operators with a ROLLBACK button. Rollback POSTs to `POST /devui/rollback` — restores state to pre-execution snapshot (stored in `sustain_state_history` table). Add a confirmation modal before any EXECUTE or ROLLBACK action."

- [ ] 🟢 **1.9.6** Phase 2 upgrade path note — ReactFlow DAG
  - Mark this in the codebase: when React is introduced in Phase 2, replace the HTMX editor with a ReactFlow-based DAG editor (per §12.3.1)
  - In the meantime, the text-mode spec editor and console are sufficient for development purposes
  - Add `TODO(phase2): replace HTMX spec viewer with ReactFlow DAG (see Master Strategy §12.3.1)` comment in `editor.html`

**[TEST] Epic 1.9 — Milestone Criteria:**
- [ ] Open Monitor Panel in browser; onboard a test user via WhatsApp; confirm state tree updates in real time
- [ ] Use Operator Console to run `budget.allocate pocket=food amount=2000`; confirm result and state diff appear inline
- [ ] Run a 3-step simulation (allocate → spend → summary); confirm simulation returns state diff for each step; confirm DB is unchanged after simulation
- [ ] Execute a PASSED proposal from the Controller queue; confirm state updated in DB; confirm event fired
- [ ] Rollback the executed proposal; confirm state restored to pre-execution snapshot

---

### Epic 1.10 — Orchie Web UI (HTMX Widget System)

**Reference:** Master Strategy §5.5 (Generated UI), §12.1 (Claude Design First), §10.3 Week 5-6.

Before implementing: run Claude Design mockups for all key Orchie interactions (budget ring, proposal card, chama view, Orchie conversation thread). Mockup first — implement second.

- [ ] 🔴 **1.10.1** Claude Design mockups — all Orchie interaction flows

  > **[DESIGN]** "Create Claude Design mockups for the following Orchie web UI screens, using the Sustena design system: bg-base=#0f0f0f, bg-surface=#181818, bg-raised=#212121, amber=#E8A020, teal=#2AB8A0, text-primary=#e8e4dc, danger=#e05050, DM Mono (data), Inter Tight (UI chrome). Screens to mock: (1) Homestead dashboard — budget ring (donut chart), pocket summary (card grid), goal progress bar, quick action buttons (Allocate, Record Income, Summary). (2) Orchie conversation thread — bottom-sheet chat panel with widget cards inline: budget allocation card, proposal card with vote buttons, transaction confirmation card. (3) Chama view — rotation calendar, contribution matrix table (members × months), Trust Score badges, DAO proposal feed. (4) Budget proposal card — summary text, simulation chart (before/after balance), CTA buttons: Approve / Modify / Defer. Mockups must match The Expanse UI aesthetic (dense, dark, state-driven colour) as described in Master Strategy §12.0."

- [ ] 🔴 **1.10.2** ResponseWidget system — base widget rendering engine

  > **[CODE]** "Create `sustena/core/widgets.py`. Define `ResponseWidget` dataclass with fields: `widget_type: str`, `title: str`, `summary: str | None`, `data: dict`, `ctas: list[str]`, `chart: dict | None`, `metadata: dict`. Implement `WidgetRenderer` class: `render_to_htmx(widget) -> str` — returns an HTMX HTML fragment for the given widget type. Register handlers: `budget_summary_card`, `budget_allocation_card`, `proposal_card`, `goal_progress_bar`, `transaction_confirmation`, `alert_banner`, `chama_table`, `chart_widget`. Each handler reads `widget.data` and returns the correct HTML fragment. The rendered HTML must use only Sustena design system CSS classes (no inline styles except colours from the palette). Store widget HTML templates in `sustena/templates/widgets/`."

- [ ] 🔴 **1.10.3** Orchie conversation interface — web UI

  > **[CODE]** "Create `sustena/templates/orchie/chat.html` and `sustena/api/routes/orchie.py`. The Orchie chat interface is a single-page HTMX app. Layout: (1) Sustain state summary header (pocket balance ring, DAU summary, goal progress). (2) Conversation thread: a scrolling list of message bubbles and widget cards. User input is a text field at the bottom — `hx-post='/orchie/message' hx-target='#thread' hx-swap='beforeend'`. (3) POSTed message goes to `POST /orchie/message` — parses user intent, executes appropriate operator or runs Orchie inference, returns HTMX fragment (either a plain text response bubble or a rendered ResponseWidget card). The response card for a proposal includes Vote buttons (Approve/Defer) that POST to `/council/vote/{proposal_id}`. Implement intent parsing: simple keyword matching in Phase 1 (allocate, spend, summary, goal, chama) → Haiku classification in Phase 2."

- [ ] 🟡 **1.10.4** Sustain home screen — the Orchie dashboard

  > **[CODE]** "Create `sustena/templates/orchie/home.html`. The Orchie home screen. Panels: (1) Budget ring — D3.js donut chart of pocket allocations vs. spending, rendered server-side as an SVG with data embedded; amber for over-budget pockets, teal for healthy, grey for unused. (2) Pocket summary cards — one card per pocket: name, allocated, spent, remaining, burn indicator. (3) Goal progress bars — one bar per active goal with current/target/deadline. (4) Quick actions bar — three buttons: 'Allocate Budget', 'Record Spend', 'Ask Orchie'. Each button opens the Orchie chat in the bottom sheet. (5) Recent events feed — last 10 operator executions summarised. The entire page uses `hx-trigger='load' hx-get='/orchie/home-data'` for initial data load, and `hx-poll-interval='30s'` for live refresh. No full page reload needed."

- [ ] 🟡 **1.10.5** MPesa Global Pay integration — STK Push approval

  > **[CODE]** "Create `sustena/operators/mpesa.py`. Implement `mpesa.stk.push(phone_number, amount, account_ref, description)` operator — calls Safaricom Daraja STK Push API (sandbox mode initially). Inputs: phone (254XXXXXXXXX format), amount (integer KES), account_ref (string), description (string). Steps: (1) get OAuth token from Daraja (`/oauth/v1/generate`), (2) POST to `/mpesa/stkpush/v1/processrequest` with Business Shortcode, Lipa Na M-Pesa Online Passkey, timestamp, base64 password. (3) Store CheckoutRequestID in `operators_log`. (4) Implement callback handler `POST /mpesa/callback` — receives STK Push result, updates operator status, fires `event.payment.mpesa_confirmed` or `event.payment.mpesa_failed`. Constraint: amount > 0 AND amount <= user_policy.payment_ceiling. Reference Daraja API v3 documentation."

**[TEST] Epic 1.10 — Milestone Criteria:**
- [ ] Claude Design mockups reviewed and approved before any code is written
- [ ] Load Orchie home screen in browser; confirm budget ring renders with live pocket data
- [ ] Type "allocate 2000 to food" in Orchie chat; confirm intent parsed, operator executed, allocation card returned in thread
- [ ] Trigger a Council proposal via WhatsApp; confirm proposal card appears in web UI; click Approve; confirm proposal executes
- [ ] `mpesa.stk.push` in sandbox mode: trigger STK prompt on test phone; confirm CheckoutRequestID stored; simulate callback; confirm event fired

---

### Epic 1.11 — Generated UI / ui_schema System

**Reference:** Master Strategy §12.4 — "The spec is the UI. Developers publish an operator and its card in one JSON object."

- [ ] 🔴 **1.11.1** `ui_schema` block spec parser

  > **[CODE]** "Create `sustena/core/ui_schema.py`. Implement `UISchemaParser` class. `parse(operator_spec: dict) -> UISchema` — reads the `ui_schema` block from an operator/operative/sustain JSON spec and returns a typed `UISchema` object. `UISchema` fields: `sub_operator: str` (e.g. 'ui.render.operator_card'), `widget_type: str`, `fields: list[UIField]`, `ctas: list[str]`, `chart: str | None`. `UIField` fields: `label, source, display, colour_rule | None`. `source` is a dot-path expression that can reference `inputs.*`, `state.*`, or a formula expression (subtraction only in Phase 1 — e.g. `state.finances.pockets.food.allocated - state.finances.pockets.food.spent`). Implement `resolve_source(source_expr, operator_inputs, state) -> Any` — evaluates the source expression against live data."

- [ ] 🔴 **1.11.2** `ui.render.*` sub-operator implementations

  > **[CODE]** "Create `sustena/operators/ui_render.py`. Implement the following `ui.render.*` sub-operators using `@sustena_operator`: `ui.render.operator_card(ui_schema, operator_inputs, state) -> ResponseWidget` — builds a widget from a parsed UISchema and live data. `ui.render.operative_dashboard(ui_schema, operative_state) -> ResponseWidget`. `ui.render.sustain_home(ui_schema, full_state, active_operatives) -> list[ResponseWidget]` — returns an ordered list of widgets for the home screen composition. Each sub-operator: pawa_cost=0 (rendering is free), license_tier='free', no side_effects. Include a `ui.render.preview(spec_json, mock_state) -> ResponseWidget` operator for the Developer UI's live preview — lets you see the widget before writing any frontend code."

- [ ] 🟡 **1.11.3** Widget type registry and Mycelium hooks

  > **[CODE]** "Create `sustena/core/widget_registry.py`. Implement `WidgetTypeRegistry` class. `register(widget_type: str, html_template_path: str, schema: dict) -> None`. `render(widget: ResponseWidget) -> str` — looks up widget_type in registry, loads HTML template, renders with widget data using Jinja2. `fetch_from_mycelium(widget_type: str) -> dict | None` — Phase 1 stub: returns None (widget must be registered locally). Phase 2 hook: fetches widget type from Mycelium Library API. Add a `GET /devui/widgets` endpoint that returns all registered widget types with their schemas — visible in the Developer UI's widget catalogue section."

- [ ] 🟢 **1.11.4** Developer UI — Generated UI Preview panel

  > **[CODE]** "Add a 'UI Preview' tab to the Developer UI Editor panel. The tab has two columns: (left) a code editor pane where you paste a raw operator or sustain spec JSON, (right) a live widget preview. When the left pane changes (debounced 500ms), it POSTs to `POST /devui/preview-widget` — which calls `ui.render.preview()` with mock state data and returns the rendered HTML widget. The right pane updates via `hx-post` swap. This lets you design operator specs and see exactly what Orchie card they will produce, in real time, without writing any frontend code. Mock state is loaded from a `mock_state.json` file in the sustain spec directory."

**[TEST] Epic 1.11 — Milestone Criteria:**
- [ ] Add a `ui_schema` block to `budget.allocate` operator; confirm `UISchemaParser.parse()` returns correct UISchema
- [ ] Call `ui.render.operator_card()` with live state; confirm ResponseWidget returned with correct field values resolved
- [ ] Open Developer UI → Editor → UI Preview tab; paste `budget.allocate` spec; confirm widget renders correctly in right pane
- [ ] Modify a field's `display` type in the spec; confirm right pane updates within 1 second
- [ ] Register a custom widget type; confirm it appears in `GET /devui/widgets`

---

### Epic 1.12 — Simulation Engine: Monte Carlo + Strategy Comparison

**Reference:** Master Strategy §2.3 (Feynman path integrals), §12.3.2 (Simulator Panel), §9.3 (pawa cost for simulation: 500–2,000 pawa per 100 paths).

- [ ] 🔴 **1.12.1** Deterministic simulation engine (single path)

  > **[CODE]** "Create `sustena/core/simulation.py`. Implement `SimulationEngine` class. `fork(sustain_id) -> SimulationContext` — creates a deep copy of the current sustain state in memory; does not write to DB. `run_path(context: SimulationContext, operator_sequence: list[dict]) -> SimulationPath` — executes each operator in sequence on the forked state using the same ConstraintEngine and StateAccessor; records: step-by-step state deltas, constraint pass/fail results, events fired (in-memory only), pawa cost per step. `SimulationPath` fields: `steps: list[SimulationStep]`, `final_state: dict`, `total_pawa_cost: int`, `constraints_violated: list[str]`, `score: float` (calculated by the path's scoring function). Scoring: default = final liquid balance / initial liquid balance (higher is better). Custom scoring function injectable per sustain type."

- [ ] 🟡 **1.12.2** Monte Carlo engine — N-path simulation

  > **[CODE]** "Extend `SimulationEngine` with `run_monte_carlo(context, strategy_candidates, n_paths=100) -> MonteCarloResult`. Each strategy candidate is a parameterised operator sequence. The engine runs all candidates in parallel using Python `multiprocessing.Pool` (or `asyncio.gather` for I/O-bound). Returns `MonteCarloResult`: `paths: list[SimulationPath]` (all N paths), `score_distribution: list[float]` (sorted scores), `top_paths: list[SimulationPath]` (top 5 by score), `median_score: float`, `score_std: float`. Pawa cost: 500 pawa base + 15 pawa per path. The Monte Carlo result is attached to Council proposals as `simulation_evidence` — the Council operatives read this evidence before voting. Reference Master Strategy §2.3: each path is a strategy branch in the Feynman path integral formulation; the score-weighted sum of paths IS the Orchie recommendation."

- [ ] 🟡 **1.12.3** Time-series simulation — forward projection

  > **[CODE]** "Add `SimulationEngine.project_forward(context, days=30, assumptions: dict) -> TimeSeriesProjection`. Simulates the sustain's state forward N days by: (1) applying recurring operator templates (monthly income, recurring expenses, chama contributions) on their scheduled dates, (2) applying configurable uncertainty: ±20% income variability (uniform distribution), ±15% expense variability, (3) recording state snapshot per day. Returns `TimeSeriesProjection`: `daily_snapshots: list[dict]`, `projected_liquid_balance_series: list[float]`, `risk_of_zero_balance: float` (fraction of Monte Carlo paths that hit zero before day N), `safe_to_spend_today: float`. This projection feeds the Mentor operative's proactive alert logic."

- [ ] 🟢 **1.12.4** Simulator Panel — frontend visualisation

  > **[CODE]** "Update `sustena/templates/devui/simulator.html`. Add Monte Carlo results section: (1) Score distribution histogram — a bar chart (D3.js or Chart.js) showing the distribution of path scores across all N paths. (2) Top paths table — top 5 paths ranked by score, each with a 'View' button that loads step-by-step execution detail. (3) Risk metric cards: P(zero balance), median projected balance at day 30, standard deviation. Time-series projection chart: line chart of projected liquid balance over 30 days, with a shaded confidence interval (P25-P75 range across paths). Add 'Run Monte Carlo (N=100)' button — posts to `POST /devui/simulate/monte-carlo` with the operator sequence and receives results; renders histogram via `hx-swap='innerHTML'`."

**[TEST] Epic 1.12 — Milestone Criteria:**
- [ ] Fork a Homestead sustain state; run a 5-step simulation; confirm final state matches expected manual calculation; confirm DB unchanged
- [ ] Run Monte Carlo with N=10 paths; confirm 10 `SimulationPath` objects returned; scores are non-identical (parameterisation working)
- [ ] `project_forward(days=30)` on a Homestead state with known recurring expenses; confirm projected balance within ±5% of expected value
- [ ] Simulator Panel renders Monte Carlo histogram; clicking a bar highlights matching paths in the scenario tree
- [ ] Pawa cost correctly calculated: N=100 paths = 500 + 1500 = 2000 pawa; confirm deducted from ledger

---

### Epic 1.13 — Mkulima Sustain

**Reference:** Master Strategy §8.7, §8.6 (Biashara superset), §10.3 Week 7-10.

Mkulima is forked from Biashara. Build Biashara spec first (Epic 1.5), then fork.

- [ ] 🟡 **1.13.1** Mkulima sustain spec (JSON)

  > **[CODE]** "Create `sustena/sustains/mkulima.json`. Mkulima Sustain spec — fork of `vyyb_biashara.json`, extending it with agricultural-specific state domains and operators. State schema additions: `farm.profile` (parcels array: {id, name_gps_area_soil_type}, cooperative_memberships), `farm.crop_calendar` (crops array: {crop_id, crop_name, parcel_id, phase, planting_date, expected_harvest_date, actual_harvest_date, yield_kg_actual, yield_kg_estimated}), `farm.inputs_inventory` (seeds, fertiliser, pesticides, fuel — each: item_id, name, quantity, unit, cost_per_unit, reorder_threshold), `farm.market_intelligence` (price_feeds array: {market, crop, price_per_kg, date, source}), `farm.buyer_pipeline` (supply_signals array, committed_buyers array). Operators: `mkulima.crop.plant`, `mkulima.crop.advance_phase`, `mkulima.crop.record_harvest`, `mkulima.input.purchase`, `mkulima.market.update_price`, `mkulima.supply.broadcast_signal`, `mkulima.supply.receive_commitment`. Inherited from Biashara: all accounting, P&L, and procurement operators."

- [ ] 🟡 **1.13.2** Market price intelligence feed

  > **[CODE]** "Create `sustena/operators/mkulima.py`. Implement `mkulima.market.update_price(crop_name, market_name, price_per_kg, source)` — appends to farm.market_intelligence price feed; emits `event.mkulima.price_updated`. Implement `mkulima.market.get_sell_window(crop_name) -> ResponseWidget` — reads last 14 days of price feed entries for crop; calculates: current price, 14-day average, price vs. cost-plus target (input cost × 1.3 default). Returns `price_intelligence_card` widget: current price badge (amber if above target, teal if below), 14-day price sparkline, sell recommendation ('SELL: price at 23% above target' / 'WAIT: price 8% below target'), nearest buyer option from buyer_pipeline. Create a `sustena/services/market_price_fetcher.py` scheduled job (runs daily via Cloud Functions) that scrapes or fetches prices from configured sources (Wakulima Market, Kongowea — Phase 1: manual seeding via admin endpoint; Phase 2: web scraping)."

- [ ] 🟡 **1.13.3** WhatsApp-native Mkulima mode

  > **[CODE]** "Create `sustena/core/mkulima_whatsapp.py`. Mkulima WhatsApp mode is permanent — not a graduation path. Keyword trigger: `SHAMBA` activates Mkulima mode. Implement Mkulima-specific intent handlers for WhatsApp: `harvest <crop> <quantity_kg>` → `mkulima.crop.record_harvest`, `price <crop>` → `mkulima.market.get_sell_window` (returns formatted text response with price card), `input <item> <quantity> <cost>` → `mkulima.input.purchase`, `broadcast <crop> <quantity> <price>` → `mkulima.supply.broadcast_signal`. Each handler: parse the natural language command, map to operator, execute, return Orchie-voice confirmation in Swahili/English mix. Orchie voice for Mkulima: practical, no-nonsense, KES-denominated ('Tomato leo: KES 48/kg — 23% juu ya target yako. Sema nami niandike buyer.')."

**[TEST] Epic 1.13 — Milestone Criteria:**
- [ ] Instantiate a Mkulima sustain; plant a crop via operator; advance through phases to harvest; confirm crop_calendar state correctly updated at each phase
- [ ] `mkulima.market.get_sell_window('tomato')` returns correct sell recommendation based on seeded price feed
- [ ] WhatsApp: send `SHAMBA` → activation confirmed; send `price tomato` → price card returned; send `harvest tomato 120` → harvest recorded, revenue calculated
- [ ] `mkulima.supply.broadcast_signal('tomato', 120, 50, 3)` writes to events table with `destination='mycelium.public'`
- [ ] Vyyb Biashara sustain receives supply signal; Biashara operative surfaces it as a procurement opportunity

---

### Epic 1.14 — M-Pesa Daraja Full Integration

**Reference:** Master Strategy §8.2 (HOE suboperative), §8.4 (Chama C2B), §10.3 Week 5-6, §10.2 (Day 2 — transaction parsing).

- [ ] 🔴 **1.14.1** Daraja production upgrade — credentials and sandbox → production

  - [ ] Complete Safaricom Daraja production upgrade process: submit CR12, KRA PIN, certified bank account, business name
  - [ ] Obtain Production Business Shortcode and Lipa Na M-Pesa Online Passkey
  - [ ] Update `config.py`: add `DARAJA_CONSUMER_KEY`, `DARAJA_CONSUMER_SECRET`, `DARAJA_SHORTCODE`, `DARAJA_PASSKEY`, `DARAJA_ENV` (sandbox/production)
  - [ ] Test STK Push in sandbox on own phone; confirm callback received before going production

- [ ] 🔴 **1.14.2** HOE — Historical Onboarding Engine (M-Pesa SMS parsing)

  > **[CODE]** "Create `sustena/operatives/hoe.py`. Implement `HOESuboperative` — runs on first user activation. Phase 1: accepts M-Pesa transaction history as a plain text dump (user copies from phone). Phase 2 (Android app): BroadcastReceiver intercepts M-Pesa SMS directly. Parse all 60 M-Pesa text variants using Claude Haiku: C2B payments, B2C receipts, M-Shwari deposits/withdrawals, Till payments, Paybill, airtime purchase, loan repayments. For each parsed transaction: extract {date, type, counterparty_name, amount, balance_after, reference}. Run recurring payment detection: find patterns where same counterparty receives payment within the same ±5-day window for 2+ months → flag as 'possible recurring obligation'. Run income pattern detection: identify regular large inflows. After parsing: call `budget.record_income` and `budget.allocate` operators to seed initial pocket allocations based on detected patterns. Then ask 7 clarifying questions (1 per day for 7 days) to confirm detected patterns. Reference Master Strategy §8.2 HOE spec."

- [ ] 🔴 **1.14.3** C2B listener — Chama contribution matching

  > **[CODE]** "Create `sustena/api/routes/mpesa_c2b.py`. Register Safaricom Daraja C2B callback URL. `POST /mpesa/c2b/confirmation` — receives incoming M-Pesa payment to the Sustena shortcode. Parse: `TransID, TransTime, TransAmount, MSISDN (sender phone), BillRefNumber`. Match `BillRefNumber` to a chama contribution record: `chama_id-member_id-period` format (e.g. `CHAMA001-MEM03-2026-06`). If match found: call `chama.contribution.record(member_id, amount, period)` operator. If no match: route to support queue as `event.support.unmatched_mpesa`. Return Safaricom-required `{ResultCode: 0, ResultDesc: 'Accepted'}` immediately. Also implement `POST /mpesa/c2b/validation` — pre-validates incoming payment, returns 0 to accept or 1 to reject. Log all C2B events to `operators_log`."

- [ ] 🟡 **1.14.4** Automated payout — B2C disbursement

  > **[CODE]** "Add `mpesa.b2c.disburse(phone_number, amount, occasion, remarks)` operator to `sustena/operators/mpesa.py`. Calls Daraja B2C API. Use case: chama rotation payout to winning member. Constraint: requires Council PASSED proposal with status='APPROVED_FOR_DISBURSEMENT'; amount <= proposal.approved_amount; phone must be verified member phone. Steps: OAuth token → POST to `/mpesa/b2c/v3/paymentrequest` with `CommandID=BusinessPayment`. Store `ConversationID` in proposal record. Implement `POST /mpesa/b2c/result` callback — receives result, updates proposal status, fires `event.chama.payout_confirmed` or `event.chama.payout_failed`. Test in sandbox with 600XX test phone numbers before any production run."

**[TEST] Epic 1.14 — Milestone Criteria:**
- [ ] HOE: paste a 20-transaction M-Pesa history dump; confirm all transactions parsed; recurring payments flagged; 7 clarifying questions generated
- [ ] STK Push (sandbox): trigger from Orchie conversation; confirm STK received on test phone; confirm callback processed; `event.payment.mpesa_confirmed` fired
- [ ] C2B: simulate incoming payment with correct `BillRefNumber`; confirm contribution recorded in chama state; incorrect ref routes to support queue
- [ ] B2C (sandbox): create a passed chama payout proposal; trigger disbursement; confirm callback received; payout event fired
- [ ] All M-Pesa callbacks return correct HTTP 200 with `ResultCode: 0` within 5 seconds (Safaricom SLA)

---

### Epic 1.15 — Curator, Attaché, and Navigator Operatives

**Reference:** Master Strategy §8.9 (Curator), §8.10 (Attaché), §8.11 (Navigator), §6.1 (Council composition).

These are the remaining three standard Council councillors. Mentor and Protégé ship in Epics 1.2–1.3. Build these three to complete the full 5-councillor Council.

- [ ] 🟡 **1.15.1** Curator — Asset register and inventory operative

  > **[CODE]** "Implement `sustena/operatives/curator.py`. `CuratorOperative(BaseOperative)` manages the asset register and inventory state. System prompt: include all assets in `state.assets.register` and inventory in `state.inventory.items` with their status, age, and threshold levels. `evaluate()`: triggered by `event.inventory.stock_level_updated` — checks all items against their `reorder_threshold`; if any item is below threshold, creates a procurement proposal. Also triggered daily by time event: checks warranty expiry dates within 30 days and depreciation schedule (posts journal entries via Accounting Operator for monthly depreciation). `deliberate()`: votes YES on procurement proposals that address genuinely depleted stock; NO on purchases for items already above threshold. Implements `suggest_disposal(asset_id) -> OperativeProposal` — surfaces underutilised assets (last_used > 45 days) as disposal or resale candidates."

- [ ] 🟡 **1.15.2** Attaché — Profiler and network operative

  > **[CODE]** "Implement `sustena/operatives/attache.py`. `AttachéOperative(BaseOperative)` manages the contacts graph and reputational intelligence. System prompt: include all contacts in `state.network.contacts` with their transaction history, last interaction, and Chama Cred score. `evaluate()`: triggered by `event.finances.mpesa_transaction` — checks if the counterparty is an unrecognised phone number; if yes, creates a contact enrichment proposal ('I see KES 1,200 to +254722XXXXX for the third time. Should I add them to your network?'). `deliberate()`: votes YES on proposals that improve social capital (chama membership, business partnerships); NO on proposals that involve counterparties with poor Chama Cred scores. Implements `contact_profile(phone_number) -> ResponseWidget` — returns a contact card: name (if known), transaction history summary, relationship type, Chama Cred score badge."

- [ ] 🟡 **1.15.3** Navigator — Logistics operative

  > **[CODE]** "Implement `sustena/operatives/navigator.py`. `NavigatorOperative(BaseOperative)` manages logistics and delivery coordination. System prompt: include active orders in `state.orders.active`, delivery fleet state in `state.fleet` (Vyyb use case), and any pending physical tasks. `evaluate()`: triggered by `event.vyyb.order_placed` (Vyyb) or `event.homestead.grocery_run_needed` (Homestead) — evaluates available delivery options (Sendy API, Glovo delivery, in-house driver) on cost, time, and current fleet state; creates a logistics proposal with the recommended option and cost estimate. Implements `route_optimize(orders: list, outlet_id: str) -> ResponseWidget` — for Vyyb multi-order dispatch: groups orders by delivery zone and assigns to available riders. Phase 1: simple zone bucketing (1km radius from outlet); Phase 2: Google Maps API integration for actual routing."

**[TEST] Epic 1.15 — Milestone Criteria:**
- [ ] Add an asset with `warranty_expiry` 15 days from today; confirm Curator fires a warning proposal the next day
- [ ] Inventory item drops below `reorder_threshold`; confirm Curator creates procurement proposal; Council votes on it
- [ ] New M-Pesa transaction to unrecognised number; confirm Attaché creates contact enrichment proposal
- [ ] Place a Vyyb order; confirm Navigator fires and creates logistics proposal with cost estimate
- [ ] Full 5-councillor Council vote: create a budget proposal → all 5 operatives vote → resolve() applies Nash bargaining correctly

---

### Epic 1.16 — Mycelium v1: Operative and Operator Registry

**Reference:** Master Strategy §7 (The Mycelium), §9.4 (Contributor Royalty Program), §10.3 Week 7-10.

- [ ] 🟡 **1.16.1** Operative template registry

  > **[CODE]** "Create `sustena/mycelium/operative_registry.py`. Implement `OperativeRegistry` class. `publish(operative_spec: dict) -> str` (returns operative_id). `discover(query: str, sustain_type: str | None) -> list[OperativeCard]` — searches registered operatives by capability keyword, sustain type compatibility, trust rating. `install(operative_id, sustain_id) -> bool` — loads operative template from registry, personalises with sustain context, registers as active operative in sustain. `OperativeCard` fields: `operative_id, name, author, version, description, pawa_cost_profile, trust_rating, sustain_compatibility, license_tier`. Phase 1: in-memory registry backed by SQLite `mycelium_operatives` table. Phase 2: Mycelium API (external service). Seed with Sustena's own operatives: Mentor, Protégé, Curator, Attaché, Navigator, Chama Secretary, Biashara, Mkulima, Orchie."

- [ ] 🟡 **1.16.2** Operator registry with license enforcement

  > **[CODE]** "Create `sustena/mycelium/operator_registry.py`. Extend the existing `OPERATOR_REGISTRY` with Mycelium licensing. `register_licensed(operator_fn, metadata, license: ComponentLicense)` — wraps the operator with a license check; calls `license.charge(caller_user_id, pawa_ledger)` before each execution; blocks if insufficient pawa. Add `GET /mycelium/operators` endpoint — returns all registered operators with their metadata, license tier, usage count, and pawa cost. Add `GET /mycelium/operators/{name}/usage` — returns 30-day usage stats for a specific operator. These endpoints populate the Mycelium Library panel in the Developer UI."

- [ ] 🟡 **1.16.3** Sustain Spore Bank — template fork workflow

  > **[CODE]** "Create `sustena/mycelium/spore_bank.py`. Implement `SporeBank` class. `publish_template(sustain_spec: dict, author_id: str, license: ComponentLicense) -> str` (returns template_id). `list_templates(category: str | None) -> list[TemplateCard]` — returns available templates. `fork(template_id, user_id, customisation_params: dict) -> str` (returns new sustain_id) — loads template, resolves `{{placeholder}}` tokens with `customisation_params`, runs `SustainEngine.instantiate()`. Charge license fee on fork. Phase 1: seed with Homestead, Vyyb Biashara, Mkulima, Chama templates. Add `GET /mycelium/templates` and `POST /mycelium/templates/{id}/fork` API endpoints. The fork workflow is the user-facing 'Create a Sustain' action — users browse templates in the web UI and fork directly."

- [ ] 🟢 **1.16.4** Community contribution workflow (stub)

  > **[CODE]** "Add a `POST /mycelium/contribute` endpoint (staff-only in Phase 1). Accepts: operator Python file or operative JSON spec. Runs validation: (1) type-check all operator inputs/outputs, (2) confirm `@sustena_operator` decorator present, (3) dry-run against a mock sustain state. If validation passes: register in the local Mycelium registry. Log to `mycelium_contributions` table. Phase 2: public endpoint with API key authentication for community contributors. This stub lays the plumbing for the Mycelium contributor programme without requiring external infrastructure in Phase 1."

**[TEST] Epic 1.16 — Milestone Criteria:**
- [ ] `OperativeRegistry.discover('budget', sustain_type='homestead')` returns Mentor and Curator
- [ ] Install Mentor operative into a new Homestead sustain via registry; confirm it activates and evaluates correctly
- [ ] Fork Homestead template for a new user; confirm all state fields initialised; sustain functions end-to-end
- [ ] Licensed operator (per-use): confirm pawa deducted on each execution; insufficient pawa blocks execution
- [ ] `GET /mycelium/operators` returns all registered operators with usage counts and license tiers

---

### Epic 1.17 — Platform Ops Sustain

**Reference:** Master Strategy §12.5 — "Sustena monitors itself using its own primitives. This is the deepest proof-of-concept."

- [ ] 🟡 **1.17.1** Platform Ops Sustain spec + instantiation

  > **[CODE]** "Create `sustena/sustains/platform_ops.json` using the spec from Master Strategy §12.5.1. Instantiate it on server startup as a reserved sustain (`sustain_id='sustena.platform_ops'`, `owner_ids=['bonnie', 'brian']`). The sustain is never exposed to regular users — it is staff-only. Wire up FastAPI middleware to call `platform.api.record_request` after every request (async, non-blocking). Wire up Claude API wrapper to call `platform.llm.record_call` after every inference call. Wire up operative runtime to call `platform.operative.record_task` on every task completion."

- [ ] 🟡 **1.17.2** Platform monitoring operators

  > **[CODE]** "Implement `sustena/operators/platform.py`. Operators: `platform.api.record_request(endpoint, method, response_time_ms, status_code)` — increments request count, updates rolling P50/P95/P99 latency using Welford's online algorithm. `platform.llm.record_call(model, input_tokens, output_tokens, latency_ms, cost_usd)` — increments haiku/sonnet counters, accumulates cost. `platform.operative.record_task(operative_name, task_name, duration_ms, success: bool)` — increments task counters. `platform.user.record_session(user_id, channel)` — updates DAU/WAU/MAU. `platform.alert.raise(constraint_id, current_value, threshold, severity)` — appends to state.alerts; fires `event.platform.sla_breach`. All operators: pawa_cost=0 (internal telemetry), no side effects outside Platform Ops state."

- [ ] 🟡 **1.17.3** SLA constraint monitoring

  > **[CODE]** "Add constraint evaluation for the Platform Ops Sustain. Register the SLA constraints from Master Strategy §12.5.3: `api.p95_latency_ms < 800`, `api.error_rate_pct < 1.0`, `llm.haiku_cost_usd_today < 15.0`, `llm.sonnet_cost_usd_today < 50.0`, `llm.classification_accuracy_pct > 92`, `operatives.tasks_failed_today < 10`. Run constraint evaluation on every state update (after every `platform.*` operator). When violated: call `platform.alert.raise`; route to Staff Orchie; send WhatsApp notification to staff group (use `WhatsAppSender.send_text` with a factual, terse message — no Sheng, per Master Strategy §12.5.4 Staff Orchie spec)."

- [ ] 🟡 **1.17.4** Staff Orchie — Platform Ops interface

  > **[CODE]** "Create `sustena/operatives/staff_orchie.py`. `StaffOrchieOperative(BaseOperative)` — specialised Orchie for the Platform Ops Sustain. Personality: terse, data-first, no warmth, no Sheng (per Master Strategy §12.5.4). Implements: `cost_forecast() -> str` — projects LLM cost for next 24h at current burn rate; flags if ceiling will be hit and which operator type is driving it. `incident_route(alert: dict) -> OperativeProposal` — given an SLA breach alert, identifies likely cause from event log, proposes remediation (e.g. 'switch classification to batch mode', 'rollback to Haiku for all non-urgent calls'). `anomaly_detect() -> str | None` — checks user activity patterns for anomalies (8× normal message rate from one cohort → flag as possible viral growth or abuse). Add `GET /admin/platform-status` endpoint — returns current Platform Ops state summary as JSON; accessible by staff only."

**[TEST] Epic 1.17 — Milestone Criteria:**
- [ ] Make 20 API calls; confirm `state.api.p95_latency_ms` updates correctly in Platform Ops sustain
- [ ] Make 3 Claude Haiku calls; confirm `state.llm.haiku_calls_today` and `state.llm.haiku_cost_usd_today` correct
- [ ] Mock `api.p95_latency_ms` = 900 (above 800 threshold); confirm SLA alert raised; WhatsApp notification sent to staff number
- [ ] `GET /admin/platform-status` returns complete Platform Ops state with current metrics
- [ ] Staff Orchie `cost_forecast()` returns accurate prediction based on current burn rate

---

## Phase 2 — Platformisation (Months 3–6)

### Epic 2.1 — React Native Mobile App (Android Primary)

**Reference:** Master Strategy §10.4, §10.3 (Week 7-10 mobile PWA as precursor).

- [ ] 🔴 **2.1.1** React Native project scaffold

  > **[CODE]** "Scaffold a React Native (Expo) project in `sustena-mobile/`. Configure: (1) TypeScript strict mode, (2) Expo Router for file-based navigation, (3) React Query for API state management against the FastAPI backend, (4) NativeWind for Tailwind-equivalent styling. Set up environment config: `SUSTENA_API_URL`, `SUSTENA_WS_URL`. Create the main navigation stack: `(tabs)/_layout.tsx` with 4 tabs: Orchie (home/chat), Sustains (list), Council (proposals), Profile. Implement a reusable `SustenaWidget` component that accepts a `ResponseWidget` JSON object and renders the appropriate native component (Budget ring → RN `Svg`, Proposal card → RN `View` with buttons, Alert banner → RN `Animated.View`). Theme: bg #0f0f0f, amber #E8A020, teal #2AB8A0, text #e8e4dc."

- [ ] 🔴 **2.1.2** Orchie chat — native implementation

  > **[CODE]** "Implement `sustena-mobile/app/(tabs)/orchie.tsx`. Native Orchie conversation interface. Components: (1) Conversation thread — `FlatList` with inverted scroll (latest at bottom); each item is either a `TextBubble` (plain message) or a `WidgetCard` (rendered ResponseWidget). (2) Input bar — `TextInput` with send button; on send: POST to `/orchie/message` via React Query mutation; append response to thread on success. (3) WebSocket connection for push events: connect to `/devui/state-stream/{sustain_id}` on mount; update sustain state in React Query cache on each delta; budget ring re-renders automatically. Implement offline-first: cache last 50 thread messages and last known sustain state in AsyncStorage; display cached state with 'offline' banner when API unreachable."

- [ ] 🟡 **2.1.3** M-Pesa SMS BroadcastReceiver (Android)

  > **[CODE]** "Implement Android SMS BroadcastReceiver in `sustena-mobile/android/app/src/main/java/`. Create `MpesaSmsReceiver.java` (or Kotlin) that: (1) listens for SMS_RECEIVED intent filtered to sender '22395' (Safaricom M-Pesa shortcode), (2) on receive: passes raw SMS body to `MpesaParser.parse()` — calls the FastAPI `/mpesa/parse` endpoint with the raw text, (3) destroys raw SMS content from memory immediately after parsing (privacy tenet — Tenet 2). `POST /mpesa/parse` endpoint: runs Claude Haiku parsing against the 60 M-Pesa text variants; returns structured transaction object; calls `budget.record_income` or `budget.spend` operator accordingly. The user sees an Orchie notification: 'Nimeona M-Pesa ya KES 2,400 — niweke kwa food pocket?' — tapping opens the Orchie chat with the pre-filled proposal."

**[TEST] Epic 2.1 — Milestone Criteria:**
- [ ] App runs on Android emulator; Orchie tab renders with live budget ring from production sustain state
- [ ] Send a message in Orchie chat; confirm response widget rendered correctly in native UI
- [ ] Budget ring updates in real time when budget.spend operator executes (WebSocket delta received and rendered)
- [ ] M-Pesa SMS from '22395': BroadcastReceiver fires; transaction parsed; Orchie notification shown; user taps → chat opens with proposal pre-filled
- [ ] Airplane mode: app shows cached state with offline banner; resumes sync on reconnect

---

### Epic 2.2 — Multi-User Orchie (Shared Sustains)

**Reference:** Master Strategy §4.5 (Privilege tiers), §8.5 (Homestead role system).

- [ ] 🔴 **2.2.1** Multi-user access control enforcement

  > **[CODE]** "Extend the Sustain Engine to enforce the 4-tier privilege system from Master Strategy §4.5. Add `privilege.check` sub-operator that runs before every operator execution: reads caller's tier from `sustain.access_policy`; checks against operator's `min_privilege` field; raises `PrivilegeViolationError` if insufficient. Implement `sustain.access.add_member(user_id, tier, scope: list | None)` operator — adds a user to the sustain access policy; requires Level 0 owner approval via Council proposal. Implement `sustain.access.invite(phone_number, tier)` — sends a WhatsApp invitation to the phone number with a join code; on acceptance, calls `access.add_member`. Test: 5-member Homestead sustain; confirm Level 2 member cannot execute Level 1 operators."

- [ ] 🟡 **2.2.2** Chama multi-member live state sync

  > **[CODE]** "Extend `WS /devui/state-stream/{sustain_id}` to support multiple concurrent subscribers. Each chama member's Orchie connects to the same WebSocket room (keyed by `sustain_id`). When a contribution is recorded, all connected members receive the delta simultaneously. Implement `chama.notify_all(sustain_id, event) -> None` — broadcasts to all subscribers of the sustain's WebSocket room. Add a `GET /chama/{sustain_id}/live-board` HTML endpoint — serves an HTMX page showing the chama's real-time contribution matrix, Trust Scores, and pending proposals. Shareable link — any member with their session token can open it and see live updates."

---

### Epic 2.3 — Bank Integration Agent

**Reference:** Master Strategy §10.4, §11.2 (NCBA open banking API).

- [ ] 🟡 **2.3.1** Bank statement PDF/CSV parser

  > **[CODE]** "Create `sustena/agents/bank_import.py`. Implement `BankImportAgent` class. `parse_pdf(pdf_bytes: bytes, bank_name: str) -> list[dict]` — uses PyMuPDF (or pdfplumber) to extract text from a bank statement PDF; applies bank-specific regex patterns to extract: date, description, debit/credit amount, balance, reference. Support: NCBA, Equity, KCB, Cooperative Bank statement formats. `parse_csv(csv_content: str, bank_name: str) -> list[dict]` — parses standardised CSV export from banking apps. After parsing: classify each transaction using Claude Haiku (income/expense category); call `budget.record_income` or `budget.spend` operators for each classified transaction. Return an import summary: total records, total classified, total unclassified (for user review). This extends the HOE engine to handle bank statements, not just M-Pesa SMS."

---

### Epic 2.4 — IoT Integration: Pilot Hardware

**Reference:** Master Strategy §8.5 (Homestead IoT), §8.7 (Mkulima IoT), §10.4, §11.2 (KES 50,000 pilot budget).

- [ ] 🟡 **2.4.1** MQTT listener and IoT event ingestion pipeline

  > **[CODE]** "Create `sustena/iot/mqtt_listener.py`. Connect to a local MQTT broker (Mosquitto, running on Raspberry Pi). Subscribe to topics: `homestead/pantry/+` (pantry scale weight events), `homestead/power/meter` (smart meter readings), `mkulima/soil/+` (soil sensor readings). For each received MQTT message: parse JSON payload `{sensor_id, value, unit, timestamp}`, call the appropriate Sustena operator: `pantry.scale.update(item_id, weight_g)` → Curator updates inventory; `power.meter.record(kwh_today)` → Mentor updates utility forecast; `soil.moisture.record(parcel_id, pct)` → Mkulima updates crop calendar. All IoT operators: pawa_cost=0 (sensor data is free), `min_privilege: 4` (service agent tier only). Phase 1: simulate IoT events with a test script that publishes to MQTT every 30 seconds."

- [ ] 🟢 **2.4.2** Pilot hardware deployment — 5 households

  - [ ] Procure 5× Raspberry Pi 4 Model B kits (est. KES 8,000 each = KES 40,000)
  - [ ] Flash custom Sustena IoT image onto each: Raspbian + Mosquitto + Python 3.11 + MQTT listener script
  - [ ] Install at 5 pilot households (Bonnie + Brian + 3 chama members)
  - [ ] Confirm MQTT events flowing to Platform Ops Sustain
  - [ ] Monitor for 2 weeks; document reliability, event frequency, Mentor integration quality

---

### Epic 2.5 — Monte Carlo Strategy Engine: Orchie Recommendations

**Reference:** Master Strategy §2.3 (Feynman path integrals), §12.3.2 (Monte Carlo mode in Simulator).

- [ ] 🟡 **2.5.1** Orchie strategy generation via Monte Carlo

  > **[CODE]** "Extend `sustena/operatives/orchie.py`. Add `generate_strategy_recommendation(goal: str) -> OperativeProposal`. Steps: (1) Parse goal into target state (natural language → structured {metric, target_value, deadline}). (2) Generate 5 candidate operator sequences using Haiku — each sequence represents a different path to the target state. (3) Run `SimulationEngine.run_monte_carlo(context, candidates, n_paths=50)`. (4) Select the top candidate by score. (5) Construct an `OperativeProposal` with the winning sequence, simulation evidence (score distribution, top path, risk_of_zero_balance), and a plain-English rationale (Haiku-generated). (6) Submit proposal to Council for deliberation. This is the first end-to-end loop of the Sustena Loop: DESCRIBE (goal) → MODEL (state) → SIMULATE (Monte Carlo) → OPTIMISE (score ranking) → EXECUTE (if approved) → OBSERVE (event log)."

---

### Epic 2.6 — Mycelium Marketplace: External Contributors

**Reference:** Master Strategy §9.4 (Contributor Royalty Program), §7 (Mycelium), §10.4.

- [ ] 🟡 **2.6.1** Developer portal and API key management

  > **[CODE]** "Create `sustena/api/routes/developer.py`. Endpoints: `POST /developer/register` — creates a developer account (name, email, KYC fields for pawa earners above threshold). `POST /developer/api-key` — generates an API key scoped to Mycelium contribution operations. `POST /developer/operatives/publish` — accepts operative spec JSON, validates against the operative schema, runs automated checks (type safety, prompt injection test, pawa cost declaration), registers in Mycelium with the developer's author_id. `POST /developer/operators/publish` — same for operator implementations. `GET /developer/earnings` — returns pawa earned from royalties, usage stats per published component. Phase 1: Bonnie reviews all submissions manually before they go live (staff approval gate)."

---

### Epic 2.7 — Vyyb B2B: The Hive Production + Multi-Outlet

**Reference:** Master Strategy §8.8 (Biashara), §10.4, Vyyb OS Dev Roadmap §3.

- [ ] 🟡 **2.7.1** Hive production batch system

  > **[CODE]** "Implement `vyyb.hive.production.plan_batch(recipe_id, quantity_units, dispatch_outlets: list[str]) -> OperativeProposal` operator. The Hive produces in bulk; batches are distributed to outlets. Operator steps: (1) check ingredient inventory against recipe COGS (Curator verifies stock), (2) calculate batch cost (ingredients + labour + overhead), (3) assign outlet allocations, (4) create dispatch record in `state.hive.dispatch_log`. Also implement `vyyb.hive.production.complete_batch(batch_id, actual_yield_kg, quality_grade)` — closes the batch, updates inventory at Hive, triggers `vyyb.outlet.receive_stock` for each assigned outlet. Add a Kitchen Display System (KDS) widget: real-time production queue visible to Hive kitchen staff via `GET /vyyb/kds/{hive_id}` (no auth required — staff-facing display URL)."

---

## Phase 3 — Decentralisation (Month 12+)

### Epic 3.1 — Web3.py Live Integration

**Reference:** Master Strategy §9.1 (Mocked → Live), §9.3 (Pawa token launch), §10.5.

- [ ] 🔴 **3.1.1** Ethereum L2 integration — Optimism/Arbitrum

  > **[CODE]** "Migrate `sustena/core/pawa.py` from SQLite to Web3.py. The `PawaLedger` interface stays identical — only the backend changes. Replace `debit()`/`credit()` with `web3.eth.contract().functions.transfer().transact()` calls. Configure: `WEB3_PROVIDER_URL` (Alchemy/Infura Optimism endpoint), `PAWA_TOKEN_ADDRESS` (ERC-20 contract address), `NETWORK_TREASURY_ADDRESS`. Testnet first: Optimism Sepolia. All Phase 1–2 pawa ledger entries have matching blockchain-equivalent log structure — migration is a bulk transaction issuance, not a schema change. Require smart contract audit before mainnet. Reference Master Strategy §9.1 Stage 2 transition plan."

- [ ] 🔴 **3.1.2** Pawa token launch — ERC-20 deployment

  - [ ] Commission independent smart contract audit (budget $5,000–15,000)
  - [ ] Deploy ERC-20 Pawa token on Optimism mainnet
  - [ ] Initial distribution: founders governance reserve, Mycelium contributors, onboarding allocation for early users
  - [ ] Activate pawa conversion: Phase 1–2 accumulated pawa balances → on-chain ERC-20 tokens at DAO-governed rate
  - [ ] Publish audit report publicly; announce on Substack + blog

- [ ] 🟡 **3.1.3** Chama DAO on-chain — governance proposals as transactions

  > **[CODE]** "Implement `sustena/blockchain/chama_dao.py`. Deploy Chama DAO smart contract (Solidity, OpenZeppelin Governor pattern) on Optimism. Each DAO proposal is submitted as an on-chain transaction; votes are signed wallet transactions from each member. Payout execution requires on-chain proposal status = 'PASSED'. Python interface: `ChamaDAOContract.submit_proposal()`, `cast_vote()`, `execute_payout()` — mirror the Phase 1 Python class interface exactly (per Master Strategy §9.1 design intent). Phase 1 SQLite chama records are the migration source — all historical proposals and votes are archived as on-chain events."

---

### Epic 3.2 — CBK DCP Licence Application

**Reference:** Master Strategy §10.5, §9.5.2 (Phase 3 legal considerations), §11.2 (KES 20M+ capital requirement).

- [ ] 🔴 **3.2.1** CBK Deposit-Taking MFI pre-application preparation
  - [ ] Engage CBK-experienced fintech counsel (budget: KES 100,000–500,000)
  - [ ] Conduct legal gap analysis: Data Protection Act 2019 compliance, AML/CFT policy, KYC procedures
  - [ ] Apply to CBK Regulatory Sandbox (Category A: Financial Technology) — lower barrier to entry before full DCP
  - [ ] Begin KES 20M capital reserve accumulation (this is a balance sheet requirement, not spent)
  - [ ] Implement AML/CFT screening for pawa earners > 10,000 pawa/month (per §9.5.1 legal note)

---

### Epic 3.3 — IPFS and Decentralised Storage

**Reference:** Master Strategy §9.6 (Storage Phase 2–3), §10.5.

- [ ] 🟢 **3.3.1** Custom IPFS network — operative memory and event log archiving

  > **[CODE]** "Integrate IPFS for long-term storage of: (1) Operative memory archives (compressed conversation history older than 90 days), (2) Sustain event log archiving (events older than 30 days moved from SQLite to IPFS; retrievable by CID), (3) Sustain template publication (each published template has an IPFS CID as its canonical identifier — immutable, content-addressed). Use `ipfshttpclient` Python library. Pinning fee paid in pawa tokens to registered pinning node operators. Reference Master Strategy §9.6 Storage Phase 2 target."

---

### Epic 3.4 — East African Expansion

**Reference:** Master Strategy §10.5 ("Uganda, Tanzania, Rwanda — same stack, localised M-Pesa analogues").

- [ ] 🟢 **3.4.1** Multi-country payment adapter layer

  > **[CODE]** "Create `sustena/payments/adapter.py`. Abstract `PaymentAdapter` interface: `stk_push()`, `c2b_listen()`, `b2c_disburse()`, `parse_sms()`. Country-specific implementations: `MpesaKenyaAdapter` (Daraja — existing), `MtnMomoAdapter` (Uganda/Rwanda — MTN MoMo API), `AirtelMoneyAdapter` (Tanzania/Uganda — Airtel Money API), `TigoMomoAdapter` (Tanzania — Tigo Pesa API). The `mpesa.*` operator namespace becomes a country-agnostic `mobile_money.*` namespace in Phase 3. All M-Pesa references in the existing codebase are wrapped in the Kenya adapter — country detection happens at sustain instantiation time (`state.meta.country_code`)."

---

## Personal Operating Instructions — Applies Across All Phases

**Discipline for the solo developer (your rules, not suggestions):**

1. **Commit daily.** Even a 5-line fix is a commit. Use conventional format (`feat:`, `fix:`, `docs:`, `test:`). Frequency is the credibility signal.

2. **Test before merge.** No code merges to `dev` without passing `pytest tests/`. The CI gate is not a suggestion — it is the anti-regress boundary.

3. **Prompt before implementation.** Every non-trivial function block gets a `[CODE]` prompt first. Run it, review the output, integrate. Never write boilerplate from scratch.

4. **Mockup before frontend.** Every new UI surface starts as a Claude Design mockup. The mockup is the spec. Implementation follows the mockup, not the other way round.

5. **Log support issues.** Check `GET /admin/support-queue` every morning. Log every recurring issue in `support_log.md`. Convert to GitHub issues weekly.

6. **Write before you ship.** Every milestone gets a Substack post draft. The post does not need to be published before the next milestone — but the draft must exist. Writing forces clarity about what you built.

7. **Simulate before you execute.** Every operator sequence that touches money or external APIs must pass through the Simulator Panel first. The Council proposal workflow is not a formality — it is your error-correction layer.

8. **Platform Ops first every morning.** Open `GET /admin/platform-status` before starting work. If any SLA is breached, resolve it before building new features.

9. **Update this document at each phase transition.** This roadmap is a living document. At the end of Phase 0 (before starting Phase 1), read all Phase 0 epics — mark completed, add notes, revise Phase 1 based on what you learned. The roadmap is not a contract — it is a navigation system. Correct your course.

10. **Lane discipline.** You own: code, documentation, customer support, Substack, blog, media, demos, community. Brian owns: investor relations, partnerships, legal logistics, business development content, Colosso Finance product decisions. Overlap is coordination, not competition.

---

*Bonnie_Master_Roadmap.md — Version 2.1 — Updated May 2026*
*Cross-reference: Sustena_XII_Master_Strategy.md | Sustena_XII_Feature_Spec.md | Brian_Master_Roadmap.md | Vyyb_OS_Dev_Roadmap.md*
