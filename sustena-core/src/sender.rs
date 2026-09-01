//! **Which thread a text came from** — one decision, made in one place.
//!
//! ★★★ **Source is decided by SENDER, never by wording.** That rule is older
//! than this module and was learned the hard way: a KCB message can say
//! "M-PESA" three times in its own sentence, and a parser set chosen by content
//! would read it with the wrong bank's rules. The sender is who sent it. The
//! body is what they said. Only the first answers "whose format is this?".
//!
//! ★★★ **And it must be user-extensible, or it is a list of two banks.** M-Pesa
//! and KCB were hard-coded, so a household on Equity, or a sacco, or any of the
//! dozens of Kenyan services that send patterned money texts, had no way in at
//! all — not "worse support", none. A person can now declare a thread, and from
//! that moment its texts reach the same train page every other shape does.
//!
//! ★★ **Declared threads win over the built-ins.** A household's own statement
//! about its own senders is more specific than anything shipped, and shipping
//! the two originals as unremovable special cases would make them a different
//! KIND of thing from the ones he adds. They are the same kind; they are just
//! already there.
//!
//! ★ **This is mirrored in Java**, because the Android receiver decides whether
//! a text is even looked at, and it runs where no Rust does. Two copies of one
//! rule is a real cost, paid deliberately and noted in both — see
//! `SmsSenderFilter.java`.

use serde::{Deserialize, Serialize};

/// A thread a household reads money texts from.
///
/// ★★ `senders` are matched case-insensitively as SUBSTRINGS of the sender the
/// network reports, because that field is not standardised: the same bank
/// arrives as `KCB`, `KCBBANK` or a short code depending on the route. A
/// substring is the honest amount of precision to claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    /// The `source_id` captures are tagged with, e.g. `equity`.
    pub id: String,
    /// What a person calls it.
    pub label: String,
    /// Sender strings that belong to this thread.
    pub senders: Vec<String>,
}

/// The two that shipped, as ordinary threads rather than special cases.
///
/// ★★ Kept here rather than in the matcher so they can be SHOWN to a person
/// alongside the ones he adds — a list where two entries are invisible and
/// unexplained is a list nobody trusts.
pub fn built_in_threads() -> Vec<Thread> {
    vec![
        // ★★★ KCB FIRST, and it is not cosmetic. Real KCB messages arrive from
        //     a sender carrying BOTH names -- "KCB-MPESA" -- and they are the
        //     bank's. Reading one with M-Pesa's rules would parse a bank text
        //     with the wrong format set, which is how a direction gets
        //     inverted. A test pins it.
        Thread { id: "kcb".into(), label: "KCB".into(), senders: vec!["KCB".into()] },
        // ★★★ `MPESA` only, and NOT the hyphenated `M-PESA`. The hyphen form
        //     appears in message BODIES constantly and in the sender id not at
        //     all, so claiming it here buys nothing and makes a body look like
        //     a sender -- which an existing guard test caught the moment it was
        //     added speculatively. Only what is verified goes in this list.
        Thread { id: "mpesa".into(), label: "M-Pesa".into(), senders: vec!["MPESA".into()] },
    ]
}

/// **Which thread a sender belongs to, if any.**
///
/// ★★★ Returning `None` is the safety property, not a shortfall. A text from a
/// sender no thread claims is never captured, never forwarded and never stored
/// — which is what keeps an SMS reader to the money texts a household asked it
/// to read, and out of everything else on the phone.
///
/// ★★★ **First match in order wins, and the order is the meaning.** An earlier
/// draft took the LONGEST match instead, which reads as more precise and is
/// wrong: `KCB-MPESA` is a real KCB sender, and "MPESA" is the longer needle,
/// so a bank's text would have been read with M-Pesa's rules. An existing test
/// caught it. Precedence is explicit rather than emergent:
///
///   1. what a household DECLARED, in the order it declared it -- its own
///      statement about its own senders is the most specific thing there is
///   2. the shipped threads, KCB before M-Pesa for the reason above
///
/// `all_threads` builds exactly that order, so this only has to walk it.
pub fn thread_for_sender<'a>(sender: &str, ordered: &'a [Thread]) -> Option<&'a Thread> {
    let upper = sender.to_ascii_uppercase();
    ordered.iter().find(|t| {
        t.senders.iter().any(|s| {
            let needle = s.trim().to_ascii_uppercase();
            !needle.is_empty() && upper.contains(&needle)
        })
    })
}

/// Every thread this node reads: the household's own, then the built-ins it
/// has not overridden.
///
/// ★★ An id a person has declared REPLACES the built-in of that id rather than
/// sitting beside it, so "add M-Pesa's other short code" is a thing he can do
/// without ending up with two M-Pesas that disagree.
pub fn all_threads(declared: &[Thread]) -> Vec<Thread> {
    let mut out = declared.to_vec();
    for b in built_in_threads() {
        if !out.iter().any(|t| t.id.eq_ignore_ascii_case(&b.id)) {
            out.push(b);
        }
    }
    out
}

/// Is this a sender any thread reads?
pub fn is_known_sender(sender: &str, declared: &[Thread]) -> bool {
    thread_for_sender(sender, &all_threads(declared)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn equity() -> Thread {
        Thread {
            id: "equity".into(),
            label: "Equity".into(),
            senders: vec!["EQUITY".into(), "EQUITYBK".into()],
        }
    }

    #[test]
    fn the_shipped_two_still_answer_with_nothing_declared() {
        let all = all_threads(&[]);
        assert_eq!(thread_for_sender("MPESA", &all).map(|t| t.id.as_str()), Some("mpesa"));
        assert_eq!(thread_for_sender("KCB", &all).map(|t| t.id.as_str()), Some("kcb"));
    }

    #[test]
    fn a_declared_thread_is_read_like_any_other() {
        // ★★★ The whole point. Before this there was no way in at all for a
        //     household that banks anywhere else.
        let all = all_threads(&[equity()]);
        assert_eq!(thread_for_sender("EQUITY BANK", &all).map(|t| t.id.as_str()), Some("equity"));
    }

    #[test]
    fn a_sender_nobody_claims_is_not_read() {
        // ★★★ The safety property. This is what keeps the reader to the money
        //     texts he asked for and out of everything else on his phone.
        assert!(thread_for_sender("MUM", &all_threads(&[equity()])).is_none());
        assert!(!is_known_sender("Safaricom Promotions Ltd", &[]));
    }

    #[test]
    fn case_and_punctuation_in_the_sender_do_not_matter() {
        let all = all_threads(&[]);
        assert_eq!(thread_for_sender("mpesa", &all).map(|t| t.id.as_str()), Some("mpesa"));
        assert_eq!(thread_for_sender("kcbbank", &all).map(|t| t.id.as_str()), Some("kcb"));
        // ★ And a household whose route spells it with a hyphen declares that
        //   itself, rather than this guessing on everyone's behalf.
    }

    #[test]
    fn a_sender_carrying_both_names_is_the_bank() {
        // ★★★ Real KCB messages arrive as "KCB-MPESA". Reading one with
        //     M-Pesa's rules would parse a bank text with the wrong format
        //     set, which is how a direction gets inverted. This is why the
        //     order is explicit and not a longest-match.
        let all = all_threads(&[]);
        assert_eq!(thread_for_sender("KCB-MPESA", &all).map(|t| t.id.as_str()), Some("kcb"));
        assert_eq!(thread_for_sender("MPESA-KCB", &all).map(|t| t.id.as_str()), Some("kcb"));
    }

    #[test]
    fn the_more_specific_declaration_wins() {
        // ★★★ A household declaring KCBBANK must not be swallowed by the
        //     built-in KCB. Otherwise an ordering accident decides whose rules
        //     read his money.
        let mine = Thread {
            id: "kcb_business".into(),
            label: "KCB business".into(),
            senders: vec!["KCBBANK".into()],
        };
        let all = all_threads(&[mine]);
        assert_eq!(
            thread_for_sender("KCBBANK", &all).map(|t| t.id.as_str()),
            Some("kcb_business")
        );
        // and the plain one still goes where it always did
        assert_eq!(thread_for_sender("KCB", &all).map(|t| t.id.as_str()), Some("kcb"));
    }

    #[test]
    fn declaring_an_id_replaces_its_built_in() {
        // ★★ So "add M-Pesa's other short code" cannot leave two M-Pesas that
        //    disagree about which senders are M-Pesa.
        let mine = Thread {
            id: "mpesa".into(),
            label: "M-Pesa".into(),
            senders: vec!["MPESA".into(), "SAFARICOM".into()],
        };
        let all = all_threads(&[mine]);
        assert_eq!(all.iter().filter(|t| t.id == "mpesa").count(), 1);
        assert_eq!(thread_for_sender("SAFARICOM", &all).map(|t| t.id.as_str()), Some("mpesa"));
    }

    #[test]
    fn a_blank_sender_entry_claims_nothing() {
        // ★ Otherwise an empty string would substring-match everything, and one
        //   careless declaration would capture the whole inbox.
        let sloppy =
            Thread { id: "x".into(), label: "X".into(), senders: vec!["".into(), "  ".into()] };
        assert!(thread_for_sender("ANYONE AT ALL", &[sloppy]).is_none());
    }
}
