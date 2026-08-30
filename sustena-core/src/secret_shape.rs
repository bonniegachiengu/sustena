//! **A second detector that fails differently** (Immunity · §III).
//!
//! ★★★ **Three layers checking a message against the same ten regexes are one
//! layer written three times.** The Android filter, the ingest engine and the
//! transducer each refuse a message carrying a one-time secret, and that reads
//! as defence in depth until you ask what happens when the vocabulary misses:
//! all three miss, together, for the same reason. Layering buys **independence
//! of failure**, not a count — and identical detectors have none.
//!
//! ★★★ **So this one does not read the words at all.** It looks at the *shape*
//! of the message: is there a short bare code, is there a transaction reference,
//! is anything shouted, does any figure carry cents. A bank inventing new
//! phrasing, or writing in Swahili, defeats the vocabulary completely and leaves
//! the shape untouched — which is the only sense in which a second layer is a
//! second layer.
//!
//! ## What this is not
//!
//! ★★★ **It is a heuristic with a declared threshold, and it is worse than the
//! vocabulary at the cases the vocabulary knows.** That is fine and it is the
//! point: two detectors that are each other's blind spot beat two that are each
//! other's copy. Both are consulted, and **either one refusing is a refusal** —
//! because a false positive costs one capture somebody re-enters, and a false
//! negative puts a live credential in a database.
//!
//! ★★ **Calibrated against the real corpus, not against intuition.** Every
//! threshold here was set by running the real M-Pesa and KCB messages this
//! codebase holds and looking at where the scores actually fell — the real OTP
//! sample *contains a currency amount*, so the obvious "no money mentioned" rule
//! would have missed it outright. The tests keep the whole corpus as a standing
//! negative control.

/// A shape reading of one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shape {
    /// A standalone 4–8 character token of mostly digits that is not a date, a
    /// time, or an amount. **Necessary**: a message with no code in it cannot be
    /// a message delivering a code.
    pub bare_code: bool,
    /// No transaction reference anywhere. Real confirmations carry one; a
    /// credential rarely does.
    pub no_reference: bool,
    /// Short. Confirmations narrate; a code is handed over.
    pub brief: bool,
    /// A run of shouted words. Structural emphasis — what is being emphasised is
    /// not read.
    pub shouted: bool,
    /// No figure carrying cents. Money is quoted to two places; a code is not a
    /// quantity.
    pub no_cents: bool,
}

impl Shape {
    /// How many corroborating features fired, out of four.
    pub fn corroboration(&self) -> usize {
        [self.no_reference, self.brief, self.shouted, self.no_cents]
            .iter()
            .filter(|f| **f)
            .count()
    }

    pub fn describe(&self) -> String {
        let mut said = Vec::new();
        if self.bare_code {
            said.push("a short bare code");
        }
        if self.no_reference {
            said.push("no transaction reference");
        }
        if self.brief {
            said.push("very short");
        }
        if self.shouted {
            said.push("a shouted clause");
        }
        if self.no_cents {
            said.push("no figure quoted in cents");
        }
        if said.is_empty() {
            return "nothing about its shape stands out".into();
        }
        said.join(", ")
    }
}

/// Words above which a message stops being brief.
///
/// ★★ Set from the corpus: the real credential messages run to about ten words
/// and the real confirmations to twenty-something, so the line sits above both
/// credentials and below the longer notices rather than splitting the difference.
pub const BRIEF_WORDS: usize = 25;

/// Corroborating features required alongside a bare code.
///
/// ★★★ Three of four. At two the real paybill confirmation — which genuinely
/// contains a bare eight-digit account number — starts to fire, and a detector
/// that refuses real payments is one somebody turns off.
pub const REQUIRED_CORROBORATION: usize = 3;

fn is_digits(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

/// Split on whitespace and strip the punctuation a sentence leaves behind.
fn tokens(text: &str) -> Vec<&str> {
    text.split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| !t.is_empty())
        .collect()
}

/// Is this token a date or a clock time rather than a code?
///
/// ★★ Checked on the *raw* token, before punctuation is stripped, because the
/// separators are the whole signal — `20/7/26` and `2:15` are only telling
/// apart from a code by the slashes and the colon.
fn is_date_or_time(raw: &str) -> bool {
    let sep = raw.chars().filter(|c| *c == '/' || *c == ':' || *c == '-').count();
    sep > 0 && raw.chars().all(|c| c.is_ascii_digit() || c == '/' || c == ':' || c == '-')
}

/// A reference: a token mixing letters and digits, long enough not to be a word
/// with a number stuck on it.
fn is_reference(t: &str) -> bool {
    t.len() >= 8
        && t.chars().all(|c| c.is_ascii_alphanumeric())
        && t.chars().any(|c| c.is_ascii_digit())
        && t.chars().any(|c| c.is_ascii_alphabetic())
}

/// **Read the shape of a message, without reading the message.**
pub fn shape_of(text: &str) -> Shape {
    let raw: Vec<&str> = text.split_whitespace().collect();
    let toks = tokens(text);

    let bare_code = raw.iter().any(|r| {
        if is_date_or_time(r) {
            return false;
        }
        // ★★ Trim the sentence's own punctuation FIRST. A full stop at the end
        //    of `000000.` is a sentence ending, not a decimal point — and
        //    reading it as one made the detector miss a real TAN code, which is
        //    how this line came to be written.
        let t = r.trim_matches(|c: char| !c.is_alphanumeric());
        // An amount is disqualified by a separator sitting BETWEEN digits.
        if has_inner_separator(t) {
            return false;
        }
        (4..=8).contains(&t.len()) && is_digits(t)
    });

    let no_reference = !toks.iter().any(|t| is_reference(t));
    let brief = toks.len() <= BRIEF_WORDS;

    // Three consecutive shouted words. Two is a name.
    let shouted = toks
        .windows(3)
        .any(|w| w.iter().all(|t| t.len() > 1 && t.chars().all(|c| c.is_ascii_uppercase())));

    let no_cents = !has_cents(text);

    Shape { bare_code, no_reference, brief, shouted, no_cents }
}

/// A separator between two digits — the mark of an amount rather than a code.
fn has_inner_separator(t: &str) -> bool {
    let b: Vec<char> = t.chars().collect();
    (1..b.len().saturating_sub(1)).any(|i| {
        (b[i] == '.' || b[i] == ',') && b[i - 1].is_ascii_digit() && b[i + 1].is_ascii_digit()
    })
}

/// Does any figure carry two decimal places?
fn has_cents(text: &str) -> bool {
    let b: Vec<char> = text.chars().collect();
    for i in 0..b.len() {
        if b[i] != '.' {
            continue;
        }
        let before = i > 0 && b[i - 1].is_ascii_digit();
        let two = i + 2 < b.len() && b[i + 1].is_ascii_digit() && b[i + 2].is_ascii_digit();
        let no_third = i + 3 >= b.len() || !b[i + 3].is_ascii_digit();
        if before && two && no_third {
            return true;
        }
    }
    false
}

/// **Does this look like a message delivering a credential?**
///
/// ★★★ A bare code is necessary and not sufficient — a paybill confirmation has
/// an account number in it. Three of the four corroborating features must fire
/// alongside it.
pub fn looks_like_a_secret(text: &str) -> bool {
    let s = shape_of(text);
    s.bare_code && s.corroboration() >= REQUIRED_CORROBORATION
}

/// Which layer caught it — the point of having two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caught {
    /// Neither. The message may be stored.
    Nothing,
    /// The vocabulary knew the phrasing.
    ByVocabulary,
    /// ★★★ The wording was novel and the shape was not. The case that only
    /// exists because the layers are different kinds.
    ByShape { because: String },
    /// Both, which is the ordinary case and proves nothing on its own.
    ByBoth,
}

impl Caught {
    pub fn refuses(&self) -> bool {
        !matches!(self, Self::Nothing)
    }
}

/// **Consult both detectors.**
///
/// ★★★ `OR`, not `AND`. The costs are not symmetric: a false positive is one
/// capture a person re-enters, and a false negative is a live credential in a
/// database. An `AND` would make each layer able to veto the other's catch,
/// which is the opposite of layering.
pub fn guard(text: &str, vocabulary_says: bool) -> Caught {
    let shape_says = looks_like_a_secret(text);
    match (vocabulary_says, shape_says) {
        (false, false) => Caught::Nothing,
        (true, false) => Caught::ByVocabulary,
        (false, true) => Caught::ByShape { because: shape_of(text).describe() },
        (true, true) => Caught::ByBoth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transducer::contains_sensitive_secret;

    // ── the real corpus, verbatim from the shipped fixtures ─────────────────
    // Codes replaced with placeholders where the originals were live-ish, the
    // same convention the Python fixtures use.

    const OTP: &str = "Your card ending with 0319 has initiated an online transaction of \
                       USD 113.8 at ANTHROPIC. Your OTP is 083345. DO NOT SHARE WITH ANYONE.";
    const TAN: &str = "Your tan code is 000000. It will be active for the next 02:00 minutes";
    const ACTIVATION: &str =
        "Please use Activation code: 0000. This code is valid for only 90s";
    const CARD_PIN: &str = "Your KCB card secret PIN is 0000.";

    const RECEIVED: &str = "QGH7XJ2K9L Confirmed. You have received Ksh5,000.00 from JOHN \
                            KAMAU 254712345678 on 20/7/26 at 2:15 PM. New M-PESA balance is \
                            Ksh15,000.00";
    const PAYBILL: &str = "QGH7XJ3M1N Confirmed. Ksh2,500.00 paid to KPLC PREPAID for account \
                           12345678 on 20/7/26 at 3:00 PM. New M-PESA balance is Ksh12,500.00";
    const BUYGOODS: &str = "QGH7XJ4P2Q Confirmed. Ksh450.00 paid to NAIVAS SUPERMARKET on \
                            20/7/26 at 4:30 PM. New M-PESA balance is Ksh12,050.00";
    const WITHDRAW: &str = "QGH7XJ6T4U Confirmed. Ksh3,000.00 withdrawn from Agent 123456 - \
                            TOWN SHOP on 20/7/26 at 6:00 PM. New M-PESA balance is Ksh8,050.00";
    const KCB_BALANCE: &str = "Dear MARY, Actual balance KES 6,372.36. Available balance KES \
                               13,382.60. Transaction reference number CHSEC6BDHU";
    const KCB_OVERDUE: &str = "your KCB Mobile Loan repayment of KES 14522.94 is overdue since \
                               31/07/2025 please pay to avoid penalties.";
    const KCB_DEFAULT: &str = "Your loan number 173389621 has a balance of KES 14515.23 which \
                               is in DEFAULT please contact us.";
    const SYSTEM_BUSY: &str = "M-PESA is unable to process your request because a similar \
                               transaction is currently underway. Please wait while we \
                               complete your initial request.";

    fn real_transactions() -> Vec<(&'static str, &'static str)> {
        vec![
            ("received", RECEIVED),
            ("paybill", PAYBILL),
            ("buygoods", BUYGOODS),
            ("withdraw", WITHDRAW),
            ("kcb balance", KCB_BALANCE),
            ("kcb overdue", KCB_OVERDUE),
            ("kcb default", KCB_DEFAULT),
            ("system busy", SYSTEM_BUSY),
        ]
    }

    fn real_secrets() -> Vec<(&'static str, &'static str)> {
        vec![("otp", OTP), ("tan", TAN), ("activation", ACTIVATION), ("card pin", CARD_PIN)]
    }

    #[test]
    fn every_real_credential_shape_is_caught_without_reading_a_word_of_it() {
        for (name, text) in real_secrets() {
            assert!(looks_like_a_secret(text), "{name} slipped past the shape detector");
        }
    }

    #[test]
    fn no_real_transaction_is_refused_by_it() {
        // ★★★ The standing negative control. A detector that refuses real
        //     payments is one somebody turns off, and then there is no second
        //     layer at all.
        for (name, text) in real_transactions() {
            assert!(!looks_like_a_secret(text), "{name} would have been refused: {:?}", shape_of(text));
        }
    }

    #[test]
    fn novel_wording_defeats_the_vocabulary_and_not_the_shape() {
        // ★★★ The whole row, in one test. This is a credential message in
        //     Swahili: the ten regexes have nothing to match, and the shape is
        //     unchanged. Two layers that fail for different reasons — which is
        //     what "layered" was supposed to mean and did not.
        let swahili = "Nambari yako ya siri ni 483920. Usimshirikishe mtu yeyote.";
        assert!(!contains_sensitive_secret(swahili), "the vocabulary genuinely misses it");
        match guard(swahili, contains_sensitive_secret(swahili)) {
            Caught::ByShape { because } => assert!(because.contains("bare code")),
            other => panic!("the shape layer must be the one that catches it: {other:?}"),
        }
    }

    #[test]
    fn the_two_layers_agreeing_is_the_ordinary_case_and_proves_nothing() {
        // ★★ Worth naming: most catches are by both, and a report that only
        //    counted those would say the layers are redundant when the whole
        //    value is in the cases where they are not.
        assert_eq!(guard(OTP, contains_sensitive_secret(OTP)), Caught::ByBoth);
    }

    #[test]
    fn the_vocabulary_still_catches_what_the_shape_cannot() {
        // ★★★ The detectors are each other's blind spot, and this is the other
        //     direction. No bare code at all, so shape has nothing to see — and
        //     the phrasing is exactly what the vocabulary is for.
        let wordy = "Please remember that bank staff will never ask for your one-time \
                     password, and you should not share it with anyone under any \
                     circumstances whatsoever, at any time.";
        assert!(!looks_like_a_secret(wordy));
        assert!(matches!(
            guard(wordy, contains_sensitive_secret(wordy)),
            Caught::ByVocabulary
        ));
    }

    #[test]
    fn either_layer_refusing_is_a_refusal() {
        // ★★★ OR, not AND. A false positive is one capture somebody re-enters;
        //     a false negative is a live credential in a database.
        assert!(guard(OTP, true).refuses());
        assert!(guard("Nambari yako ya siri ni 483920 tu.", false).refuses());
        assert!(!guard(RECEIVED, false).refuses());
    }

    #[test]
    fn a_bare_code_alone_is_not_enough() {
        // ★★★ The paybill confirmation genuinely contains a bare eight-digit
        //     account number. Necessary, not sufficient — at a threshold of two
        //     this real payment starts being refused.
        let s = shape_of(PAYBILL);
        assert!(s.bare_code, "the account number really is a bare code");
        assert!(s.corroboration() < REQUIRED_CORROBORATION);
    }

    #[test]
    fn a_message_with_no_code_in_it_is_never_a_message_delivering_one() {
        assert!(!shape_of(SYSTEM_BUSY).bare_code);
        assert!(!looks_like_a_secret(SYSTEM_BUSY));
    }

    #[test]
    fn a_date_and_a_clock_time_are_not_codes() {
        // ★★ Checked on the raw token, because the separators are the whole
        //    signal — `20/7/26` is only distinguishable from a code by them.
        assert!(!shape_of("Paid on 20/7/26 at 2:15 PM to the shop today").bare_code);
    }

    #[test]
    fn the_real_otp_carries_an_amount_and_is_still_caught() {
        // ★★★ The calibration the corpus forced. The obvious structural rule —
        //     "a credential message does not mention money" — would have missed
        //     this outright, because it names a USD transaction. It is caught on
        //     `no_cents`: money is quoted to two places and 113.8 is not.
        assert!(OTP.contains("USD 113.8"));
        assert!(shape_of(OTP).no_cents);
        assert!(looks_like_a_secret(OTP));
    }

    #[test]
    fn two_shouted_words_are_a_name_and_three_are_emphasis() {
        assert!(!shape_of("Paid to NAIVAS SUPERMARKET for 1234 today").shouted);
        assert!(shape_of("Your code 1234 DO NOT SHARE").shouted);
    }

    #[test]
    fn a_shape_says_why_rather_than_only_that() {
        // ★★ "Refused" alone leaves somebody guessing at their own message.
        let because = shape_of(TAN).describe();
        assert!(because.contains("bare code"));
        assert!(because.contains("no transaction reference"));
    }
}
