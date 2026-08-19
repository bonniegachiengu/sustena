//! `cargo run --bin smoke -- <dir>` — the persistence proof, as text.
//!
//! ★★★ It runs the household **twice against the same directory**, in two
//! separate processes' worth of `World`, and prints both. The second open has
//! no memory of the first beyond what is on disk: every state it shows was
//! rebuilt by folding the log.
//!
//! ★ Run it with no argument and it uses a scratch directory under the system
//! temp dir, so it never touches the real app data. Point it at the app's own
//! store to inspect the real household.

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use mycelium_lib::dto::{Committed, ConstraintReading, GateResult, Refused, SustainSummary};
use mycelium_lib::definitions::{AuthoredDefinition, DimDecl, InvariantDecl};
use mycelium_lib::world::{memberships, PRINCIPAL};
use mycelium_lib::store::Store;
use mycelium_lib::templates::TemplateId;
use mycelium_lib::world::World;
use sustena_core::{compute_rollup, ChildState};

fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn rule(title: &str) {
    println!("\n── {title} {}", "─".repeat(58usize.saturating_sub(title.len())));
}

fn household(world: &World) {
    let rows: Vec<SustainSummary> =
        world.with(|i| {
            i.order()
                .iter()
                .filter_map(|id| i.get(id))
                .map(|x| SustainSummary::of(x, Vec::new()))
                .collect()
        });
    for r in &rows {
        let liquid = r.liquid.map(|v| format!("{v:>10.2}")).unwrap_or_else(|| "         —".into());
        let under = r.parent.clone().map(|p| format!("  ⊕ under {p}")).unwrap_or_default();
        println!("   {:<18} {:<10} liquid {liquid}   log {:>2} events{under}", r.id, r.label, r.events);
    }
    match world.check_holarchy() {
        Ok(n) => println!("   holarchy: HOLDS · {n} linked (checked by MonitorEngine)"),
        Err(e) => println!("   holarchy: BROKEN · {e}"),
    }
}

fn call(world: &World, sustain: &str, op: &str, p: &[(&str, Value)]) {
    let x = world.call(sustain, op, &params(p)).expect("disk");
    match x {
        None => println!("   {sustain} · no such Sustain"),
        Some((x, seq)) => {
            let r = GateResult::of(op, &x);
            println!(
                "   {sustain} · {op} -> {:?}  mutations={} events={}{}",
                r.verdict,
                r.mutations,
                r.events.len(),
                r.reason.clone().map(|s| format!("\n      reason: {s}")).unwrap_or_default()
            );
            // The message the push channel would carry, built by the SAME
            // constructors the Tauri command uses -- so this is what the UI
            // receives, not a demo's imitation of it.
            if x.committed() {
                let readings: Vec<ConstraintReading> = world
                    .constraints(sustain)
                    .into_iter()
                    .map(|(id, expression, holds, reason)| ConstraintReading {
                        id,
                        expression,
                        holds,
                        reason,
                    })
                    .collect();
                let msg = Committed::of(sustain, op, seq, &x, readings);
                println!(
                    "      ~> PUSH Committed  seq={}  liquid={:?}  events=[{}]  rules={}  (state attached)",
                    msg.seq,
                    msg.liquid,
                    msg.events.iter().map(|e| e.name.clone()).collect::<Vec<_>>().join(", "),
                    msg.constraints.len()
                );
            } else {
                let msg = Refused::of(sustain, op, &r);
                println!(
                    "      ~> PUSH Refused    rule={:?}  (this message has NO state field at all)",
                    msg.constraint_violated
                );
            }
        }
    }
}

fn main() {
    let dir: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("mycelium-smoke"));
    println!("store: {}", dir.display());

    // ── first run ────────────────────────────────────────────────────────────
    {
        let world = World::open(Store::at(&dir).expect("store")).expect("open");
        let seeded = world.seed_if_empty().expect("seed");
        rule(&format!("RUN 1 · {}", if seeded { "seeded" } else { "loaded from log" }));
        household(&world);

        rule("RUN 1 · real operator calls");
        call(&world, "homestead", "budget.record_income", &[("amount", json!(4500.0)), ("source", json!("salary"))]);
        call(&world, "homestead", "budget.allocate", &[("pocket_name", json!("food")), ("amount", json!(1200.0))]);
        call(&world, "habitat-bonnie", "budget.record_income", &[("amount", json!(900.0)), ("source", json!("side work"))]);
        // ★ The one that must be REFUSED — and must therefore write nothing.
        call(&world, "homestead", "budget.spend", &[("pocket_name", json!("food")), ("amount", json!(9000.0))]);

        rule("RUN 1 · after");
        household(&world);

        rule("RUN 1 · homestead log (what the Monitor renders)");
        for e in world.log("homestead").expect("log") {
            let names: Vec<String> = e.events.iter().map(|x| x.name.clone()).collect();
            println!("   #{}  {:<24} {} mutation(s)  {}", e.seq, e.operator, e.mutations.len(), names.join(", "));
        }

        rule("RUN 1 · the world, as the Constellation reads it");
        for r in world.with(|i| {
            i.order()
                .iter()
                .filter_map(|id| i.get(id))
                .map(|x| {
                    let readings: Vec<ConstraintReading> = Vec::new();
                    SustainSummary::of(x, readings)
                })
                .collect::<Vec<_>>()
        }) {
            let broken: Vec<String> = world
                .constraints(&r.id)
                .into_iter()
                .filter(|(_, _, holds, _)| !holds)
                .map(|(id, _, _, _)| id)
                .collect();
            println!(
                "   {:<18} {:<9} liquid {:>9}  pockets {}  events {:>2}  {}",
                r.id,
                r.label,
                r.liquid.map(|v| format!("{v:.2}")).unwrap_or_else(|| "—".into()),
                r.pockets.len(),
                r.events,
                if broken.is_empty() { "rules hold".to_string() } else { format!("BROKEN: {}", broken.join(", ")) }
            );
        }


        // ── V1.4 proofs ──────────────────────────────────────────────────────

        rule("V1.4 · ECONOMY -- the real ledger, in the real gate");
        world.with_economy(|e| {
            println!("   genesis            {} -> {:.2} juul", e.genesis.id(), e.genesis.total());
            println!("   balance now        {:.2}", e.ledger.balance_of(PRINCIPAL));
            println!("   circulation        {:.2}  (mints {:.2}, issued {:.2})",
                e.ledger.total_in_circulation(), e.ledger.minted_total(), e.ledger.issued_total());
            let audit = e.genesis.audit(&e.ledger);
            println!("   genesis audit      {}", audit.describe());
            let p = e.parameters();
            println!("   governed kappa_c   {}   kappa_s {}   issuance_rate {}",
                p.kappa_compute(), p.kappa_storage(), p.issuance_rate());
            println!("   ledger tail:");
            for entry in e.ledger.entries().iter().rev().take(4) {
                println!("      {entry:?}");
            }
        });

        rule("V1.4 · CONSOLE -- measured pawa, per operator");
        world.with_meter(|m| {
            for (name, st) in m.by_operator() {
                println!("   {:<24} runs {:>2}  mean {:>7.2} pawa  compute {:>3}  storage {:>5}B",
                    name, st.runs, st.mean_pawa(), st.total_compute, st.total_storage);
            }
        });
        println!("   (an operator the meter has never seen returns None -> the UI shows");
        println!("    \"not measured\", never a zero)");
        println!("   budget.spend measured? {:?}",
            world.with_meter(|m| m.stats_for("budget.spend")).map(|s| s.runs));
        println!("   egress.* measured?     {:?}",
            world.with_meter(|m| m.stats_for("egress.prepare_household_summary")).map(|s| s.runs));

        rule("V1.4 · SIMULATE -- a fork through the real gate, writing NOTHING");
        let before_state = world.with(|i| i.get("homestead").map(|s| s.state.clone())).unwrap();
        let before_log = world.log("homestead").expect("log").len();
        let before_balance = world.with_economy(|e| e.ledger.balance_of(PRINCIPAL));

        let branch = world
            .fork(
                "homestead",
                &[
                    ("budget.record_income".to_string(), params(&[("amount", json!(10000.0)), ("source", json!("hypothetical"))])),
                    ("budget.allocate".to_string(), params(&[("pocket_name", json!("food")), ("amount", json!(5000.0))])),
                    // The one that must be refused inside the fork too.
                    ("budget.spend".to_string(), params(&[("pocket_name", json!("food")), ("amount", json!(99999.0))])),
                ],
            )
            .expect("a real sustain");
        for (op, x) in &branch {
            let r = GateResult::of(op, x);
            println!("   {:<24} {:?}{}", op, r.verdict,
                r.reason.clone().map(|s| format!("  <- {s}")).unwrap_or_default());
        }
        let after_state = world.with(|i| i.get("homestead").map(|s| s.state.clone())).unwrap();
        println!();
        println!("   live state unchanged?   {}", before_state == after_state);
        println!("   log unchanged?          {}", before_log == world.log("homestead").expect("log").len());
        println!("   ledger unchanged?       {}", before_balance == world.with_economy(|e| e.ledger.balance_of(PRINCIPAL)));
        assert_eq!(before_state, after_state, "a fork must not touch live state");

        rule("V1.4 · GOVERNANCE -- the only path to a parameter");
        let ok = world.set_parameter("issuance_rate", 1.5);
        println!("   set issuance_rate 1.5   -> {:?}", GateResult::of("governance.set_parameter", &ok).verdict);
        println!("   read back in force      -> {}", world.with_economy(|e| e.parameters().issuance_rate()));
        let bad = world.set_parameter("issuance_rate", 9999.0);
        let badr = GateResult::of("governance.set_parameter", &bad);
        println!("   set issuance_rate 9999  -> {:?}  rule={:?}", badr.verdict, badr.constraint_violated);
        println!("   still in force          -> {}", world.with_economy(|e| e.parameters().issuance_rate()));
        let unknown = world.set_parameter("kappa_mystery", 1.0);
        println!("   set kappa_mystery       -> {:?}  rule={:?}",
            GateResult::of("governance.set_parameter", &unknown).verdict,
            GateResult::of("governance.set_parameter", &unknown).constraint_violated);

        rule("V1.4 · ISSUANCE -- the seam, now that the rate is governed on");
        let before_issued = world.with_economy(|e| e.ledger.issued_total());
        call(&world, "habitat-cira", "budget.record_income", &[("amount", json!(2500.0)), ("source", json!("work"))]);
        world.with_economy(|e| {
            println!("   issued before {:.4}  ->  after {:.4}", before_issued, e.ledger.issued_total());
            println!("   balance now   {:.4}", e.ledger.balance_of(PRINCIPAL));
        });

        rule("V1.4 · COMPOSITION -- what the core does and does not have");
        println!("   holarchy            checked by MonitorEngine (real)");
        println!("   roll-up rho         NOT in sustena-core -> honest unavailable");
        println!("   holon.transfer      NOT in sustena-core -> honest unavailable");
        println!("   (juul::transfer exists, but that is the ECONOMY's internal unit,");
        println!("    not a conserved cross-Sustain move of household money)");


        rule("V1.5 · DEFINE -- the engine decides what lands");
        let mk = |id: &str, inv: Vec<InvariantDecl>| AuthoredDefinition {
            id: id.into(),
            label: "Garden".into(),
            dimensions: vec![
                DimDecl { path: "soil".into(), kind: "number".into(), lo: Some(0.0), hi: Some(100.0) },
                DimDecl { path: "notes".into(), kind: "text".into(), lo: None, hi: None },
            ],
            operators: vec!["budget.record_income".into()],
            invariants: inv,
            opening_state: json!({ "soil": 40.0, "notes": "" }),
        };

        // (a) well-typed -> accepted and persisted
        let good = mk("garden", vec![InvariantDecl { id: "soil_ok".into(), expression: "soil >= 0".into() }]);
        println!("   well-typed        -> {:?}", world.author_definition(&good).expect("disk"));

        // (b) invariant over a dimension the schema does not declare
        let undeclared = mk("garden-bad-dim",
            vec![InvariantDecl { id: "rain_ok".into(), expression: "rainfall >= 0".into() }]);
        println!("   undeclared dim    -> {:?}", world.author_definition(&undeclared).expect("disk"));

        // (c) an invariant that will not parse at all
        let unparseable = mk("garden-bad-syntax",
            vec![InvariantDecl { id: "nonsense".into(), expression: "soil >>= ??".into() }]);
        println!("   unparseable       -> {:?}", world.author_definition(&unparseable).expect("disk"));

        println!("   persisted definitions: {:?}",
            world.definitions().iter().map(|d| d.id.clone()).collect::<Vec<_>>());
        println!("   (only the well-typed one landed -- the store never holds a broken Sigma)");

        // (d) instantiate from the authored definition, then EDIT it into a
        //     rule the live instance breaks -> the engine names what it strands.
        world.instantiate_from("garden-1", "Garden 1", TemplateId::Habitat, Some("garden"), None)
            .expect("instantiate from an authored definition");
        println!("   instantiated garden-1 from the authored definition");
        let strands = mk("garden", vec![
            InvariantDecl { id: "soil_ok".into(), expression: "soil >= 0".into() },
            InvariantDecl { id: "soil_rich".into(), expression: "soil >= 90".into() },
        ]);
        println!("   edit that strands -> {:?}", world.author_definition(&strands).expect("disk"));

        rule("V1.5 · PROFILE -- the real capability model");
        {
            use sustena_core::principal::{effective_privilege, permitted};
            let m = memberships();
            let path = vec!["homestead".to_string()];
            println!("   principal          {PRINCIPAL}");
            println!("   memberships        {}", m.len());
            println!("   effective tier     {:?}  (0 = owner)", effective_privilege(&m, PRINCIPAL, &path));
            for op in ["budget.record_income", "budget.spend"] {
                if let Some(meta) = world.operators.get(op) {
                    println!("   {:<24} needs tier {}  permitted={}",
                        op, meta.min_privilege,
                        permitted(&m, PRINCIPAL, &path, meta.min_privilege).is_ok());
                }
            }
            println!("   NOTE: displayed, not enforced -- the gate still runs Authorization::Unchecked");
        }

        rule("V1.5 · COUNCIL -- resolve, by the engine");
        {
            use sustena_core::council::{aggregate_delegated_votes, resolve, DelegatedVote, ResolutionInput, VoteChoice};
            let dv = |p: VoteChoice, c: f64| DelegatedVote { position: p, confidence: c, reasoning: String::new() };
            let votes = vec![dv(VoteChoice::Yes, 0.9), dv(VoteChoice::Yes, 0.6), dv(VoteChoice::No, 0.4)];
            let agg = aggregate_delegated_votes(&votes);
            println!("   aggregated         {:?}  utility {:.2}", agg.vote, agg.utility);
            println!("   reasoning          {}", agg.reasoning);
            let cast: Vec<VoteChoice> = votes.iter().map(|v| v.position).collect();
            for (label, user) in [
                ("council alone", None),
                ("person says yes", Some(VoteChoice::Yes)),
                ("person says no", Some(VoteChoice::No)),
                ("person abstains", Some(VoteChoice::Abstain)),
            ] {
                let st = resolve(&ResolutionInput { votes: &cast, votes_collected: true, user_vote: user, expired: false });
                println!("   {:<18} -> {:?}", label, st);
            }
        }

        rule("V1.5 · WHAT IS NOT IN THE CORE (grep-0)");
        println!("   ingest / transducer / parse rules   NOT in sustena-core");
        println!("   arena / library / marketplace       NOT in sustena-core");
        println!("   (the one 'ingest' hit in the core is clocks.rs's t_ingest --");
        println!("    an unrelated sense of the word, named so a re-grepper is not misled)");
        println!("   network: the PRIMITIVES exist (crdt, vclock, consensus, router)");
        println!("            but this host has no peer transport -- single node, honestly");

        rule("RUN 1 · V, evaluated by the engine");
        for (id, expr, holds, reason) in world.constraints("homestead") {
            println!("   {:<22} {}  {}{}", id, if holds { "HOLDS " } else { "BROKEN" }, expr,
                if reason.is_empty() { String::new() } else { format!("  <- {reason}") });
        }
    } // the World is dropped: nothing survives but the files.

    // ── second run ───────────────────────────────────────────────────────────
    {
        let world = World::open(Store::at(&dir).expect("store")).expect("reopen");
        let seeded = world.seed_if_empty().expect("seed");
        rule(&format!("RUN 2 · reopened · {}", if seeded { "SEEDED AGAIN (BUG)" } else { "loaded from log" }));
        household(&world);

        // ★★★ The property, checked rather than asserted: the cached state and
        //     a fresh fold of the persisted log must be the same value.
        rule("RUN 2 · state == fold(persisted log)");
        let store = Store::at(&dir).expect("store");
        let ids: Vec<String> = world.with(|i| i.order().to_vec());
        let mut all = true;
        for id in ids {
            let cached: Value = world.with(|i| i.get(&id).map(|s| s.state.clone())).expect("present");
            let (folded, _) = store.load_state(&id).expect("fold");
            let same = cached == folded;
            all &= same;
            println!("   {:<18} {}", id, if same { "match" } else { "DIVERGED" });
        }
        println!("\n   rebuild == fold(persisted log)  ->  {all}");
        assert!(all, "the fold must reproduce every Sustain's state");

        // -- roll-up rho ---------------------------------------------------
        //
        // The engine arithmetic, checked against a HAND SUM computed here from
        // the same states -- so this proves rho rather than restating whatever
        // rho said.
        rule("RUN 2 - roll-up rho -- the engine total vs a hand sum");
        let r = world.rollup("homestead").expect("the household exists");
        for a in &r.aggregates {
            let hand: f64 = a.included.iter().map(|c| c.value).sum();
            let engine = a.value.unwrap_or(f64::NAN);
            let same = (hand - engine).abs() < 1e-9;
            println!(
                "   {:<24} {:>12.2}   hand {:>12.2}   {}   {} in / {} out{}",
                a.id, engine, hand,
                if same { "match" } else { "DIVERGED" },
                a.included.len(), a.excluded.len(),
                if a.includes_household_own { "   (household own included)" } else { "" },
            );
            for c in &a.included {
                println!("        {:<14} {:>12.2}{}", c.label, c.value,
                    if c.is_household { "   <- the household itself" } else { "" });
            }
            for x in &a.excluded {
                println!("        {:<14} EXCLUDED  {}", x.label, x.reason);
            }
            assert!(same, "rho must equal the sum of what it says it counted");
        }

        // Fresh every call: a second read with nothing changed reproduces
        // itself, the same discipline the fold has.
        let again = world.rollup("homestead").expect("the household exists");
        let stable = serde_json::to_string(&r).unwrap() == serde_json::to_string(&again).unwrap();
        println!("   recompute reproduces itself     ->  {stable}");
        assert!(stable, "rho is computed fresh and must be reproducible");

        // The honest-exclusion contract, exercised for real: a link to a child
        // whose state cannot be read is NAMED and LEFT OUT, never counted as 0.
        rule("RUN 2 - an unreadable member is excluded and NAMED");
        let own = world.with(|i| i.get("homestead").map(|s| s.state.clone())).expect("present");
        let decls = world
            .with(|i| i.get("homestead").map(|s| s.definition.aggregates.clone()))
            .expect("present");
        let mut kids: Vec<ChildState> = world
            .with(|i| {
                i.children_of("homestead")
                    .into_iter()
                    .map(|c| (c.record.id.clone(), c.record.label.clone(), c.state.clone()))
                    .collect::<Vec<_>>()
            })
            .into_iter()
            .map(|(id, label, st)| ChildState::readable(&id, st).named(&label))
            .collect();
        let counted_before = kids.len();
        kids.push(ChildState::unreadable("habitat-ghost").named("Ghost"));
        let with_ghost = compute_rollup(&decls, "homestead", Some(&own), &kids);
        let a = with_ghost.aggregate("household_liquid_total").expect("declared");
        println!("   counted {} of {} contributors", a.included().len(),
                 a.included().len() + a.excluded().len());
        for x in a.excluded() {
            println!("   EXCLUDED  {:<12} {}", x.contributor().label(), x.reason());
        }
        let unchanged = a.value().as_f64() == r.aggregates[0].value;
        println!("   the total is unchanged by the ghost -> {unchanged}   (a zero would have moved it)");
        assert_eq!(a.included().len(), counted_before + 1, "the household plus every readable member");
        assert_eq!(a.excluded().len(), 1, "exactly the unreadable one");
        assert!(unchanged, "an excluded member must not contribute a zero");
    }
}
