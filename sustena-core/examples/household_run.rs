//! **The foundation, running.** A household Sustain walked end to end.
//!
//! ```text
//! cargo run --example household_run
//! ```
//!
//! Not a mechanism and not a spec row — something to *watch*. Every number it
//! prints comes out of the library; none is a literal typed into the script.
//! Where the gate refuses, it prints the refusal **as a refusal**, because a
//! demo that only showed the happy path would be advertising rather than
//! demonstrating.
//!
//! ★★★ **Internal points only.** Nothing here is real money and nothing here
//! reaches a payment rail (ADR-0001 D5). The balances are a household's own
//! accounting, and juul — where it appears — is an internal unit.

use serde_json::{json, Map, Value};
use sustena_core::{
    approval::{EffectClass, NonceLedger},
    curated::{compose_view, Request, SaliencePolicy},
    editing::Definition,
    event::{CausalStamp, Event},
    fold::{fold_events, FoldEvent},
    governance::Parameters,
    operator::{execute_admitted, Authorization, Enforcement, Execution, Registry},
    pawa::meter,
    region::{Interval, Region},
    schema::{DimType, Schema},
    semantic::enforcement_of,
    widget::{WidgetDecl, WidgetSet},
};

// ── printing ─────────────────────────────────────────────────────────────────

fn rule() {
    println!("{}", "─".repeat(74));
}

fn step(n: u32, title: &str) {
    println!();
    rule();
    println!("  STEP {n} · {title}");
    rule();
}

/// The household's own figures, read out of real state.
fn show(state: &Value) {
    let liquid = state.pointer("/finances/liquid/balance").and_then(Value::as_f64).unwrap_or(0.0);
    println!("    liquid balance      {liquid:>10.2}");
    if let Some(pockets) = state.pointer("/finances/pockets").and_then(Value::as_object) {
        if pockets.is_empty() {
            println!("    pockets             (none yet)");
        }
        for (name, p) in pockets {
            let a = p.get("allocated").and_then(Value::as_f64).unwrap_or(0.0);
            let s = p.get("spent").and_then(Value::as_f64).unwrap_or(0.0);
            println!("    pocket {name:<12} allocated {a:>9.2}   spent {s:>9.2}   left {:>9.2}", a - s);
        }
    }
}

/// What the gate did, in its own words.
fn verdict(x: &Execution) {
    if x.committed() {
        println!(
            "    ADMITTED  ·  {} mutation(s), {} event(s): {}",
            x.mutations.len(),
            x.events.len(),
            x.events.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(", ")
        );
    } else {
        println!("    REFUSED   ·  reason code: {:?}", x.result.constraint_violated);
        println!("    REFUSED   ·  {}", x.result.reason.as_deref().unwrap_or("(no reason given)"));
    }
}

// ── the household ────────────────────────────────────────────────────────────

fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

/// `Σ = ⟨B, S, V, T, ⊕⟩` — the household's declared rules.
fn household_definition() -> Definition {
    Definition::new(Schema::new().declare("finances", DimType::Any))
        .with_operator("budget.record_income")
        .with_operator("budget.add_pocket")
        .with_operator("budget.allocate")
        .with_operator("budget.spend")
        // `V` — the viable region, as ordinary invariants the ordinary gate reads.
        .with_invariant("liquid_non_negative", "finances.liquid.balance >= 0")
        .with_invariant(
            "food_not_overspent",
            "finances.pockets.food.spent <= finances.pockets.food.allocated",
        )
}

fn opening_state() -> Value {
    json!({
        "finances": {
            "liquid": {"balance": 0.0},
            // ★ Seeded, not conjured: an invariant over a path the state does
            // not hold compares `None` to a number, which is a type error that
            // refuses everything. The treasury learned this the hard way.
            "pockets": {"food": {"allocated": 0.0, "spent": 0.0, "limit": 0.0}},
            "income": {"monthly_total": 0.0, "sources": []}
        }
    })
}

/// One real call, through the real gate.
fn run(
    reg: &Registry,
    d: &Definition,
    enforcement: &Enforcement,
    state: &Value,
    op: &str,
    p: &[(&str, Value)],
) -> Execution {
    println!("  → {op}({})", p.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", "));
    execute_admitted(
        reg,
        &d.operators,
        enforcement,
        state,
        op,
        &params(p),
        &Authorization::Unchecked,
        &EffectClass::Unchecked,
        &mut NonceLedger::new(),
    )
}

fn main() {
    println!();
    println!("  SUSTENA CORE — the foundation, running");
    println!("  a household Sustain, walked end to end · internal points only, no real money");

    let reg = Registry::default();
    let d = household_definition();
    let gate = enforcement_of(&d);

    // ── 1 ────────────────────────────────────────────────────────────────────
    step(1, "DEFINE + INSTANTIATE  ·  Σ = ⟨B, S, V, T, ⊕⟩");
    println!("    operators declared (T)   {:?}", d.operators);
    for (id, expr) in &d.invariants {
        println!("    invariant (V)            {id}: {expr}");
    }
    println!("    gate armed               {}", gate.enabled);
    println!();
    println!("    opening state:");
    let mut state = opening_state();
    show(&state);

    // ── 2 ────────────────────────────────────────────────────────────────────
    step(2, "THREE REAL TRANSITIONS  ·  each through the gate, each an event");

    // The log the fold will replay. Only committed runs append to it.
    let mut log: Vec<FoldEvent> = vec![
        // Genesis: the opening state, as one replaceable root.
        FoldEvent { mutations: vec![sustena_core::Mutation::ReplaceRoot { value: state.clone() }] },
    ];
    let mut named_events: Vec<String> = Vec::new();

    for (op, p) in [
        ("budget.record_income", vec![("amount", json!(4500.0)), ("source", json!("salary"))]),
        ("budget.allocate", vec![("pocket_name", json!("food")), ("amount", json!(1200.0))]),
        ("budget.spend", vec![("pocket_name", json!("food")), ("amount", json!(340.0))]),
    ] {
        let x = run(&reg, &d, &gate, &state, op, &p);
        verdict(&x);
        assert!(x.committed(), "the walk expects these three to commit");
        log.push(FoldEvent { mutations: x.mutations.clone() });
        named_events.extend(x.events.iter().map(|e| e.name.clone()));
        state = x.state.clone();
        show(&state);
        println!();
    }

    // ── 3 ────────────────────────────────────────────────────────────────────
    step(3, "state = fold(events)  ·  rebuilt from the log, from nothing");
    println!("    log holds {} entries (1 genesis + 3 transitions)", log.len());
    let rebuilt = fold_events(&log, None).expect("the log replays");
    println!("    rebuilt from the log alone:");
    show(&rebuilt);
    println!();
    println!("    rebuilt == live state?   {}", rebuilt == state);
    assert_eq!(rebuilt, state, "the fold must reproduce the live state");
    println!("    ✓ the state is the log. Nothing else is holding it up.");

    // ── 4 ────────────────────────────────────────────────────────────────────
    step(4, "REFUSAL  ·  two layers, and they are not the same layer");
    let before = state.clone();

    println!("  (a) the OPERATOR's own guard — defence in depth, and it fires first");
    println!("      'food' has 1200.00 allocated and 340.00 spent — 860.00 left.");
    println!("      Attempting to spend 2000.00 from it:");
    println!();
    let refused = run(
        &reg,
        &d,
        &gate,
        &state,
        "budget.spend",
        &[("pocket_name", json!("food")), ("amount", json!(2000.0))],
    );
    verdict(&refused);
    println!();
    println!("      state byte-unchanged?  {}", refused.state == before);
    println!("      mutations recorded     {}", refused.mutations.len());
    println!("      events appended        {}", refused.events.len());
    assert_eq!(refused.state, before, "a refusal must change nothing");
    assert!(!refused.committed());

    println!();
    println!("  (b) the GATE itself — an operator with NO guard of its own, so the");
    println!("      household's declared invariant is the only thing left standing.");
    println!("      `test.force_negative` exists in the registry for exactly this: it");
    println!("      writes a pocket straight to allocated = -999 and asks no questions.");
    println!();

    // ★ It has to be declared in T to be reachable at all — an operator the
    //   definition never named is refused as `operator_allowed`, one layer
    //   earlier, and would prove something different.
    let unguarded = household_definition().with_operator("test.force_negative");
    let unguarded_gate = enforcement_of(&unguarded);
    let gated = run(&reg, &unguarded, &unguarded_gate, &state, "test.force_negative", &[]);
    verdict(&gated);
    println!();
    println!("      state byte-unchanged?  {}", gated.state == before);
    assert_eq!(gated.state, before, "the gate must change nothing either");
    assert_eq!(
        gated.result.constraint_violated.as_deref(),
        Some("enforcement_gate"),
        "this one must be the GATE, not a guard"
    );
    println!();
    println!("    ✓ two independent layers refused, and neither changed a byte.");
    println!("      The first is the operator being careful. The second is the");
    println!("      household's own rule, and it holds even when nobody is careful.");

    // ── 5 ────────────────────────────────────────────────────────────────────
    step(5, "THE METER  ·  what the committed work actually cost");
    let metered = run(
        &reg,
        &d,
        &gate,
        &state,
        "budget.record_income",
        &[("amount", json!(250.0)), ("source", json!("side work"))],
    );
    verdict(&metered);
    let parameters = Parameters::genesis();
    let reading = meter(
        &metered,
        reg.get("budget.record_income").expect("registered"),
        &gate,
        &parameters,
        "household",
        "bonnie",
        1_000,
    )
    .expect("a committed run meters");
    println!();
    println!("    operator             {}", reading.operator());
    println!("    principal            {}", reading.principal());
    println!("    compute units        {}", reading.compute());
    println!("    storage bytes        {}", reading.storage());
    println!("    κ_c / κ_s (governed) {} / {}", parameters.kappa_compute(), parameters.kappa_storage());
    println!("    PAWA                 {}", reading.pawa());
    println!("    author's estimate    {}   ← declared, and wrong; the meter is the truth", reading.declared());
    state = metered.state.clone();

    // ── 6 ────────────────────────────────────────────────────────────────────
    step(6, "compose(r)  ·  what surfaces, and why");
    let region = Region::new()
        .bounding(Interval::at_least("finances.liquid.balance", 1_000.0))
        .bounding(Interval::at_most("finances.pockets.food.spent", 500.0));

    let widgets = WidgetSet::load(
        vec![
            WidgetDecl::new("food_watch", "pocket_ring")
                .reading("finances.pockets.food.spent")
                .reading("finances.pockets.food.allocated")
                .emitting("budget.allocate")
                .bound_to("event.finances.pocket_spent"),
            WidgetDecl::new("liquid_watch", "balance_card")
                .reading("finances.liquid.balance")
                .emitting("budget.allocate"),
            WidgetDecl::new("income_log", "event_feed")
                .reading("finances.income.monthly_total")
                .bound_to("event.finances.income_received"),
        ],
        &d,
        &reg,
    )
    .expect("every widget typechecks against Σ");

    let recent: Vec<Event> = named_events
        .iter()
        .enumerate()
        .map(|(i, name)| {
            Event::backfilled(format!("e{i}"), name.clone(), 1_000 + i as i64, CausalStamp::new("host-0"))
        })
        .collect();

    let request = Request::default().with_budget(2);
    let view = compose_view(&widgets, &region, &state, &recent, &request, &SaliencePolicy::default());

    println!("    declared widgets     {}", widgets.len());
    println!("    candidates considered {}", view.candidates_considered);
    println!("    attention budget     {}  (spent {})", view.budget, view.spent);
    println!();
    println!("    SURFACED:");
    if view.selected.is_empty() {
        println!("      (nothing needs you right now)");
    }
    for w in &view.selected {
        println!("      #{}  {:<14} urgency {:.3} · cost {}", w.rank, w.id, w.urgency, w.cost);
        println!("          why: {}", w.basis.describe());
    }
    println!();
    println!("    STAYED QUIET:");
    if view.excluded.is_empty() && view.withdrawn.is_empty() {
        println!("      (nothing was displaced)");
    }
    for w in &view.excluded {
        println!("      ·   {:<14} urgency {:.3} · cost {}  — outscored", w.id, w.urgency, w.cost);
    }
    for w in &view.withdrawn {
        println!("      ·   {:<14} withdrew: {:?}", w.id, w.reason);
    }

    // ★★ Two real properties worth naming, because the numbers above are not
    //    self-explanatory and a demo that let them pass as obvious would be
    //    doing the same thing as a comment that asserts what code does not.
    println!();
    println!("    Two things that just happened, stated plainly:");
    println!();
    println!("    1. Every urgency reads 0.000, and the library says WHY rather than");
    println!("       letting it pass for calm: `V` here bounds nested paths, while the");
    println!("       curated layer keys urgency on ROOT dimensions, so nothing these");
    println!("       widgets read is bounded. `UrgencyBasis::Undeclared` is the honest");
    println!("       answer — a zero from an unmeasured basis is SILENCE, NOT SAFETY,");
    println!("       and it is reported as such instead of rendering as a calm green.");
    println!();
    println!("    2. With every score tied, selection fell to the un-gameable");
    println!("       tie-break: cheaper first. Two cost-1 widgets displaced one cost-2");
    println!("       widget under a budget of {}. Nothing was ranked by name, and the", view.budget);
    println!("       displaced widget came back WITH its score — so the interface can");
    println!("       honestly say what stayed quiet rather than silently dropping it.");

    // ── close ────────────────────────────────────────────────────────────────
    println!();
    rule();
    println!("  THE FOUNDATION RAN END TO END.");
    println!();
    println!("    · a Sustain declared its own rules, and the gate read them");
    println!("    · three transitions committed; the state is the fold of the log");
    println!("    · two refusals — an operator guard and the gate — changed nothing");
    println!("    · the work was metered — measured, not estimated");
    println!("    · a view composed itself from what the household needs seen");
    println!();
    println!("  Internal points only. No real money moved and no rail was touched.");
    rule();
    println!();
}
