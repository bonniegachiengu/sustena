//! **One transfer, two senders, one fact** (Ingest · §IV).
//!
//! ```text
//!   KCB:    "Ksh 40,000 sent to KCB Pay Bill … M-PESA ref UH1B91GYNW"
//!   M-Pesa: "UH1B91GYNW Confirmed. Ksh40,000.00 sent to …"
//!                        └──────── one movement of money ────────┘
//! ```
//!
//! ★★★ **The raw-text dedup cannot see this, by construction.** It keys on the
//! text, and these two texts are genuinely different — different senders,
//! different wording, describing one event. So both are captured (correctly:
//! they ARE two messages) and, if both are applied, the household's books show
//! forty thousand shillings moving twice. That is not a rounding error; it is
//! money that never existed appearing in somebody's real accounts.
//!
//! ## Why this SURFACES rather than suppresses
//!
//! ★★★ **A confident auto-suppression here would be the more dangerous bug.**
//! Silently dropping the second message means a real, separate transaction that
//! happened to share a reference code vanishes without trace — and nobody finds
//! out, because the whole point of suppression is that nothing is shown. A
//! false positive that asks costs one tap; a false positive that hides costs a
//! transaction and the trust in every other number.
//!
//! ★★ So this reports a **suspicion, with both messages named**, and a person
//! says. That is the same division every other judgment in this engine keeps:
//! the machine finds the pattern, the human decides what it means.
//!
//! ## What makes a key, and what deliberately does not
//!
//! ★★★ **A shared reference AND a matching amount.** Either alone is not a
//! fact: reference codes are allocated by different systems and could collide,
//! and two payments of forty thousand on one day are perfectly ordinary. Both
//! together is a coincidence worth asking about.
//!
//! ★★★ **No text-hash fallback across sources.** Within one source, identical
//! text means the same message. Across sources it means nothing at all — two
//! descriptions of one event have different text *by definition*, so a
//! text-based key would be exactly backwards here.
//!
//! ★★ **Direction is not required to match, and opposite directions are
//! evidence FOR rather than against.** One side sent and the other received is
//! what a single transfer looks like from two vantage points; requiring
//! agreement would rule out the commonest real case.

use std::collections::BTreeMap;

use serde_json::Value;

/// The fewest characters a reference can have and still identify anything.
///
/// ★★★ Short codes collide. An M-Pesa reference is ten characters; something
/// four long is a fragment somebody's regex caught, and matching on it would
/// join two unrelated transactions with confidence.
const MIN_REF: usize = 8;

/// The tolerance on "the same amount", in shillings.
///
/// ★★ Not zero, because the two sides of one transfer are sometimes quoted
/// with and without a fee — but small, because a real difference of even a few
/// shillings is a different transaction rather than a rounding of one.
const AMOUNT_TOLERANCE: f64 = 0.005;

/// What makes two messages one fact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FactKey {
    /// The transaction reference, upper-cased and stripped to alphanumerics.
    pub reference: String,
    /// The amount in whole minor units, so two spellings of one number agree.
    pub amount_minor: i64,
}

/// One captured message, as correlation needs to see it.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub message_id: String,
    /// Which sender it came from — `mpesa`, `kcb`.
    pub source_id: String,
    /// What the transducer read off it.
    pub parsed_fields: BTreeMap<String, Value>,
}

/// Two messages that look like one movement of money.
#[derive(Debug, Clone, PartialEq)]
pub struct Suspected {
    pub key: FactKey,
    /// The one already applied, or the earlier of the two.
    pub first: String,
    /// The one that would double it.
    pub second: String,
    pub first_source: String,
    pub second_source: String,
    /// In plain words, for the person who has to decide.
    pub why: String,
}

/// The reference a message carries, if it carries one worth trusting.
pub fn reference_of(fields: &BTreeMap<String, Value>) -> Option<String> {
    // ★★ Several rules name it differently; all of them mean the same thing.
    for name in ["ref", "reference", "mpesa_ref", "external_ref"] {
        if let Some(raw) = fields.get(name).and_then(Value::as_str) {
            let cleaned: String =
                raw.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
            if cleaned.len() >= MIN_REF {
                return Some(cleaned);
            }
        }
    }
    None
}

fn amount_of(fields: &BTreeMap<String, Value>) -> Option<f64> {
    let v = fields.get("amount")?;
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.replace(',', "").parse().ok()))
}

/// The fact a message describes, when it describes one identifiably.
///
/// ★★★ `None` is the common and correct answer. A message with no reference is
/// not a message about an unknown fact — it is a message this mechanism has
/// nothing to say about, and guessing would be worse than silence.
pub fn fact_key(fields: &BTreeMap<String, Value>) -> Option<FactKey> {
    let reference = reference_of(fields)?;
    let amount = amount_of(fields)?;
    Some(FactKey { reference, amount_minor: (amount * 100.0).round() as i64 })
}

fn same_amount(a: i64, b: i64) -> bool {
    ((a - b) as f64 / 100.0).abs() <= AMOUNT_TOLERANCE
}

/// **Find messages from different senders that describe one movement.**
///
/// ★★★ Never within one source. Two identical-reference messages from the same
/// sender are the raw-text dedup's business, and it already handles them;
/// reporting them here would be a second voice on a settled question.
///
/// ★★ `candidates` is expected in arrival order, so `first` is the one already
/// in the books and `second` is the one that would double it.
pub fn find_duplicates(candidates: &[Candidate]) -> Vec<Suspected> {
    let mut seen: BTreeMap<FactKey, (&Candidate, ())> = BTreeMap::new();
    let mut out = Vec::new();

    for candidate in candidates {
        let Some(key) = fact_key(&candidate.parsed_fields) else { continue };

        // A near-amount match on the same reference counts, so the lookup is a
        // scan over the references already seen rather than an exact map hit.
        let hit = seen.iter().find(|(k, (earlier, _))| {
            k.reference == key.reference
                && same_amount(k.amount_minor, key.amount_minor)
                && earlier.source_id != candidate.source_id
        });

        if let Some((_, (earlier, _))) = hit {
            out.push(Suspected {
                why: format!(
                    "both quote reference {} for the same amount, from {} and {} — \
                     one transfer seen from two sides, or two payments that happen to match",
                    key.reference, earlier.source_id, candidate.source_id
                ),
                key: key.clone(),
                first: earlier.message_id.clone(),
                second: candidate.message_id.clone(),
                first_source: earlier.source_id.clone(),
                second_source: candidate.source_id.clone(),
            });
        }
        seen.entry(key).or_insert((candidate, ()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fields(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
    }

    fn msg(id: &str, source: &str, reference: &str, amount: f64) -> Candidate {
        Candidate {
            message_id: id.into(),
            source_id: source.into(),
            parsed_fields: fields(&[("ref", json!(reference)), ("amount", json!(amount))]),
        }
    }

    #[test]
    fn one_transfer_seen_from_two_sides_is_found() {
        // ★★★ The failure this exists for: both are captured correctly — they
        //     ARE two messages — and applying both puts forty thousand
        //     shillings into the books twice.
        let found = find_duplicates(&[
            msg("a", "kcb", "UH1B91GYNW", 40_000.0),
            msg("b", "mpesa", "UH1B91GYNW", 40_000.0),
        ]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].first, "a");
        assert_eq!(found[0].second, "b");
        assert!(found[0].why.contains("two sides"), "{}", found[0].why);
    }

    #[test]
    fn two_messages_from_one_sender_are_not_this_mechanisms_business() {
        // ★★★ The raw-text dedup already settles those, and a second voice on a
        //     settled question is how one honest answer becomes two.
        assert!(find_duplicates(&[
            msg("a", "mpesa", "UH1B91GYNW", 40_000.0),
            msg("b", "mpesa", "UH1B91GYNW", 40_000.0),
        ])
        .is_empty());
    }

    #[test]
    fn a_shared_reference_with_a_different_amount_is_two_facts() {
        // ★★ Reference codes are allocated by different systems and can
        //    collide. A reference alone is not a fact.
        assert!(find_duplicates(&[
            msg("a", "kcb", "UH1B91GYNW", 40_000.0),
            msg("b", "mpesa", "UH1B91GYNW", 1_500.0),
        ])
        .is_empty());
    }

    #[test]
    fn the_same_amount_on_different_days_is_not_one_transfer() {
        // ★★ Two payments of forty thousand are perfectly ordinary. The amount
        //    alone is not a fact either.
        assert!(find_duplicates(&[
            msg("a", "kcb", "UH1B91GYNW", 40_000.0),
            msg("b", "mpesa", "QGH7XJ4P2Q", 40_000.0),
        ])
        .is_empty());
    }

    #[test]
    fn a_message_with_no_reference_is_left_alone() {
        // ★★★ `None` is the common and correct answer. There is no text-hash
        //     fallback across sources: two descriptions of one event have
        //     different text BY DEFINITION, so a text key would be backwards.
        let bare = Candidate {
            message_id: "a".into(),
            source_id: "kcb".into(),
            parsed_fields: fields(&[("amount", json!(40_000.0))]),
        };
        let other = Candidate { message_id: "b".into(), source_id: "mpesa".into(), ..bare.clone() };
        assert!(find_duplicates(&[bare, other]).is_empty());
    }

    #[test]
    fn a_reference_too_short_to_identify_anything_is_ignored() {
        // ★★★ An M-Pesa reference is ten characters. Something four long is a
        //     fragment a regex caught, and matching on it would join two
        //     unrelated transactions with confidence.
        assert!(find_duplicates(&[
            msg("a", "kcb", "UH1B", 40_000.0),
            msg("b", "mpesa", "UH1B", 40_000.0),
        ])
        .is_empty());
    }

    #[test]
    fn a_reference_written_two_ways_is_one_reference() {
        // ★★ One side prints it spaced or lower-cased. That is a spelling, not
        //    a different transaction.
        let found = find_duplicates(&[
            msg("a", "kcb", "uh1b 91gynw", 40_000.0),
            msg("b", "mpesa", "UH1B91GYNW", 40_000.0),
        ]);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn opposite_directions_are_evidence_for_rather_than_against() {
        // ★★★ One side sent and the other received IS what a single transfer
        //     looks like from two vantage points. Requiring agreement would
        //     rule out the commonest real case.
        let mut a = msg("a", "kcb", "UH1B91GYNW", 40_000.0);
        a.parsed_fields.insert("direction".into(), json!("received"));
        let mut b = msg("b", "mpesa", "UH1B91GYNW", 40_000.0);
        b.parsed_fields.insert("direction".into(), json!("sent"));
        assert_eq!(find_duplicates(&[a, b]).len(), 1);
    }

    #[test]
    fn a_fee_sized_difference_still_counts_as_one_amount() {
        // ★★ The two sides are sometimes quoted with and without a fee. Small,
        //    though — a real difference of a few shillings is a different
        //    transaction rather than a rounding of one.
        let close = find_duplicates(&[
            msg("a", "kcb", "UH1B91GYNW", 40_000.00),
            msg("b", "mpesa", "UH1B91GYNW", 40_000.004),
        ]);
        assert_eq!(close.len(), 1);

        let apart = find_duplicates(&[
            msg("a", "kcb", "UH1B91GYNW", 40_000.0),
            msg("b", "mpesa", "UH1B91GYNW", 40_005.0),
        ]);
        assert!(apart.is_empty(), "five shillings is a different transaction");
    }

    #[test]
    fn it_names_both_messages_so_a_person_can_actually_look() {
        // ★★★ It suspects; it does not suppress. A false positive that asks
        //     costs one tap; a false positive that HIDES costs a transaction
        //     and the trust in every other number — and nobody finds out,
        //     because the point of suppression is that nothing is shown.
        let found = find_duplicates(&[
            msg("earlier", "kcb", "UH1B91GYNW", 40_000.0),
            msg("later", "mpesa", "UH1B91GYNW", 40_000.0),
        ]);
        assert_eq!(found[0].first, "earlier");
        assert_eq!(found[0].second, "later");
        assert_eq!(found[0].first_source, "kcb");
        assert_eq!(found[0].second_source, "mpesa");
    }

    #[test]
    fn three_sources_describing_one_fact_report_against_the_first() {
        let found = find_duplicates(&[
            msg("a", "kcb", "UH1B91GYNW", 40_000.0),
            msg("b", "mpesa", "UH1B91GYNW", 40_000.0),
            msg("c", "equity", "UH1B91GYNW", 40_000.0),
        ]);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|s| s.first == "a"), "{found:?}");
    }
}
