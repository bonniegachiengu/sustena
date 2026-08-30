//! What the field reader recovers from real messages the hand-written rules
//! could not read.
//!
//! Aggregates only — it prints counts and never a message. Run with a JSON
//! array of `{source, text, status, parser}` on argv[1].

use std::collections::BTreeMap;

use sustena_core::field_shape::{induce, read, Direction, Role};

fn main() {
    let path = std::env::args().nth(1).expect("usage: field_coverage <backlog.json>");
    let raw = std::fs::read_to_string(&path).expect("read");
    let msgs: Vec<serde_json::Value> = serde_json::from_str(&raw).expect("parse");

    let mut total = 0usize;
    let mut unparsed = 0usize;
    let mut recovered: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut any_field = 0usize;
    let mut inducible = 0usize;
    let mut would_map = 0usize;
    let mut dir: BTreeMap<&'static str, usize> = BTreeMap::new();

    for m in &msgs {
        total += 1;
        let text = m["text"].as_str().unwrap_or("");
        let parser = m["parser"].as_str().unwrap_or("");
        // Only the ones the shipped rules did NOT read.
        if !parser.is_empty() {
            continue;
        }
        unparsed += 1;

        let r = read(text);
        if r.recovered() > 0 {
            any_field += 1;
        }
        for f in &r.fields {
            *recovered.entry(name(f.role)).or_default() += 1;
        }
        *dir.entry(match r.direction {
            Direction::In => "in",
            Direction::Out => "out",
            Direction::Unclear => "unclear",
        })
        .or_default() += 1;

        if let Ok(rule) = induce(text, m["source"].as_str().unwrap_or("?"), "probe") {
            inducible += 1;
            if rule.operator.is_some() {
                would_map += 1;
            }
        }
    }

    println!("messages in the extract : {total}");
    println!("no shipped rule read it : {unparsed}");
    println!();
    println!("at least one field read : {any_field}  ({}%)", pct(any_field, unparsed));
    println!("a rule could be induced : {inducible}  ({}%)", pct(inducible, unparsed));
    println!("  of which would MAP    : {would_map}  (inbound only, by design)");
    println!();
    println!("fields recovered, by role:");
    for (k, v) in &recovered {
        println!("  {k:14} {v}");
    }
    println!();
    println!("direction:");
    for (k, v) in &dir {
        println!("  {k:8} {v}");
    }
}

fn pct(a: usize, b: usize) -> usize {
    (a * 100).checked_div(b).unwrap_or(0)
}

fn name(r: Role) -> &'static str {
    match r {
        Role::Amount => "amount",
        Role::Balance => "balance",
        Role::Fee => "fee",
        Role::Reference => "reference",
        Role::Counterparty => "counterparty",
        Role::Account => "account",
        Role::Date => "date",
        Role::Time => "time",
    }
}
