# Sustena Dot Protocol — Complete DSL Reference
### Companion Document to the Sustena XII Master Strategy
**Version 1.0 — May 2026 · CONFIDENTIAL**
**Cross-reference:** Sustena_XII_Master_Strategy.md (Sections 4.1, 5.3, 8.1, 9.4)

---

## About This Document

The Sustena platform expresses system behaviour through a small, composable vocabulary. Every operator call, every state path reference, every constraint predicate, and every event name follows a strict dot notation — the **dot protocol**. This document is the complete specification of that protocol: its grammar, its namespacing rules, its type system, and its real-world usage patterns.

Sustena's dot protocol is a **naming convention + evaluation grammar** that sits on top of Python function calls and JSON state documents. Understanding it is the precondition for writing operators, declaring constraints, specifying sustain state, and building operatives.

---

## Part 1 — The Dot Protocol: Core Grammar

### 1.1 What the Dot Represents

A dot (`.`) in Sustena always means one of two things:

1. **Namespace separator** (in operator names): separates the domain from the action
   - `budget.allocate` → domain `budget`, action `allocate`
   - `sales.fulfill` → domain `sales`, action `fulfill`
   - `mkulima.broadcast_supply_signal` → domain `mkulima`, action `broadcast_supply_signal`

2. **Path separator** (in state references): traverses nested state structure
   - `state.finances.pockets.food.allocated` → state → finances → pockets → food → allocated
   - `state.orders.daily_revenue` → state → orders → daily_revenue
   - `state.members.colo.income.monthly` → state → members → colo → income → monthly

These two uses are syntactically identical — both are dot-separated identifiers. Context (whether you are in an operator declaration, a constraint predicate, or a state reference expression) determines the interpretation.

### 1.2 Operator Name Grammar

```
operator_name ::= namespace "." action_name
namespace     ::= domain_segment ("." domain_segment)*
domain_segment ::= [a-z][a-z0-9_]*
action_name   ::= [a-z][a-z0-9_]*
```

**Examples:**

| Operator Name | Namespace | Action |
|---|---|---|
| `budget.allocate` | `budget` | `allocate` |
| `sales.fulfill` | `sales` | `fulfill` |
| `mkulima.broadcast_supply_signal` | `mkulima` | `broadcast_supply_signal` |
| `mkulima.receive_signal` | `mkulima` | `receive_signal` |
| `procurement.raise_po` | `procurement` | `raise_po` |
| `platform.api.record_request` | `platform.api` | `record_request` |
| `platform.llm.record_call` | `platform.llm` | `record_call` |
| `ui.render.sustain_home` | `ui.render` | `sustain_home` |
| `vyyb.production.start_batch` | `vyyb.production` | `start_batch` |
| `homestead.calendar.add_event` | `homestead.calendar` | `add_event` |
| `chama.contribution.record` | `chama.contribution` | `record` |

**Rules:**
- Lowercase only. No uppercase, no camelCase, no kebab-case. Use `snake_case` for multi-word names.
- Namespaces are hierarchical: `platform.api` is a child of `platform`. Child operators inherit the parent's registry entry.
- The final segment before the last dot is always the namespace; the last segment is always the action.
- There is no practical depth limit on namespace nesting, but convention caps at 3 levels (`domain.subdomain.action`).

### 1.3 State Path Grammar

```
state_path    ::= "state" ("." path_segment)+
path_segment  ::= identifier | "[" index "]"
identifier    ::= [a-zA-Z_][a-zA-Z0-9_]*
index         ::= [0-9]+
```

**Examples:**

| State Path | Type | Description |
|---|---|---|
| `state.finances.pockets.food.allocated` | `float` | Allocated budget for the food pocket |
| `state.finances.pockets.food.spent` | `float` | Actual spend in the food pocket |
| `state.finances.liquid.balance` | `float` | Current liquid savings balance |
| `state.members.colo.income.monthly` | `float` | Colo's monthly income |
| `state.orders.daily_revenue` | `float` | Today's total revenue |
| `state.inventory.items.tomato.quantity_kg` | `float` | Tomatoes in stock, kg |
| `state.production.active_batches[0].recipe_id` | `str` | Recipe ID of first active batch |
| `state.procurement.mkulima_supply_signals` | `list` | Array of incoming supply signals |
| `state.staff.roster[2].shift_start` | `datetime` | Third staff member's shift start time |

**Rules:**
- Always begins with `state`. The root of the state tree is always `state`, never the sustain name.
- Path segments traverse the sustain's state JSON document in order.
- Array indexing uses bracket notation: `state.staff.roster[0]` is the first element of `roster`.
- Paths that do not exist in the current state raise a `StatePathError` — not a silent null. This is intentional: bad paths should fail loudly.

---

## Part 2 — Operator Declarations

### 2.1 The Operator Function Signature

An operator in Sustena is a Python function decorated with `@sustena_operator`. The decorator registers the function in the Operator Registry and associates it with its dot-protocol name.

```python
from sustena.core import sustena_operator, OperatorContext, OperatorResult

@sustena_operator(
    name="budget.allocate",
    description="Allocate funds from a sustain's liquid balance into a named pocket.",
    input_schema={
        "pocket_name": {"type": "string", "description": "Name of the budget pocket (e.g., 'food', 'transport')"},
        "amount": {"type": "number", "description": "KES amount to allocate"},
        "period": {"type": "string", "enum": ["weekly", "monthly"], "description": "Budget period"}
    },
    output_schema={
        "pocket_name": {"type": "string"},
        "allocated": {"type": "number"},
        "previous_balance": {"type": "number"},
        "new_balance": {"type": "number"},
        "timestamp": {"type": "string", "format": "datetime"}
    },
    side_effects=["state.finances.pockets.{pocket_name}.allocated", "state.finances.liquid.balance"],
    constraints=["budget.allocate.amount > 0", "budget.allocate.amount <= state.finances.liquid.balance"],
    license_tier="free",  # or "per_use", "subscription", "one_time"
    pawa_cost=0,
    author="sustena_core"
)
def budget_allocate(ctx: OperatorContext, pocket_name: str, amount: float, period: str) -> OperatorResult:
    """
    Allocate funds from liquid balance into a named pocket.
    Creates the pocket if it does not exist.
    """
    current_balance = ctx.state.get("finances.liquid.balance")
    
    if amount > current_balance:
        return OperatorResult.fail(
            reason=f"Insufficient liquid balance: need {amount}, have {current_balance}",
            constraint_violated="budget.allocate.amount <= state.finances.liquid.balance"
        )
    
    # Apply state mutation
    ctx.state.set(f"finances.pockets.{pocket_name}.allocated", amount)
    ctx.state.set(f"finances.pockets.{pocket_name}.period", period)
    ctx.state.decrement("finances.liquid.balance", amount)
    
    return OperatorResult.ok({
        "pocket_name": pocket_name,
        "allocated": amount,
        "previous_balance": current_balance,
        "new_balance": ctx.state.get("finances.liquid.balance"),
        "timestamp": ctx.timestamp.isoformat()
    })
```

### 2.2 Operator Metadata Fields

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | `str` | ✅ | Dot-protocol name; must be unique in the registry |
| `description` | `str` | ✅ | Human-readable description; shown in Mycelium Library and Orchie explanations |
| `input_schema` | `dict` | ✅ | JSON Schema for input parameters |
| `output_schema` | `dict` | ✅ | JSON Schema for return value |
| `side_effects` | `list[str]` | ✅ | State paths this operator may write to; used by the Simulator to plan forks |
| `constraints` | `list[str]` | ✅ | Precondition constraint predicates (see Part 3) |
| `license_tier` | `str` | ✅ | One of: `free`, `per_use`, `subscription`, `one_time` |
| `pawa_cost` | `int` | ✅ | Pawa charged per call (0 for free tier) |
| `author` | `str` | ✅ | Contributor ID or `sustena_core` for platform operators |
| `tags` | `list[str]` | ❌ | Search tags in the Mycelium Library |
| `deprecated` | `bool` | ❌ | If `true`, operator still runs but Orchie warns; old sustains continue working |
| `version` | `str` | ❌ | Semver string; defaults to `"1.0.0"` |
| `ui_schema` | `dict` | ❌ | Declares the `ResponseWidget` this operator returns (see Section 2.4) |

### 2.3 The OperatorContext Object

Every operator receives a `ctx: OperatorContext` as its first argument. This object provides access to the sustain's runtime without requiring direct database calls.

```python
class OperatorContext:
    state: StateAccessor         # Read/write access to sustain state
    timestamp: datetime          # Execution timestamp (from platform clock, not system)
    sustain_id: str              # ID of the sustain executing this operator
    operative_id: str | None     # ID of the calling operative (None if direct call)
    user_id: str                 # Owner/user of the sustain
    pawa_ledger: PawaLedger      # Access to pawa balance for royalty deduction
    event_bus: EventBus          # Publish events to the sustain's event log
    logger: OperatorLogger       # Structured logging (appears in Monitor Panel)
```

**StateAccessor methods:**

```python
ctx.state.get("finances.liquid.balance")                    # Read a path → value
ctx.state.set("finances.pockets.food.allocated", 5000.0)    # Write a path
ctx.state.increment("finances.liquid.balance", 10000.0)     # Atomic increment
ctx.state.decrement("finances.liquid.balance", 2500.0)      # Atomic decrement
ctx.state.append("orders.active_orders", new_order)         # Append to array
ctx.state.remove("procurement.mkulima_supply_signals", id)  # Remove from array by ID
ctx.state.exists("finances.pockets.food")                   # True/False existence check
ctx.state.snapshot()                                        # Returns full state dict (read-only)
```

### 2.4 The ui_schema Sub-Operator

Every operator can declare a `ui_schema` in its metadata. This sub-operator returns a typed `ResponseWidget` that the Sustena client renders without any additional UI code.

```python
@sustena_operator(
    name="budget.summary",
    description="Return a budget summary for the current period.",
    input_schema={},
    output_schema={"widget": {"type": "object", "$ref": "#/definitions/ResponseWidget"}},
    side_effects=[],
    constraints=[],
    license_tier="free",
    pawa_cost=0,
    author="sustena_core",
    ui_schema={
        "sub_operator": "ui.render.budget_ring",
        "widget_type": "budget_ring",
        "fields": {
            "total_income": "state.finances.income.monthly",
            "total_allocated": "state.finances.pockets",        # aggregate across all pockets
            "total_spent": "state.finances.pockets",            # aggregate spent
            "liquid_balance": "state.finances.liquid.balance"
        },
        "theme": "homestead_default",
        "override": False  # True = bypass Sustena default; use custom widget
    }
)
def budget_summary(ctx: OperatorContext) -> OperatorResult:
    ...
```

**Key `ui_schema` fields:**

| Field | Description |
|---|---|
| `sub_operator` | The `ui.render.*` operator that constructs the widget |
| `widget_type` | The registered widget type identifier (must match a registered `ResponseWidget` type in the Mycelium) |
| `fields` | Map of widget field names to state paths — the sub-operator populates the widget from these paths |
| `theme` | Default theme from the Sustena Design System, or a custom theme identifier |
| `override` | If `true`, bypasses Sustena's default rendering and uses the custom widget exclusively; only complex or domain-specific queries should set this |

A custom `ui_schema` with `override: true` is appropriate for domain-specific displays (e.g., a KDS widget for a kitchen display system, or a farm sensor map for Mkulima). For standard financial or scheduling displays, Sustena's default widgets are preferred — they stay consistent across the platform and require no maintenance.

---

## Part 3 — The Constraint Predicate DSL

### 3.1 What Constraints Are

Constraints in Sustena are **preconditions** — boolean expressions that must evaluate to `true` before an operator executes, or `true` at all times for a sustain to remain valid. They are declared in two places:

1. **Operator constraints** — must be true for the operator call to proceed
2. **Sustain invariants** — must be true at all times; checked after every operator execution

Constraints are expressed as DSL strings. They are evaluated by the Sustena constraint engine (a small Python expression evaluator with controlled scope — not `eval()`).

### 3.2 Constraint Grammar

```
constraint_expr ::= simple_expr | compound_expr | quantifier_expr | temporal_expr
simple_expr     ::= path_or_value operator path_or_value
compound_expr   ::= constraint_expr ("AND" | "OR") constraint_expr
                  | "NOT" constraint_expr
                  | "(" constraint_expr ")"
quantifier_expr ::= "ALL" "[" path "]" "." attribute operator value
                  | "EXISTS" "[" path "]" "." attribute operator value
temporal_expr   ::= "WITHIN" duration constraint_expr
                  | "AFTER" timestamp constraint_expr
                  | "BEFORE" timestamp constraint_expr
path_or_value   ::= state_path | operator_param | literal
operator        ::= ">" | ">=" | "<" | "<=" | "==" | "!=" | "IN" | "NOT IN"
duration        ::= number ("hours" | "days" | "weeks" | "months")
```

### 3.3 Constraint Examples — Simple Predicates

**Operator preconditions (evaluated before execution):**

```
# budget.allocate — cannot allocate more than you have
budget.allocate.amount > 0
budget.allocate.amount <= state.finances.liquid.balance

# budget.spend — cannot spend from a pocket more than allocated
budget.spend.amount > 0
budget.spend.amount <= state.finances.pockets.{pocket_name}.allocated
budget.spend.pocket_name IN state.finances.pockets

# sales.fulfill — can only fulfill an order that exists and is active
sales.fulfill.order_id IN state.orders.active_orders
state.orders.active_orders[{order_id}].status == "pending"

# mkulima.broadcast_supply_signal — must have produce to broadcast
mkulima.broadcast_supply_signal.quantity_kg > 0
state.inventory.items.{produce_type}.quantity_kg >= mkulima.broadcast_supply_signal.quantity_kg

# procurement.raise_po — PO can only be raised after Council PASSED
state.procurement.pending_proposal.status == "PASSED"
procurement.raise_po.quantity_kg <= procurement.raise_po.committed_kg
```

**Sustain invariants (checked after every operation):**

```
# Homestead — liquid balance must never go negative
state.finances.liquid.balance >= 0

# Vyyb Biashara — daily COGS must not exceed 90% of daily revenue (gross margin floor)
state.orders.daily_cogs <= state.orders.daily_revenue * 0.90

# Chama — member contribution balance must never exceed declared ceiling
ALL [state.members].contribution_balance <= state.rules.max_contribution_per_member

# Mkulima — cannot commit more produce than exists in the ground
state.procurement.committed_produce.total_kg <= state.crops.total_planted_yield_forecast_kg

# Platform — SLA: no operator execution must take longer than 30 seconds
WITHIN 30 seconds platform.operator.execution_time <= 30
```

### 3.4 Compound Constraints

```
# Budget: either income has been received this month OR deferred spending is flagged
state.finances.income.received_this_month == True 
  OR state.finances.deferred_spending_flag == True

# Staff: a shift can start only if the staff member is not already on shift AND the outlet is open
NOT EXISTS [state.staff.active_shifts].staff_id == homestead.calendar.add_shift.staff_id
  AND state.outlets.{outlet_id}.status == "open"

# Procurement: a supply signal can be accepted only if quality grade and price both match
mkulima.receive_signal.price_per_kg <= state.procurement.spec.max_price_per_kg
  AND mkulima.receive_signal.quality_grade IN state.procurement.spec.accepted_grades
  AND mkulima.receive_signal.quantity_kg >= state.procurement.spec.min_order_kg
```

### 3.5 Quantifier Constraints

Quantifier constraints apply a condition to every element of an array state path.

```
# ALL — every active batch must have a recipe ID assigned
ALL [state.production.active_batches].recipe_id != null

# ALL — every pocket must have a non-negative balance
ALL [state.finances.pockets].allocated >= 0

# EXISTS — there must be at least one staff member currently on shift
EXISTS [state.staff.active_shifts].status == "active"

# EXISTS — at least one supply signal must meet Vyyb's quality threshold
EXISTS [state.procurement.mkulima_supply_signals].quality_grade IN ["A", "A+"]
```

### 3.6 Temporal Constraints

```
# A deferred spending approval expires after 48 hours
WITHIN 48 hours state.procurement.pending_proposal.status == "PASSED"

# KRA VAT filing must be submitted before the 20th of each month
BEFORE 20th_of_month state.tax.vat_filing_status == "submitted"

# Chama loan repayment must begin within 30 days of disbursement
WITHIN 30 days state.loans.{loan_id}.first_repayment_date != null

# A supply signal commitment must be honoured within the harvest window
WITHIN state.procurement.pending_po.harvest_window_days days
  state.procurement.pending_po.delivery_confirmation == True
```

### 3.7 Constraint Violation Responses

When a constraint fails, the operator halts and returns an `OperatorResult.fail()` with:

- The constraint string that was violated
- The evaluated values of all referenced paths at the time of evaluation
- A human-readable explanation (generated by Orchie, not the constraint engine)

The Sustena UI renders this as an alert on the appropriate widget, with the Orchie explanation surfaced inline. Council proposals that would violate a sustain invariant are rejected at proposal time, before execution — the Simulator runs all constraints before the Council deliberates.

---

## Part 4 — Event DSL

### 4.1 Event Names

Events in Sustena follow the same dot protocol as operators, but with the domain prefix `event.`:

```
event.finances.income_received
event.finances.pocket_exceeded
event.production.batch_started
event.production.batch_completed
event.orders.order_placed
event.orders.order_fulfilled
event.procurement.supply_signal_received
event.procurement.po_raised
event.procurement.delivery_confirmed
event.council.proposal_created
event.council.vote_cast
event.council.proposal_passed
event.council.proposal_failed
event.council.overridden_by_user
event.platform.operator_called
event.platform.sla_breach
event.mycelium.component_published
event.mycelium.royalty_charged
```

### 4.2 Publishing Events

Operators publish events via the `event_bus` on the OperatorContext:

```python
ctx.event_bus.publish(
    name="event.procurement.supply_signal_received",
    payload={
        "signal_id": signal_id,
        "produce": signal["produce"],
        "quantity_kg": signal["quantity_kg"],
        "price_per_kg": signal["price_per_kg"],
        "farm_id": signal["farm_id"],
        "harvest_window_days": signal["harvest_window_days"]
    },
    timestamp=ctx.timestamp
)
```

### 4.3 Event Triggers in Operative Configuration

Operatives declare their event subscriptions in their configuration JSON:

```json
{
  "operative_id": "biashara.vyyb",
  "event_subscriptions": [
    {
      "event": "event.procurement.supply_signal_received",
      "handler": "biashara.evaluate_supply_signal",
      "filter": {
        "payload.produce": { "$in": ["state.procurement.spec.accepted_produces"] }
      }
    },
    {
      "event": "event.orders.order_placed",
      "handler": "biashara.update_demand_forecast",
      "filter": null
    },
    {
      "event": "event.finances.pocket_exceeded",
      "handler": "biashara.flag_budget_alert",
      "filter": {
        "payload.pocket_name": { "$eq": "procurement" }
      }
    }
  ]
}
```

---

## Part 5 — Sustain Spec DSL

### 5.1 The Sustain Spec Format

A sustain is declared as a JSON document. The spec is the **genotype** — it describes the system's structure. A running instance is the **phenotype** — the live state populated by real data.

```json
{
  "sustain": "{{sustain_type_id}}",
  "version": "{{semver}}",
  "meta": {
    "name": "{{Human-readable name}}",
    "description": "{{Purpose description}}",
    "tags": ["{{tag1}}", "{{tag2}}"]
  },
  "state": {
    "{{domain}}": {
      "{{field}}": "{{default_value_or_null}}"
    }
  },
  "operators": [
    {
      "ref": "{{operator_dot_name}}",
      "config": {}
    }
  ],
  "operatives": [
    {
      "ref": "{{operative_template_id}}",
      "instance_id": "{{sustain_id}}.{{operative_role}}",
      "config": {
        "temperament": "{{conservative|balanced|aggressive}}",
        "event_subscriptions": []
      }
    }
  ],
  "constraints": {
    "invariants": [
      "{{constraint_predicate}}"
    ],
    "policies": {}
  },
  "access_policy": {
    "owner_ids": ["{{user_id}}"],
    "members": [
      {
        "id": "{{member_id}}",
        "tier": 1,
        "scope": ["{{operator_prefix}}.*"]
      }
    ],
    "guests": [
      {
        "id": "{{guest_id}}",
        "tier": 3,
        "scope": ["{{operator_prefix}}.read.*"]
      }
    ]
  },
  "ui_schema": {
    "sub_operator": "ui.render.{{sustain_home_widget}}",
    "widget_type": "{{widget_type_id}}",
    "panels": []
  }
}
```

### 5.2 Template vs. Instance

A **sustain template** is a spec with `{{placeholder}}` values. It is published to the Mycelium Library.

A **sustain instance** is a spec with all placeholders resolved with real values — a specific household's Homestead, a specific farm's Mkulima, a specific business's Biashara. Instances are private to their owner; templates are public (or licensed).

The Sustena platform resolves templates to instances during the **onboarding flow** — Orchie asks the user the minimum questions required to populate the template's placeholders and initialise the instance's state.

### 5.3 The Operator Reference Block

Within a sustain spec, operators are referenced, not redefined. The `ref` field is the dot-protocol name; the `config` block passes sustain-specific configuration to the operator's initialisation:

```json
"operators": [
  { "ref": "budget.allocate", "config": {} },
  { "ref": "budget.spend", "config": {} },
  { "ref": "budget.summary", "config": { "default_period": "monthly" } },
  { "ref": "vyyb.production.start_batch", "config": { "outlet_id": "{{outlet_id_1}}" } },
  { "ref": "mkulima.receive_signal", "config": {
    "accepted_produces": ["tomato", "onion", "cabbage", "kale"],
    "max_price_multiplier": 1.1
  }}
]
```

---

## Part 6 — Namespace Registry

### 6.1 Reserved Namespaces (Sustena Core)

The following namespaces are reserved for Sustena core operators. Third-party contributors may not publish operators under these namespaces.

| Namespace | Domain | Examples |
|---|---|---|
| `budget.*` | Personal and business budgeting | `budget.allocate`, `budget.spend`, `budget.transfer` |
| `state.*` | State read/write primitives | `state.get`, `state.set`, `state.snapshot` |
| `event.*` | Event bus operations | (internal; operators publish via `ctx.event_bus`) |
| `council.*` | DAO governance operations | `council.propose`, `council.vote`, `council.resolve` |
| `orchie.*` | Orchie delegate operations | `orchie.notify`, `orchie.summarise`, `orchie.propose` |
| `ui.render.*` | Widget rendering operations | `ui.render.budget_ring`, `ui.render.sustain_home` |
| `platform.*` | Platform observability | `platform.api.record_request`, `platform.llm.record_call` |
| `mycelium.*` | Network and library operations | `mycelium.publish`, `mycelium.fork`, `mycelium.charge` |
| `auth.*` | Access policy operations | `auth.grant`, `auth.revoke`, `auth.check` |

### 6.2 Sustain-Specific Namespaces (Core Sustains)

These namespaces are defined by Sustena for its first-party sustain implementations. They are not reservable by third parties, but the operator functions themselves are open-source.

| Namespace | Sustain | Description |
|---|---|---|
| `homestead.*` | Homestead Sustain | Household management operators |
| `chama.*` | Chama Secretary Sustain | Group finance operators |
| `mkulima.*` | Mkulima Sustain | Farm management and supply signal operators |
| `vyyb.*` | Vyyb Biashara Sustain | Food business operators (Vyyb IP) |
| `biashara.*` | Biashara Sustain | Generic business management operators |
| `colosso.*` | Colosso Finance Sustain | Personal and business finance intelligence |

### 6.3 Contributor Namespaces

A Mycelium contributor registers a namespace when they publish their first operator. The namespace must:

- Not conflict with any reserved or core sustain namespace
- Be at least 4 characters
- Be descriptive of the domain (not the contributor's name)

**Examples of valid contributor namespaces:**
- `sacco.*` — a contributor building SACCO management operators
- `matatu.*` — a contributor building PSV/transport operators
- `mpesa.*` — a contributor building M-Pesa parsing operators (if not already claimed by core)
- `kra.*` — a contributor building KRA filing operators
- `greenhouse.*` — a contributor building greenhouse management operators

Contributors own their namespace — no other contributor can publish under it without a transfer or dispute resolution.

---

## Part 7 — Complete Worked Examples

### 7.1 Homestead Budget Cycle — End-to-End

This example traces the full dot-protocol path from income receipt to pocket allocation to spend:

**Step 1: Income received (event triggers)**
```python
# Operator: budget.record_income
ctx.state.increment("finances.liquid.balance", 85000.0)
ctx.state.set("finances.income.received_this_month", True)
ctx.state.set("finances.income.last_received", ctx.timestamp.isoformat())
ctx.event_bus.publish("event.finances.income_received", {"amount": 85000.0})
```

**Step 2: Orchie proposes budget allocation (Mentor operative)**
```
Orchie notification (WhatsApp):
"Ksh 85,000 received. Mentor suggests allocating:
• Food: Ksh 20,000 (budget.allocate: pocket=food, amount=20000)
• Transport: Ksh 8,000
• Rent: Ksh 30,000
• Emergency fund: Ksh 10,000
• Remaining liquid: Ksh 17,000
Approve? YES / Adjust"
```

**Step 3: Council votes → User approves → budget.allocate executes**
```
Constraint check:
  budget.allocate.amount > 0                               → 20000 > 0 ✅
  budget.allocate.amount <= state.finances.liquid.balance  → 20000 <= 85000 ✅
State mutation:
  state.finances.pockets.food.allocated = 20000
  state.finances.liquid.balance = 65000
Event published:
  event.finances.pocket_allocated { pocket: "food", amount: 20000 }
```

**Step 4: Spend from pocket**
```python
# Operator: budget.spend (pocket_name="food", amount=3500, description="Naivas weekly shop")
# Constraint: budget.spend.amount <= state.finances.pockets.food.allocated
#             3500 <= 20000 ✅
ctx.state.decrement("finances.pockets.food.allocated", 3500.0)
ctx.state.increment("finances.pockets.food.spent", 3500.0)
ctx.event_bus.publish("event.finances.pocket_spent", {"pocket": "food", "amount": 3500})
```

### 7.2 Mkulima → Vyyb Cross-Sustain Supply Signal

```python
# On Mkulima sustain — farmer's Orchie proposes broadcast
# Operator: mkulima.broadcast_supply_signal

# Constraints checked:
# mkulima.broadcast_supply_signal.quantity_kg > 0         → 200 > 0 ✅
# state.inventory.items.tomato.quantity_kg >= 200         → 210 >= 200 ✅

signal_id = ctx.state.append("procurement.outbound_signals", {
    "produce": "tomato",
    "quantity_kg": 200,
    "price_per_kg": 48,
    "farm_id": ctx.sustain_id,
    "harvest_window_days": 3
})
ctx.event_bus.publish("event.procurement.supply_signal_broadcast", {
    "signal_id": signal_id,
    "destination": "mycelium.public"  # broadcast to Mycelium
})

# ---

# On Vyyb Biashara sustain — Biashara operative receives signal
# Operator: mkulima.receive_signal
# Filter: produce IN accepted_produces AND price_per_kg <= spec.max_price

ctx.state.append("procurement.mkulima_supply_signals", incoming_signal)
ctx.event_bus.publish("event.procurement.supply_signal_received", incoming_signal)

# Biashara operative evaluates, proposes to Council
# Council: Biashara YES, Curator YES, Protégé ABSTAIN → Recommendation PASSED
# User 51% vote required → IN_VOTING (Orchie notifies user)
# User replies YES → status: PASSED

# Operator: procurement.raise_po
# Constraint: state.procurement.pending_proposal.status == "PASSED" ✅

po_id = ctx.state.append("procurement.purchase_orders", {
    "signal_id": incoming_signal["signal_id"],
    "produce": "tomato",
    "quantity_kg": 100,
    "price_per_kg": 48,
    "total_kes": 4800,
    "status": "pending_delivery"
})
ctx.event_bus.publish("event.procurement.po_raised", {"po_id": po_id})
# WhatsApp message dispatched to farmer via Mkulima operative → Orchie-to-Orchie
```

### 7.3 Constraint Violation Example

```python
# Attempt: budget.allocate(pocket_name="food", amount=90000)
# Current state: state.finances.liquid.balance = 17000

# Constraint evaluation:
# budget.allocate.amount <= state.finances.liquid.balance
# 90000 <= 17000 → FALSE ❌

# OperatorResult.fail() returned:
{
  "status": "FAILED",
  "constraint_violated": "budget.allocate.amount <= state.finances.liquid.balance",
  "evaluated_values": {
    "budget.allocate.amount": 90000,
    "state.finances.liquid.balance": 17000
  },
  "orchie_explanation": "You've asked me to allocate Ksh 90,000 to food, but your liquid balance is only Ksh 17,000. Would you like to allocate Ksh 17,000 instead, or adjust another pocket to free up funds?"
}
```

---

## Part 8 — DSL Quick Reference Card

### Naming Convention Summary

| Context | Pattern | Example |
|---|---|---|
| Operator name | `namespace.action` | `budget.allocate` |
| Nested operator | `domain.subdomain.action` | `platform.api.record_request` |
| State path | `state.field.subfield` | `state.finances.liquid.balance` |
| Array index | `state.array[n].field` | `state.staff.roster[0].name` |
| Event name | `event.domain.event_type` | `event.finances.income_received` |
| Operative id | `sustain_type.role` | `vyyb.biashara` |

### Constraint Operators

| Symbol | Meaning | Example |
|---|---|---|
| `>` | Greater than | `amount > 0` |
| `>=` | Greater than or equal | `balance >= 0` |
| `<` | Less than | `cogs < revenue` |
| `<=` | Less than or equal | `amount <= balance` |
| `==` | Equal | `status == "PASSED"` |
| `!=` | Not equal | `recipe_id != null` |
| `IN` | Member of list | `grade IN ["A", "A+"]` |
| `NOT IN` | Not member of list | `status NOT IN ["FAILED", "OVERRIDDEN"]` |
| `AND` | Logical and | `a > 0 AND b > 0` |
| `OR` | Logical or | `a > 0 OR b > 0` |
| `NOT` | Logical not | `NOT state.locked == True` |
| `ALL [path]` | Universal quantifier | `ALL [pockets].allocated >= 0` |
| `EXISTS [path]` | Existential quantifier | `EXISTS [signals].quality == "A"` |
| `WITHIN n unit` | Temporal constraint | `WITHIN 48 hours status == "confirmed"` |

### State Mutation Methods

| Method | Description |
|---|---|
| `ctx.state.get(path)` | Read a value |
| `ctx.state.set(path, value)` | Write a value |
| `ctx.state.increment(path, delta)` | Atomic add |
| `ctx.state.decrement(path, delta)` | Atomic subtract |
| `ctx.state.append(path, item)` | Add to array |
| `ctx.state.remove(path, id)` | Remove from array |
| `ctx.state.exists(path)` | Boolean existence check |
| `ctx.state.snapshot()` | Full state read-only dict |

---

*End of Sustena Dot Protocol DSL Reference v1.0*
*Cross-reference: Sustena_XII_Master_Strategy.md — Sections 4.1 (Seven Primitives), 5.3 (Constraint Structure), 8.1 (Operative Template), 9.4 (Royalty Program)*
