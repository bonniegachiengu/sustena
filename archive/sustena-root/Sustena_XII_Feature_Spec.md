# Sustena XII — Complete Feature Specification
**Version 1.0 — May 2026**
**Author:** Derived from Sustena_XII_Master_Strategy.md by Bonventure Gachiengu
**Cross-reference:** Sustena_XII_Master_Strategy.md | Sustena_DSL_Dot_Protocol.md | Bonnie_Master_Roadmap.md | Brian_Master_Roadmap.md

---

## How to Read This Document

Each feature record follows this format:

```
Feature ID   : F-NNN
Feature Name : Human-readable name
Phase        : 0 | 1 | 2 | 3 (when to build it)
Owner        : Bonnie | Brian | Both
Status       : [ ] Pending | [x] Complete | [~] In Progress
Strategy Ref : §section in Sustena_XII_Master_Strategy.md
Dependencies : Other F-NNN IDs this feature requires first
```

Followed by: **Description**, **Inputs**, **Outputs / State Changes**, **Constraints**, **Acceptance Criteria**, and the `[CODE]` vibe-coding prompt for Bonnie.

---

## PART 1 — CORE ENGINE

### F-001 — StateAccessor

```
Feature ID   : F-001
Feature Name : StateAccessor — sustain state read/write primitive
Phase        : 0
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §4.1
Dependencies : none
```

**Description:** Wraps a sustain's JSON state dict. All operators read and write state exclusively through this object. Dot-path traversal (`finances.liquid.balance`), array indexing (`staff.roster[0].name`), atomic increment/decrement, append/remove for list entries, snapshot (deep-copy, read-only).

**Inputs:** JSON state dict (loaded from `sustain_states` table on each operator call).

**Outputs:** Mutated state dict (persisted back after successful operator execution).

**Constraints:**
- `decrement()` must not allow negative values unless `allow_negative=True` is explicitly passed
- `append()` must generate a UUID for the new item's `id` field
- All path operations must raise `StatePathError` (not `KeyError`) with a human-readable message

**Acceptance Criteria:**
- 20+ unit tests passing: null values, empty arrays, nested paths, invalid paths
- `snapshot()` returns deep copy — mutations to snapshot do not affect live state
- `increment()` and `decrement()` are atomic under concurrent write simulation

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/core/state.py` per Sustena XII Master Strategy §4.1 (State primitive). The `StateAccessor` class wraps a JSON dict and provides: `get(path: str) -> Any`, `set(path: str, value: Any) -> None`, `increment(path: str, delta: float) -> float`, `decrement(path: str, delta: float) -> float` (validates result ≥ 0 by default; accepts `allow_negative=True`), `append(path: str, item: dict) -> str` (returns UUID), `remove(path: str, item_id: str) -> None`, `exists(path: str) -> bool`, `snapshot() -> dict`. Paths use dot notation. Array indexing via brackets `roster[0].name`. Raise `StatePathError` (custom exception) with descriptive message on any path error. Write comprehensive unit tests in `tests/test_state.py` with 20+ cases."

---

### F-002 — ConstraintEngine

```
Feature ID   : F-002
Feature Name : ConstraintEngine — predicate evaluator without eval()
Phase        : 0
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §4.3
Dependencies : F-001
```

**Description:** Evaluates constraint predicate strings against a `StateAccessor` and operator input parameters. Must NOT use Python `eval()` — implement a recursive descent parser. Supports comparison operators, membership operators, logical operators, quantifiers (`ALL`, `EXISTS`), and temporal constraints (`WITHIN n hours|days`).

**Inputs:** `state: StateAccessor`, `params: dict`, `constraint_str: str`

**Outputs:** `(bool, str)` — result and human-readable failure reason for rejected proposals.

**Constraints:**
- No `eval()`, `exec()`, or `compile()` — security requirement
- Temporal constraints must use the sustain's event log, not system time (allows simulation)
- Quantifiers must traverse list paths in state

**Acceptance Criteria:**
- 20+ test cases covering all operator types
- `ALL [finances.pockets].spent <= allocated` evaluates correctly
- Temporal constraints return correct result under simulated time
- Invalid constraint syntax raises `ConstraintSyntaxError` with line/position

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/core/constraints.py` per Sustena XII Master Strategy §4.3 (Constraints primitive) and the DSL spec in Sustena_DSL_Dot_Protocol.md. The `ConstraintEngine` must NOT use Python `eval()`. Implement a recursive descent parser handling: comparison operators (>, >=, <, <=, ==, !=), membership (IN, NOT IN), logical (AND, OR, NOT), quantifiers (ALL [path].field op value, EXISTS [path].field op value), temporal (WITHIN n hours|days constraint_expr). Takes `StateAccessor`, params dict, and constraint string. Returns `(bool, str)`. Include 20+ test cases in `tests/test_constraints.py`. Raise `ConstraintSyntaxError` with position info on bad syntax."

---

### F-003 — EventBus

```
Feature ID   : F-003
Feature Name : EventBus — event publish/subscribe within a sustain
Phase        : 0
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §4.4
Dependencies : F-001
```

**Description:** All operator side-effects are expressed as published events. Events follow the Dot Protocol naming: `event.domain.type` (e.g., `event.finances.pocket_spent`). Written to the `events` SQLite table asynchronously. Operatives subscribe to event patterns and trigger `evaluate()` when their subscribed events fire.

**Inputs:** `name: str` (dot-protocol format), `payload: dict`, `timestamp: datetime`

**Outputs:** `event_id: str` (UUID); row written to `events` table.

**Constraints:**
- Event name must start with `event.`, contain ≥ 3 segments, use lowercase snake_case
- `publish()` must be non-blocking (async) — operator execution must not wait for handlers
- `get_history()` must support filtering by event name prefix (e.g., `event.finances.*`)

**Acceptance Criteria:**
- `publish()` writes to SQLite without blocking operator return
- `subscribe()` handlers called after event fires with correct payload
- `get_history()` returns events in ascending timestamp order

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/core/events.py` per Sustena XII Master Strategy §4.4 (Events primitive). `EventBus` methods: `publish(name, payload, timestamp) -> str`, `subscribe(event_name, handler_fn)`, `get_history(sustain_id, event_name|None, limit) -> list[dict]`. Validate dot-protocol format. Async writes to SQLite `events` table. Support wildcard subscriptions via prefix matching (e.g., `event.finances.*`). Write unit tests in `tests/test_events.py`."

---

### F-004 — PawaLedger

```
Feature ID   : F-004
Feature Name : PawaLedger — token accounting (Phase 1: SQLite mock)
Phase        : 0
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §9.5, §4 (Consensus primitive)
Dependencies : none
```

**Description:** Phase 1 implementation is a pure Python + SQLite accounting system that mirrors exactly what the Phase 3 ERC-20 smart contract will do. Every method maps to a Solidity function — making Phase 3 a drop-in replacement. Includes `ComponentLicense` class that splits royalties: contributor 70%, network treasury 20%, referrer 5%, validator 5%.

**Inputs:** `user_id`, `sustain_id`, `amount`, `reason`

**Outputs:** New balance; ledger row in `pawa_ledger` table.

**Constraints:**
- `deduct()` must be atomic — returns `False` and makes no state change if balance insufficient
- `transfer()` must succeed or fail atomically (no partial transfers)
- All ledger rows are append-only (no updates, no deletes) — audit trail

**Phase 3 mapping:**
- `deduct()` → ERC-20 `transfer()` from user to contract
- `credit()` → ERC-20 `mint()`
- `get_balance()` → `balanceOf()`
- `ComponentLicense.charge()` → `chargeRoyalty()` Solidity function

**Acceptance Criteria:**
- `deduct()` with insufficient balance returns False, zero rows changed in `pawa_ledger`
- `ComponentLicense.charge()` splits sum to exactly 100% across 4 parties
- Concurrent deducts on same user cannot overdraft (use SQLite transaction isolation)

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/core/pawa.py` per Sustena XII Master Strategy §9.5. `PawaLedger`: `get_balance(user_id) -> int`, `deduct(user_id, sustain_id, amount, reason) -> bool`, `credit(user_id, sustain_id, amount, reason) -> int`, `transfer(from_user, to_user, amount, reason) -> bool`, `get_history(user_id, limit) -> list[dict]`. `ComponentLicense.charge(caller_user_id, ledger) -> dict` splits: contributor 70%, treasury 20%, referrer 5%, validator 5%. Phase 1: SQLite-backed. Document each method with Phase 3 ERC-20 mapping in docstring. Tests in `tests/test_pawa.py`."

---

### F-005 — OperatorRegistry

```
Feature ID   : F-005
Feature Name : OperatorRegistry — function registry with metadata
Phase        : 0
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §4.2
Dependencies : F-001, F-002, F-003, F-004
```

**Description:** A global dict mapping operator names (dot-protocol: `budget.allocate`) to `OperatorMeta` objects containing: the function, pawa_cost, license_tier, author, side_effects list, constraint_strings list, and ui_schema. The `@sustena_operator` decorator registers functions automatically. `OperatorContext` dataclass bundles all execution dependencies.

**Inputs:** `operator_name: str`, `OperatorContext`, `params: dict`

**Outputs:** `OperatorResult` — `ok(data)`, `fail(reason, constraint_violated)`, or `deferred(proposal_id)`

**Acceptance Criteria:**
- Registry validates all required metadata fields at import time (fail fast)
- `OperatorResult.to_response()` produces a serialisable dict
- All core operators listed in this spec have matching registry entries

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/core/operator.py` per Sustena XII Master Strategy §4.2. `OperatorContext` dataclass: `state, timestamp, sustain_id, operative_id, user_id, pawa_ledger, event_bus, logger`. `OperatorResult` class methods: `ok(data)`, `fail(reason, constraint_violated|None)`, `deferred(proposal_id)`, `to_response() -> dict`. `@sustena_operator` decorator: registers function in global `OPERATOR_REGISTRY: dict[str, OperatorMeta]`. `OperatorMeta` fields: `name, fn, pawa_cost, license_tier, author, side_effects, constraints, ui_schema`. Raise `OperatorRegistrationError` if required fields missing."

---

### F-006 — SustainEngine

```
Feature ID   : F-006
Feature Name : SustainEngine — operator execution and simulation runtime
Phase        : 0
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §4, §12.3.1
Dependencies : F-001 through F-005
```

**Description:** The core runtime. Loads a sustain's spec and state, routes operator calls through the constraint engine, executes from the registry, persists mutations, and fires events. Also exposes a `simulate()` method that forks state and runs an operator sequence without persisting — used by the Council before deliberation and by the Simulator Panel in the Developer UI.

**Methods:**
- `instantiate(template_id, user_id, parameters) -> str` — creates a sustain instance from a JSON spec
- `execute_operator(sustain_id, operator_name, params, operative_id=None) -> OperatorResult`
- `get_state(sustain_id) -> dict`
- `simulate(sustain_id, operator_sequence: list[dict]) -> list[dict]` — forks, runs, discards

**Constraints:**
- `simulate()` must NEVER write to the database
- State mutations are only persisted after all constraint checks pass
- `execute_operator()` logs to `operators_log` regardless of success/failure

**Acceptance Criteria:**
- `simulate()` with a 5-step sequence returns correct intermediate states without DB writes
- `execute_operator()` with failing constraint: OperatorResult.fail returned, no DB mutation
- `instantiate()` resolves all `{{placeholder}}` tokens in JSON spec before persisting

**[CODE] Vibe-coding prompt:**
> "Create `sustena/core/sustain_engine.py` per Sustena XII Master Strategy §4 and §12.3.1 (simulate mode). `SustainEngine` methods: `instantiate(template_id, user_id, parameters) -> str`, `execute_operator(sustain_id, operator_name, params, operative_id=None) -> OperatorResult`, `get_state(sustain_id) -> dict`, `simulate(sustain_id, operator_sequence) -> list[dict]`. Simulate forks state in memory, runs sequence, returns list of intermediate states. NEVER writes to DB during simulate. Wire: StateAccessor → ConstraintEngine → OperatorRegistry → EventBus → PawaLedger. Log all executions to `operators_log`."

---

### F-007 — CouncilDAO

```
Feature ID   : F-007
Feature Name : CouncilDAO — Nash bargaining governance runtime
Phase        : 0
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §6, §4.6
Dependencies : F-005, F-006
```

**Description:** The Council is the governance layer for all operator proposals that require multi-party consent. A proposal is created when: an operative recommends an operator execution; an operator's `HIGH` autonomy level triggers it; or a constraint prevents direct execution. Operatives vote (YES/NO/ABSTAIN + reasoning). The user holds 51% weight. Nash utility score determines whether the vote passes.

**Methods:**
- `create_proposal(sustain_id, proposed_by, operator_name, input_params, simulation_results) -> str`
- `collect_votes(proposal_id, operatives) -> dict`
- `resolve(proposal_id, user_vote=None) -> str` — PASSED | OVERRIDDEN_BY_USER | DEFERRED
- `nash_utility_score(votes) -> float` — geometric mean of utility improvements

**States:** PENDING → IN_VOTING → PASSED | FAILED | OVERRIDDEN_BY_USER | DEFERRED (48h no response)

**Constraints:**
- User vote = 51% weight; each operative = 9.8% weight (Council of 5 operatives + user = 100%)
- Proposal expires and moves to DEFERRED if user does not respond within 48 hours
- `simulation_results` must be attached before votes are collected (operatives deliberate on simulation evidence)

**Acceptance Criteria:**
- 2 YES operative votes + user YES → PASSED
- User NO overrides any operative vote combination
- 48h mock timeout → DEFERRED status

**[CODE] Vibe-coding prompt:**
> "Create `sustena/core/council.py` per Sustena XII Master Strategy §6 (Council DAO) and §4.6 (Consensus primitive). `CouncilSession` methods: `create_proposal()`, `collect_votes()`, `resolve()`, `nash_utility_score()`. Nash bargaining: user = 51% vote weight, each operative = 9.8%. Proposal states: PENDING→IN_VOTING→PASSED|FAILED|OVERRIDDEN_BY_USER|DEFERRED. 48h user non-response → DEFERRED. Simulation results must be attached as evidence before collect_votes. Include `GeometricMean` utility calculation. Tests in `tests/test_council.py`."

---

## PART 2 — OPERATOR LIBRARY

### F-010 — Budget Domain Operators

```
Feature ID   : F-010
Feature Name : budget.* — household and business financial operators
Phase        : 0/1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §8.3, Appendix A.2
Dependencies : F-005, F-006
```

**Operators:**

| Operator | Inputs | State Change | Pawa Cost |
|---|---|---|---|
| `budget.record_income` | source, amount, period | liquid.balance += amount | 0 |
| `budget.allocate` | pocket_name, amount, period | liquid.balance -= amount; pocket.allocated += amount | 1 |
| `budget.spend` | pocket_name, amount, description, category | pocket.spent += amount | 1 |
| `budget.transfer` | from_pocket, to_pocket, amount | from.allocated -= amount; to.allocated += amount | 1 |
| `budget.summary` | — | read-only → returns ResponseWidget | 0 |
| `budget.set_goal` | goal_type, target_amount, deadline | finances.goals upserted | 0 |

**Constraints:**
- `budget.allocate`: `amount <= state.finances.liquid.balance` AND `amount > 0`
- `budget.spend`: `amount <= state.finances.pockets[pocket_name].allocated`
- `budget.transfer`: `from_pocket.allocated >= amount`

**Events emitted:**
- `event.finances.income_received`
- `event.finances.pocket_allocated`
- `event.finances.pocket_spent` (triggers Mentor evaluation)

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operators/budget.py` per Sustena XII Master Strategy §8.3 and Appendix A.2. Use `@sustena_operator` decorator. Operators: `budget.record_income`, `budget.allocate`, `budget.spend`, `budget.transfer`, `budget.summary` (returns budget_ring ResponseWidget), `budget.set_goal`. Each operator must have: correct `side_effects` declaration, constraint strings, `license_tier='free'`, `pawa_cost`, `author='sustena_core'`, `ui_schema` block (see Master Strategy §12.4 for format). `budget.summary` returns a ResponseWidget with pocket_donut chart and liquid balance. Tests in `tests/test_budget_operators.py`."

---

### F-011 — Chama Domain Operators

```
Feature ID   : F-011
Feature Name : chama.* — rotating savings group operators
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §8.5
Dependencies : F-005, F-006, F-007
```

**Operators:**

| Operator | Inputs | State Change | Council Required |
|---|---|---|---|
| `chama.contribution.record` | member_id, amount, period | member.contributions += amount; group.pool += amount | No |
| `chama.loan.request` | member_id, amount, purpose | Creates Council proposal | Yes (DAO vote) |
| `chama.loan.disburse` | loan_id | member.loans_active += loan; pool -= amount | Yes (PASSED required) |
| `chama.loan.repay` | loan_id, amount | loan.balance -= amount; pool += amount | No |
| `chama.fine.record` | member_id, amount, reason | member.fines += amount | No |
| `chama.meeting.schedule` | date, agenda, venue | meetings.upcoming.append(meeting) | No |
| `chama.dividend.calculate` | period | Read-only → ResponseWidget with split | No |
| `chama.rotation.advance` | — | rotation.current_position increments | Yes |

**Constraints:**
- `chama.loan.request`: `amount <= state.pool.balance * 0.5` (no more than 50% of pool as single loan)
- `chama.loan.disburse`: Council proposal status == PASSED
- `chama.fine.record`: `reason IN state.rules.fine_reasons`
- `chama.contribution.record`: `amount > 0` AND `member_id IN state.members`

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operators/chama.py` per Sustena XII Master Strategy §8.5. Use `@sustena_operator`. Operators: `chama.contribution.record`, `chama.loan.request` (creates Council proposal, returns OperatorResult.deferred(proposal_id)), `chama.loan.disburse` (fails if proposal not PASSED), `chama.loan.repay`, `chama.fine.record`, `chama.meeting.schedule`, `chama.dividend.calculate` (returns pie ResponseWidget), `chama.rotation.advance`. All constraints as defined in Master Strategy §8.5. Events: `event.chama.contribution_recorded`, `event.chama.loan_requested`, `event.chama.loan_disbursed`. Tests in `tests/test_chama_operators.py`."

---

### F-012 — Vyyb Biashara Domain Operators

```
Feature ID   : F-012
Feature Name : vyyb.* — food business operations operators
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §8.6, §8.8
Dependencies : F-005, F-006
```

**Operators:**

| Operator | Inputs | State Change |
|---|---|---|
| `vyyb.inventory.restock` | item_id, quantity, unit_cost, supplier | inventory.items[item].quantity += qty; accounts.payable += cost |
| `vyyb.production.start_batch` | recipe_id, quantity_units, outlet_id | production.batches_active.append(batch) |
| `vyyb.production.complete_batch` | batch_id, actual_yield | batch.status=done; inventory updated; COGS recorded |
| `vyyb.orders.place` | items, outlet_id, customer_ref | orders.active.append(order) |
| `vyyb.orders.fulfill` | order_id | order.status=fulfilled; revenue recorded; inventory decremented |
| `vyyb.tax.calculate_vat` | period | Read-only → VAT summary ResponseWidget |
| `vyyb.staff.clock_in` | staff_id | staff[id].shift_start = now |
| `vyyb.staff.clock_out` | staff_id | staff[id].shift_hours += delta; payroll_accrual += hours * rate |

**Special:** KDS widget (`vyyb.production.*` operators) has `"override": true` in ui_schema — uses custom kitchen display rather than generic card. Standard financial widgets use `"override": false`.

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operators/vyyb.py` per Sustena XII Master Strategy §8.6 and §8.8 (Vyyb Biashara Sustain). Use `@sustena_operator`. All operators listed above. KDS-facing operators (`vyyb.production.*`) must include `'override': true` in their `ui_schema` block (per Document Analysis §1.6 — custom KDS widget). Financial operators use `'override': false`. Journal entry creation for inventory and fulfillment operators (debit/credit pairs). Events: `event.vyyb.inventory_restocked`, `event.vyyb.batch_completed`, `event.vyyb.order_fulfilled`, `event.vyyb.revenue_recorded`. Tests in `tests/test_vyyb_operators.py`."

---

### F-013 — Mkulima Domain Operators

```
Feature ID   : F-013
Feature Name : mkulima.* — smallholder farm management operators
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §8.7
Dependencies : F-005, F-006
```

**Operators:**

| Operator | Inputs | State Change |
|---|---|---|
| `mkulima.crop.plant` | crop_type, parcel_id, area_m2, planting_date | farm.parcels[id].crop = crop; calendar.milestones generated |
| `mkulima.crop.harvest` | parcel_id, quantity_kg, quality_grade | inventory.harvest_log.append; parcel.status = harvested |
| `mkulima.broadcast_supply_signal` | produce, quantity_kg, price_per_kg, harvest_window_days | Writes to shared events (destination='mycelium.public') |
| `mkulima.receive_supply_signal` | signal_id, produce, quantity_kg, price_per_kg, farm_id | signals.incoming.append; triggers Biashara evaluation |
| `mkulima.expense.record` | category, amount, description | farm.finances.expenses += amount |
| `mkulima.weather.log` | date, rainfall_mm, temp_min, temp_max | weather.log.append(entry) |

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operators/mkulima.py` per Sustena XII Master Strategy §8.7. `mkulima.broadcast_supply_signal` writes to shared `events` table with `destination='mycelium.public'` (Phase 1 Mycelium mock). `mkulima.crop.plant` generates calendar milestones (germination, weeding, harvest window) based on crop type from a lookup table. `mkulima.receive_supply_signal` triggers `event.mkulima.supply_signal_received` which Biashara operative subscribes to. Events: `event.mkulima.crop_planted`, `event.mkulima.harvest_recorded`, `event.mkulima.supply_signal_broadcast`. Tests in `tests/test_mkulima_operators.py`."

---

### F-014 — Procurement Domain Operators

```
Feature ID   : F-014
Feature Name : procurement.* — cross-sustain purchase order operators
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §8.7, §8.6
Dependencies : F-005, F-006, F-007, F-013
```

**Operators:**

| Operator | Inputs | State Change |
|---|---|---|
| `procurement.raise_po` | signal_id, quantity_kg, total_kes | po_register.active.append(po); Mycelium event published |
| `procurement.confirm_delivery` | po_id, actual_quantity_kg | po.status=delivered; inventory updated; payment recorded |
| `procurement.cancel_po` | po_id, reason | po.status=cancelled; event emitted |
| `procurement.dispute.raise` | po_id, claim_type, amount | dispute_register.append(dispute); Council proposal created |

**Constraints:**
- `procurement.raise_po`: Council PASSED required; checks `signal_id` exists in signals.incoming
- `procurement.confirm_delivery`: `actual_quantity_kg >= po.quantity_kg * 0.9` (10% shortfall tolerance)

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operators/procurement.py` per Sustena XII Master Strategy §8.7 (Mkulima-Biashara supply chain). `procurement.raise_po` requires Council PASSED proposal. It writes a `event.procurement.po_raised` to the Mycelium event bus (Phase 1: shared SQLite events table with destination='mycelium.public'). `procurement.confirm_delivery` checks 10% tolerance. Cross-sustain: the PO event is readable by the Mkulima sustain's operative. Tests in `tests/test_procurement_operators.py`."

---

### F-015 — Platform Monitoring Operators

```
Feature ID   : F-015
Feature Name : platform.* — platform ops observability operators
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.5
Dependencies : F-003, F-005
```

**Operators (all called by Sustena runtime, not users):**

| Operator | Trigger |
|---|---|
| `platform.api.record_request` | Every FastAPI request |
| `platform.llm.record_call` | Every Claude API call |
| `platform.operative.record_task` | Operative task completion/failure |
| `platform.user.record_session` | User session start/end |
| `platform.pawa.record_burn` | Every pawa-consuming action |
| `platform.alert.raise` | Any SLA constraint violation |

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operators/platform.py` per Sustena XII Master Strategy §12.5. These operators are called automatically by the Sustena runtime middleware, not by users. `platform.api.record_request`: FastAPI middleware writes to Platform Ops Sustain state (p50/p95/p99 latency, error_rate). `platform.llm.record_call`: wraps every Claude API call; records cost, tokens, latency to Platform Ops Sustain. `platform.alert.raise`: checks constraints from §12.5.3, routes to staff Council if violated. Wire into FastAPI middleware and Claude client wrapper in `sustena/core/claude_client.py`."

---

## PART 3 — OPERATIVE LIBRARY

### F-020 — Mentor Operative (Budget Watchdog)

```
Feature ID   : F-020
Feature Name : Mentor — budget watchdog operative
Phase        : 0/1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §5.1, §8.3
Dependencies : F-007, F-010
```

**Description:** Monitors all financial state fields. Triggered by `event.finances.pocket_spent`. Evaluates whether any pocket has exceeded 80% of allocation. If yes, proposes a `budget.reallocate` operator call or raises an alert. Votes in Council on any financial proposals based on whether they improve or worsen the budget position.

**Utility function:** Minimise budget variance from plan.
**Disagreement point:** Status quo — no operator executes.

**System prompt context:**
- Current pocket allocations and spent amounts
- Liquid balance and runway
- Income sources
- Historical spending patterns

**`should_evaluate()` thresholds (zero LLM calls):**
- `event.finances.pocket_spent` fires AND pocket.spent > pocket.allocated * 0.8
- OR liquid.balance < income * 0.1 (10% emergency threshold)

**`deliberate()` vote logic:**
- YES if proposal reduces budget variance from plan
- YES if proposal is within liquid balance and improves any pocket
- NO if proposal pushes liquid.balance < income * 0.1
- ABSTAIN if proposal is outside financial domain

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operatives/mentor.py` per Sustena XII Master Strategy §5.1 and §8.3, and Document Analysis §2.1 and §2.2. `MentorOperative(BaseOperative)`. `should_evaluate(state)`: returns True only when pocket.spent > allocated * 0.8 OR liquid.balance < income * 0.1 — NO LLM calls. `evaluate(trigger_event)`: called only when threshold breached; calls Haiku with state context (current pockets, burn rate, liquid balance) to generate a `budget.reallocate` proposal or alert. `deliberate(proposal)`: votes YES if proposal improves budget position; NO if it would push liquid below 10% income. System prompt includes Sustena XII Master Strategy §5.1 operative identity definition."

---

### F-021 — Protégé Operative (Scheduler)

```
Feature ID   : F-021
Feature Name : Protégé — time and task management operative
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §5.2
Dependencies : F-007, F-003
```

**Description:** Monitors all task and calendar state fields. Sends daily briefing (7am), reminders 2 hours before due tasks, weekly planning prompt (Sunday evening). Proposes schedule adjustments when conflicts detected. Evaluates task feasibility against available time windows.

**Triggers:** `event.time.morning_briefing`, `event.task.created`, `event.task.overdue`

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operatives/protege.py` per Sustena XII Master Strategy §5.2. `ProtegeOperative(BaseOperative)`. Handles: daily briefing generation (7am cron), 2-hour pre-task reminders, weekly planning prompt. `evaluate(trigger_event)`: when `event.task.created`, checks for calendar conflicts and capacity; when `event.task.overdue`, proposes reassignment. `should_evaluate()`: zero LLM calls; returns True on time-based triggers and task events. System prompt context: full tasks list, calendar events for next 7 days, member schedules."

---

### F-022 — Curator Operative (Asset/Inventory Manager)

```
Feature ID   : F-022
Feature Name : Curator — asset register and inventory manager operative
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §5.3
Dependencies : F-007
```

**Description:** Monitors `state.assets` (appliances, electronics, furniture, vehicles) and `state.pantry`. Tracks warranty expiry dates, maintenance schedules, and pantry thresholds. Proposes purchase orders for pantry restocks (within auto-approve budget) and warranty renewals (above threshold, escalates to Council).

**Triggers:** `event.pantry.item_below_threshold`, `event.asset.warranty_expiring`, `event.time.weekly_check`

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operatives/curator.py` per Sustena XII Master Strategy §5.3. Handles asset register (warranty expiry alerts, maintenance scheduling) and pantry management (threshold monitoring, reorder proposals). `should_evaluate()`: checks pantry items against min_quantity thresholds and warranty dates without LLM calls. `evaluate()`: Haiku call only when threshold breach or warranty within 14 days. Auto-approve M-Pesa payment proposals under KES 500; escalate to Council above KES 2,000."

---

### F-023 — Attaché Operative (Network Profiler)

```
Feature ID   : F-023
Feature Name : Attaché — relationship and opportunity mapping operative
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §5.4
Dependencies : F-007
```

**Description:** Monitors the sustain's contacts and interaction history. Identifies strategic connections, dormant relationships, and introduction opportunities. Proposes outreach sequences for business development (Biashara) or community connections (Homestead). Tracks mutual benefit potential across the Mycelium network.

**Triggers:** `event.contact.added`, `event.interaction.recorded`, `event.time.weekly_network_review`

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operatives/attache.py` per Sustena XII Master Strategy §5.4. Monitors contacts, interaction history, and opportunity signals. `evaluate()`: uses Sonnet (not Haiku) for relationship graph analysis — this is a higher-order inference task. Outputs: contact outreach proposals, introduction opportunity alerts, network gap analysis. `should_evaluate()`: triggers on new contact or weekly review event."

---

### F-024 — Navigator Operative (Logistics)

```
Feature ID   : F-024
Feature Name : Navigator — logistics and supply chain operative
Phase        : 1/2
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §5.5
Dependencies : F-007, F-013, F-014
```

**Description:** Handles route optimisation, delivery coordination, and supplier logistics. For Mkulima: coordinates harvest transport to buyer. For Biashara: manages supplier delivery schedules. Phase 2: integrates with ride-hailing APIs for delivery cost estimates.

**Triggers:** `event.procurement.po_raised`, `event.mkulima.harvest_recorded`, `event.logistics.delivery_window_approaching`

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operatives/navigator.py` per Sustena XII Master Strategy §5.5. Handles logistics coordination: delivery scheduling, route planning, supplier coordination. Phase 1: proposes WhatsApp-based coordination (generates message templates for supplier contact). Phase 2: integrate ride-hailing API for delivery cost estimation. `evaluate()`: when PO is raised, generates logistics plan (who transports what, by when, at what cost estimate)."

---

### F-025 — Orchie Operative (User Avatar/Delegate)

```
Feature ID   : F-025
Feature Name : Orchie — the user's AI delegate
Phase        : 0/1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §5.6, §7
Dependencies : F-007, F-020 through F-024
```

**Description:** Orchie is the Avatar — the user's player character in the Sustena system. Not an NPC. Orchie proposes actions, executes with approval, and communicates in the user's preferred language (Kenyan English + Sheng). Orchie is the ONLY operative the user directly interacts with. All other operatives communicate via Council and via Orchie.

**Voice:** Warm, professional, moderately Kenyan, bilingual (English + moderate Swahili/Sheng). First name only. Uses "I" for proposals ("I recommend...") not "The system suggests...".

**Autonomy levels:**
- `AUTO`: executes without asking (pantry restocks under KES 500)
- `NOTIFY`: executes and tells user after (routine scheduling)
- `CONFIRM`: asks user before executing (budget reallocations)
- `HIGH`: always creates Council proposal (M-Pesa payments above KES 2,000)

**Communication channels:** WhatsApp (Phase 0), Web UI chat (Phase 1), React Native (Phase 2)

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operatives/orchie.py` per Sustena XII Master Strategy §5.6 and §7. `OrchieOperative(BaseOperative)`. Orchie is the user's delegate — always uses first person ('I recommend', 'I've noticed'). Four autonomy levels: AUTO (execute), NOTIFY (execute + inform), CONFIRM (ask first), HIGH (full Council). `deliberate()`: always votes YES to proposals that have user benefit evidence. Response generation: Haiku for classification and routing; Sonnet for strategy and nuanced recommendations. Voice config: warm, Kenyan, bilingual. Include a `format_for_whatsapp(response) -> str` method and `format_for_widget(response) -> ResponseWidget` method."

---

### F-026 — Staff Orchie

```
Feature ID   : F-026
Feature Name : Staff Orchie — platform monitoring operative for Sustena team
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.5.4
Dependencies : F-025, F-015
```

**Description:** A separate Orchie instance pointed at the Platform Ops Sustain. Personality is terse, data-first, no Sheng, no warmth — a staff tool not a user product. Handles cost forecasting, incident routing, anomaly detection, and weekly cost optimisation proposals.

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operatives/staff_orchie.py` per Sustena XII Master Strategy §12.5.4. Staff Orchie is an `OrchieOperative` subclass with overridden personality config: terse, data-first, no Sheng. Pointed at Platform Ops Sustain. Monitors: API latency, LLM cost burn, operative failure rate, user anomalies. Generates weekly cost optimisation proposals (e.g., switch batch classification to fine-tuned local model). Routes to Bonnie and Brian via WhatsApp staff group and Developer UI Monitor Panel."

---

### F-027 — Chama Secretary Operative

```
Feature ID   : F-027
Feature Name : Chama Secretary — group savings governance operative
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §8.5
Dependencies : F-011, F-007
```

**Description:** Specialised operative for the Chama sustain. Tracks contribution schedule, sends payment reminders, records minutes, manages loan book, and runs the dividend calculation. Communicates to all chama members via WhatsApp broadcast.

**Triggers:** `event.time.contribution_reminder`, `event.chama.loan_requested`, `event.chama.meeting_scheduled`

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/operatives/chama_secretary.py` per Sustena XII Master Strategy §8.5. `ChamaSecretaryOperative(BaseOperative)`. Handles: contribution reminders (3 days before, 1 day before, day-of), loan book management, meeting minute capture via WhatsApp voice-to-text, dividend calculation and broadcast. `evaluate()`: triggered by time events and chama operator events. Sends broadcast WhatsApp messages to all chama members. Cross-sustain: communicates with member Homestead sustains via Mycelium operative messaging to update their chama contribution budget pocket."

---

## PART 4 — SUSTAIN TEMPLATES

### F-030 — Homestead Sustain

```
Feature ID   : F-030
Feature Name : Homestead Sustain — household management
Phase        : 0/1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §8.3, Appendix A
Dependencies : F-010, F-020, F-021, F-022
```

**State schema:** members, finances (liquid, pockets: food/utilities/transport/rent/emergency/chama/discretionary, income_sources, accounts), pantry, tasks (chores/maintenance/shopping/appointments), assets, wish_list, household_goals (vision/missions/objectives), contacts, recipes, meal_plan.

**Default operative set:** Mentor + Protégé + Orchie. Curator added when assets are registered.

**Autonomy policy:** AUTO-approve under KES 500; CONFIRM for KES 500–2,000; Council (HIGH) above KES 2,000.

**[CODE] Vibe-coding prompt:**
> "Create `sustena/sustains/homestead.json` per Sustena XII Master Strategy Appendix A. Full Homestead Sustain spec: complete state schema with all fields from Appendix A.1, all operators from A.2, operative configurations from A.3, chama external microsustain reference pattern from A.4, and ui_schema from A.5. Use {{placeholder}} template tokens for household_id, member names, and financial parameters. Validate that all state field names match what the budget.* and homestead.* operators read and write."

---

### F-031 — Biashara Sustain (Vyyb)

```
Feature ID   : F-031
Feature Name : Vyyb Biashara Sustain — food business management
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §8.6, §8.8
Dependencies : F-012, F-014
```

**State schema:** inventory (items, reorder_thresholds), production (batches_active, recipes_library), orders (active, history, kds_queue), accounts (revenue, cogs, payable, receivable), staff (roster, shifts, payroll), tax (vat_collected, vat_payable, itax_accrual), procurement (mkulima_supply_signals, pos_active), analytics (daily_revenue, margin_by_recipe, outlet_performance).

**Special:** Migrate existing Vyyb OS SQLite data into this sustain format via migration script.

**[CODE] Vibe-coding prompt:**
> "Create `sustena/sustains/vyyb_biashara.json` per Sustena XII Master Strategy §8.6 and §8.8. Full state schema including: inventory, production, orders, accounts, staff, tax, procurement, analytics. Mark KDS operators with `'override': true` in ui_schema. Also create `sustena/scripts/migrate_vyyb.py` — reads old Vyyb OS SQLite and writes seed data into Sustena XII Vyyb Biashara sustain instance. Map: old `products` → `state.inventory.items`, old `recipes` → `state.recipes.library`, old `orders` → `state.orders.order_history`, old `accounts` → `state.accounts`."

---

### F-032 — Chama Sustain

```
Feature ID   : F-032
Feature Name : Chama Sustain — rotating savings group management
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §8.5
Dependencies : F-011, F-027
```

**State schema:** members (id, phone, contributions_paid, loans_active, fines_balance), rules (contribution_amount, contribution_day, fine_reasons, loan_policy), pool (balance, committed, available), rotation (order, current_position, next_payout_date, payout_history), meetings (minutes, schedule, attendance), disputes.

**Council composition:** All chama members are Council members (not operatives). Each member has equal vote weight. User (chairperson) holds tie-breaking vote.

**[CODE] Vibe-coding prompt:**
> "Create `sustena/sustains/chama.json` per Sustena XII Master Strategy §8.5. Full state schema for a rotating savings group. Council configuration: all chama members are voters with equal weight; chairperson holds tie-breaking vote. Operative: Chama Secretary only. Operators: all chama.* operators. Include rules.fine_reasons as a configurable list. External microsustain references: each member's Homestead sustain ID (for cross-sustain contribution tracking)."

---

### F-033 — Mkulima Sustain

```
Feature ID   : F-033
Feature Name : Mkulima Sustain — smallholder farm management
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §8.7
Dependencies : F-013, F-014, F-024
```

**State schema:** farm (parcels with boundaries/soil data, equipment, location), crops (by parcel: type, planted_date, milestones, current_status), harvest (log with quantity/quality/batch), inventory (harvest_batches, inputs_stock), finances (income, expenses, input_costs, revenue_by_crop), supply_signals (broadcast, received), procurement (pos_active, delivery_log), weather (log, forecast_cache), analytics (yield_per_acre, margin_by_crop, price_history).

**[CODE] Vibe-coding prompt:**
> "Create `sustena/sustains/mkulima.json` per Sustena XII Master Strategy §8.7. Full smallholder farm state schema. Operatives: Navigator + Orchie (farm persona: practical, Swahili-first, uses farmer terminology). Cross-sustain: supply signal broadcast writes to shared events table; Biashara sustains can subscribe via Mycelium. Include crop milestone generation logic in `mkulima.crop.plant` — generate calendar events based on crop type lookup table (maize: germination 14d, weeding 21d, harvest 90d; tomatoes: germination 7d, transplant 14d, first harvest 60d; etc)."

---

### F-034 — Platform Ops Sustain

```
Feature ID   : F-034
Feature Name : Platform Ops Sustain — Sustena's self-monitoring sustain
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.5
Dependencies : F-015, F-026
```

**State schema:** api (endpoint latency histograms, error_rate, rpm), llm (haiku/sonnet call counts, costs, accuracy), operatives (active_count, tasks_completed/failed, avg_duration, council_votes), users (dau/wau/mau, whatsapp_active, web_active, new_signups), pawa (burned_today, earned_today, network_balance), alerts (queue).

**SLA constraints (built-in):**
```python
"api.p95_latency_ms < 800"
"api.error_rate_pct < 1.0"
"llm.haiku_cost_usd_today < 15.0"
"llm.sonnet_cost_usd_today < 50.0"
"llm.classification_accuracy_pct > 92"
"operatives.tasks_failed_today < 10"
```

**[CODE] Vibe-coding prompt:**
> "Create `sustena/sustains/platform_ops.json` per Sustena XII Master Strategy §12.5. The Platform Ops Sustain monitors the platform itself. Full state schema from §12.5.1. Operators: all `platform.*` operators from F-015. Constraints: SLA definitions from §12.5.3. Operative: Staff Orchie. Access policy: `['bonnie', 'brian', 'engineering_team']`. This sustain is instantiated at server startup and persists forever — it is never deleted. UI schema: Monitor Panel (4-section dashboard: API Health, LLM Costs, Operative Performance, User Metrics)."

---

## PART 5 — INTERFACE LAYER

### F-040 — WhatsApp Bot

```
Feature ID   : F-040
Feature Name : WhatsApp Bot — Phase 0 primary interface
Phase        : 0
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §11.2, §8.1
Dependencies : F-025, F-006
```

**Architecture:**
- FastAPI webhook: `POST /webhook` receives Meta Cloud API messages
- X-Hub-Signature-256 verification on all inbound messages
- Orchie interprets message → routes to operator → sends response
- Interactive buttons: Max 3 per message (Meta limit)
- List messages: for operator parameter selection (e.g., pocket selection)
- Template messages: for proactive alerts and reminders

**M-Pesa parsing pipeline:**
1. User forwards M-Pesa SMS to Orchie
2. Haiku classifies: SEND_MONEY | RECEIVE_MONEY | PAYBILL | BUY_GOODS | WITHDRAW | AIRTIME
3. Extracts: amount, counterparty, reference, timestamp, balance
4. Maps to operator call: `budget.spend(pocket_name, amount, description)`
5. If pocket ambiguous: Orchie asks "Which pocket? Food / Transport / Other"

**[CODE] Vibe-coding prompt:**
> "Create `sustena/api/whatsapp.py` and `sustena/core/whatsapp_handler.py` per Sustena XII Master Strategy §11.2. Two endpoints: `GET /webhook` (Meta verification), `POST /webhook` (inbound message processing). Signature verification required. Message routing: (1) M-Pesa SMS forwarded → Haiku classification (SEND_MONEY|RECEIVE_MONEY|PAYBILL|BUY_GOODS|WITHDRAW|AIRTIME) → extract amount/counterparty/reference → call `budget.spend`. (2) Natural language → Orchie NLU routing → appropriate operator. (3) Unmatched → support queue. Create `sustena/core/whatsapp_sender.py` with: `send_text`, `send_interactive_buttons` (max 3), `send_list_message`, `send_template`. Meta Graph API v20.0. Retry logic: 3 attempts, exponential backoff."

---

### F-041 — Orchie Web Chat UI

```
Feature ID   : F-041
Feature Name : Orchie Web Chat UI — browser-based chat interface
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.3, §7
Dependencies : F-025, F-040
```

**Description:** The Orchie web chat is the same conversational interface as WhatsApp but rendered in the browser. Identical UX — the user should feel no difference between the two channels. Messages stream via Server-Sent Events (SSE). Widgets (MetricWidget, PieWidget, LineWidget, MpesaWidget, AlertWidget, ProposalWidget) render inline in the chat thread.

**Files:** `brand/components/orchie-chat.jsx`, `brand/components/orchie-mobile.jsx`, `brand/components/ios-frame.jsx`

**Widget types (all implemented in orchie-chat.jsx):**
- `metric` → MetricWidget: large number + delta + subtext
- `pie` → PieWidget: donut chart with legend
- `line` → LineWidget: sparkline with threshold line
- `mpesa` → MpesaWidget: STK push preview card
- `alert` → AlertWidget: amber/red/teal tone cards with action buttons
- `proposal` → ProposalWidget: Council vote card with quorum progress bar

**[CODE] Vibe-coding prompt:**
> "The Orchie Web Chat UI already has a working prototype in `brand/components/orchie-chat.jsx` and `brand/components/orchie-mobile.jsx`. Wire it to the live FastAPI backend. Create `sustena/api/routes/orchie.py` with: `POST /orchie/message` (send a message, get Orchie's response with optional widget JSON), `GET /orchie/stream` (SSE endpoint for streaming Orchie's response token by token), `GET /orchie/history/{sustain_id}` (last 50 messages with widgets). The frontend `OrchieBubble` component already renders all widget types from the `widget` field in the message JSON. Use the design system in `brand/stylesheets/styles.css` — amber #E8A020, bg-base #0f0f0f, DM Mono + Inter Tight fonts."

---

### F-042 — Dashboard Shell (4-Panel Developer UI)

```
Feature ID   : F-042
Feature Name : Dashboard Shell — 4-panel developer control interface
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.3
Dependencies : F-034, F-041
```

**Layout:** CSS Grid `"header header" / "rail main" / "footer footer"` with rows `44px 1fr 30px`.

**Header:** SustenaLogo, sustain switcher, PROD·LIVE badge, operator name (B.GACHIENGU), UTC clock.

**Left rail:** Collapsible nav with 4 panel selectors. PAWA balance display (8,420 / −28hr / 68% bar).

**4 panels (toggled via left rail):**
1. Monitor Panel → F-043
2. Simulator Panel → F-044
3. Controller Panel → F-045
4. Library Panel → F-046

**Footer:** NOMINAL status dot, live metrics strip (LATENCY / OPS·MIN / ORCHIE LOAD / EVENTS / GAS / LAST SYNC / BUILD).

**Orchie FAB:** Bottom-right floating button, expands to 360px chat drawer, links to Orchie.html.

**File:** `brand/components/mcp-shell.jsx` (already prototyped — wire to backend)

**[CODE] Vibe-coding prompt:**
> "The Dashboard shell is prototyped in `brand/components/mcp-shell.jsx` (931 lines). Wire it to the live FastAPI backend. Connect: (1) TopBar UTC clock to `GET /health` response, (2) PAWA balance to `GET /users/{id}/pawa`, (3) Footer live metrics to `GET /platform/metrics` SSE stream (latency, ops/min, orchie load, events count, last sync). TWEAK_DEFAULTS are already defined in the component. Use design system in `brand/stylesheets/styles.css`."

---

### F-043 — Monitor Panel

```
Feature ID   : F-043
Feature Name : Monitor Panel — live system health dashboard
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.3.1
Dependencies : F-034, F-042
```

**Description:** 4-section dashboard: Sustain Network Graph (ReactFlow DAG of active sustains and connections), Event Stream (live scrolling event log filtered by domain), Operative Health (status cards for each operative — last run, last proposal, response time), System Alerts (SLA constraint violation cards).

**Data sources:** Platform Ops Sustain state + real-time `events` table SSE stream.

**[CODE] Vibe-coding prompt:**
> "Create `brand/components/mcp-monitor.jsx` per Sustena XII Master Strategy §12.3.1 (Monitor Panel). Sections: (1) Sustain Network: ReactFlow graph of active sustains, coloured by type (homestead #5090e0, biashara #E8A020, chama #9a7fb8, mkulima #2ab8a0). (2) Event Stream: SSE-driven live scroll of events from `GET /events/stream`, filterable by domain. (3) Operative Health: status cards per operative — last_run, last_proposal, response_time_ms, status (green/amber/red). (4) System Alerts: SLA constraint violations from Platform Ops Sustain alerts queue. Use design system colours. Export `MonitorPanel` to window."

---

### F-044 — Simulator Panel

```
Feature ID   : F-044
Feature Name : Simulator Panel — Monte Carlo simulation UI
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.3.1
Dependencies : F-006, F-042
```

**Description:** Allows the user (or developer) to select a sustain, define an operator sequence, and run `SustainEngine.simulate()` to preview the resulting state without committing. For Monte Carlo: run N simulations with variable input parameters, display result distribution as histogram (D3.js). Council uses this panel's output as the simulation_results attached to proposals.

**[CODE] Vibe-coding prompt:**
> "Create `brand/components/mcp-simulator.jsx` per Sustena XII Master Strategy §12.3.1 (Simulator Panel). Features: (1) Sustain selector + current state summary. (2) Operator sequence builder: drag-and-drop operator cards from Library, set parameters. (3) Single simulation: calls `POST /sustains/{id}/simulate`, shows before/after state diff. (4) Monte Carlo: N runs with parameter ranges — displays output distribution histogram using D3.js (colours from design system). (5) 'Submit to Council' button — attaches simulation results to a proposal. Use DAG node colours: state #5090e0, operator #E8A020, constraint #e05050, event #9a7fb8."

---

### F-045 — Controller Panel

```
Feature ID   : F-045
Feature Name : Controller Panel — live operator console and state inspector
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.3.1
Dependencies : F-042
```

**Description:** Two sub-panels: (a) **Operator Console** — xterm.js browser terminal for executing operators via CLI syntax. Tab autocomplete on operator names and parameter keys. (b) **State Inspector** — live JSON tree viewer with collapse/expand. Click any node to open edit modal (sends `state.set()` call via admin API for emergency state correction).

**[CODE] Vibe-coding prompt:**
> "Create `brand/components/mcp-controller.jsx` per Sustena XII Master Strategy §12.3.1 (Controller Panel). Two sub-panels: (1) Operator Console — embed xterm.js terminal. Command syntax: `execute sustain_id operator.name --param1=value1`. Tab completion via `GET /operators/autocomplete?prefix=`. (2) State Inspector — recursive JSON tree viewer with Tailwind styling matching design system. Clicking a leaf value opens an inline edit modal calling `PATCH /sustains/{id}/state` (admin-only endpoint with PIN confirmation). Use amber #E8A020 for active elements."

---

### F-046 — Library Panel

```
Feature ID   : F-046
Feature Name : Library Panel — Mycelium operator/operative browser
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.3.1
Dependencies : F-042
```

**Description:** Browseable registry of all operators and operatives — core platform + Mycelium community contributions. Card-based grid with: operator name, author, license_tier, pawa_cost, usage_count, rating. Filter by domain, license tier, and sustain type. Click card → full spec modal with ui_schema preview and "Deploy to Sustain" button.

**[CODE] Vibe-coding prompt:**
> "Create `brand/components/mcp-library.jsx` per Sustena XII Master Strategy §12.3.1 (Library Panel). Card grid of all operators from `GET /operators/registry`. Card fields: name, author, license_tier badge (free/standard/premium), pawa_cost, usage_count, rating (5-star). Filter bar: domain dropdown, license_tier, sustain_type. Click card: full-screen modal with operator spec, constraint list, ui_schema JSON preview, 'Deploy' button (calls `POST /sustains/{id}/operators/install`). Export `LibraryPanel` to window."

---

### F-047 — Orchie Mobile UI

```
Feature ID   : F-047
Feature Name : Orchie Mobile — full-screen iOS frame chat app
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.1, §7
Dependencies : F-041, F-025
```

**Description:** The Orchie.html page wraps the chat interface in an iOS device frame for demo purposes. Already prototyped in `brand/components/orchie-mobile.jsx`. Displays the full ORCHIE_DEMO scripted conversation with all widget types. Links back to Dashboard via "← MYCELIUM CONTROL PANEL" button.

**Fix required:** `brand/pages/Orchie.html` references JSX files with incorrect relative paths. Files are in `brand/components/` not `brand/pages/`.

**[CODE] Vibe-coding prompt:**
> "Fix `brand/pages/Orchie.html` — the script src paths currently reference files as if they're in the same directory (e.g., `src='ios-frame.jsx'`) but the JSX files are in `brand/components/`. Update all 4 script src paths to `../components/ios-frame.jsx`, `../components/core.jsx`, `../components/orchie-chat.jsx`, `../components/orchie-mobile.jsx`. Then verify the ORCHIE_DEMO scripted conversation in `orchie-chat.jsx` renders all 7 messages correctly including MetricWidget, PieWidget, LineWidget, MpesaWidget, AlertWidget, ProposalWidget. Design system: `brand/stylesheets/styles.css`."

---

## PART 6 — GENERATED UI SYSTEM

### F-050 — ui_schema Declaration Format

```
Feature ID   : F-050
Feature Name : ui_schema — operator and sustain UI declaration
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.4
Dependencies : F-005
```

**Description:** Every operator and operative declares its UI through a `ui_schema` block embedded in its metadata. This is the "spec IS the UI" principle. Three levels: operator card (single operation preview), operative dashboard (operative's full state window), sustain home (composite of operative panels).

**Operator ui_schema format:**
```json
{
  "sub_operator": "ui.render.operator_card",
  "widget_type": "budget_allocation_card",
  "fields": [
    { "label": "Pocket", "source": "inputs.pocket_name", "display": "badge" },
    { "label": "Amount", "source": "inputs.amount", "display": "currency_ksh" },
    { "label": "Remaining", "source": "state.finances.pockets.{pocket_name}.allocated - state.finances.pockets.{pocket_name}.spent", "display": "progress_bar", "colour_rule": "green_if_positive" }
  ],
  "ctas": ["Approve", "Modify", "Defer"]
}
```

**[CODE] Vibe-coding prompt:**
> "Implement `sustena/core/ui_renderer.py` per Sustena XII Master Strategy §12.4. `UIRenderer.render_operator_card(operator_meta, params, state) -> dict`: reads `operator_meta.ui_schema`, resolves field sources against state and params, returns a structured dict that the frontend can render as a widget. `UIRenderer.render_operative_dashboard(operative_meta, state) -> dict`. `UIRenderer.render_sustain_home(sustain_spec, state) -> dict`. Handle `override: true` for custom widget types (return raw widget_type and data; let frontend handle rendering). All `{placeholder}` interpolation in source paths must be resolved against params."

---

### F-051 — Widget Renderer (Frontend)

```
Feature ID   : F-051
Feature Name : Widget Renderer — frontend dynamic widget rendering
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.4
Dependencies : F-050, F-041
```

**Description:** The frontend widget renderer reads the `widget_type` from an operator's ui_schema response and renders the appropriate React component. If `widget_type` is unknown locally, it fetches the widget spec from the Mycelium Library (Phase 2). Phase 1: only built-in widget types are supported.

**Built-in widget types:**
- `budget_allocation_card` — fields + progress bars + CTA buttons
- `mentor_watchdog_panel` — pocket donut + burn sparkline + safe_to_spend + alert list
- `scheduler_summary_card` — task list + calendar preview
- `pantry_status_card` — item grid with colour-coded thresholds
- `asset_register_summary` — asset cards with warranty status
- `homestead_dashboard` — 4-panel composite

**[CODE] Vibe-coding prompt:**
> "Add to `brand/components/core.jsx`: a `WidgetRenderer` component that takes `{ widget_type, data }` props and renders the appropriate widget. Support: `budget_allocation_card`, `mentor_watchdog_panel`, `scheduler_summary_card`, `pantry_status_card`, `asset_register_summary`, `homestead_dashboard`. Use the existing MetricWidget, PieWidget, LineWidget, AlertWidget patterns already in `orchie-chat.jsx` as style references. Unknown `widget_type`: render a fallback card with the raw data as formatted JSON (for developer debugging). Export `WidgetRenderer` to window."

---

## PART 7 — M-PESA INTEGRATION

### F-060 — M-Pesa STK Push (WhatsApp Preview)

```
Feature ID   : F-060
Feature Name : M-Pesa STK push preview in Orchie
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §8.3, §11.3
Dependencies : F-025, F-040
```

**Description:** Phase 1 — Orchie shows an MpesaWidget preview card (recipient, amount, memo, till number) and waits for user confirmation before sending an STK push. Phase 1 uses IntaSend API (simpler onboarding than Daraja; no business account required initially). Phase 2: migrates to Safaricom Daraja API.

**Flow:**
1. Orchie identifies a payment opportunity (e.g., pantry restock at KES 1,120)
2. Renders MpesaWidget: `{ recipient, amount, memo, till, sustain }`
3. User taps "Pay" → Orchie calls IntaSend STK push API
4. User receives M-Pesa prompt on phone → enters PIN
5. IntaSend callback → Orchie updates `budget.spend()` in state
6. Orchie confirms: "Umelipa ✓ Mama Mboga 1,120 KSH"

**[CODE] Vibe-coding prompt:**
> "Create `sustena/core/mpesa_client.py` per Sustena XII Master Strategy §11.3. Phase 1: IntaSend API integration. `MpesaClient.initiate_stk_push(phone, amount, memo, account_ref) -> str` (returns checkout_id). `MpesaClient.check_status(checkout_id) -> dict`. Create `POST /webhook/mpesa` endpoint to receive IntaSend callbacks — on success: call `budget.spend()` to record the payment in state. MpesaWidget (already in `orchie-chat.jsx`) shows preview before execution. Phase 2 note: document exactly which methods map to Daraja B2C API calls for future migration."

---

### F-061 — M-Pesa SMS Classification

```
Feature ID   : F-061
Feature Name : M-Pesa SMS parser and classifier
Phase        : 0
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §8.1
Dependencies : F-040
```

**Description:** When a user forwards an M-Pesa SMS to Orchie on WhatsApp, Haiku classifies the transaction type and extracts structured data. Classification accuracy SLA: > 92% (enforced by Platform Ops constraint). Handles all major M-Pesa transaction types.

**SMS patterns:**
```
SEND:     "Confirmed. Ksh200.00 sent to JANE WANJIKU ... Your new M-PESA balance is Ksh3,800.00"
RECEIVE:  "Confirmed. You have received Ksh2,000.00 from JOHN MWANGI ... balance is Ksh5,800.00"
PAYBILL:  "Confirmed. Ksh850.00 paid to 000200 KPLC PREPAID ... balance is Ksh4,950.00"
BUY_GOODS:"Confirmed. Ksh250.00 paid to 5826141 MAMA MBOGA ... balance is Ksh4,700.00"
```

**[CODE] Vibe-coding prompt:**
> "Create `sustena/core/mpesa_parser.py` per Sustena XII Master Strategy §8.1. Use Haiku to classify and extract from raw M-Pesa SMS text. `MpesaParser.classify(sms_text) -> dict`: fields: transaction_type (SEND_MONEY|RECEIVE_MONEY|PAYBILL|BUY_GOODS|WITHDRAW|AIRTIME|DEPOSIT), amount (float), counterparty (str), reference (str|None), balance_after (float), timestamp_from_sms (datetime|None). System prompt includes examples of all 6 transaction types. Falls back to regex extraction if Haiku confidence < 0.85. Accuracy target: >92% on test set of 100 real M-Pesa SMS samples."

---

## PART 8 — SIMULATION ENGINE

### F-070 — Monte Carlo Simulation Engine

```
Feature ID   : F-070
Feature Name : Monte Carlo simulation engine
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.3.1, §4
Dependencies : F-006
```

**Description:** Extension of `SustainEngine.simulate()`. Instead of a single deterministic operator sequence, Monte Carlo runs N paths with variable input parameters sampled from configurable distributions. Returns: outcome distribution (histogram data), path statistics (mean, p5, p95 outcomes), most likely path, and worst-case path.

**Use cases:**
- "If I increase food budget by KES 500, what is the probability of hitting my savings goal by December?"
- "What is the distribution of chama pool sizes after 12 months with variable contribution compliance rates?"
- "What is Vyyb's most likely monthly revenue under current recipe margins?"

**[CODE] Vibe-coding prompt:**
> "Create `sustena/core/simulation.py` per Sustena XII Master Strategy §12.3.1 (Monte Carlo). `MonteCarloEngine.run(sustain_id, operator_sequence_template, parameter_distributions, n_runs=1000) -> SimulationResult`. `parameter_distributions`: dict of param_name → {distribution: 'uniform'|'normal'|'triangular', min, max, mean, std}. `SimulationResult`: outcome_histogram, mean_outcome, p5_outcome, p95_outcome, most_likely_path, worst_case_path. All N runs: fork state, run sequence, collect terminal state metric. No DB writes. Return as JSON-serialisable dict for Simulator Panel D3.js histogram rendering."

---

## PART 9 — MYCELIUM MARKETPLACE

### F-080 — Mycelium v1 (Operator/Operative Registry)

```
Feature ID   : F-080
Feature Name : Mycelium v1 — community operator and operative marketplace
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §10
Dependencies : F-005, F-004
```

**Description:** Phase 1 Mycelium is a curated registry — not yet open-contribution. Core platform operators and operatives are published by `author='sustena_core'`. The architecture supports community contributions but the review/publish pipeline is not yet live. The shared `events` table with `destination='mycelium.public'` is the Phase 1 mock of cross-sustain event routing.

**Phase 1 deliverables:**
1. `GET /operators/registry` — list all registered operators with metadata
2. `GET /operators/{name}` — full spec for a single operator
3. `GET /operatives/registry` — list all operatives
4. `POST /operators/install` — install an operator into a sustain (from registry)
5. Pawa royalty split recorded in `pawa_ledger` on each install

**Phase 2 additions:** Submission portal, sandbox testing harness, promotion pipeline (Sandbox → Library on 50 successful runs).

**[CODE] Vibe-coding prompt:**
> "Create `sustena/api/routes/mycelium.py` per Sustena XII Master Strategy §10. Endpoints: `GET /operators/registry`, `GET /operators/{name}`, `GET /operatives/registry`, `POST /operators/install` (body: {sustain_id, operator_name}). On install: record pawa royalty split via `ComponentLicense.charge()` — contributor 70%, treasury 20%, referrer 5%, validator 5%. Phase 1: all operators are `sustena_core` authored; contributor = sustena_core treasury. Phase 2: open contributor submissions. Return full operator metadata including ui_schema."

---

## PART 10 — IOT INTEGRATION

### F-090 — IoT MQTT Bridge

```
Feature ID   : F-090
Feature Name : IoT MQTT bridge — sensor-to-event pipeline
Phase        : 2
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §14.1
Dependencies : F-003, F-006
```

**Description:** MQTT broker (Mosquitto on Raspberry Pi KES 8,000) receives sensor messages. Sustena IoT Gateway subscribes to MQTT topics and translates sensor readings into Sustena events (`event.iot.pantry_weight_updated`, `event.iot.electricity_reading`). These events trigger operative evaluation the same way any other event does — no special IoT code path.

**Architecture:** `Physical Sensor → MQTT Broker → sustena/core/iot_gateway.py → EventBus`

**[CODE] Vibe-coding prompt:**
> "Create `sustena/core/iot_gateway.py` per Sustena XII Master Strategy §14.1. `IoTGateway` subscribes to MQTT topics (paho-mqtt library). Topic format: `sustena/{sustain_id}/{device_type}/{device_id}`. On message: validate payload schema, translate to Sustena event format, publish to EventBus. Supported device types: weight_scale (pantry → `event.iot.pantry_weight_updated`), smart_meter (electricity → `event.iot.electricity_reading`), soil_moisture (farm → `event.iot.soil_moisture_updated`). Include a simulated sensor publisher for testing (`sustena/tests/iot_sim.py`)."

---

## PART 11 — TOKEN ECONOMY

### F-110 — Pawa Token (Phase 1 Mock)

```
Feature ID   : F-110
Feature Name : Pawa utility token — Phase 1 SQLite mock
Phase        : 0/1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §9.5
Dependencies : F-004
```

**Description:** Already implemented via F-004 (PawaLedger). This feature record covers the earning mechanisms.

**Earning mechanisms:**
- Contributing an operator to Mycelium: 100 pawa on approval
- Operator usage royalty: pawa per use (70% of pawa_cost to contributor)
- Onboarding a new sustain via referral: 50 pawa
- Completing sustain milestones (first budget allocation, first month on-track): 25 pawa each
- Data contribution consent grant: 10 pawa/month per active sustain

**Spending:**
- Each operator execution: 0–5 pawa (most free in Phase 1, premium operators cost pawa)
- Mycelium operator install: 20 pawa
- Monte Carlo simulation run: 2 pawa per 100 simulations

**[CODE] Vibe-coding prompt:**
> "Create `sustena/core/pawa_rewards.py` per Sustena XII Master Strategy §9.5. `PawaRewards.on_operator_contributed(user_id, operator_name)` → credits 100 pawa. `on_sustain_milestone(user_id, sustain_id, milestone_type)` → credits 25 pawa per milestone. `on_referral_signup(referrer_id, new_user_id)` → credits 50 pawa to referrer. `on_data_consent_grant(user_id, sustain_id)` → credits 10 pawa/month (scheduled). All earnings logged to `pawa_ledger` with reason string. Integrate with PawaLedger from F-004."

---

## PART 12 — API AND DEVELOPER EXPERIENCE

### F-120 — FastAPI Application and Routes

```
Feature ID   : F-120
Feature Name : FastAPI application — full API surface
Phase        : 0/1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12.3
Dependencies : F-006, F-007
```

**API surface:**
```
GET  /health
POST /users
GET  /users/{id}
POST /sustains
GET  /sustains/{id}
GET  /sustains/{id}/state
POST /sustains/{id}/operators/{name}
POST /sustains/{id}/simulate
GET  /sustains/{id}/events
GET  /sustains/{id}/proposals
POST /sustains/{id}/proposals/{id}/vote
GET  /sustains/{id}/export
GET  /operators/registry
GET  /operators/{name}
POST /operators/install
GET  /operatives/registry
GET  /orchie/history/{sustain_id}
POST /orchie/message
GET  /orchie/stream (SSE)
GET  /platform/metrics (SSE)
GET  /admin/support-queue
PATCH /admin/support-queue/{id}
GET  /webhook (Meta verify)
POST /webhook (Meta inbound)
POST /webhook/mpesa (IntaSend callback)
```

**[CODE] Vibe-coding prompt:**
> "Create the full FastAPI application in `sustena/api/main.py` and all route files per Sustena XII Master Strategy §12.3. CORS middleware (restrict to known origins in prod), request logging middleware (method, path, response_time, status), JWT auth on all `/api/v1/*` endpoints. Dependency injection: database session, claude client. OpenAPI docs in dev only. Response envelope: `{status, data, error, timestamp}`. Add `GET /sustains/{id}/export` returning full sustain spec + current state as downloadable JSON (per Document Analysis §4.3). SSE endpoints for `/orchie/stream` and `/platform/metrics`."

---

### F-121 — Developer Documentation (MkDocs)

```
Feature ID   : F-121
Feature Name : Developer documentation — docs.sustena.io
Phase        : 1
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §12, §13
Dependencies : F-120
```

**Sections:**
1. Getting Started — what is Sustena, the 7 primitives
2. Operator Reference — auto-generated from operator metadata
3. DSL Guide — constraint language reference with examples
4. API Reference — auto-generated from OpenAPI spec
5. Sustain Templates — gallery
6. Contributing to Mycelium — how to publish

**[CODE] Vibe-coding prompt:**
> "Set up MkDocs at `docs/` per Sustena XII Master Strategy §13 (Media & Brand). `mkdocs.yml`: site name 'Sustena Developer Docs', theme material (dark, amber accent #E8A020), plugins: mkdocstrings, search. Create initial pages: index.md (what is Sustena, the thesis, 7 primitives in plain English), primitives.md (each primitive with Python example), dsl.md (full constraint DSL reference with 10 example constraints), operators.md (how to write and register an operator with `@sustena_operator`), contributing.md (how to submit to Mycelium Library). `make docs` runs `mkdocs serve`."

---

## PART 13 — PHASE 2 AND PHASE 3 FEATURES

### F-130 — React Native Mobile App (Phase 2)

```
Feature ID   : F-130
Feature Name : React Native app — iOS and Android
Phase        : 2
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §11.2, §12
Dependencies : F-041, F-120
```

**Description:** Native mobile app wrapping the Orchie web chat interface and the homestead/biashara dashboard. WhatsApp remains the primary channel in Phase 2; the native app provides push notifications, biometric auth, and the full dashboard experience on mobile.

**[CODE] Vibe-coding prompt (Phase 2):**
> "Scaffold a React Native (Expo) app in `sustena-mobile/`. Screens: (1) Onboarding (3-step: name, income, goal — same as WhatsApp flow), (2) Home (Orchie chat — reuse JSX component logic from `orchie-chat.jsx`), (3) Dashboard (homestead/biashara summary widgets), (4) Notifications (Council proposals and Orchie alerts). Auth: biometric (expo-local-authentication). Push notifications: Expo Push Notifications → backend sends via `POST /notifications/push`. Use React Query for API calls. Design system: same CSS variables translated to React Native StyleSheet."

---

### F-131 — Blockchain Mock → Hybrid → Full (Phase 1/2/3)

```
Feature ID   : F-131
Feature Name : Blockchain architecture — 3-stage migration
Phase        : 1/2/3
Owner        : Bonnie
Status       : [ ]
Strategy Ref : §9.5
Dependencies : F-004
```

**Stage 1 (Phase 1, now–Month 12):** Pure Python mock. `UserPolicy`, `ProposalRecord`, `PawaLedger` as Python classes backed by SQLite. Log structure mirrors ERC-20 events exactly.

**Stage 2 (Phase 2, Month 12–18):** Hybrid. High-value multi-party transactions (chama loan disbursements > KES 20,000, large procurement POs) go on-chain to Ethereum Sepolia testnet via Web3.py. Everything else stays SQLite.

**Stage 3 (Phase 3, Month 18+):** Full on-chain. Optimism or Arbitrum L2. `chargeRoyalty()` Solidity function. ZK-rollup proofs for privacy. IPFS for sustain state snapshots. STC token launch (CMA sandbox approval required first).

**[CODE] Vibe-coding prompt (Stage 2 — Phase 2):**
> "Create `sustena/blockchain/hybrid.py` per Sustena XII Master Strategy §9.5. Uses Web3.py. `HybridLedger.should_go_onchain(operator_name, params) -> bool`: returns True for chama loan disburse > KES 20,000 and procurement POs > KES 50,000. `submit_onchain(action_type, data) -> str`: signs and submits tx to Sepolia testnet; returns tx hash. Fallback: if Sepolia unreachable, fall back to SQLite mock and queue for later submission. Store: `blockchain_transactions` SQLite table with (id, tx_hash, action_type, data_json, submitted_at, confirmed_at, chain)."

---

## APPENDIX — FEATURE SUMMARY TABLE

| Feature ID | Name | Phase | Owner | Priority |
|---|---|---|---|---|
| F-001 | StateAccessor | 0 | Bonnie | 🔴 Critical |
| F-002 | ConstraintEngine | 0 | Bonnie | 🔴 Critical |
| F-003 | EventBus | 0 | Bonnie | 🔴 Critical |
| F-004 | PawaLedger | 0 | Bonnie | 🔴 Critical |
| F-005 | OperatorRegistry | 0 | Bonnie | 🔴 Critical |
| F-006 | SustainEngine | 0 | Bonnie | 🔴 Critical |
| F-007 | CouncilDAO | 0 | Bonnie | 🔴 Critical |
| F-010 | budget.* operators | 0 | Bonnie | 🔴 Critical |
| F-011 | chama.* operators | 1 | Bonnie | 🔴 Critical |
| F-012 | vyyb.* operators | 1 | Bonnie | 🟡 Important |
| F-013 | mkulima.* operators | 1 | Bonnie | 🟡 Important |
| F-014 | procurement.* operators | 1 | Bonnie | 🟡 Important |
| F-015 | platform.* operators | 1 | Bonnie | 🟡 Important |
| F-020 | Mentor operative | 1 | Bonnie | 🔴 Critical |
| F-021 | Protégé operative | 1 | Bonnie | 🟡 Important |
| F-022 | Curator operative | 1 | Bonnie | 🟡 Important |
| F-023 | Attaché operative | 1 | Bonnie | 🟢 Nice-to-have |
| F-024 | Navigator operative | 1 | Bonnie | 🟡 Important |
| F-025 | Orchie operative | 0 | Bonnie | 🔴 Critical |
| F-026 | Staff Orchie | 1 | Bonnie | 🟡 Important |
| F-027 | Chama Secretary | 1 | Bonnie | 🟡 Important |
| F-030 | Homestead Sustain | 0 | Bonnie | 🔴 Critical |
| F-031 | Biashara Sustain | 1 | Bonnie | 🟡 Important |
| F-032 | Chama Sustain | 1 | Bonnie | 🟡 Important |
| F-033 | Mkulima Sustain | 1 | Bonnie | 🟡 Important |
| F-034 | Platform Ops Sustain | 1 | Bonnie | 🟡 Important |
| F-040 | WhatsApp Bot | 0 | Bonnie | 🔴 Critical |
| F-041 | Orchie Web Chat | 1 | Bonnie | 🔴 Critical |
| F-042 | Dashboard Shell | 1 | Bonnie | 🟡 Important |
| F-043 | Monitor Panel | 1 | Bonnie | 🟡 Important |
| F-044 | Simulator Panel | 1 | Bonnie | 🟡 Important |
| F-045 | Controller Panel | 1 | Bonnie | 🟡 Important |
| F-046 | Library Panel | 1 | Bonnie | 🟡 Important |
| F-047 | Orchie Mobile UI | 1 | Bonnie | 🟡 Important |
| F-050 | ui_schema format | 1 | Bonnie | 🟡 Important |
| F-051 | Widget Renderer | 1 | Bonnie | 🟡 Important |
| F-060 | M-Pesa STK Push | 1 | Bonnie | 🔴 Critical |
| F-061 | M-Pesa SMS Parser | 0 | Bonnie | 🔴 Critical |
| F-070 | Monte Carlo Engine | 1 | Bonnie | 🟡 Important |
| F-080 | Mycelium v1 | 1 | Bonnie | 🟡 Important |
| F-090 | IoT MQTT Bridge | 2 | Bonnie | 🟢 Nice-to-have |
| F-110 | Pawa Rewards | 1 | Bonnie | 🟡 Important |
| F-120 | FastAPI full surface | 1 | Bonnie | 🔴 Critical |
| F-121 | Developer Docs | 1 | Bonnie | 🟡 Important |
| F-130 | React Native App | 2 | Bonnie | 🟡 Important |
| F-131 | Blockchain 3-stage | 1/2/3 | Bonnie | 🟡 Important |

---

*Document ends. Version 1.0. Living document — update as features are built, tested, and revised.*
*Cross-reference: Sustena_XII_Master_Strategy.md for full narrative context per section.*
