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
use mycelium_lib::world::DEFAULT_HANDLE;
use mycelium_lib::store::{LoggedEvent, Store};
use mycelium_lib::ingest::Capture;
use mycelium_lib::templates::TemplateId;
use mycelium_lib::world::World;
use sustena_core::{
    compute_rollup, holon_transfer, ChildState, Leg, Link, Linked, Moving, Party, Transfer,
};

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

        // ── identity ───────────────────────────────────────
        //
        // ★★★ NOTHING acts before this. The private key is recovered from disk by
        //   a passphrase that genuinely decrypts it; until then there is no
        //   principal, and a call with no principal is refused rather than run
        //   under a declared string.
        rule("IDENTITY - the key, unlocked");
        let locked = world.call("homestead", "budget.record_income", &income(1.0)).expect("call");
        match &locked {
            Some((x, _)) => println!("   while LOCKED   {:?}   {}   rule={:?}",
                x.result.status,
                x.result.reason.clone().unwrap_or_default(),
                x.result.constraint_violated.clone().unwrap_or_default()),
            None => println!("   while LOCKED   no such sustain"),
        }
        assert!(
            locked.as_ref().is_some_and(|(x, _)| !x.committed()),
            "a locked world must not commit anything"
        );

        if !world.identity_store().exists() {
            world.enrol(DEFAULT_HANDLE, PASSPHRASE).expect("enrol");
            println!("   enrolled       {DEFAULT_HANDLE}");
        }
        world.lock();
        let wrong = world.unlock("not the passphrase");
        println!("   wrong pass     {:?}", wrong.as_ref().err().map(|e| e.to_string()));
        assert!(wrong.is_err(), "a wrong passphrase must fail closed");
        assert!(!world.is_unlocked(), "and must leave the world locked");

        let who = world.unlock(PASSPHRASE).expect("unlock");
        println!("   unlocked as    {who}");
        println!("   public key     {}", world.public_key().unwrap_or_default());
        assert!(world.is_unlocked());
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
            println!("   balance now        {:.2}", e.ledger.balance_of(&who_now(&world)));
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
        let before_balance = world.with_economy(|e| e.ledger.balance_of(&who_now(&world)));

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
        println!("   ledger unchanged?       {}", before_balance == world.with_economy(|e| e.ledger.balance_of(&who_now(&world))));
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
            println!("   balance now   {:.4}", e.ledger.balance_of(&who_now(&world)));
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

        rule("AUTH - the capability model, ENFORCED at the gate");
        {
            use sustena_core::principal::{effective_privilege, permitted};
            let who = who_now(&world);
            let m = world.memberships_for(&who);
            println!("   principal          {who}   (authenticated: a key was unlocked)");
            println!("   memberships        {}", m.len());
            for target in ["homestead", "habitat-bonnie", "habitat-cira"] {
                let path = world.authority_path(target);
                println!("   {:<18} path {:?}  effective tier {:?}",
                    target, path, effective_privilege(&m, &who, &path));
            }

            // The household model: owner of the household and of your OWN
            // habitat, observer on everyone else\'s.
            let op = "budget.record_income";
            let meta = world.operators.get(op).expect("registered");
            for target in ["habitat-bonnie", "habitat-cira"] {
                let path = world.authority_path(target);
                let verdict = permitted(&m, &who, &path, meta.min_privilege);
                println!("   {op} on {:<16} -> {}", target,
                    match &verdict { Ok(()) => "PERMITTED".to_string(), Err(d) => format!("{d}") });
            }

            // \u2605\u2605\u2605 And the GATE agrees, which is the whole point. The same call
            //   on the same two Sustains, through the real path.
            let before = world.with(|i| i.get("habitat-cira").map(|s| s.state.clone()));
            let mine = world.call("habitat-bonnie", op, &income(10.0)).expect("call");
            let theirs = world.call("habitat-cira", op, &income(10.0)).expect("call");
            let after = world.with(|i| i.get("habitat-cira").map(|s| s.state.clone()));
            println!("   gate on my own habitat     -> {:?}",
                mine.as_ref().map(|(x, _)| x.result.status));
            println!("   gate on another\'s habitat  -> {:?}   rule={:?}",
                theirs.as_ref().map(|(x, _)| x.result.status),
                theirs.as_ref().and_then(|(x, _)| x.result.constraint_violated.clone()));
            println!("   reason: {}",
                theirs.as_ref().and_then(|(x, _)| x.result.reason.clone()).unwrap_or_default());
            println!("   their state byte-unchanged -> {}", before == after);
            assert!(mine.as_ref().is_some_and(|(x, _)| x.committed()), "owner may act");
            assert!(theirs.as_ref().is_some_and(|(x, _)| !x.committed()), "observer may not");
            assert_eq!(
                theirs.as_ref().and_then(|(x, _)| x.result.constraint_violated.clone()).as_deref(),
                Some("insufficient_privilege"),
                "an authorization refusal has its OWN name, not the gate\'s"
            );
            assert_eq!(before, after, "a refused call must leave state untouched");
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
        // A reopened world starts LOCKED. The key is not on disk in usable
        // form and nothing acts until the passphrase recovers it -- which is
        // the property, stated by exercising it rather than by a comment.
        assert!(!world.is_unlocked(), "a reopened world must start locked");
        world.unlock(PASSPHRASE).expect("unlock");
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

        // -- holon.transfer -------------------------------------------------
        //
        // The keystone: an atomic, conserved move between two linked Sustains.
        rule("RUN 2 - holon.transfer - conserved, and both legs or neither");
        let bal = |w: &World, id: &str| -> f64 {
            w.with(|i| i.get(id).map(|s| s.state.clone()))
                .and_then(|st| st.pointer("/finances/liquid/balance").and_then(Value::as_f64))
                .unwrap_or(f64::NAN)
        };
        let (src, dst) = ("habitat-bonnie", "homestead");
        let b0 = bal(&world, src);
        let h0 = bal(&world, dst);
        let total0 = b0 + h0;
        println!("   before   {src} {b0:>10.2}   {dst} {h0:>10.2}   total {total0:>10.2}");

        let moved = 250.0;
        let t = world.transfer(src, dst, LIQUID, moved).expect("store");
        let b1 = bal(&world, src);
        let h1 = bal(&world, dst);
        let total1 = b1 + h1;
        println!("   after    {src} {b1:>10.2}   {dst} {h1:>10.2}   total {total1:>10.2}");
        println!("   moved {moved:.2}   committed={}", t.committed());
        println!("   CONSERVED  total_before == total_after  ->  {}", total0 == total1);
        assert!(t.committed(), "a funded, linked transfer must commit");
        assert_eq!(b1, b0 - moved, "the sender is debited exactly");
        assert_eq!(h1, h0 + moved, "the receiver is credited exactly");
        assert_eq!(total0, total1, "a transfer is a move, never a mint or a burn");

        // The refused case: nothing moves, on either side.
        rule("RUN 2 - a refused transfer moves NOTHING on either side");
        let over = bal(&world, src) + 1.0;
        let r = world.transfer(src, dst, LIQUID, over).expect("store");
        let b2 = bal(&world, src);
        let h2 = bal(&world, dst);
        match &r {
            Transfer::Refused { rule: why, reason } => println!("   REFUSED  {why}: {reason}"),
            Transfer::Committed(_) => panic!("an over-balance transfer must be refused"),
        }
        println!("   {src} {b2:>10.2} (was {b1:>10.2})   {dst} {h2:>10.2} (was {h1:>10.2})");
        assert_eq!((b2, h2), (b1, h1), "a refusal leaves both sides byte-identical");
        assert_eq!(b2 + h2, total1, "and the total is exactly what it was");

        // -- CRASH SAFETY -----------------------------------------------------
        //
        // Two .jsonl files cannot be appended atomically, so the store writes a
        // write-ahead journal first. Here the crash is PRODUCED, not imagined:
        // the journal lands, the first leg is appended, and then nothing else
        // happens -- exactly the state a power cut between the two appends
        // leaves behind.
        rule("RUN 2 - a crash between the two appends, produced and recovered");
        let st = Store::at(&dir).expect("store");
        let before_src = st.read_log(src).expect("log").len();
        let before_dst = st.read_log(dst).expect("log").len();
        let seq_a = world.with(|i| i.get(src).map(|s| s.next_seq)).expect("open");
        let seq_b = world.with(|i| i.get(dst).map(|s| s.next_seq)).expect("open");

        let crash_amount = 100.0;
        let links = vec![Link::new(src, dst)];
        let link = Linked::between(src, dst, &links).expect("linked");
        let party = |id: &str| {
            let (state, enforcement) = world
                .with(|i| i.get(id).map(|s| (s.state.clone(), s.enforcement.clone())))
                .expect("open");
            Party::new(id, state, enforcement)
        };
        let settled = holon_transfer(
            &link, &party(src), &party(dst), &Moving::money(LIQUID), crash_amount,
        );
        let Transfer::Committed(settlement) = &settled else { panic!("should commit") };
        let (leg_a, leg_b) = settlement.legs();
        st.crash_after_first_leg(
            "smoke-crash",
            [
                (leg_a.sustain_id().to_string(), line(seq_a, leg_a)),
                (leg_b.sustain_id().to_string(), line(seq_b, leg_b)),
            ],
        )
        .expect("the interrupted write");

        let mid_src = st.read_log(src).expect("log").len();
        let mid_dst = st.read_log(dst).expect("log").len();
        println!("   mid-crash  {src} log {before_src} -> {mid_src}   {dst} log {before_dst} -> {mid_dst}");
        println!("   (one leg on disk, one missing -- a half transfer, on purpose)");
        assert_eq!(mid_src, before_src + 1, "the first leg landed");
        assert_eq!(mid_dst, before_dst, "the second did not");

        // Now reopen. Recovery runs BEFORE any state is folded.
        drop(world);
        let healed = World::open(Store::at(&dir).expect("store")).expect("reopen");
        let st = Store::at(&dir).expect("store");
        let after_src = st.read_log(src).expect("log").len();
        let after_dst = st.read_log(dst).expect("log").len();
        let b3 = bal(&healed, src);
        let h3 = bal(&healed, dst);
        println!("   recovered  {src} log {after_src}   {dst} log {after_dst}");
        println!("   {src} {b3:>10.2}   {dst} {h3:>10.2}   total {:>10.2}", b3 + h3);
        println!("   CONSERVED after recovery  ->  {}", (b3 + h3) == total1);
        assert_eq!(after_src, before_src + 1, "the first leg is not duplicated");
        assert_eq!(after_dst, before_dst + 1, "the second leg was finished");
        assert_eq!(b3, b1 - crash_amount, "the sender ended debited exactly once");
        assert_eq!(h3, h1 + crash_amount, "the receiver ended credited exactly once");
        assert_eq!(b3 + h3, total1, "conservation survived the crash");

        // Recovery is idempotent: opening again changes nothing.
        drop(healed);
        let again = World::open(Store::at(&dir).expect("store")).expect("reopen");
        // ★ Every reopened world starts locked, and ingest is no exception: a
        //   captured message applies through the same gated path a person's own
        //   call takes, so it needs the same authenticated principal.
        again.unlock(PASSPHRASE).expect("unlock");
        println!(
            "   reopening again           ->  {src} {:>10.2}   {dst} {:>10.2}   (unchanged)",
            bal(&again, src),
            bal(&again, dst)
        );
        assert_eq!((bal(&again, src), bal(&again, dst)), (b3, h3), "recovery is idempotent");

        // And the fold still reproduces both sides from their logs alone.
        let st = Store::at(&dir).expect("store");
        for id in [src, dst] {
            let cached = again.with(|i| i.get(id).map(|s| s.state.clone())).expect("open");
            let (folded, _) = st.load_state(id).expect("fold");
            println!("   fold check {id:<18} {}", if cached == folded { "match" } else { "DIVERGED" });
            assert_eq!(cached, folded, "state must still be fold(log) after a recovered transfer");
        }

        // -- INGEST ----------------------------------------------------------
        rule("INGEST - a real message, through the real gate");
        let target = "homestead";
        let income = sustena_core::seed_rules("mpesa")
            .iter()
            .find(|r| r.id == "mpesa_received")
            .and_then(|r| r.examples.first().cloned())
            .expect("the shipped rule carries its own real example");
        let before_liquid = again
            .with(|i| i.get(target).map(|s| s.state.clone()))
            .and_then(|st| st.pointer("/finances/liquid/balance").and_then(Value::as_f64))
            .unwrap_or(f64::NAN);

        match again.capture(target, "mpesa", &income).expect("capture") {
            Capture::Stored(m) => {
                println!("   tier           {}   via {}", m.status, m.parser_name);
                println!("   amount         {:?}", m.params.get("amount"));
                println!("   applied        {}", m.applied);
                assert_eq!(m.status, "mapped");
                assert!(m.applied, "a mapped income must apply through the gate");
            }
            other => panic!("expected stored, got {other:?}"),
        }
        let after_liquid = again
            .with(|i| i.get(target).map(|s| s.state.clone()))
            .and_then(|st| st.pointer("/finances/liquid/balance").and_then(Value::as_f64))
            .unwrap_or(f64::NAN);
        println!("   liquid         {before_liquid:.2} -> {after_liquid:.2}   (through World::call, gated and logged)");
        assert!(after_liquid > before_liquid, "the operator really ran");

        // A replay applies nothing twice.
        match again.capture(target, "mpesa", &income).expect("capture") {
            Capture::Duplicate(_) => println!("   replay         duplicate, nothing applied twice"),
            other => panic!("expected duplicate, got {other:?}"),
        }
        let after_replay = again
            .with(|i| i.get(target).map(|s| s.state.clone()))
            .and_then(|st| st.pointer("/finances/liquid/balance").and_then(Value::as_f64))
            .unwrap_or(f64::NAN);
        assert_eq!(after_replay, after_liquid, "a replay must move nothing");

        // -- THE ONE THAT MUST NOT REGRESS -----------------------------------
        rule("INGEST - an OTP is refused, and NOTHING is stored");
        let rows_before = again.ingest().current().expect("queue").len();
        let otp = "Your OTP is 483920. Do not share it with anyone.";
        match again.capture(target, "mpesa", otp).expect("capture") {
            Capture::Rejected { reason } => println!("   REJECTED       {reason}"),
            other => panic!("an OTP must be rejected, got {other:?}"),
        }
        let rows_after = again.ingest().current().expect("queue").len();
        println!("   queue rows     {rows_before} -> {rows_after}   (unchanged)");
        assert_eq!(rows_after, rows_before, "a rejection must leave no row");

        // And the code appears NOWHERE under the store root.
        let mut leaked = Vec::new();
        for entry in std::fs::read_dir(dir.join("ingest")).expect("ingest dir") {
            let path = entry.expect("entry").path();
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            if content.contains("483920") || content.contains("OTP") {
                leaked.push(path.display().to_string());
            }
        }
        println!("   files holding the code or the word OTP: {leaked:?}");
        assert!(leaked.is_empty(), "the secret leaked into {leaked:?}");
        println!("   the only trace is a count: {}", again.ingest().rejected_count());

        // -- source-strict ---------------------------------------------------
        rule("INGEST - the sender decides the rules, never the body");
        match again.capture(target, "kcb", &income).expect("capture") {
            Capture::Stored(m) => {
                println!("   the same M-Pesa text, tagged kcb -> {}", m.status);
                assert_eq!(m.status, "unparsed", "a body must not cross senders");
                assert!(m.needs_attention(), "and it queues for a person");
            }
            other => panic!("expected stored, got {other:?}"),
        }

        // -- effect-first capture --------------------------------------------
        rule("INGEST - effect-first capture recovers what a person did not type");
        let pockets: Vec<String> = again
            .with(|i| i.get(target).map(|s| s.state.clone()))
            .and_then(|st| {
                st.pointer("/finances/pockets")
                    .and_then(|p| p.as_object().map(|o| o.keys().cloned().collect()))
            })
            .unwrap_or_default();
        let candidates: Vec<String> =
            vec!["budget.spend".to_string(), "budget.allocate".to_string()];
        let narration = format!("spent 500 on {}", pockets.first().cloned().unwrap_or_default());
        let capture = sustena_core::Capture {
            candidates: &candidates,
            pockets: &pockets,
            effect_text: Some(&narration),
            ..Default::default()
        };
        match sustena_core::infer(&again.operators, &capture) {
            sustena_core::Inference::Ready { operator, params, why, .. } => {
                println!("   \"{narration}\"");
                println!("   -> {operator}  {params:?}");
                println!("   why: {why}");
            }
            other => println!("   -> {other:?}"),
        }

        // -- correction-learning ---------------------------------------------
        rule("INGEST - a correction becomes a rule that then matches");
        let unseen = "ZZ99XYZ123 Confirmed. Ksh2,500.00 has been moved to your wallet by CHAMA on 1/8/26.";
        let Capture::Stored(m) = again.capture(target, "kcb", unseen).expect("capture") else {
            panic!("expected stored")
        };
        println!("   before learning  {} via {:?}", m.status, m.parser_name);
        assert_eq!(m.status, "unparsed");

        let mut params: std::collections::BTreeMap<String, Value> = Default::default();
        params.insert("amount".into(), json!(2500.0));
        params.insert("source".into(), json!("Chama"));
        let candidate = sustena_core::synthesize_from_correction(
            "kcb", unseen, "budget.record_income", &params, "learned_smoke",
        )
        .expect("synthesised");
        let existing = again.rules_for("kcb").expect("rules");
        sustena_core::verify_candidate(&candidate, unseen, &existing, &again.operators)
            .expect("verified");
        again.ingest().add_rule(&candidate).expect("stored");
        println!("   learned          {} (status {:?}, trust {:?})",
            candidate.id, candidate.status, candidate.trust);

        // The SAME shape next month, with a different amount and reference.
        let next = unseen.replace("2,500.00", "4,100.00").replace("ZZ99XYZ123", "AB12CD34EF");
        let Capture::Stored(m2) = again.capture(target, "kcb", &next).expect("capture") else {
            panic!("expected stored")
        };
        println!("   after learning   {} via {}  amount {:?}",
            m2.status, m2.parser_name, m2.params.get("amount"));
        assert_eq!(m2.status, "mapped", "the learned rule now recognises the shape");
        assert_eq!(m2.parser_name, "learned_smoke");
        assert!(m2.applied, "and it applied through the gate");

        // -- ORCHIE ----------------------------------------------------------
        //
        // The curated face, over the same real household. Nothing below sorts
        // anything: compose(r) does, and this reports what came back.
        rule("ORCHIE - the feed, ranked by the engine");
        let state = again.with(|i| i.get(target).map(|s| s.state.clone())).expect("open");
        let queued = again
            .ingest()
            .current()
            .expect("queue")
            .into_iter()
            .filter(|m| m.sustain_id == target && m.needs_attention())
            .count();
        let recent: Vec<sustena_core::event::Event> = again
            .log(target)
            .expect("log")
            .iter()
            .rev()
            .take(25)
            .flat_map(|e| {
                e.events.iter().map(|ev| {
                    sustena_core::event::Event::backfilled(
                        format!("{}-{}", e.seq, ev.name),
                        ev.name.clone(),
                        e.seq as i64,
                        sustena_core::event::CausalStamp::new("host"),
                    )
                })
            })
            .collect();

        let (reading, view) =
            mycelium_lib::orchie::feed(&again.operators, &state, queued, &recent, None, Vec::new())
                .expect("composed");
        println!("   projection     {}", serde_json::to_string(&reading).unwrap_or_default());
        println!("   budget {} spent {}   {} candidates considered",
                 view.budget, view.spent, view.candidates_considered);
        for c in &view.selected {
            println!("   [{}] {:<20} urgency {:.3} ({}) relevance {:.2} score {:.3} cost {}",
                c.rank, c.id, c.urgency,
                if c.basis.is_measured() { "measured" } else { "NOT measured" },
                c.relevance, c.score, c.cost);
        }
        for w in &view.withdrawn {
            println!("   quiet: {:<20} withdrew -- {}", w.id, w.reason);
        }
        for x in &view.excluded {
            println!("   quiet: {:<20} outranked -- score {:.3}", x.id, x.score);
        }
        assert!(view.spent <= view.budget, "the attention budget is a budget");
        assert!(!view.selected.is_empty(), "a real household has something to show");

        // A strained pocket produces REAL measured urgency, and the calm card
        // on the same household does not.
        rule("ORCHIE - urgency is measured against the household own ceiling");
        let strained = serde_json::json!({
            "finances": { "liquid": { "balance": 500.0 },
                          "pockets": { "food": { "allocated": 8000.0, "spent": 9000.0 } } }
        });
        let (_, v2) = mycelium_lib::orchie::feed(&again.operators, &strained, 0, &[], None, Vec::new())
            .expect("composed");
        for c in v2.selected.iter().chain(v2.excluded.iter()) {
            println!("   {:<20} urgency {:.3}  {}", c.id, c.urgency,
                if c.basis.is_measured() { "measured" } else { "NOT measured" });
        }
        let strain = v2.selected.iter().chain(v2.excluded.iter())
            .find(|c| c.id == "pocket_strain").expect("candidate");
        assert!(strain.urgency > 0.0 && strain.basis.is_measured());
        let calm = v2.selected.iter().chain(v2.excluded.iter())
            .find(|c| c.id == "household_summary").expect("candidate");
        assert_eq!(calm.urgency, 0.0, "the same household, a different dimension");

        // -- effect-first capture, all the way through the gate --------------
        rule("ORCHIE - a narrated effect, confirmed through the real gate");
        let pockets: Vec<String> = state
            .pointer("/finances/pockets")
            .and_then(|p| p.as_object().map(|o| o.keys().cloned().collect()))
            .unwrap_or_default();
        let pocket = pockets.first().cloned().expect("the household has a pocket");
        let narration = format!("spent 40 on {pocket}");
        let candidates: Vec<String> =
            vec!["budget.spend".to_string(), "budget.allocate".to_string()];
        let inferred = sustena_core::infer(
            &again.operators,
            &sustena_core::Capture {
                candidates: &candidates,
                pockets: &pockets,
                effect_text: Some(&narration),
                ..Default::default()
            },
        );
        let sustena_core::Inference::Ready { operator, params, why, .. } = inferred else {
            panic!("expected ready, got {inferred:?}")
        };
        println!("   \"{narration}\"");
        println!("   -> {operator}  {params:?}");
        println!("   why: {why}");

        let spent_before = state
            .pointer(&format!("/finances/pockets/{pocket}/spent"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let call_params: Map<String, Value> = params.clone().into_iter().collect();
        let (x, _) = again.call(target, &operator, &call_params).expect("call").expect("sustain");
        let spent_after = again
            .with(|i| i.get(target).map(|s| s.state.clone()))
            .and_then(|st| st.pointer(&format!("/finances/pockets/{pocket}/spent"))
                .and_then(Value::as_f64))
            .unwrap_or(0.0);
        println!("   gate: {:?}   {pocket} spent {spent_before:.2} -> {spent_after:.2}",
                 x.result.status);
        assert!(x.committed(), "a confirmed capture commits through the real gate");
        assert_eq!(spent_after, spent_before + 40.0);

        // -- and an honest refusal on drift ----------------------------------
        rule("ORCHIE - a refusal on drift, honestly");
        let over: Map<String, Value> = [
            ("pocket_name".to_string(), json!(pocket)),
            ("amount".to_string(), json!(9_999_999.0)),
        ]
        .into_iter()
        .collect();
        let before = again.with(|i| i.get(target).map(|s| s.state.clone()));
        let (refused, _) =
            again.call(target, "budget.spend", &over).expect("call").expect("sustain");
        let after = again.with(|i| i.get(target).map(|s| s.state.clone()));
        println!("   {:?}   {}", refused.result.status,
                 refused.result.reason.clone().unwrap_or_default());
        assert!(!refused.committed());
        assert_eq!(before, after, "a refusal leaves the household byte-unchanged");

        // -- what stayed quiet, and why --------------------------------------
        //
        // Two different kinds of quiet, and the feed does not conflate them:
        // a card that WITHDREW (it had nothing to say, and says so), and a card
        // that beta never offered (no such event has happened).
        rule("ORCHIE - the two kinds of quiet");
        let (_, v3) = mycelium_lib::orchie::feed(&again.operators, &strained, 0, &[], None, Vec::new())
            .expect("composed");
        for w in &v3.withdrawn {
            println!("   withdrew   {:<20} -- {}", w.id, w.reason);
        }
        assert!(
            v3.withdrawn.iter().any(|w| w.id == "classify_capture"),
            "nothing to classify -- the card withdraws rather than showing an empty box",
        );
        assert!(v3.withdrawn.iter().all(|w| !w.reason.is_empty()), "and each says why");
        assert!(
            !v3.selected.iter().chain(v3.excluded.iter()).any(|c| c.id == "recent_spend"),
            "no spend has happened, so beta never offers the card",
        );

        // The same household, after money actually moved: beta now binds it.
        let spent_event = sustena_core::event::Event::backfilled(
            "e-spent".to_string(),
            "event.finances.pocket_spent".to_string(),
            1,
            sustena_core::event::CausalStamp::new("host"),
        );
        let (_, v4) =
            mycelium_lib::orchie::feed(&again.operators, &strained, 0, &[spent_event], None, Vec::new())
                .expect("composed");
        println!("   after a real spend, beta offers {} candidates (was {})",
                 v4.candidates_considered, v3.candidates_considered);
        assert!(
            v4.selected.iter().chain(v4.excluded.iter()).any(|c| c.id == "recent_spend"),
            "money moved, so the card is a candidate -- beta decided that, not an `if`",
        );
    }
}

/// Where the household money lives. The core has no money concept and takes
/// the dimension as a parameter, so naming it is the HOST job.
const LIQUID: &str = "finances.liquid.balance";

/// One leg as a log line, matching what `World::transfer` writes.
fn line(seq: u64, leg: &Leg) -> LoggedEvent {
    LoggedEvent {
        seq,
        operator: "holon.transfer".to_string(),
        events: vec![mycelium_lib::dto::EventDto::from(leg.event())],
        mutations: leg.mutations().to_vec(),
        origin: None,
        lamport: None,
    }
}

/// The smoke passphrase. ★ A fixture for a scratch store the run creates and
/// throws away — never a real identity.
const PASSPHRASE: &str = "smoke-run passphrase";

/// `budget.record_income` params, for the locked-world check above.
fn income(amount: f64) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("amount".into(), json!(amount));
    m.insert("source".into(), json!("smoke"));
    m
}

/// The authenticated handle, read off the world rather than declared.
fn who_now(world: &World) -> String {
    world.principal().unwrap_or_else(|| "<locked>".to_string())
}
