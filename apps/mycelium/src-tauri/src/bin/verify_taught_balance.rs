//! Prove the taught-balance path against a REAL store.
//!
//! Not shipped. It answers the one question the feature has to answer: after a
//! person tags a figure as "the balance" on a shape nobody wrote a rule for,
//! does that account's displayed balance become the bank's own latest number?
//!
//! Usage: verify_taught_balance <store root> <sustain_id>
use mycelium_lib::ingest::Ingested;

fn main() {
    let mut a = std::env::args().skip(1);
    let root = a.next().expect("a store root");
    let sustain = a.next().expect("a sustain id");
    let store = Ingested::at(&root).expect("the store opens");
    let seeded = sustena_core::all_seed_rules();

    // The shape the feed would offer, built exactly as `get_feed` builds it.
    let corpus: Vec<(String, String)> = store
        .current()
        .expect("current")
        .into_iter()
        .filter(|m| m.sustain_id == sustain && m.status == "unparsed" && !m.ignored)
        .map(|m| (m.id, m.raw_payload))
        .collect();
    let clusters =
        sustena_core::corpus::cluster(corpus.iter().map(|(i, r)| (i.as_str(), r.as_str())));
    let Some(top) = clusters.first() else {
        println!("nothing unreadable to teach");
        return;
    };
    println!("OFFERED: {} messages share this shape", top.count());
    println!("   {}", top.example.replace('\n', " "));

    let id = top.members.first().expect("a member").clone();
    let m = store.current().unwrap().into_iter().find(|x| x.id == id).expect("the message");
    println!("   resolved={}  status={:?}", m.resolved, m.status);

    // Teach it the way the button does: every currency figure, first one out.
    let mut figures: Vec<sustena_core::TrainedFigure> = Vec::new();
    let mut seen = 0;
    let text = m.raw_payload.clone();
    let mut rest = text.as_str();
    while let Some(at) = rest.to_uppercase().find("KSH") {
        let after = &rest[at + 3..];
        let n: String = after
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
            .collect();
        let n = n.trim_end_matches('.').to_string();
        if n.chars().any(|c| c.is_ascii_digit()) {
            seen += 1;
            figures.push(sustena_core::TrainedFigure {
                text: n,
                role: if seen == 1 {
                    sustena_core::RouteRole::Out
                } else {
                    sustena_core::RouteRole::Balance
                },
                pocket: if seen == 1 { "Leisure".into() } else { String::new() },
            });
        }
        rest = &rest[at + 3..];
        if seen >= 2 {
            break;
        }
    }
    println!("   teaching {} figure(s)", figures.len());

    let rule = sustena_core::synthesize_from_training("mpesa", &m.raw_payload, &figures, "taught_loop")
        .expect("it learns");
    store.add_rule(&rule).expect("saved");
    let rules: Vec<_> = seeded.iter().cloned().chain(store.learned_rules().unwrap()).collect();
    let n = store.reparse_unparsed(&sustain, &rules).expect("re-read");
    println!("
AFTER TEACHING: {n} message(s) became readable");

    // Would the feed offer the SAME shape again? That is the loop.
    let corpus2: Vec<(String, String)> = store
        .current()
        .expect("current")
        .into_iter()
        .filter(|m| m.sustain_id == sustain && m.status == "unparsed" && !m.ignored)
        .map(|m| (m.id, m.raw_payload))
        .collect();
    let again =
        sustena_core::corpus::cluster(corpus2.iter().map(|(i, r)| (i.as_str(), r.as_str())));
    let same = again.iter().any(|c| c.skeleton == top.skeleton);
    println!("SAME SHAPE OFFERED AGAIN: {same}   <-- must be false");
    let after = store.current().unwrap().into_iter().find(|x| x.id == id).expect("still there");
    println!("that message: status={:?} resolved={} filed={}",
        after.status, after.resolved, after.filed.len());
    println!("back in his queue: {}   <-- must be false if it was settled",
        after.needs_attention() && m.resolved);
}
