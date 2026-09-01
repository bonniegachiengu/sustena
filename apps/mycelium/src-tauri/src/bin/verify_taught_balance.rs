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

    // The newest message nobody can read that states a balance.
    // Plain scanning rather than a regex: this is a throwaway checker and it
    // is not worth a dependency the app itself does not need here.
    fn figure_after(text: &str, marker: &str) -> Option<String> {
        let at = text.find(marker)? + marker.len();
        let rest = text[at..].trim_start();
        let rest = rest.strip_prefix("Ksh").unwrap_or(rest).trim_start();
        let n: String =
            rest.chars().take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.').collect();
        let n = n.trim_end_matches('.').to_string();
        (!n.is_empty() && n.chars().any(|c| c.is_ascii_digit())).then_some(n)
    }
    fn figure_before(text: &str, marker: &str) -> Option<String> {
        let at = text.find(marker)?;
        let head = text[..at].trim_end();
        let n: String = head
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        (!n.is_empty() && n.chars().any(|c| c.is_ascii_digit())).then_some(n)
    }

    let mut best: Option<(i64, mycelium_lib::ingest::IngestedMessage, String, String)> = None;
    for m in store.current().expect("current") {
        if m.sustain_id != sustain || !m.parser_name.trim().is_empty() {
            continue;
        }
        let (Some(b), Some(x)) = (
            figure_after(&m.raw_payload, "balance is"),
            figure_before(&m.raw_payload, " paid to"),
        ) else {
            continue;
        };
        // Order by what the text says, using the same reader the engine uses.
        let t = sustena_core::parse_message(&m.raw_payload, Some(&m.source_id), &seeded);
        let at = sustena_core::event_time(&t.parsed_fields()).map(|d| d.to_epoch_ms(180));
        let at = at.or(m.sent_at_ms).unwrap_or(0);
        if best.as_ref().is_none_or(|(prev, ..)| at > *prev) {
            best = Some((at, m, x, b));
        }
    }

    let Some((_, m, amount_text, balance_text)) = best else {
        println!("no unreadable message states a balance -- nothing to demonstrate");
        return;
    };

    println!("BEFORE");
    let before = store.reported_balances(&sustain).expect("balances");
    for (k, v) in &before {
        println!("   {k:>10} {:>12.2}", v.balance);
    }
    println!("\nteaching this message, which nothing reads today:");
    println!("   {}", m.raw_payload.replace('\n', " "));
    println!("   spend {amount_text}  ·  balance {balance_text}");

    let figures = vec![
        sustena_core::TrainedFigure {
            text: amount_text,
            role: sustena_core::RouteRole::Out,
            pocket: "shopping".into(),
        },
        sustena_core::TrainedFigure {
            text: balance_text,
            role: sustena_core::RouteRole::Balance,
            pocket: String::new(),
        },
    ];
    let rule = sustena_core::synthesize_from_training("mpesa", &m.raw_payload, &figures, "taught_demo")
        .expect("it learns");
    println!("
   status of the message: {:?}  resolved={} ignored={}", m.status, m.resolved, m.ignored);
    println!("   the rule reads its own example: {}",
        sustena_core::run_rules(std::slice::from_ref(&rule), &m.raw_payload).is_some());
    if let Some(t) = sustena_core::run_rules(std::slice::from_ref(&rule), &m.raw_payload) {
        println!("   fields: {:?}", t.parsed_fields());
    }
    store.add_rule(&rule).expect("saved");

    let rules: Vec<_> = seeded.iter().cloned().chain(store.learned_rules().unwrap()).collect();
    let n = store.reparse_unparsed(&sustain, &rules).expect("re-read");
    let r = store.refresh_readings(&sustain, &rules).expect("refresh");
    let t = store.backfill_event_times(&sustain, &rules).expect("times");
    println!("\n{n} message(s) became readable, {t} recovered when they happened");

    println!("\nAFTER");
    let after = store.reported_balances(&sustain).expect("balances");
    for (k, v) in &after {
        println!("   {k:>10} {:>12.2}", v.balance);
    }

    // And the message each figure now comes from, so the number can be read back.
    println!("\nthe message behind each figure:");
    for (acct, rep) in &after {
        for msg in store.current().unwrap() {
            if msg.sustain_id != sustain {
                continue;
            }
            if msg.event_at_ms.or(msg.sent_at_ms) == Some(rep.at) {
                if let Some((a, b)) = mycelium_lib::ingest::reported_account_of(&msg) {
                    if &a == acct && (b - rep.balance).abs() < 0.005 {
                        println!("   {acct:>10} = {:.2}", rep.balance);
                        println!("        {}\n", msg.raw_payload.replace('\n', " "));
                        break;
                    }
                }
            }
        }
    }
}
