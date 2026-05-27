# Vyyb — Biashara Sustain Development Plan
### Companion Document to the Sustena XII Master Strategy
**Version 2.0 — May 2026 · CONFIDENTIAL**
**Cross-reference:** Sustena_XII_Master_Strategy.md (Sections 8.8, 8.9, 10.3, 11.5, Appendix A)

---

## About This Document

The original Vyyb OS Dev Roadmap (v1, April 2026) was a set of Claude Code prompts for the standalone Vyyb OS codebase — a FastAPI/SQLite food business ledger built to production grade (24 tables, 107 recipes, double-entry accounting, 89 live orders). That codebase is **deprecated** — not because it failed, but because it was architecturally standalone. It proved the data model and business logic. It cannot, as-is, participate in the Sustena ecosystem (Mycelium, operative reasoning, cross-sustain collaboration, pawa token economy).

This document defines how **Vyyb is rebuilt from scratch as a Biashara Sustain** on the Sustena XII platform. Fresh codebase. Vyyb's full business intelligence is expressed as a Sustain JSON spec, with operators derived from the proven Vyyb OS logic, and operatives providing the intelligence layer that the old system lacked.

The old codebase remains accessible as a data source and reference. Seed scripts import its SQLite data into the new Sustena XII database on first run. The 107 recipes are Vyyb IP and are **not published to the Mycelium Library** — they live only in the Vyyb Biashara sustain's private state.

---

## 1. Vyyb's Role in the Sustena Ecosystem

Vyyb is the **first Biashara Sustain** — the reference implementation of the Biashara operative (Section 8.8 of the master strategy). Its role is threefold:

1. **Proof-of-concept:** demonstrates that a production-grade food business (real revenue, real operations, real staff) can be fully expressed and governed as a Sustain.
2. **Operator library seed:** the double-entry accounting system, production batch logic, and demand forecasting from the old Vyyb OS become the first entries in the Sustena Operator Registry — reusable by any food business. The 107 recipes are Vyyb IP and are excluded.
3. **Mkulima buyer node:** Vyyb is a buyer in the Mkulima farmer pipeline. When the Mkulima operative detects produce matching Vyyb's procurement requirements, it routes supply signals to Vyyb's Biashara operative for pre-harvest commitment (Section 10.3, Week 3–4 of the master strategy).

Vyyb is not a revenue generator for the founding team. Its value is architectural and demonstrative.

---

## 1b. The Hive + Outlet Architecture — Why the Old Codebase Was Deprecated

The old Vyyb OS modelled the business as a flat structure: one production facility, one set of orders, one till. The real Vyyb operation is not flat. It is **hub-and-spoke**: a central Hive (production kitchen) that prepares batches, and multiple Outlets (satellite sales points) that receive and sell. Every inventory movement, every order, every revenue figure is outlet-specific — but procurement, production scheduling, and Mkulima supply signals are Hive-level.

**The Hive + Outlet model is the correct architecture from Day 1.** The old codebase did not express this. The Biashara Sustain does.

**Hive:** The central production and procurement node.
- Manages: inventory (raw materials), production batches, procurement (Mkulima supply signals, POs), Curator operative (asset register for kitchen equipment)
- Operators: `vyyb.production.start_batch`, `vyyb.production.complete_batch`, `vyyb.inventory.restock`, `procurement.raise_po`
- State domain: `state.hive.inventory`, `state.hive.production`, `state.hive.procurement`

**Outlet:** A satellite sales node — a street stall, a shop, a delivery point.
- Manages: finished goods received from Hive, orders (placed + fulfilled), till balance, daily revenue
- Operators: `vyyb.outlet.receive_goods`, `vyyb.orders.place`, `vyyb.orders.fulfill`, `vyyb.till.reconcile`
- State domain: `state.outlets.{outlet_id}.inventory`, `state.outlets.{outlet_id}.orders`, `state.outlets.{outlet_id}.accounts`

**Cross-Hive-Outlet flow:**
```
Hive produces: 50 units of Chapati Pilau batch
  → vyyb.production.complete_batch(batch_id, yield=50)
  → Biashara operative allocates: 30 units to Outlet A, 20 units to Outlet B
  → vyyb.outlet.transfer(from=hive, to=outlet_a, item=chapati_pilau, qty=30)
  → vyyb.outlet.transfer(from=hive, to=outlet_b, item=chapati_pilau, qty=20)

Outlet A sells: 12 units
  → vyyb.orders.fulfill(order_id, outlet_id=outlet_a)
  → state.outlets.outlet_a.inventory.chapati_pilau -= 12
  → state.outlets.outlet_a.accounts.daily_revenue += 12 × selling_price
  → event.orders.order_fulfilled published

Hive Biashara operative reviews at 18:00:
  → Both outlets: current stock vs. tomorrow's demand forecast
  → Proposes production schedule for morning
  → Council votes → User approves → Schedule committed
```

**Biashara-first architecture:** Every Mkulima is a Biashara; not every Biashara is a Mkulima. Similarly, every QSR (Vyyb) is a Biashara; not every Biashara is a QSR. The Biashara Sustain is built first as a generic business operative. The Vyyb QSR fork adds the Hive+Outlet layer, production batch logic, and food-specific operators (`vyyb.*`) on top. This means:

1. Build `biashara.*` operators first (generic: inventory, accounts, orders, staff, tax)
2. Fork Biashara sustain as `vyyb.biashara` — add Hive+Outlet structure + `vyyb.*` operators
3. Any other QSR (a coffee shop, a catering company, a school canteen) can fork the Vyyb sustain template and adapt it without rebuilding from scratch

---

## 2. The Vyyb Biashara Sustain Spec

This is the complete state schema for the Vyyb Biashara Sustain. Every field maps to a table or column in the proven Vyyb OS data model.

### 2.1 State Schema

```json
{
  "sustain": "vyyb.biashara",
  "version": "1.0",
  "meta": {
    "name": "Vyyb Food Business",
    "description": "Street food / QSR business management sustain",
    "business_type": "food_qsr",
    "outlets": ["{{outlet_id_1}}", "{{outlet_id_2}}"]
  },
  "state": {
    "inventory": {
      "items": {},
      "low_stock_alerts": [],
      "reorder_queue": []
    },
    "production": {
      "active_batches": [],
      "batch_history": [],
      "production_schedule": []
    },
    "orders": {
      "active_orders": [],
      "order_history": [],
      "daily_revenue": 0,
      "daily_cogs": 0,
      "daily_gross_margin": 0
    },
    "recipes": {
      "library": {},
      "standard_costs": {},
      "margin_by_item": {}
    },
    "accounts": {
      "chart_of_accounts": {},
      "journal_entries": [],
      "petty_cash_balance": 0,
      "till_balance": 0,
      "pochi_la_biashara": 0
    },
    "assets": {
      "register": [],
      "accumulated_depreciation": 0,
      "maintenance_schedule": []
    },
    "staff": {
      "roster": [],
      "active_shifts": [],
      "payroll_period": {}
    },
    "tax": {
      "vat_collected_mtd": 0,
      "vat_payable": 0,
      "tot_payable": 0,
      "kra_filing_status": "current"
    },
    "procurement": {
      "vendors": [],
      "purchase_orders": [],
      "mkulima_supply_signals": []
    },
    "analytics": {
      "top_items_by_margin": [],
      "demand_forecast_14d": [],
      "busiest_hours": {},
      "weekly_revenue_trend": []
    }
  }
}
```

### 2.2 UI Schema

The Vyyb Biashara Sustain's home screen is assembled via its `ui_schema` sub-operator (see master strategy Section 12.4):

```json
"ui_schema": {
  "sub_operator": "ui.render.sustain_home",
  "widget_type": "biashara_command_centre",
  "panels": [
    {
      "title": "Live P&L",
      "widget": "biashara_pl_card",
      "data_source": "state.orders",
      "refresh": "real_time"
    },
    {
      "title": "Inventory Alerts",
      "widget": "low_stock_alert_list",
      "data_source": "state.inventory.low_stock_alerts",
      "refresh": "on_event"
    },
    {
      "title": "Production Queue",
      "widget": "production_schedule_card",
      "data_source": "state.production.production_schedule",
      "refresh": "on_event"
    },
    {
      "title": "KDS Orders",
      "widget": "kds_active_orders",
      "data_source": "state.orders.active_orders",
      "refresh": "real_time"
    },
    {
      "title": "Tax Status",
      "widget": "tax_status_badge",
      "data_source": "state.tax",
      "refresh": "daily"
    }
  ],
  "orchie_thread": {
    "position": "bottom_sheet",
    "always_visible": true
  }
}
```

---

## 3. Operator Library — Derived from Vyyb OS

The following operators are derived directly from the proven Vyyb OS service functions. Each was validated in production. They are registered to the Sustena Operator Registry as the `vyyb.*` namespace and are reusable by any Biashara sustain.

| Operator | Inputs | State Change | Constraint | Vyyb OS Source |
|---|---|---|---|---|
| `sales.fulfill` | outlet_id, items[], payment_method | orders.active_orders → history; inventory deducted; journal posted | stock_before_sale; balanced_journal | `record_pos_order()` |
| `production.batch` | recipe_id, quantity, outlet_id | inventory deducted (ingredients); finished goods added; COGS posted | recipe_exists; ingredients_sufficient | `record_production_batch()` |
| `inventory.purchase` | item_id, quantity, unit_cost, vendor_id | inventory.items[item].quantity += qty; journal posted (Dr Inventory / Cr AP) | balanced_journal; quantity_positive | `record_purchase()` |
| `inventory.adjust` | item_id, quantity_delta, reason | inventory.items[item].quantity adjusted; journal posted | reason_required; balanced_journal | `record_stock_adjustment()` |
| `expense.record` | account_id, amount, description, vendor_id | accounts.journal_entries appended; expense account debited | account_exists; amount_positive | `record_expense()` |
| `payroll.run` | staff_id, amount, period | payroll expense posted; journal entry (Dr Wages / Cr Cash) | staff_exists; amount_positive | `record_payroll()` |
| `asset.depreciate` | asset_id, period | accumulated_depreciation += monthly_dep; journal posted | asset_active; useful_life_remaining | `run_depreciation_engine()` |
| `tax.calculate_vat` | period | tax.vat_collected_mtd updated; vat_payable calculated | — | `compute_vat_report()` |
| `procurement.raise_po` | vendor_id, lines[], expected_delivery | purchase_orders appended; vendor notified | vendor_active | `create_purchase_order()` |
| `mkulima.receive_signal` | produce_type, quantity, farm_id, price_per_kg | procurement.mkulima_supply_signals appended | signal_matches_procurement_spec | New (cross-sustain) |

### 3.1 Constraints Inherited from Vyyb OS

These constraints are encoded in the Vyyb Biashara Sustain spec and enforced by the Constraint Engine on every relevant operator:

```python
constraints = [
  # Double-entry accounting
  "sum(journal_entry.debits) == sum(journal_entry.credits)",

  # Stock integrity
  "inventory.items[item_id].quantity >= 0",  # no negative stock

  # Recipe cost accuracy
  "abs(production.batch.calculated_cogs - production.batch.posted_cogs) < 1.0",  # within KES 1

  # Tax compliance
  "tax.vat_collected_mtd >= 0",
  "tax.tot_payable == gross_revenue_mtd * 0.03 if gross_revenue_mtd > 500000 else 0",

  # Business health alerts
  "orders.daily_gross_margin > 0.25",  # warn if margin drops below 25%
  "inventory.low_stock_alerts.count < 10",  # alert if 10+ items below threshold
]
```

---

## 4. Operative Configuration — Vyyb Biashara

The Vyyb Biashara Sustain runs four operatives from the Mycelium Library, each specialised to its domain:

### 4.1 Biashara Operative (Mentor Speciation for Business)

The Biashara operative is the Mentor operative configured for a food business context — same Budget Watchdog architecture, different state domain.

```json
{
  "operative": "Biashara",
  "base": "Mentor",
  "config": {
    "primary_state_domain": "state.orders, state.accounts, state.tax",
    "alert_rules": [
      { "trigger": "daily_gross_margin < 0.25", "message": "Gross margin below 25% today. Check COGS — possible recipe cost drift or over-discounting." },
      { "trigger": "petty_cash_balance < 500", "message": "Petty cash running low. Send to petty cash from NCBA Business account before tomorrow's operations." },
      { "trigger": "vat_payable > 50000", "message": "VAT payable is significant this period. Review before KRA filing date." }
    ],
    "weekly_summary": "Monday 7am",
    "kpi_targets": {
      "daily_gross_margin_pct": 40,
      "monthly_revenue_growth_pct": 5,
      "cogs_to_revenue_ratio_max": 0.55
    }
  },
  "ui_schema": {
    "sub_operator": "ui.render.operative_dashboard",
    "widget_type": "biashara_watchdog_panel",
    "sections": [
      { "title": "Today's P&L", "chart": "revenue_cogs_bar", "source": "state.orders" },
      { "title": "Margin Trend", "chart": "margin_sparkline_14d", "source": "state.analytics.weekly_revenue_trend" },
      { "title": "Top Performers", "display": "item_margin_list", "source": "state.analytics.top_items_by_margin" },
      { "title": "Tax Status", "display": "tax_badge", "source": "state.tax" }
    ]
  }
}
```

### 4.2 Protégé Operative (Production Scheduler)

The Protégé operative manages production batches and staff shifts.

- Demand forecast → production schedule: uses `state.analytics.demand_forecast_14d` to propose batch sizes for the next 3 days.
- Staff shift scheduling: assigns staff to stations based on the weekly schedule; flags conflicts (overlapping shifts, minimum rest violations).
- Market day awareness: Nairobi market days (Wednesday, Saturday at Wakulima) trigger procurement schedule suggestions.

### 4.3 Navigator Operative (Delivery and Logistics)

- Order dispatch routing: for The Hive production runs, assigns outlet deliveries to drivers.
- Delivery tracking: integrates with Bodawerk/Sendy/Pickup Mtaani for last-mile.
- Route optimisation: minimises total delivery time and fuel cost across active outlets.

### 4.4 Curator Operative (Asset and Inventory Management)

- Full equipment and inventory register for all outlets.
- Depreciation schedule monitoring: flags assets approaching end-of-life.
- Surplus asset detection: equipment not used for 30+ days surfaced for disposal or redeployment.
- Mkulima supply matching: the Curator maintains the Biashara procurement spec (which produce types, quantities, and quality grades Vyyb buys) — used by the `mkulima.receive_signal` operator.

---

## 5. The Mkulima–Vyyb Integration (Cross-Sustain Collaboration)

This is the first real cross-sustain operative collaboration in the Sustena network. The flow:

```
Mkulima Operative (farmer's sustain)
  │  detects: tomatoes ready for harvest, 200kg, KES 48/kg
  │  matches: Vyyb procurement spec (tomatoes, min 50kg, max KES 55/kg)
  ↓
operator: mkulima.broadcast_supply_signal
  │  publishes supply signal to Mycelium
  │  payload: { produce: "tomato", quantity_kg: 200, price_per_kg: 48, farm_id, harvest_window: "3 days" }
  ↓
Vyyb Biashara Sustain
  │  operator: mkulima.receive_signal — appends to state.procurement.mkulima_supply_signals
  │  Biashara operative evaluates: price vs. 14-day price history (market: KES 52 avg) → 8% below market
  │  Biashara operative proposes: Council proposal to commit to 100kg pre-harvest
  ↓
Council vote: Biashara YES (cost saving), Curator YES (inventory spec met), Protégé ABSTAIN
  → Recommendation: PASSED (2 YES, 0 NO, 1 ABSTAIN)
  ↓
User notification (Orchie → WhatsApp / in-app):
  │  "Council recommends committing to 100kg tomatoes @ KES 48/kg from [Farm].
  │   Saves ~KES 400 vs. market rate. Harvest window: 3 days.
  │   Approve? YES / NO / See details"
  │  Status: IN_VOTING — no action taken yet
  ↓
User casts 51% vote (supreme consent):
  │  User replies YES → status: PASSED (user confirmed)
  │  User replies NO  → status: OVERRIDDEN_BY_USER (proposal dies; reason logged)
  │  No reply in 24h  → Orchie re-notifies; after 48h with no reply → DEFERRED
  ↓
operator: procurement.raise_po — PO raised, WhatsApp message sent to farmer via Mkulima operative
  │  pre-harvest commitment logged in Vyyb procurement state
  │  payment scheduled for delivery confirmation (M-Pesa via Orchie)
```

This is the Sustena marketplace in embryonic form — two sustains exchanging value through the Mycelium, governed by their respective Councils, without either party needing a centralised intermediary.

---

## 6. Migration of Vyyb OS Data

The old Vyyb OS SQLite database contains production-grade data that seeds the new sustain:

```python
# scripts/migrate_vyyb_os.py
# Reads from old SQLite, transforms to Sustena XII schema, writes to new DB

MIGRATION_MAP = {
  "items":                "state.inventory.items (Curator asset/inventory register)",
  "recipes + recipe_lines": "state.recipes.library + Operator Registry (recipe → production.batch operator)",
  "orders + order_items": "state.orders.order_history (event log replay)",
  "inventory_movements":  "Event log entries (each movement = one immutable event)",
  "accounts + journals":  "state.accounts (chart of accounts + journal as event log)",
  "assets":               "state.assets.register (Curator asset register)",
  "parties":              "state.staff.roster + state.procurement.vendors (Attaché profiles)",
}

# Run: python scripts/migrate_vyyb_os.py --source old_vyyb.db --target sustena_xii.db
```

Data quality notes from the May 2026 analysis (preserve in the new system):
- 107 recipes across 3 tiers (raw material → semi-finished → finished goods)
- 61 active menu items with selling prices in KES
- 89 orders from May 14–18, 2026 — the first real demand signal dataset
- Electricity and labour costed as ingredients (a Vyyb OS innovation, retained in the new operator spec)
- Pochi la Biashara as a first-class account (not a memo field — retained)

---

## 7. Development Phases for Vyyb Biashara Sustain

### Phase 1 (Week 3–4 of Sustena XII master plan)

Parallel to Chama Secretary operative development (Section 10.3 of master strategy):

- [ ] Define and register all `vyyb.*` operators in the Sustena XII Operator Registry
- [ ] Port the Vyyb Biashara state schema (Section 2.1 above) to the Sustain Spec store
- [ ] Run `migrate_vyyb_os.py` — import historical data as seed events
- [ ] Wire the Biashara operative to the Vyyb sustain state
- [ ] Basic POS operator (`sales.fulfill`) functioning end-to-end with constraint validation
- [ ] Live P&L card (widget type: `biashara_pl_card`) rendering via the `ui_schema` sub-operator

### Phase 2 (Months 3–6)

- [ ] Full recipe costing and margin analytics via Curator + Biashara operative
- [ ] Staff scheduling via Protégé operative (replaces the old `scheduled_shifts` table approach)
- [ ] Navigator operative for multi-outlet delivery dispatch
- [ ] Mkulima supply signal integration (`mkulima.receive_signal` operator live)
- [ ] KDS widget (`kds_active_orders`) rendered on a tablet at the production station
- [ ] Depreciation engine (Curator operative, monthly scheduled trigger)
- [ ] VAT and TOT tax constraint monitoring (Biashara operative)
- [ ] Platform observability: Vyyb sustain metrics streaming to the Platform Ops Sustain

### Phase 3 (Month 12+)

- [ ] The Hive Microsustain: multi-outlet central production hub as a sub-sustain within Vyyb
- [ ] Vyyb as a purchaseable Biashara Sustain template on the Mycelium Library — any food entrepreneur can fork it with a one-time pawa license fee; Bonnie earns 70% of every fork as a royalty (see master strategy Section 9.4)
- [ ] Vyyb `vyyb.*` operator library published under per-use pawa licensing — every food business that uses `sales.fulfill`, `production.batch`, or `recipe.cost` pays a micro-royalty per execution
- [ ] Vyyb B2B: institutional food buyers (schools, hotels, corporate canteens) integrated as Attaché profiles with automated procurement proposals
- [ ] Pawa token billing for Vyyb's operative usage (Biashara Premium tier)
- [ ] Contributor earnings dashboard: Bonnie can view cumulative pawa earned from Vyyb template forks and operator royalties; elect STC conversion when STC goes live

---

## 8. Key Differentiators Preserved from Vyyb OS

The old Vyyb OS solved several problems that most food business software does not. These solutions are encoded as constraints and operator logic in the new sustain spec:

**Electricity and labour as recipe ingredients.** The COGS of every production batch includes a prorated electricity cost and a prorated labour cost per unit produced. This is the most accurate food business COGS model for a Kenyan informal QSR. Encoded in the `production.batch` operator.

**Pochi la Biashara as a first-class account.** Most mobile money integrations treat Pochi la Biashara as a memo. In Vyyb OS, it is a full account in the chart of accounts with debit/credit journal entries. Retained in `state.accounts.pochi_la_biashara`.

**Demand-driven production planning.** The 14-day rolling sales trend drives batch size recommendations. The Protégé operative's production schedule proposals are grounded in this signal, not guesswork.

**Three-tier recipe hierarchy.** Raw material → semi-finished goods → finished goods. This is the correct structure for a street food operation that makes its own base ingredients (chapati dough, mandazi batter) before assembling final products. Retained in `state.recipes.library` with the three-tier type field.

---

## 9. Domains and Brand

| Asset | Primary | Secondary | Notes |
|---|---|---|---|
| Vyyb domain | `vyyb.ai` | `vyyb.io` | Acquire both immediately (see master strategy Section 11.2) |
| Vyyb design system | Biashara sustain skin on Sustena base | Safety orange (#E8600A) as brand accent | Not the full Sustena design system |
| Vyyb blog presence | Posts on sustena.io blog tagged `vyyb` | Cross-posted to Vyyb's own page on the blog sustain | No separate Vyyb Substack |

---

*This document is a living specification. Updates are made as the Vyyb Biashara Sustain is built and validated. Cross-reference with Sustena_XII_Master_Strategy.md for the full platform context.*
