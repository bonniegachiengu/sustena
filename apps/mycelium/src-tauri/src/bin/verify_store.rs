//! A before/after fingerprint of a real store, taken through the real engine.
//!
//! ★★★ Written for one job: the person/device separation re-attributes every
//! entry a household has ever written, and his books must survive it exactly.
//! Comparing two runs of this is the proof — not "it compiled", not "the tests
//! pass", but his own fourteen pockets and two thousand messages reading the
//! same on both sides of the change.
//!
//! Usage: verify_store <app data root>
use mycelium_lib::{ingest::Ingested, store::Store, world::World};

fn main() {
    let root = std::env::args().nth(1).expect("a store root");
    let store = Store::at(&root).expect("store");
    let world = World::open(store).expect("world");

    let mut ids: Vec<String> = world.with(|i| i.order().to_vec());
    ids.sort();
    println!("sustains: {}", ids.len());

    for id in &ids {
        let (events, state) = world
            .with(|i| i.get(id).map(|s| (s.next_seq, s.state.clone())))
            .unwrap_or((0, serde_json::Value::Null));
        let liquid = state
            .pointer("/finances/liquid/balance")
            .and_then(|v| v.as_f64())
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".into());
        println!("  {id}  events={events}  liquid={liquid}");

        if let Some(p) = state.pointer("/finances/pockets").and_then(|v| v.as_object()) {
            let mut names: Vec<&String> = p.keys().collect();
            names.sort();
            for n in names {
                let a = p[n].get("allocated").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let s = p[n].get("spent").and_then(|v| v.as_f64()).unwrap_or(0.0);
                println!("      {n:<18} allocated={a:.2} spent={s:.2}");
            }
        }
        // ★★ The fold, recomputed from the log and compared. A relabelled
        //    stamp that changed what the log MEANS would show here.
        // ★★ The fold, recomputed from the log on disk and compared with what
        //    the world is holding. A relabelled stamp that changed what the log
        //    MEANS would show up right here.
        match world.store().load_state(id) {
            Ok((rebuilt, _)) => println!("      file-order rebuild==state: {}", rebuilt == state),
            Err(e) => println!("      file-order rebuild FAILED: {e}"),
        }
        // ★★★ The causal reading, which is what a shared Sustain uses. If the
        //     two disagree, the numbers on his screen change the day he shares
        //     a household -- so this is the one that has to match.
        let node = world.node_id().unwrap_or_default();
        match world.store().load_replicated(id, &node, None) {
            Ok((rec, _)) => {
                println!("      causal rebuild==state: {}", rec.state == state);
                if rec.state != state {
                    let a = rec.state.pointer("/finances/liquid/balance").cloned();
                    let b = state.pointer("/finances/liquid/balance").cloned();
                    println!("      causal liquid={a:?}  cached liquid={b:?}");
                }
            }
            Err(e) => println!("      causal rebuild FAILED: {e}"),
        }
    }

    let ingest = Ingested::at(&root).expect("ingest");
    let msgs = ingest.current().expect("messages");
    println!("\ncaptured messages: {}", msgs.len());
    let unparsed = msgs.iter().filter(|m| m.status == "unparsed" && !m.ignored).count();
    println!("unparsed: {unparsed}");
    println!("learned rules: {}", ingest.learned_rules().expect("rules").len());

    for id in &ids {
        if let Ok(b) = ingest.reported_balances(id) {
            if !b.is_empty() {
                let mut accts: Vec<_> = b.into_iter().collect();
                accts.sort_by(|a, c| a.0.cmp(&c.0));
                for (a, r) in accts {
                    println!("  balance {a:<10} {:.2}", r.balance);
                }
            }
        }
    }
}
