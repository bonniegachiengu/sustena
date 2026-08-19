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
