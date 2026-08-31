//! **What the whole inbox has in common** — recurring shapes, found without
//! being told.
//!
//! ★★★ **The honest limit, which is also the design.** A message's SHAPE can be
//! induced from the corpus; its MEANING cannot. Nothing here can know whether
//! an unrecognised text is money arriving, money leaving, or a bank telling him
//! his card expires — that is what the person knows and the machine does not.
//! So this does not guess an operator, and it does not invent a rule. It finds
//! the shapes that recur, counts them, and says: *twelve messages look like
//! this one; classify one and the other eleven follow.*
//!
//! That turns one correction into twelve without a single guess about meaning,
//! which is the whole of the leverage and the whole of the safety.
//!
//! ★★ **Deterministic, no model, no `eval`.** The same discipline as
//! `predicates.rs` and `transducer.rs`: this is normalisation and counting.
//! There is no scoring function to tune and nothing to be confidently wrong
//! about — a cluster is either a set of texts sharing a skeleton or it is not.
//!
//! ★ **It reads only what already crossed the intake boundary.** No new
//! capture, no new source, no network. The corpus is the messages a household
//! already holds.

use std::collections::BTreeMap;

/// A recurring message shape and the messages that share it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeCluster {
    /// The normalised skeleton these messages share.
    pub skeleton: String,
    /// Ids of every message in the cluster, in the order given.
    pub members: Vec<String>,
    /// One real message, so a person can see what they are teaching.
    pub example: String,
}

impl ShapeCluster {
    pub fn count(&self) -> usize {
        self.members.len()
    }
}

/// Reduce a message to the shape underneath its particulars.
///
/// ★★★ **What is replaced is exactly what varies between two texts of the same
/// kind**: the amount, the date, the time, the reference, the counterparty's
/// digits. What is KEPT is the wording, which is what a bank holds constant and
/// therefore what identifies the format.
///
/// ★★ Deliberately crude. A cleverer normaliser collapses genuinely different
/// formats into one skeleton, and a cluster that mixes two kinds of message
/// teaches the wrong rule for half of them. When in doubt this splits rather
/// than merges — two clusters of six is a worse offer than one of twelve, and a
/// far better mistake.
pub fn skeleton(text: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for raw in text.split_whitespace() {
        let core = raw.trim_matches(|c: char| !c.is_alphanumeric());
        if core.is_empty() {
            continue;
        }
        let tag = if is_amount(core) {
            "<num>"
        } else if is_reference(core) {
            "<ref>"
        } else if looks_like_date(core) {
            "<date>"
        } else if looks_like_time(core) {
            "<time>"
        } else if is_name_word(core) {
            "<name>"
        } else if core.chars().any(|c| c.is_ascii_digit()) {
            "<id>"
        } else {
            // Wording, kept as written -- it is what identifies the format.
            parts.push(core.to_string());
            continue;
        };
        // ★★ Consecutive names collapse into ONE. "NAIVAS SUPERMARKET" and
        //    "CARREFOUR KAREN" are both simply the counterparty, and a
        //    skeleton that counted its words would split one format by how
        //    many words a shop happens to have in its name.
        if tag == "<name>" && parts.last().map(String::as_str) == Some("<name>") {
            continue;
        }
        parts.push(tag.to_string());
    }
    parts.join(" ")
}

/// A money figure, with or without its currency prefix.
fn is_amount(core: &str) -> bool {
    let digits = core.trim_start_matches(|c: char| c.is_ascii_alphabetic());
    let prefix = &core[..core.len() - digits.len()];
    let prefix_ok = prefix.is_empty()
        || matches!(prefix.to_ascii_uppercase().as_str(), "KSH" | "KES" | "USD");
    prefix_ok
        && !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '.')
        && digits.chars().any(|c| c.is_ascii_digit())
}

/// A word that is part of somebody's NAME rather than the bank's wording.
///
/// ★★★ Case is the signal, and it is a real one: banks write their own
/// sentences in ordinary case and shout the counterparty back at you in caps.
/// So "sent to" is wording and "NAIVAS" is a name, without anyone declaring a
/// list of shops.
///
/// ★★ The exceptions are the caps words that belong to the WORDING -- the
/// clock markers and anything hyphenated like M-PESA, which is the sender
/// naming itself rather than naming a party.
fn is_name_word(core: &str) -> bool {
    let upper = core.to_ascii_uppercase();
    core.len() >= 3
        && core.chars().all(|c| c.is_ascii_alphabetic())
        && core == upper
        && !matches!(upper.as_str(), "AM" | "PM" | "NEW")
}

/// A transaction reference: long, all caps, letters and digits mixed.
fn is_reference(core: &str) -> bool {
    core.len() >= 8
        && core.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && core.chars().any(|c| c.is_ascii_alphabetic())
        && core.chars().any(|c| c.is_ascii_digit())
}

fn looks_like_date(core: &str) -> bool {
    let parts: Vec<&str> = core.split(['/', '-']).collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn looks_like_time(core: &str) -> bool {
    let head = core.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let parts: Vec<&str> = head.split(':').collect();
    parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Group messages by the shape they share.
///
/// ★★ Returned largest-first, because the biggest cluster is the most work one
/// answer can save — which is the only ranking this needs and the only one it
/// can defend.
///
/// ★ Singletons are dropped. A shape seen once is not a recurring format, and
/// offering it would turn "teach me your bank's formats" back into "classify
/// every message", which is the thing being fixed.
pub fn cluster<'a>(messages: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<ShapeCluster> {
    let mut groups: BTreeMap<String, (Vec<String>, String)> = BTreeMap::new();
    for (id, raw) in messages {
        let key = skeleton(raw);
        if key.is_empty() {
            continue;
        }
        let entry = groups.entry(key).or_insert_with(|| (Vec::new(), raw.to_string()));
        entry.0.push(id.to_string());
    }

    let mut out: Vec<ShapeCluster> = groups
        .into_iter()
        .filter(|(_, (members, _))| members.len() > 1)
        .map(|(skeleton, (members, example))| ShapeCluster { skeleton, members, example })
        .collect();

    // Largest first; ties by skeleton so the order is stable across runs.
    out.sort_by(|a, b| b.members.len().cmp(&a.members.len()).then(a.skeleton.cmp(&b.skeleton)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "TFF9J0ABCD Confirmed. Ksh1,500.00 sent to NAIVAS SUPERMARKET on 1/8/26 at \
                     11:15 AM. New M-PESA balance is Ksh12,154.47.";
    const B: &str = "QKL2M8XYZW Confirmed. Ksh320.00 sent to CARREFOUR KAREN on 3/8/26 at \
                     9:02 AM. New M-PESA balance is Ksh8,900.00.";
    const C: &str = "You have joined Fuliza M-PESA with a limit of Ksh1,000.00.";

    #[test]
    fn two_texts_of_the_same_kind_share_a_skeleton() {
        // ★★★ The whole mechanism. Different amount, different shop, different
        //     day, different reference -- one format.
        assert_eq!(skeleton(A), skeleton(B));
    }

    #[test]
    fn a_different_kind_of_message_does_not() {
        assert_ne!(skeleton(A), skeleton(C));
    }

    #[test]
    fn the_wording_survives_and_the_particulars_do_not() {
        let s = skeleton(A);
        assert!(s.contains("Confirmed"), "wording is kept as written: {s}");
        assert!(s.contains("<num>"), "amounts are generalised: {s}");
        assert!(s.contains("<ref>"), "references are generalised: {s}");
        assert!(s.contains("<date>"), "dates are generalised: {s}");
        assert!(!s.contains("NAIVAS"), "the counterparty must not pin the shape: {s}");
        assert!(s.contains("<name>"), "it is generalised instead: {s}");
        assert!(!s.contains("1,500.00"), "nor the amount: {s}");
    }

    #[test]
    fn recurring_shapes_come_back_largest_first() {
        let got = cluster(vec![("1", A), ("2", B), ("3", C), ("4", C), ("5", C)]);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].count(), 3, "the commonest shape leads: it saves the most work");
        assert_eq!(got[1].members, vec!["1", "2"]);
    }

    #[test]
    fn a_shape_seen_once_is_not_an_offer() {
        // ★★ Offering singletons would turn "teach me your formats" back into
        //    "classify everything", which is the thing this exists to end.
        assert!(cluster(vec![("1", A), ("2", C)]).is_empty());
    }

    #[test]
    fn a_cluster_carries_a_real_message_to_show() {
        // ★ A person is being asked to teach ONE message. They have to be able
        //   to read it, so the offer carries the text and not just the skeleton.
        let got = cluster(vec![("1", A), ("2", B)]);
        assert_eq!(got[0].example, A);
        assert_eq!(got[0].members, vec!["1", "2"]);
    }

    #[test]
    fn clustering_is_stable_however_the_corpus_arrives() {
        // ★★ Same corpus, different order, same answer -- otherwise the offer
        //    a household sees would shuffle between two refreshes.
        let one = cluster(vec![("1", A), ("2", B), ("3", C), ("4", C)]);
        let two = cluster(vec![("4", C), ("2", B), ("3", C), ("1", A)]);
        let shapes = |v: &[ShapeCluster]| v.iter().map(|c| c.skeleton.clone()).collect::<Vec<_>>();
        assert_eq!(shapes(&one), shapes(&two));
    }

    #[test]
    fn an_empty_or_blank_message_contributes_nothing() {
        assert!(cluster(vec![("1", ""), ("2", "   ")]).is_empty());
    }
}
