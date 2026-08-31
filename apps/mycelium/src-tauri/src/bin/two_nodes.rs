//! Two nodes, in two OS processes, converging over a real socket.
//!
//! ★★★ The test suite proves two `World`s in one binary converge. This proves
//! it across a process boundary, which is the rung between "two replicas" and
//! "two machines" — same sockets, same handshake, two schedulers.
//!
//! It also runs the case the suite could not until §VI landed: **both nodes
//! create the same Sustain before they meet**, so two genesis lines end up in
//! one log. Under an overwrite that erased a household; under the §VI join it
//! converges.
//!
//! ```text
//! two_nodes prepare <rootA> <rootB> <portA> <portB>
//! two_nodes serve   <rootA> <portA>
//! two_nodes dial    <rootB> <portB> 127.0.0.1:<portA>
//! two_nodes show    <root>
//! ```

use std::path::PathBuf;

use mycelium_lib::peers::Standing;
use mycelium_lib::store::Store;
use mycelium_lib::templates::TemplateId;
use mycelium_lib::world::{World, DEFAULT_HANDLE};
use serde_json::{json, Map, Value};

const PASS: &str = "a-long-enough-passphrase";
const ID: &str = "homestead";

fn open(root: &str, port: u16) -> World {
    let home = PathBuf::from(root);
    std::fs::create_dir_all(&home).expect("home");
    let store = Store::at(&home).expect("store");
    let world = World::open(store).expect("world");
    let mut net = world.network();
    net.listen_port = port;
    net.auto_reconnect = false;
    world.set_network(net).expect("settings");
    if world.unlock_if_remembered().is_none() {
        match world.enrol(DEFAULT_HANDLE, PASS) {
            Ok(_) => world.remember_unlock(PASS).expect("remember"),
            Err(_) => {
                world.unlock(PASS).expect("unlock");
                world.remember_unlock(PASS).expect("remember");
            }
        }
    }
    world
}

fn income(world: &World, amount: f64, note: &str) {
    let mut params = Map::new();
    params.insert("amount".into(), json!(amount));
    params.insert("source".into(), json!(note));
    world.call(ID, "budget.record_income", &params).expect("call").expect("sustain");
}

fn balance(world: &World) -> Value {
    world
        .with(|i| i.get(ID).map(|s| s.state.clone()))
        .and_then(|s| s.pointer("/finances/liquid/balance").cloned())
        .unwrap_or(Value::Null)
}

fn log_len(root: &str) -> usize {
    std::fs::read_to_string(PathBuf::from(root).join("events").join(format!("{ID}.jsonl")))
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("prepare") => {
            let (ra, rb) = (args[2].clone(), args[3].clone());
            let (pa, pb) = (args[4].parse().unwrap(), args[5].parse().unwrap());
            let a = open(&ra, pa);
            let b = open(&rb, pb);

            // ★★★ BOTH create it. Two genesis lines, one id — the shape two
            //     real devices were in.
            for w in [&a, &b] {
                let _ = w.instantiate_owned(
                    ID,
                    "Homestead",
                    TemplateId::Habitat,
                    None,
                    None,
                    Some(DEFAULT_HANDLE),
                );
            }
            income(&a, 300.0, "node-a");
            income(&b, 700.0, "node-b");

            let (ka, kb) = (a.node_id().unwrap(), b.node_id().unwrap());
            for (me, them, addr) in [
                (&a, &kb, format!("127.0.0.1:{pb}")),
                (&b, &ka, format!("127.0.0.1:{pa}")),
            ] {
                me.peering()
                    .edit(|book| {
                        book.seen(them, "peer", Some(addr.clone()));
                        book.set_standing(them, Standing::Trusted);
                        book.share(them, ID)
                    })
                    .expect("book")
                    .expect("share");
            }
            println!("prepared");
            println!("  A {ra} key={ka} port={pa} balance={} log={}", balance(&a), log_len(&ra));
            println!("  B {rb} key={kb} port={pb} balance={} log={}", balance(&b), log_len(&rb));
        }
        Some("serve") => {
            let (root, port) = (args[2].clone(), args[3].parse().unwrap());
            let world = open(&root, port);
            let merges = world.merges();
            println!("[A] listening on {:?}, balance {}", world.peering().port(), balance(&world));
            // ★ The absorber, exactly as the app runs it: the responder learns
            //   it changed, and re-folds.
            std::thread::spawn(move || {
                while let Ok(id) = merges.recv() {
                    world.absorb(&id);
                    println!("[A] merged and re-folded {id}: balance {}", balance(&world));
                }
            });
            std::thread::sleep(std::time::Duration::from_secs(
                args.get(4).and_then(|s| s.parse().ok()).unwrap_or(25),
            ));
            println!("[A] done");
        }
        Some("dial") => {
            let (root, port, addr) = (args[2].clone(), args[3].parse().unwrap(), args[4].clone());
            let world = open(&root, port);
            println!("[B] before: balance {} log {}", balance(&world), log_len(&root));

            let first = world.sync_peer(&addr, ID).expect("sync");
            println!(
                "[B] sync 1: received {} sent {}  -> balance {} log {}",
                first.outcome.received,
                first.outcome.sent,
                balance(&world),
                log_len(&root)
            );

            // §VI's third law, on the wire.
            let second = world.sync_peer(&addr, ID).expect("sync again");
            println!(
                "[B] sync 2: received {} sent {}  -> balance {} log {}  (idempotent: {})",
                second.outcome.received,
                second.outcome.sent,
                balance(&world),
                log_len(&root),
                second.outcome.received == 0 && second.outcome.sent == 0
            );

            let reported = world.with(|i| i.get(ID).map(|s| s.state.clone())).unwrap();
            let refolded = world.reload(ID).expect("re-fold").state;
            println!("[B] rebuild_state == get_state: {}", reported == refolded);
        }
        // ★★★ Implant the REAL device logs into two scratch nodes, with the
        //     origins remapped onto these nodes' own keys. Same seqs, same
        //     lamports, same counts -- so the replica frontiers are identical
        //     to the real ones and the push can be debugged without rebuilding
        //     a phone app.
        Some("implant") => {
            let (ra, rb) = (args[2].clone(), args[3].clone());
            let (pa, pb) = (args[4].parse().unwrap(), args[5].parse().unwrap());
            let (lap_log, ph_log) = (args[6].clone(), args[7].clone());
            let (real_lap, real_ph) = (args[8].clone(), args[9].clone());
            let a = open(&ra, pa);
            let b = open(&rb, pb);
            let (ka, kb) = (a.node_id().unwrap(), b.node_id().unwrap());

            for (root, src, id) in [(&ra, &lap_log, "A"), (&rb, &ph_log, "B")] {
                let text = std::fs::read_to_string(src).expect("read log");
                let mut out = String::new();
                for line in text.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let remapped = line.replace(&real_lap, &ka).replace(&real_ph, &kb);
                    out.push_str(&remapped);
                    out.push('\n');
                }
                let dir = std::path::PathBuf::from(root).join("events");
                std::fs::create_dir_all(&dir).expect("events dir");
                std::fs::write(dir.join(format!("{ID}.jsonl")), out).expect("write");
                println!("implanted {id} <- {src}");
            }

            for (me, them, addr) in [
                (&a, &kb, format!("127.0.0.1:{pb}")),
                (&b, &ka, format!("127.0.0.1:{pa}")),
            ] {
                me.peering()
                    .edit(|book| {
                        book.seen(them, "peer", Some(addr.clone()));
                        book.set_standing(them, Standing::Trusted);
                        book.share(them, ID)
                    })
                    .expect("book")
                    .expect("share");
            }
            println!("A key={ka} port={pa} log={}", log_len(&ra));
            println!("B key={kb} port={pb} log={}", log_len(&rb));
        }
        Some("show") => {
            let root = args[2].clone();
            let world = open(&root, args[3].parse().unwrap());
            let state = world.with(|i| i.get(ID).map(|s| s.state.clone())).unwrap_or(Value::Null);
            println!("{} log={} balance={}", root, log_len(&root), balance(&world));
            println!("{}", serde_json::to_string(&state).unwrap());
        }
        _ => eprintln!("usage: two_nodes prepare|serve|dial|show ..."),
    }
}
