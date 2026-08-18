//! `ω` assembled, replayed against its conformance vectors
//! (R2 · Operative §I/§V/§VII · follow-on wiring 1).
//!
//! ★★ No article mechanism and no Python parity: this slice connects four
//! already-built laws. The counterweight is the **omission comments** the four
//! deep-bundle slices left behind — they named, in advance, the exact defect
//! this slice could commit.
//!
//! See `conformance/README.md`.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sustena_core::{
    agent::{Agent, MemeTrustPolicy, Reasoning, Warrant, WarrantError},
    attention::{Aperture, Attention},
    capability::{Amplification, Attenuation, Capability, Rights},
    detect::CusumSpec,
    ensemble::ModelTemplate,
    learning::{Feedback, Fitness, LibraryScope, MemeLibrary, VaryOp},
    models::{EffectiveN, Fidelity, WorldModel},
    monitor::{MonitorEngine, SustainWatch},
    operative::{Cynefin, Objective, Omega, Operative, Sense, Shared, Utility},
    operator::{Enforcement, Registry},
    principal::{
        MembershipEdge, Memberships, SkinRegistry, TIER_CONTRIBUTOR, TIER_MEMBER, TIER_OWNER,
    },
    region::{Interval, Region},
    strategy::{StrategyGraph, WalkOutcome},
    CONFORMANCE_VERSION,
};

fn load() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate lives inside the repo")
        .join("conformance")
        .join("vectors")
        .join("agent.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    let doc: Value = serde_json::from_str(&text).expect("vector file is valid JSON");
    assert_eq!(
        doc["conformance_version"].as_u64().unwrap() as u32,
        CONFORMANCE_VERSION,
        "agent.json was written for a different contract version"
    );
    doc
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn world() -> Shared {
    Shared::new()
        .with_dimension("finances.liquid.balance")
        .with_move("budget.allocate")
        .with_move("budget.record_income")
}

/// The same world minus one move — used to prove the world is an argument.
fn narrower_world() -> Shared {
    Shared::new().with_dimension("finances.liquid.balance").with_move("budget.record_income")
}

fn operative() -> Operative {
    let u = Utility::new()
        .with(Objective::new("liquidity", "finances.liquid.balance", Sense::Maximise))
        .unwrap();
    Operative::new("mentor", u, &[Cynefin::Complicated]).unwrap()
}

fn attention() -> Attention {
    Attention::declared(
        Aperture::declared(2, 3, 2).unwrap(),
        Aperture::declared(12, 1, 1).unwrap(),
        100,
    )
    .unwrap()
}

fn region() -> Region {
    Region::new().bounding(Interval::at_least("finances.liquid.balance", 0.0))
}

fn meme(amount: f64) -> StrategyGraph {
    let mut kw = Map::new();
    kw.insert("amount".into(), json!(amount));
    kw.insert("source".into(), json!("salary"));
    StrategyGraph::new("a", "a").with_node(&world(), "a", "budget.record_income", kw).unwrap()
}

fn agent_with(amount: f64) -> Agent {
    let mut lib = MemeLibrary::of(LibraryScope::of("household", "mentor"));
    lib.author("m0", meme(amount)).unwrap();
    Agent::assemble(operative(), attention(), lib)
}

fn agent() -> Agent {
    agent_with(100.0)
}

fn state() -> Value {
    json!({"finances": {
        "liquid": {"balance": 500.0},
        "pockets": {},
        "income": {"monthly_total": 0.0, "sources": []}
    }})
}

fn allowed() -> Vec<String> {
    vec!["budget.allocate".to_string(), "budget.record_income".to_string()]
}

fn armed() -> Enforcement {
    Enforcement {
        enabled: true,
        invariants: vec![("liquid_non_negative".into(), "finances.liquid.balance >= 0".into())],
        ..Enforcement::default()
    }
}

fn engine() -> MonitorEngine {
    let spec = CusumSpec::new(0.0, 1.0, 5.0);
    MonitorEngine::flatten_holarchy(vec![
        SustainWatch::new("household", region(), 0.3, spec),
        SustainWatch::new("bonnie", region(), 0.3, spec).under("household"),
        SustainWatch::new("cira", region(), 0.3, spec).under("household"),
    ])
    .unwrap()
}

fn healthy(id: &str) -> Option<Value> {
    match id {
        "bonnie" => Some(json!({"finances": {"liquid": {"balance": 10.0}}})),
        "cira" => Some(json!({"finances": {"liquid": {"balance": 900.0}}})),
        _ => None,
    }
}

fn allocate_kwargs() -> Map<String, Value> {
    let mut k = Map::new();
    k.insert("pocket_name".into(), json!("food"));
    k.insert("amount".into(), json!(50.0));
    k
}

fn extend_with_allocate() -> VaryOp {
    VaryOp::Extend {
        node: "b".into(),
        mv: "budget.allocate".into(),
        after: "a".into(),
        kwargs: allocate_kwargs(),
    }
}


fn setup() -> (Memberships, SkinRegistry) {
    let mut m = Memberships::new();
    m.grant(MembershipEdge {
        principal: "bonnie".into(),
        sustain: "household".into(),
        tier: TIER_OWNER,
        skin: None,
    });
    (m, SkinRegistry::empty())
}

/// The household's own authority — what a warrant is cut from.
fn household_cap() -> Capability {
    let (m, s) = setup();
    Capability::issue(&m, &s, "bonnie", "household").unwrap()
}

/// ★ Ascending = decreasing authority. `budget.*` needs `TIER_MEMBER`, so an
/// IMPORTED meme's ceiling (`TIER_CONTRIBUTOR`) is genuinely too weak.
fn policy() -> MemeTrustPolicy {
    MemeTrustPolicy::declare(TIER_OWNER, TIER_MEMBER, TIER_CONTRIBUTOR)
}

/// A strategy whose only node allocates — so a refusal lands on the FIRST node
/// and `state` is byte-untouched.
fn allocating_meme() -> StrategyGraph {
    StrategyGraph::new("a", "a")
        .with_node(&world(), "a", "budget.allocate", allocate_kwargs())
        .unwrap()
}

fn warrant_for(a: &Agent, meme_id: &str) -> Capability {
    a.authority_for(meme_id, &household_cap(), &policy()).expect("a warrant can be cut")
}

/// One turn, from the root, with nothing already in the task's branch.
fn turn(a: &Agent, meme_id: &str, enforcement: &Enforcement) -> Reasoning {
    turn_under(a, meme_id, &warrant_for(a, meme_id), "household", enforcement)
}

/// One turn under an explicitly chosen warrant and target.
fn turn_under(
    a: &Agent,
    meme_id: &str,
    cap: &Capability,
    target: &str,
    enforcement: &Enforcement,
) -> Reasoning {
    a.act(
        meme_id,
        Warrant::new(cap, target),
        &engine(),
        "household",
        &region(),
        &BTreeSet::new(),
        healthy,
        &Registry::default(),
        &allowed(),
        enforcement,
        &state(),
    )
}

// ── attention ────────────────────────────────────────────────────────────────

#[test]
fn attention_ranks_by_the_households_own_urgency() {
    let a = agent();
    let c = a.consider(&engine(), "household", &region(), &BTreeSet::new(), healthy);
    assert!(!c.ranked.is_empty(), "the narrow aperture reached real children");
    assert!(c.focus().is_some());
    // Both children are inside V, so every score is 0 — the point is that the
    // values came from `Region::distance` and could not come from anywhere else.
    assert!(c.ranked.iter().all(|s| s.value() == 0.0));
}

#[test]
fn attention_parameterises_what_the_strategy_is_triggered_with() {
    match turn(&agent(), "m0", &Enforcement::default()) {
        Reasoning::Reasoned { focus, ref walk, .. } => {
            assert_eq!(walk.accumulated["trigger_event"]["focus"], Value::String(focus));
        }
        other => panic!("{}", other.describe()),
    }
}

#[test]
fn a_changed_frame_stops_the_turn_before_the_strategy_runs() {
    let breached = |id: &str| match id {
        "bonnie" => Some(json!({"finances": {"liquid": {"balance": 10.0}}})),
        "cira" => Some(json!({"finances": {"liquid": {"balance": -50.0}}})),
        _ => None,
    };
    let task: BTreeSet<&str> = ["bonnie"].into_iter().collect();
    let a = agent();
    let cap = warrant_for(&a, "m0");
    let out = a.act(
        "m0",
        Warrant::new(&cap, "household"),
        &engine(),
        "household",
        &region(),
        &task,
        breached,
        &Registry::default(),
        &allowed(),
        &Enforcement::default(),
        &state(),
    );
    assert!(matches!(out, Reasoning::Reframed(_)), "{}", out.describe());
    assert!(!out.ran(), "and Π did not run");
}

// ── the reasoning turn ───────────────────────────────────────────────────────

#[test]
fn a_reasoning_turn_runs_the_agents_own_strategy_through_the_gate() {
    match turn(&agent(), "m0", &Enforcement::default()) {
        Reasoning::Reasoned { ref walk, .. } => {
            assert!(walk.outcome.finished(), "{:?}", walk.outcome);
            assert_eq!(walk.state["finances"]["liquid"]["balance"], json!(600.0));
        }
        other => panic!("{}", other.describe()),
    }
}

#[test]
fn the_gate_still_refuses_inside_a_reasoning_turn() {
    // ★★★ `act` chooses whether and about what to run — never whether the gate
    // applies. A meme that would take the balance negative is refused mid-walk.
    match turn(&agent_with(-1000.0), "m0", &armed()) {
        Reasoning::Reasoned { ref walk, .. } => {
            assert!(matches!(walk.outcome, WalkOutcome::Refused { .. }), "{:?}", walk.outcome);
        }
        other => panic!("{}", other.describe()),
    }
}

#[test]
fn an_unknown_meme_is_reported_not_silently_skipped() {
    // ★ A warrant cannot even be CUT for a meme the library lacks, so the
    // attempt carries a real one and is refused inside the turn instead.
    let a = agent();
    assert!(matches!(
        a.authority_for("nope", &household_cap(), &policy()),
        Err(WarrantError::UnknownMeme { .. })
    ));
    let w = warrant_for(&a, "m0");
    assert!(matches!(
        turn_under(&a, "nope", &w, "household", &Enforcement::default()),
        Reasoning::NoSuchMeme { .. }
    ));
}

#[test]
fn nothing_in_view_is_a_distinct_answer_from_a_strategy_finding_nothing() {
    // From a leaf the scan reaches nothing, so there is no focus at all.
    let a = agent();
    let cap = warrant_for(&a, "m0");
    let out = a.act(
        "m0",
        Warrant::new(&cap, "household"),
        &engine(),
        "bonnie",
        &region(),
        &BTreeSet::new(),
        healthy,
        &Registry::default(),
        &allowed(),
        &Enforcement::default(),
        &state(),
    );
    assert!(matches!(out, Reasoning::NothingInView { scanned: 0 }), "{}", out.describe());
    assert!(!out.ran());
}

// ── learning ─────────────────────────────────────────────────────────────────

#[test]
fn an_agent_learns_over_its_own_library() {
    let mut a = agent();
    let before = a.library().len();
    let round = a
        .learn_from(
            "m0",
            &world(),
            &region(),
            &[VaryOp::Optimise { node: "a".into(), param: "amount".into(), value: json!(250.0) }],
            &Registry::default(),
            &allowed(),
            &Enforcement::default(),
            &state(),
            Feedback::Correction { by: "bonnie".into(), note: "too small".into() },
        )
        .unwrap();
    assert!(round.changed_the_library());
    assert_eq!(a.library().len(), before + 1);
    assert!(matches!(
        a.library().get(&round.retention.kept[0]).unwrap().retained_by(),
        Some(Fitness::Viable { .. })
    ));
}

#[test]
fn what_it_learned_is_immediately_reasonable_with() {
    // ★★★ The library it learned into IS the library it reasons from.
    let mut a = agent();
    let round = a
        .learn_from(
            "m0",
            &world(),
            &region(),
            &[extend_with_allocate()],
            &Registry::default(),
            &allowed(),
            &Enforcement::default(),
            &state(),
            Feedback::Objection { by: "curator".into(), note: "allocate it".into() },
        )
        .unwrap();
    let learned = round.retention.kept[0].clone();
    let out = turn(&a, &learned, &Enforcement::default());
    assert!(out.ran(), "{}", out.describe());
}

// ── the props that had to survive ────────────────────────────────────────────

#[test]
fn the_world_is_an_argument_never_a_field() {
    // ★★★ OPV-1's Props, proven behaviourally. The SAME agent, handed a
    // narrower world, refuses a variation naming a move that world lacks —
    // impossible if the agent carried a world of its own.
    let mut a = agent();
    let round = a
        .learn_from(
            "m0",
            &narrower_world(),
            &region(),
            &[extend_with_allocate()],
            &Registry::default(),
            &allowed(),
            &Enforcement::default(),
            &state(),
            Feedback::Correction { by: "b".into(), note: "n".into() },
        )
        .unwrap();
    assert_eq!(round.varied.variants.len(), 0);
    assert_eq!(round.varied.refused.len(), 1);
    assert!(round.retention.kept.is_empty());

    // And the wider world still admits it, from the very same agent.
    let wide = a
        .learn_from(
            "m0",
            &world(),
            &region(),
            &[extend_with_allocate()],
            &Registry::default(),
            &allowed(),
            &Enforcement::default(),
            &state(),
            Feedback::Correction { by: "b".into(), note: "n".into() },
        )
        .unwrap();
    assert_eq!(wide.varied.variants.len(), 1);
}

#[test]
fn the_self_model_still_moves_within_the_shared_world_after_learning() {
    // ★★★ Prop 1 → Prop 3 → variation → M_self, through the assembled agent.
    let mut a = agent();
    let round = a
        .learn_from(
            "m0",
            &world(),
            &region(),
            &[extend_with_allocate()],
            &Registry::default(),
            &allowed(),
            &Enforcement::default(),
            &state(),
            Feedback::Correction { by: "b".into(), note: "n".into() },
        )
        .unwrap();
    let m = a.self_model(&round.retention.kept[0], &["household"]).unwrap();
    assert!(m.moves().contains("budget.allocate"));
    assert!(m.moves_within(&world()));
}

#[test]
fn the_self_model_is_derived_from_the_agents_own_parts() {
    let a = agent();
    let m = a.self_model("m0", &["household"]).unwrap();
    assert_eq!(m.operative_id(), a.id());
    assert_eq!(m.attention_budget(), a.attention().cost());
    assert_eq!(m.objectives(), a.operative().utility().m());
    assert!(a.self_model("nope", &["household"]).is_none(), "not an empty model — none");
}

// ── M_world, the household's ─────────────────────────────────────────────────

fn household_model() -> WorldModel {
    WorldModel::declared(
        "household",
        &["household"],
        &["finances.liquid.balance"],
        region(),
        ModelTemplate::new().certain("s0", "budget.allocate", "s1"),
        Fidelity::claimed(0.6, "declared from the household's own spec").unwrap(),
    )
}

#[test]
fn the_world_model_hangs_on_the_population_not_on_an_agent() {
    let omega = Omega::over(world()).with(operative()).unwrap().modelling(household_model());
    assert_eq!(omega.world_model().map(|m| m.id()), Some("household"));
    // And the household's own world still answers reachability without being
    // given its members — Prop 2, untouched.
    assert_eq!(omega.reachable("s0"), world().reachable("s0"));
}

#[test]
fn several_agents_sharing_one_model_are_not_independent_confirmations() {
    let shared = Omega::over(world()).with(operative()).unwrap().modelling(household_model());
    match shared.agreement_over(5) {
        EffectiveN::BoundedAbove { headcount, shared_model } => {
            assert_eq!(headcount, 5);
            assert_eq!(shared_model, "household");
        }
        other => panic!("a number was fabricated: {other:?}"),
    }
    assert!(!shared.agreement_over(5).is_independent());

    let unmodelled = Omega::over(world()).with(operative()).unwrap();
    assert_eq!(unmodelled.agreement_over(5), EffectiveN::Independent(5));
}


// ── the authority conjunct, no longer Unchecked ──────────────────────────────

#[test]
fn a_strategy_runs_under_an_attenuated_capability_not_unchecked() {
    let a = agent();
    let w = warrant_for(&a, "m0");
    assert_eq!(w.rights(), &Rights::only(["budget.record_income"]), "least privilege");
    assert_eq!(w.tier(), TIER_OWNER, "an authored meme keeps the household's tier");
    assert_eq!(w.issued_to(), "bonnie", "and whose authority it carries is on the record");
    assert!(turn(&a, "m0", &Enforcement::default()).ran());
}

#[test]
fn a_node_outside_the_warrants_rights_is_refused_and_state_is_untouched() {
    // ★★★ The confused-deputy refusal: a warrant cut for one meme cannot admit
    // another. The out-of-rights node is FIRST, so nothing commits.
    let mut lib = MemeLibrary::of(LibraryScope::of("household", "mentor"));
    lib.author("m0", meme(100.0)).unwrap();
    lib.author("other", allocating_meme()).unwrap();
    let a = Agent::assemble(operative(), attention(), lib);

    let w = warrant_for(&a, "m0");
    match turn_under(&a, "other", &w, "household", &Enforcement::default()) {
        Reasoning::Reasoned { ref walk, .. } => {
            match &walk.outcome {
                WalkOutcome::Refused { reason, .. } => {
                    assert!(reason.contains("budget.allocate"), "{reason}");
                }
                other => panic!("{other:?}"),
            }
            assert_eq!(walk.state, state(), "state is byte-untouched");
        }
        other => panic!("{}", other.describe()),
    }
}

#[test]
fn a_warrant_for_another_household_is_refused_as_not_designated() {
    // ★★★ Redesignation is the confused deputy's own move, and `Attenuation`
    // carries no sustain precisely so it cannot be done by narrowing either.
    let a = agent();
    let w = warrant_for(&a, "m0");
    match turn_under(&a, "m0", &w, "neighbour", &Enforcement::default()) {
        Reasoning::Reasoned { ref walk, .. } => {
            assert!(matches!(walk.outcome, WalkOutcome::Refused { .. }));
            assert_eq!(walk.state, state());
        }
        other => panic!("{}", other.describe()),
    }
}

#[test]
fn an_imported_meme_runs_under_a_tighter_authority_and_is_refused() {
    // ★★★ Least privilege BY PROVENANCE — where OPV-6 and IMM-7 meet.
    let donor = agent();
    let m = donor.library().get("m0").unwrap().clone();
    let mut mine = MemeLibrary::of(LibraryScope::of("neighbour", "mentor"));
    mine.import(&m, donor.library().scope(), "borrowed").unwrap();
    let a = Agent::assemble(operative(), attention(), mine);

    let w = warrant_for(&a, "borrowed");
    assert_eq!(w.tier(), TIER_CONTRIBUTOR, "tighter than an authored meme's");
    assert_eq!(warrant_for(&donor, "m0").tier(), TIER_OWNER, "and the authored one is not");

    match turn_under(&a, "borrowed", &w, "household", &Enforcement::default()) {
        Reasoning::Reasoned { ref walk, .. } => {
            assert!(matches!(walk.outcome, WalkOutcome::Refused { .. }));
            assert_eq!(walk.state, state(), "and nothing committed");
        }
        other => panic!("{}", other.describe()),
    }
}

#[test]
fn a_strategy_naming_a_move_the_household_lacks_gets_no_warrant_at_all() {
    // ★★★ Refused at ATTENUATION — stronger than refusing node by node once a
    // warrant already exists.
    let mut lib = MemeLibrary::of(LibraryScope::of("household", "mentor"));
    lib.author("alloc", allocating_meme()).unwrap();
    let a = Agent::assemble(operative(), attention(), lib);

    let narrow = household_cap()
        .attenuate(&Attenuation::to_rights(Rights::only(["budget.record_income"])))
        .unwrap();
    assert!(matches!(
        a.authority_for("alloc", &narrow, &policy()),
        Err(WarrantError::Amplified(Amplification::Rights))
    ));
}

#[test]
fn an_agent_commits_with_no_approval_token_because_it_reasons_in_the_sandbox() {
    // ★★ Structural: `act` accepts no `ApprovalToken`, and a live effect needs
    // one bound to ONE (operator, params) pair — a strategy is many nodes.
    assert!(turn(&agent(), "m0", &Enforcement::default()).ran());
}

#[test]
fn prop_3_still_holds_alongside_the_warrant() {
    // ★ Two containments: `T` at build, the warrant at the gate.
    let a = agent();
    let w0 = world();
    let moves = a.library().get("m0").unwrap().strategy().moves();
    let t: BTreeSet<&str> = w0.moves().into_iter().collect();
    assert!(moves.is_subset(&t), "still bounded by T");
    let w = warrant_for(&a, "m0");
    assert!(moves.iter().all(|m| w.rights().carries(m)), "and by the warrant");
}

#[test]
fn an_unwarranted_run_is_still_reachable_only_below_the_agent() {
    // ★ `strategy::run` keeps its `Unchecked` behaviour for the R1 parity
    // vectors, byte-for-byte — but there is NO way to reach it through `Agent`,
    // because `act`'s warrant is a required parameter rather than a default.
    let a = agent();
    let walk = sustena_core::strategy::run(
        a.library().get("m0").unwrap().strategy(),
        &Registry::default(),
        &allowed(),
        &Enforcement::default(),
        &state(),
        Value::Object(Map::new()),
    );
    assert!(walk.outcome.finished(), "unchanged for every caller written before this");
}

// ── the recorded divergence ──────────────────────────────────────────────────

#[test]
fn the_recorded_divergence_keeps_its_counterweight() {
    let doc = load();
    let d = &doc["divergence"];
    assert_eq!(d["kind"], "rust-only-internal-wiring");

    let step0 = &doc["★★_the_STEP_0_reconcile"];
    for key in [
        "★★★_1_the_discipline_runs_in_REVERSE_now",
        "★★★_2_so_the_faculties_are_REQUIRED_not_OPTIONAL",
        "★★★_3_there_was_no_operative_level_dispatch_to_wire_INTO",
        "★★_4_M_self_is_a_DERIVATION_not_a_field",
        "★★★_5_M_world_is_the_HOUSEHOLD'S_and_belongs_on_Omega",
    ] {
        assert!(step0[key].as_str().is_some_and(|s| s.len() > 80), "missing reconcile: {key}");
    }
    // ★★★ The dispatch reconcile must stay specific about what exists.
    assert!(step0["★★★_3_there_was_no_operative_level_dispatch_to_wire_INTO"]
        .as_str()
        .unwrap()
        .contains("Dispatch::run"));

    // ★★★ The counterweight IS the omission comments, quoted.
    let cw = d["★★★_the_counterweight_is_the_omission_comments"].as_str().unwrap();
    assert!(cw.contains("A DECLARED-BUT-INERT FIELD READS AS BUILT"));
    assert!(cw.contains("harder counterweight than a parity grep"));

    // Every faculty must name its genuine-use path.
    let paths = d["★★_so_the_per_faculty_genuine_use_paths_are_stated_and_tested"]
        .as_object()
        .unwrap();
    for k in [
        "Π (OPV-4) + the library (OPV-6)",
        "attention (OPV-7)",
        "M_self (OPV-11)",
        "M_world (OPV-11)",
    ] {
        assert!(paths.contains_key(k), "no stated use path for {k}");
        assert!(paths[k].as_str().is_some_and(|s| s.len() > 60));
    }

    let semantic = d["★★_the_one_chosen_semantic_named_as_one"].as_str().unwrap();
    assert!(semantic.contains("STOPS the turn"));
    assert!(semantic.contains("drop it on the floor"));

    let props = d["the_props_that_had_to_survive_and_are_asserted"].as_str().unwrap();
    assert!(props.contains("holds NO `Shared`"));
    assert!(props.contains("moves_within"));

    let limits = d["the_honest_limits"].as_str().unwrap();
    for term in [
        "SEPARATE TYPE FROM `Operative`",
        "THE ROUTER STILL TAKES `&Operative`",
        "ATTENTION'S SCAN NEEDS A `MonitorEngine`",
        "THE FOCUS IS THE SINGLE WORST-RANKED SUSTAIN",
        "`learn_from` DOES NOT CONSULT `M_world`",
        "NO AGENT IS EVER CONSTRUCTED BY THE CORE",
    ] {
        assert!(limits.contains(term), "missing limit: {term}");
    }

    let mut seen = BTreeSet::new();
    for group in [
        "attention_cases",
        "reasoning_cases",
        "learning_cases",
        "props_cases",
        "world_model_cases",
        "authority_cases",
    ] {
        for c in doc[group].as_array().unwrap_or_else(|| panic!("{group} is an array")) {
            assert!(
                c["why"].as_str().is_some_and(|w| w.len() > 40),
                "{group}/{} needs a real why",
                c["name"]
            );
            assert!(seen.insert(c["name"].as_str().unwrap().to_string()), "duplicate case name");
        }
    }
    assert_eq!(seen.len(), 22, "every declared case must be present");
}
