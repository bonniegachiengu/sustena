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

use mycelium_lib::dto::{GateResult, SustainSummary};
use mycelium_lib::store::Store;
use mycelium_lib::world::World;

fn params(pairs: &[(&str, Value)]) -> Map<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

fn rule(title: &str) {
    println!("\n── {title} {}", "─".repeat(58usize.saturating_sub(title.len())));
}

fn household(world: &World) {
    let rows: Vec<SustainSummary> =
        world.with(|i| i.order().iter().filter_map(|id| i.get(id)).map(SustainSummary::of).collect());
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
            let _ = seq;
            println!(
                "   {sustain} · {op} -> {:?}  mutations={} events={}{}",
                r.verdict,
                r.mutations,
                r.events.len(),
                r.reason.map(|s| format!("\n      reason: {s}")).unwrap_or_default()
            );
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
    }
}
