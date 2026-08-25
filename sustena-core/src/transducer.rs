//! **`τ : RawMessage → Event | ParseFailure`** — the transducer.
//!
//! A message arrives from a phone. This decides what, if anything, it means.
//! It is **deterministic**: an ordered set of declared [`ParseRule`]s and a
//! fixed security gate, with no LLM, no `eval`, and no callable anywhere — the
//! same discipline as [`crate::predicate`] and for a sharper reason: *a wrong
//! parse of a financial message is a wrong entry in a ledger.*
//!
//! ## ★★★ The four tiers, and why a boolean would be a lie
//!
//! | | means | what happens next |
//! |---|---|---|
//! | [`Transduction::Rejected`] | it carries a **secret** | **nothing is stored** |
//! | `Mapped` | understood **and** unambiguously routed | the operator may apply |
//! | `ParsedUnmapped` | understood; the operator is **not guessed** | a person decides |
//! | `Informational` | recognised, and nothing moved | recorded, no decision |
//! | `Unparsed` | no rule recognised the shape | queued, visible, never dropped |
//!
//! *Understood* and *actionable* are different questions. Every outbound money
//! shape lands in `ParsedUnmapped` because **which pocket** is a real
//! categorisation decision, and inventing a heuristic for it would be inventing
//! a spending decision on someone's behalf.
//!
//! ## ★★★ The secret gate is FIRST, FIXED, and not a rule
//!
//! [`contains_sensitive_secret`] runs **before any rule sees the text** and
//! before a caller may store anything. A real OTP for a bank action arrives
//! from the *same sender id* as a real transaction confirmation, so this cannot
//! be a sender check — it has to be content-based, and it has to come first.
//!
//! ★★ It is deliberately **not a `ParseRule`**: no rule can be authored,
//! corrected, learned or retired to weaken it, because it is not in the space
//! of things a rule can express. [`RuleStatus`](crate::parse_rule::RuleStatus)
//! has no `Rejected` variant for the same reason.
//!
//! ★ The vocabulary is narrow and high-confidence — `OTP`, *do not share*,
//! *verification code*, *one-time pin*, *TAN code*, *activation code*, *secret
//! PIN* — words that essentially never appear in a real confirmation, which
//! says *Confirmed* / *credited* / *debited* / *received* / *balance*. False
//! **negatives** are the residual risk of any keyword approach, which is why a
//! capture client should filter on-device too: this is defence in depth, not
//! the only line.
//!
//! ## ★★ Source-strict
//!
//! A message tagged `kcb` is parsed **only** by KCB's rules. Several real KCB
//! messages say *M-PESA* in their own wording, and without this a broadly
//! written M-Pesa pattern could promote a message out of the set its **sender**
//! already established. The sender decides the set; the body never does.

use std::collections::BTreeMap;

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use crate::parse_rule::{run_rules, ParseRule};

/// What `τ` decided about one message.
///
/// ★★★ Five outcomes, never a boolean. A `Rejected` carries **no fields and no
/// text** — there is nothing on it a caller could store even by mistake.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Transduction {
    /// ★★★ Carries a secret. No parsed fields, no operator, no payload.
    Rejected { reason: String },
    /// Understood and routed.
    Mapped {
        operator: String,
        params: BTreeMap<String, serde_json::Value>,
        parsed_fields: BTreeMap<String, serde_json::Value>,
        external_ref: Option<String>,
        reason: String,
        parser_name: String,
    },
    /// Understood; the operator is deliberately not guessed.
    ParsedUnmapped {
        parsed_fields: BTreeMap<String, serde_json::Value>,
        external_ref: Option<String>,
        reason: String,
        parser_name: String,
    },
    /// Recognised, and nothing moved.
    Informational {
        parsed_fields: BTreeMap<String, serde_json::Value>,
        external_ref: Option<String>,
        reason: String,
        parser_name: String,
    },
    /// No rule recognised the shape. Visible, never a silent drop.
    Unparsed { reason: String },
}

impl Transduction {
    pub(crate) fn mapped(
        operator: String,
        params: BTreeMap<String, serde_json::Value>,
        parsed_fields: BTreeMap<String, serde_json::Value>,
        external_ref: Option<String>,
        reason: String,
        parser_name: &str,
    ) -> Self {
        Self::Mapped {
            operator,
            params,
            parsed_fields,
            external_ref,
            reason,
            parser_name: parser_name.to_string(),
        }
    }

    pub(crate) fn parsed_unmapped(
        parsed_fields: BTreeMap<String, serde_json::Value>,
        external_ref: Option<String>,
        reason: String,
        parser_name: &str,
    ) -> Self {
        Self::ParsedUnmapped {
            parsed_fields,
            external_ref,
            reason,
            parser_name: parser_name.to_string(),
        }
    }

    pub(crate) fn informational(
        parsed_fields: BTreeMap<String, serde_json::Value>,
        external_ref: Option<String>,
        reason: String,
        parser_name: &str,
    ) -> Self {
        Self::Informational {
            // ★ The reference gives an informational hit a `direction: "none"`
            //   when it extracted nothing, so a consumer can tell *nothing
            //   moved* from *we learned nothing*.
            parsed_fields: if parsed_fields.is_empty() {
                let mut m = BTreeMap::new();
                m.insert("direction".to_string(), serde_json::json!("none"));
                m
            } else {
                parsed_fields
            },
            external_ref,
            reason,
            parser_name: parser_name.to_string(),
        }
    }

    /// ★★★ Whether a caller may store the raw text at all.
    ///
    /// The one question an ingest path must ask before writing anything. It is
    /// a method rather than a convention so "did we remember to check" is not
    /// a thing a reviewer has to look for.
    pub fn storable(&self) -> bool {
        !matches!(self, Self::Rejected { .. })
    }

    pub fn status(&self) -> &'static str {
        match self {
            Self::Rejected { .. } => "rejected",
            Self::Mapped { .. } => "mapped",
            Self::ParsedUnmapped { .. } => "parsed_unmapped",
            Self::Informational { .. } => "informational",
            Self::Unparsed { .. } => "unparsed",
        }
    }

    /// Whether a person still has a decision to make about this.
    pub fn needs_attention(&self) -> bool {
        matches!(self, Self::ParsedUnmapped { .. } | Self::Unparsed { .. })
    }

    pub fn reason(&self) -> &str {
        match self {
            Self::Rejected { reason }
            | Self::Unparsed { reason }
            | Self::Mapped { reason, .. }
            | Self::ParsedUnmapped { reason, .. }
            | Self::Informational { reason, .. } => reason,
        }
    }

    pub fn parser_name(&self) -> &str {
        match self {
            Self::Mapped { parser_name, .. }
            | Self::ParsedUnmapped { parser_name, .. }
            | Self::Informational { parser_name, .. } => parser_name,
            _ => "",
        }
    }

    /// ★ Empty for a rejection — there is nothing to read, by shape.
    pub fn parsed_fields(&self) -> BTreeMap<String, serde_json::Value> {
        match self {
            Self::Mapped { parsed_fields, .. }
            | Self::ParsedUnmapped { parsed_fields, .. }
            | Self::Informational { parsed_fields, .. } => parsed_fields.clone(),
            _ => BTreeMap::new(),
        }
    }

    pub fn operator_params(&self) -> BTreeMap<String, serde_json::Value> {
        match self {
            Self::Mapped { params, .. } => params.clone(),
            _ => BTreeMap::new(),
        }
    }

    pub fn operator(&self) -> Option<&str> {
        match self {
            Self::Mapped { operator, .. } => Some(operator),
            _ => None,
        }
    }

    pub fn external_ref(&self) -> Option<&str> {
        match self {
            Self::Mapped { external_ref, .. }
            | Self::ParsedUnmapped { external_ref, .. }
            | Self::Informational { external_ref, .. } => external_ref.as_deref(),
            _ => None,
        }
    }
}

// ── the secret gate ─────────────────────────────────────────────────────────

/// The patterns. ★ Fixed code, not data — see the module docs.
const SECRET_PATTERNS: &[&str] = &[
    r"(?i)\bOTP\b",
    r"(?i)do\s+not\s+share",
    r"(?i)verification\s+code",
    r"(?i)one[\s-]?time\s+(?:pin|password|code)",
    r"(?i)security\s+code",
    // Added against real bank-thread content: TAN codes, a generic
    // activation-code framing, and a card's secret PIN. None overlap with real
    // confirmation vocabulary — checked against every real sample before
    // shipping.
    r"(?i)tan\s+code",
    r"(?i)activation\s+code",
    r"(?i)secret\s+pin",
    r"(?i)\bpin\s+is\b",
    r"(?i)code\s+is\s+valid",
];

fn secrets() -> &'static Vec<Regex> {
    static COMPILED: OnceLock<Vec<Regex>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        SECRET_PATTERNS
            .iter()
            .map(|p| Regex::new(p).expect("a pattern this crate wrote itself"))
            .collect()
    })
}

/// ★★★ Does this text carry a one-time secret?
///
/// Checked before any rule and before anything is stored. A caller that stores
/// first and asks later has already leaked the credential.
pub fn contains_sensitive_secret(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    secrets().iter().any(|p| p.is_match(text))
}

// ── the shipped rule set ────────────────────────────────────────────────────

/// The seed rules, embedded.
///
/// ★★ Declared **data**, shipped as data: the same JSON the reference's rule
/// set exports, so the two engines' rules cannot drift by transcription. A
/// correction adds to this set at runtime; it never edits this file.
const SEED_JSON: &str = include_str!("../assets/parse_rules_seed.json");

fn seed() -> &'static BTreeMap<String, Vec<ParseRule>> {
    static RULES: OnceLock<BTreeMap<String, Vec<ParseRule>>> = OnceLock::new();
    RULES.get_or_init(|| {
        serde_json::from_str(SEED_JSON).expect("the embedded seed rule set is valid")
    })
}

/// Every shipped rule for one source, in precedence order.
pub fn seed_rules(source: &str) -> &'static [ParseRule] {
    seed()
        .get(&source.to_lowercase())
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Every source the engine ships rules for.
pub fn seed_sources() -> Vec<String> {
    seed().keys().cloned().collect()
}

/// Every shipped rule, across every source — for a library view.
pub fn all_seed_rules() -> Vec<ParseRule> {
    seed().values().flatten().cloned().collect()
}

// ── τ ───────────────────────────────────────────────────────────────────────

/// **Parse one message.**
///
/// `source` makes parsing **strict** when it names a source the engine knows:
/// only that source's rules are tried, whatever the body says. An unknown or
/// absent source falls back to trying every source's rules in order — an
/// honest degrade for a genuinely unrecognised sender rather than a refusal.
///
/// `extra` is the caller's own rule set — corrections and learned rules — tried
/// **before** the shipped ones, so a person's correction takes precedence over
/// the shape it corrects.
pub fn parse_message(raw_text: &str, source: Option<&str>, extra: &[ParseRule]) -> Transduction {
    let text = raw_text.trim();
    if text.is_empty() {
        return Transduction::Unparsed { reason: "Empty message.".to_string() };
    }

    // ★★★ FIRST. Before any rule, before any caller may store anything.
    if contains_sensitive_secret(text) {
        return Transduction::Rejected {
            reason: "Message contains an OTP/verification code or similar secret — refused, \
                     never parsed or stored."
                .to_string(),
        };
    }

    let key = source.unwrap_or("").to_lowercase();
    let strict = seed().contains_key(&key);

    // A correction wins over the shape it corrects, within the same source.
    let mine: Vec<&ParseRule> = extra
        .iter()
        .filter(|r| !strict || r.source.eq_ignore_ascii_case(&key))
        .collect();
    for r in mine {
        if let Some(t) = crate::parse_rule::apply_rule(r, text) {
            return t;
        }
    }

    if strict {
        if let Some(t) = run_rules(seed_rules(&key), text) {
            return t;
        }
    } else {
        // ★ Unknown source: try everything, in a stable order, rather than
        //   refusing outright. Sources are sorted, so this is reproducible.
        for source in seed().keys() {
            if let Some(t) = run_rules(seed_rules(source), text) {
                return t;
            }
        }
    }

    Transduction::Unparsed {
        reason: "No registered parser recognised this message's shape.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_rule::{typecheck_rule, OperatorUniverse};

    struct Real;
    impl OperatorUniverse for Real {
        fn has(&self, operator: &str) -> bool {
            operator == "budget.record_income"
        }
        fn params_of(&self, _operator: &str) -> Option<Vec<String>> {
            Some(vec!["amount".into(), "source".into(), "frequency".into()])
        }
    }

    // ── the secret gate ─────────────────────────────────────────────────────

    #[test]
    fn every_secret_shape_is_rejected_and_carries_nothing() {
        for text in [
            "Your OTP is 483920. Do not share it with anyone.",
            "483920 is your verification code for KCB.",
            "Use one-time pin 112233 to authorise.",
            "Your TAN code is 55123.",
            "Activation code: 9931. This code is valid for only 90s.",
            "Your KCB card secret PIN is 4417.",
            "Security code 8821 for your transaction.",
        ] {
            let t = parse_message(text, Some("kcb"), &[]);
            assert!(matches!(t, Transduction::Rejected { .. }), "not rejected: {text}");
            // ★★★ Nothing to store, by shape.
            assert!(!t.storable());
            assert!(t.parsed_fields().is_empty());
            assert!(t.operator().is_none());
            assert!(t.external_ref().is_none());
        }
    }

    #[test]
    fn the_secret_gate_runs_before_any_rule_whatever_the_sender_says() {
        // A message that WOULD match a real income rule, with a secret in it.
        // A real OTP arrives from the same sender as a real confirmation, so
        // the sender can never be what decides this.
        let text = "SFF7ABCDEF Confirmed. You have received Ksh900.00 from JANE DOE 254712345678 \
                    on 1/8/26 at 11:15 AM. New M-PESA balance is Ksh1,000.00. Your OTP is 4417.";
        for source in [Some("mpesa"), Some("kcb"), None] {
            let t = parse_message(text, source, &[]);
            assert!(matches!(t, Transduction::Rejected { .. }), "source {source:?}");
            assert!(t.operator().is_none(), "a secret must never route to an operator");
        }
    }

    #[test]
    fn real_confirmation_vocabulary_is_not_a_false_positive() {
        // Every shipped example, through the gate. A filter that ate real
        // messages would be worse than no filter.
        for rule in all_seed_rules() {
            for example in &rule.examples {
                assert!(
                    !contains_sensitive_secret(example),
                    "the secret gate misfires on {}: {example}",
                    rule.id
                );
            }
        }
    }

    #[test]
    fn an_empty_message_is_unparsed_not_rejected() {
        assert!(matches!(parse_message("   ", Some("mpesa"), &[]), Transduction::Unparsed { .. }));
        assert!(!contains_sensitive_secret(""));
    }

    // ── the shipped rules ───────────────────────────────────────────────────

    #[test]
    fn every_shipped_rule_is_well_typed() {
        for rule in all_seed_rules() {
            let errors = typecheck_rule(&rule, &Real);
            assert!(errors.is_empty(), "{}: {errors:?}", rule.id);
        }
    }

    #[test]
    fn every_shipped_rule_matches_its_own_examples() {
        // ★ The regression corpus a rule declares about itself. A rule that
        //   stopped matching its own example is a rule that changed meaning.
        for rule in all_seed_rules() {
            for example in &rule.examples {
                let t = crate::parse_rule::apply_rule(&rule, example);
                assert!(t.is_some(), "{} no longer matches its own example: {example}", rule.id);
            }
        }
    }

    #[test]
    fn the_shipped_set_is_the_expected_shape() {
        assert_eq!(seed_rules("mpesa").len(), 7);
        assert_eq!(seed_rules("kcb").len(), 15);
        assert_eq!(seed_rules("nobody").len(), 0);
    }

    // ── source-strict ───────────────────────────────────────────────────────

    #[test]
    fn a_kcb_tagged_message_is_never_parsed_by_mpesa_rules() {
        // A real M-Pesa-shaped text, tagged as KCB. Under strict routing it
        // must NOT cross into the M-Pesa set, whatever the body says.
        let mpesa_text = seed_rules("mpesa")
            .iter()
            .find(|r| r.id == "mpesa_received")
            .and_then(|r| r.examples.first().cloned())
            .expect("the shipped rule carries its own example");

        let as_mpesa = parse_message(&mpesa_text, Some("mpesa"), &[]);
        assert_eq!(as_mpesa.status(), "mapped");
        assert_eq!(as_mpesa.parser_name(), "mpesa_received");

        let as_kcb = parse_message(&mpesa_text, Some("kcb"), &[]);
        assert_eq!(
            as_kcb.status(),
            "unparsed",
            "body content must never promote a message out of its sender's set"
        );
    }

    #[test]
    fn the_kcb_shapes_that_mention_mpesa_stay_in_kcb() {
        // ★ Confirmed against a real device: these arrive from the KCB sender
        //   even though their own wording says M-PESA.
        for id in ["kcb_mpesa_paybill", "kcb_mpesa_received", "kcb_mpesa_sent"] {
            let rule = seed_rules("kcb").iter().find(|r| r.id == id).expect(id);
            let Some(example) = rule.examples.first() else { continue };
            let t = parse_message(example, Some("kcb"), &[]);
            assert_eq!(t.parser_name(), id, "{id} must own its own shape");
        }
    }

    #[test]
    fn an_unknown_source_tries_everything_rather_than_refusing() {
        let text = seed_rules("mpesa")
            .iter()
            .find(|r| r.id == "mpesa_received")
            .and_then(|r| r.examples.first().cloned())
            .expect("example");
        let t = parse_message(&text, None, &[]);
        assert_eq!(t.status(), "mapped", "an honest degrade, not a refusal");
    }

    // ── money-safety asymmetry ──────────────────────────────────────────────

    #[test]
    fn the_only_auto_applied_operator_is_income() {
        // ★★★ Every shipped mapped rule routes to income. A spend that
        //   auto-applied would be inventing which pocket a person meant.
        let mapped: Vec<_> = all_seed_rules()
            .into_iter()
            .filter(|r| r.operator.is_some())
            .collect();
        assert!(!mapped.is_empty(), "some rules must map, or the tier is theatre");
        for r in &mapped {
            assert_eq!(
                r.operator.as_deref(),
                Some("budget.record_income"),
                "{} auto-applies {:?} — outbound money must ask a person",
                r.id,
                r.operator
            );
        }
    }

    #[test]
    fn every_outbound_shape_asks_a_person() {
        for rule in all_seed_rules() {
            let outbound = rule
                .extract
                .get("direction")
                .and_then(|s| s.value.as_ref())
                .and_then(|v| v.as_str())
                == Some("sent");
            if outbound {
                assert!(
                    rule.operator.is_none(),
                    "{} routes an outbound shape automatically",
                    rule.id
                );
            }
        }
    }

    // ── corrections ─────────────────────────────────────────────────────────

    #[test]
    fn a_correction_takes_precedence_over_the_shape_it_corrects() {
        let text = seed_rules("mpesa")
            .iter()
            .find(|r| r.id == "mpesa_received")
            .and_then(|r| r.examples.first().cloned())
            .expect("example");
        let mut correction = seed_rules("mpesa")[0].clone();
        correction.id = "my-correction".into();
        correction.pattern = "Confirmed".into();
        correction.status = crate::parse_rule::RuleStatus::ParsedUnmapped;
        correction.operator = None;
        correction.params.clear();

        let t = parse_message(&text, Some("mpesa"), std::slice::from_ref(&correction));
        assert_eq!(t.parser_name(), "my-correction");
    }

    #[test]
    fn a_correction_for_another_source_never_leaks_across() {
        let text = seed_rules("mpesa")
            .iter()
            .find(|r| r.id == "mpesa_received")
            .and_then(|r| r.examples.first().cloned())
            .expect("example");
        let mut foreign = seed_rules("kcb")[0].clone();
        foreign.id = "kcb-correction".into();
        foreign.pattern = "Confirmed".into();
        foreign.status = crate::parse_rule::RuleStatus::ParsedUnmapped;
        foreign.operator = None;
        foreign.params.clear();

        let t = parse_message(&text, Some("mpesa"), std::slice::from_ref(&foreign));
        assert_eq!(t.parser_name(), "mpesa_received", "a kcb rule must not run on mpesa");
    }

    #[test]
    fn a_correction_can_never_weaken_the_secret_gate() {
        // ★★★ The one that must not regress. A rule that matches everything,
        //   authored to swallow an OTP, still never sees the text.
        let mut greedy = seed_rules("kcb")[0].clone();
        greedy.id = "malicious".into();
        greedy.pattern = "(?s).*".into();
        greedy.status = crate::parse_rule::RuleStatus::Informational;
        greedy.operator = None;
        greedy.params.clear();
        greedy.extract.clear();

        let t = parse_message("Your OTP is 4417.", Some("kcb"), std::slice::from_ref(&greedy));
        assert!(matches!(t, Transduction::Rejected { .. }));
        assert!(!t.storable());
    }

    // ── determinism ─────────────────────────────────────────────────────────

    #[test]
    fn parsing_is_reproducible() {
        for rule in all_seed_rules() {
            for example in &rule.examples {
                let a = parse_message(example, Some(&rule.source), &[]);
                let b = parse_message(example, Some(&rule.source), &[]);
                assert_eq!(a, b, "{}", rule.id);
            }
        }
    }
}

#[cfg(test)]
mod reversal_tests {
    use super::*;

    /// ★★★ From Bonnie's phone: a reversal was asking which pocket it came
    /// from. It is not a spend, so it must never reach the classify queue.
    #[test]
    fn a_kcb_reversal_is_informational() {
        let raw = "Your card transaction of KES 100.00 at Bolt KE on 24/08/2026 has been reversed. \
                   Avail balance KES 5,000.00";
        let t = parse_message(raw, Some("kcb"), &[]);
        assert!(
            matches!(t, Transduction::Informational { .. }),
            "a reversal must be informational, got {t:?}"
        );
    }

    /// The wording varies; "was reversed" is the same fact.
    #[test]
    fn the_other_reversal_wording_also_lands() {
        let raw = "Your card transaction of KES 250.00 at NAIVAS was reversed.";
        let t = parse_message(raw, Some("kcb"), &[]);
        assert!(matches!(t, Transduction::Informational { .. }), "got {t:?}");
    }

    /// ★★ The reversal rule sits ahead of the card shape, so it wins. A real
    /// card purchase must still be a spend that asks for a pocket.
    #[test]
    fn a_real_card_purchase_is_untouched() {
        // ★ The rule's own shipped example, rather than text invented here.
        let raw = "KES 429.00 transaction made on KCB card 1234XXXXXXXX5678 at                    GOOGLE *Spotify Music on 1/8/26 12:25pm, Avail balance KES 59,055.00";
        let t = parse_message(raw, Some("kcb"), &[]);
        assert!(
            matches!(t, Transduction::ParsedUnmapped { .. }),
            "a purchase still needs a pocket, got {t:?}"
        );
    }
}
