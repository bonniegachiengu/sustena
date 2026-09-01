//! A throwaway checker: run a REAL store through the REAL balance code.
//!
//! Not a test and not shipped. It exists so "the displayed balance equals what
//! that account's latest parsed SMS says" can be checked against a household's
//! own data rather than against a fixture that agrees with me by construction.
//!
//! Usage: verify_balances <dir containing ingest/messages.jsonl> <sustain_id>
use mycelium_lib::ingest::Ingested;

fn main() {
    let mut a = std::env::args().skip(1);
    let root = a.next().expect("a store root");
    let sustain = a.next().expect("a sustain id");
    let store = Ingested::at(&root).expect("the store opens");

    let rules = store.effective_rules().unwrap_or_default();
    let seeded: Vec<_> = sustena_core::all_seed_rules().into_iter().chain(rules).collect();

    let before = store.reported_balances(&sustain).expect("balances");
    println!("BEFORE backfill: {} account(s)", before.len());
    for (k, v) in &before {
        println!("   {k:>10}  {:>12.2}  at {}", v.balance, v.at);
    }

    let n = store.backfill_event_times(&sustain, &seeded).expect("backfill");
    println!("\nbackfilled {n} message(s)\n");

    let after = store.reported_balances(&sustain).expect("balances");
    println!("AFTER backfill: {} account(s)", after.len());
    for (k, v) in &after {
        println!("   {k:>10}  {:>12.2}  at {}", v.balance, v.at);
    }

    // The message each figure came from, so the number can be read back
    // against the text that produced it.
    println!("\nthe message behind each figure:");
    for (acct, rep) in &after {
        let mut best: Option<mycelium_lib::ingest::IngestedMessage> = None;
        for m in store.current().unwrap() {
            if m.sustain_id != sustain {
                continue;
            }
            if m.event_at_ms.or(m.sent_at_ms) == Some(rep.at) {
                let is_it = mycelium_lib::ingest::reported_account_of(&m)
                    .is_some_and(|(a, b)| &a == acct && (b - rep.balance).abs() < 0.005);
                if is_it {
                    best = Some(m);
                }
            }
        }
        match best {
            Some(m) => println!(
                "   {acct:>10} = {:.2}\n        {}\n",
                rep.balance,
                m.raw_payload.replace('\n', " ")
            ),
            None => println!("   {acct:>10} = {:.2}  (no message matched back!)\n", rep.balance),
        }
    }

    // ── what filed itself, and what disagrees ────────────────────────────
    let review = store.auto_filed_review(&sustain, 500).expect("review");
    let doubted: Vec<_> = review.iter().filter(|r| !r.doubts.is_empty()).collect();
    println!("\n=== filed without asking ===");
    println!("{} auto-filed, {} with something against them", review.len(), doubted.len());
    for r in doubted.iter().take(12) {
        println!("\n   {}", r.message.raw_payload.replace('\n', " "));
        println!("      booked: {:?} {:?}", r.message.operator, r.message.params);
        for d in &r.doubts {
            println!("      ! {}", d.say());
        }
    }
}
