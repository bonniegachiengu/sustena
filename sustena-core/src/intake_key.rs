//! **What makes two captures the same intake** (Ingest · §V).
//!
//! ```text
//!   at-least-once ∘ idempotent = exactly-once EFFECT
//!
//!   key = intrinsic transaction code, where the message carries one
//!       = hash of the text, where it does not
//!       ≠ a generated id, ever
//! ```
//!
//! ★★★ **A generated id partitions by ARRIVAL, which is always the wrong
//! partition.** Mint a UUID per captured message and the same transaction
//! arriving twice gets two ids and applies twice — the retry the outbox exists
//! to make safe becomes the thing that doubles somebody's rent. The key has to
//! be a property of the *fact*, and a fact does not know how many times it was
//! reported.
//!
//! ★★★ **A text hash is the fallback and not the rule, because it partitions by
//! WORDING.** The same sender re-sending one transaction with a reformatted
//! date, an extra space or a changed footer produces a different digest, and the
//! transaction applies twice. That is not hypothetical: a carrier reformat is
//! exactly the kind of thing that happens once and silently.
//!
//! ★★★ **This closes an assumption `correlation.rs` was already resting on.**
//! Its `find_duplicates` deliberately never looks within one source, on the
//! grounds that *"two identical-reference messages from the same sender are the
//! raw-text dedup's business, and it already handles them"*. That was true only
//! for byte-identical re-sends. With the intrinsic key it is true generally, and
//! the deferral becomes a fact rather than an assumption.
//!
//! ## Why the intrinsic key is still scoped by source
//!
//! ★★★ **Because suppressing across sources would be the more dangerous bug**,
//! and `correlation.rs` argues that case at length: a genuinely separate
//! transaction that happens to share a reference would vanish without trace. So
//! the same fact seen through two senders still produces two captures and is
//! **surfaced** for a person, exactly as before. What changes is only the case
//! within one sender, where a repeat really is a repeat.

use std::collections::BTreeMap;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::correlation::{fact_key, FactKey};

/// What a capture is keyed on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum IntakeKey {
    /// ★★★ The message carried its own transaction code. Keyed on the **fact**,
    /// so a re-send in different words is still the same intake.
    Intrinsic { sustain: String, source: String, fact: FactKey },
    /// ★★ No usable code. Keyed on the text, which is honest about what it can
    /// and cannot see: it catches a byte-identical repeat and nothing subtler.
    TextHash { sustain: String, source: String, digest: String },
}

impl IntakeKey {
    /// Did this key come from the fact, or only from the wording?
    ///
    /// ★★ Worth asking at the call site: a `TextHash` intake is one where a
    /// reworded repeat would slip through, and a surface can say so rather than
    /// implying a guarantee it does not have.
    pub fn is_intrinsic(&self) -> bool {
        matches!(self, Self::Intrinsic { .. })
    }

    /// A stable string for a database's unique column.
    pub fn as_string(&self) -> String {
        match self {
            Self::Intrinsic { sustain, source, fact } => {
                format!("i:{sustain}:{source}:{}:{}", fact.reference, fact.amount_minor)
            }
            Self::TextHash { sustain, source, digest } => format!("t:{sustain}:{source}:{digest}"),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Intrinsic { fact, .. } => format!(
                "keyed on the transaction code {} — a re-send in different words is the same \
                 intake",
                fact.reference
            ),
            Self::TextHash { .. } => "keyed on the text — only a byte-identical repeat is caught"
                .into(),
        }
    }
}

/// **The key for one capture.**
///
/// ★★★ Intrinsic first, text second, and there is no third option. The
/// signature takes no id parameter, so a caller cannot pass a freshly minted one
/// even by mistake — the refusal is structural rather than a rule in a comment.
///
/// ★★ The fact includes the **amount**, not the reference alone. That is
/// `correlation.rs`'s own finding, reused rather than re-derived: a reference
/// collision that swallowed a real transaction is the failure that put the
/// amount in the key in the first place.
pub fn key_for(
    sustain_id: &str,
    source_id: &str,
    parsed_fields: &BTreeMap<String, Value>,
    raw_text: &str,
) -> IntakeKey {
    match fact_key(parsed_fields) {
        Some(fact) => {
            IntakeKey::Intrinsic { sustain: sustain_id.into(), source: source_id.into(), fact }
        }
        None => IntakeKey::TextHash {
            sustain: sustain_id.into(),
            source: source_id.into(),
            digest: digest_of(raw_text),
        },
    }
}

fn digest_of(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    format!("{:x}", h.finalize())
}

/// Would two captures be treated as one intake?
pub fn same_intake(a: &IntakeKey, b: &IntakeKey) -> bool {
    a == b
}

/// Captures that a text-only key would have let through twice.
///
/// ★★★ The value of the row, made countable on real data. If this is empty for
/// a household, intrinsic keying bought nothing there — which is a real answer
/// and better than assuming it earned its keep.
pub fn caught_only_by_the_intrinsic_key(keys: &[(String, IntakeKey)]) -> Vec<(String, String)> {
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    let mut out = Vec::new();
    for (id, key) in keys {
        if !key.is_intrinsic() {
            continue;
        }
        let k = key.as_string();
        match seen.get(&k) {
            Some(first) => out.push((first.to_string(), id.clone())),
            None => {
                seen.insert(k, id);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fields(reference: &str, amount: f64) -> BTreeMap<String, Value> {
        [("ref".to_string(), json!(reference)), ("amount".to_string(), json!(amount))]
            .into_iter()
            .collect()
    }

    const AS_SENT: &str =
        "QGH7XJ2K9L Confirmed. Ksh5,000.00 sent to MARY WANJIRU on 20/7/26 at 5:00 PM";
    /// The same transaction, re-sent with a reformatted date and a changed
    /// footer. A real thing carriers do.
    const RESENT: &str =
        "QGH7XJ2K9L Confirmed.  Ksh5,000.00 sent to MARY WANJIRU on 20/07/2026 at 17:00.";

    #[test]
    fn a_reworded_resend_of_one_transaction_is_one_intake() {
        // ★★★ The gap the row names. A text hash partitions by wording, so a
        //     reformatted date applies the transaction a second time.
        let f = fields("QGH7XJ2K9L", 5000.0);
        let first = key_for("household", "mpesa", &f, AS_SENT);
        let again = key_for("household", "mpesa", &f, RESENT);
        assert!(same_intake(&first, &again));
        assert!(first.is_intrinsic());
    }

    #[test]
    fn a_text_hash_would_genuinely_have_let_it_through() {
        // ★★ Worth asserting rather than claiming: if the two texts hashed the
        //    same, this whole row would be buying nothing.
        assert_ne!(digest_of(AS_SENT), digest_of(RESENT));
    }

    #[test]
    fn a_message_with_no_usable_code_falls_back_and_says_so() {
        // ★★ Honest about what it can and cannot see, rather than implying a
        //    guarantee it does not have.
        let bare: BTreeMap<String, Value> = BTreeMap::new();
        let k = key_for("household", "mpesa", &bare, AS_SENT);
        assert!(!k.is_intrinsic());
        assert!(k.describe().contains("only a byte-identical repeat"));
    }

    #[test]
    fn a_byte_identical_repeat_is_still_caught_by_the_fallback() {
        let bare: BTreeMap<String, Value> = BTreeMap::new();
        assert!(same_intake(
            &key_for("h", "mpesa", &bare, AS_SENT),
            &key_for("h", "mpesa", &bare, AS_SENT)
        ));
    }

    #[test]
    fn there_is_no_parameter_through_which_a_generated_id_could_arrive() {
        // ★★★ The refusal is structural. `key_for` takes a sustain, a source,
        //     the parsed fields and the raw text — a caller cannot pass a
        //     freshly minted UUID even by mistake, because there is nowhere to
        //     put one.
        let f = fields("QGH7XJ2K9L", 5000.0);
        let a = key_for("h", "mpesa", &f, AS_SENT);
        let b = key_for("h", "mpesa", &f, AS_SENT);
        assert_eq!(a, b, "two arrivals of one fact key identically");
    }

    #[test]
    fn the_same_fact_through_two_senders_is_still_two_intakes() {
        // ★★★ Deliberate, and correlation.rs argues the case: suppressing here
        //     would silently drop a genuinely separate transaction that happened
        //     to share a reference. Two captures, surfaced for a person.
        let f = fields("UH1B91GYNW", 40000.0);
        let via_mpesa = key_for("household", "mpesa", &f, "one wording");
        let via_kcb = key_for("household", "kcb", &f, "another wording");
        assert!(!same_intake(&via_mpesa, &via_kcb));
    }

    #[test]
    fn two_households_never_share_an_intake() {
        let f = fields("QGH7XJ2K9L", 5000.0);
        assert!(!same_intake(
            &key_for("bonnie", "mpesa", &f, AS_SENT),
            &key_for("aida", "mpesa", &f, AS_SENT)
        ));
    }

    #[test]
    fn the_amount_is_part_of_the_key_because_a_reference_collision_once_ate_a_transaction() {
        // ★★ correlation.rs's own finding, reused rather than re-derived.
        let five = fields("QGH7XJ2K9L", 5000.0);
        let six = fields("QGH7XJ2K9L", 6000.0);
        assert!(!same_intake(
            &key_for("h", "mpesa", &five, AS_SENT),
            &key_for("h", "mpesa", &six, AS_SENT)
        ));
    }

    #[test]
    fn a_reference_too_short_to_be_a_code_is_not_treated_as_one() {
        // ★★ Delegated to correlation.rs's own floor, so intake and correlation
        //    cannot disagree about what counts as a reference.
        let flimsy = fields("12", 5000.0);
        assert!(!key_for("h", "mpesa", &flimsy, AS_SENT).is_intrinsic());
    }

    #[test]
    fn what_the_intrinsic_key_actually_caught_is_countable() {
        // ★★★ If this is empty on a household, intrinsic keying bought nothing
        //     there — a real answer, and better than assuming it earned its keep.
        let f = fields("QGH7XJ2K9L", 5000.0);
        let keys = vec![
            ("m1".to_string(), key_for("h", "mpesa", &f, AS_SENT)),
            ("m2".to_string(), key_for("h", "mpesa", &f, RESENT)),
            ("m3".to_string(), key_for("h", "mpesa", &fields("ZZ11YY22XX", 90.0), "other")),
        ];
        assert_eq!(caught_only_by_the_intrinsic_key(&keys), vec![("m1".to_string(), "m2".to_string())]);
    }

    #[test]
    fn a_household_where_it_bought_nothing_reports_nothing() {
        let keys = vec![("m1".to_string(), key_for("h", "mpesa", &fields("QGH7XJ2K9L", 5.0), "a"))];
        assert!(caught_only_by_the_intrinsic_key(&keys).is_empty());
    }

    #[test]
    fn the_key_is_a_stable_string_a_unique_column_can_hold() {
        let f = fields("QGH7XJ2K9L", 5000.0);
        let k = key_for("h", "mpesa", &f, AS_SENT);
        assert_eq!(k.as_string(), "i:h:mpesa:QGH7XJ2K9L:500000");
        assert_eq!(k.as_string(), key_for("h", "mpesa", &f, RESENT).as_string());
    }
}
