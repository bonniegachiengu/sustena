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
    // ★★★ TWO detectors, consulted with OR — see [`crate::secret_shape`]. The
    //     vocabulary knows phrasings; the shape reads structure and no words at
    //     all, so a bank inventing new wording defeats one and not the other.
    //     Either refusing is a refusal, because a false positive costs one
    //     capture somebody re-enters and a false negative stores a credential.
    match crate::secret_shape::guard(text, contains_sensitive_secret(text)) {
        crate::secret_shape::Caught::Nothing => {}
        crate::secret_shape::Caught::ByShape { because } => {
            return Transduction::Rejected {
                reason: format!(
                    "Message has the shape of a one-time code ({because}) — refused, never                      parsed or stored, even though its wording is unfamiliar."
                ),
            };
        }
        _ => {
            return Transduction::Rejected {
                reason: "Message contains an OTP/verification code or similar secret —                          refused, never parsed or stored."
                    .to_string(),
            };
        }
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

    /// ★★★ The REAL registry, not a stand-in for it.
    ///
    /// This was a stub that knew one operator, which was true enough while one
    /// operator was all any rule mapped to — and it meant `typecheck_rule` was
    /// being checked against a fiction. A rule naming an operator that does not
    /// exist, or handing it a parameter it does not declare, would have passed.
    struct Real;
    impl OperatorUniverse for Real {
        fn has(&self, operator: &str) -> bool {
            crate::operator::Registry::default().get(operator).is_some()
        }
        fn params_of(&self, operator: &str) -> Option<Vec<String>> {
            crate::operator::Registry::default()
                .get(operator)
                .map(|m| m.params.iter().map(|p| p.name.to_string()).collect())
        }
    }

    // ── the secret gate ─────────────────────────────────────────────────────

    #[test]
    fn a_credential_in_wording_the_vocabulary_has_never_seen_is_still_refused() {
        // ★★★ IMM-11, end to end. The ten regexes have nothing to match in
        //     Swahili; the shape is unchanged. Before the second detector this
        //     message parsed as ordinary text and was stored.
        let swahili = "Nambari yako ya siri ni 483920. Usimshirikishe mtu yeyote.";
        assert!(!contains_sensitive_secret(swahili), "the vocabulary genuinely misses it");
        match parse_message(swahili, None, &[]) {
            Transduction::Rejected { reason } => {
                assert!(reason.contains("shape of a one-time code"), "{reason}");
                assert!(reason.contains("unfamiliar"), "say WHY it was caught: {reason}");
            }
            other => panic!("must be refused: {other:?}"),
        }
    }

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
    fn received_money_is_recognised_with_or_without_a_phone_number() {
        // ★★★ From his phone, 26 Aug. "You have received Ksh1,000.00 from mary
        //     ngigi" — no number after the name — matched nothing, so a plainly
        //     received KES 1,000 fell through to the classify card and was
        //     asked which POCKET it belonged to, exactly as a spend would be.
        //
        //     The anchors that make this rule safe are "You have received Ksh"
        //     and "New M-PESA balance". The phone bought no precision and cost
        //     a whole shape, so it is optional now.
        for raw in [
            "UHIE73CBRB Confirmed. You have received Ksh1,000.00 from mary ngigi \
             on 26/8/26 at 9:15 AM. New M-PESA balance is Ksh5,000.00",
            "QGH7XJ4P2Q Confirmed. You have received Ksh1,500.00 from JOHN KAMAU 254712345678 \
             on 2/8/26 at 10:15 AM. New M-PESA balance is Ksh12,050.00",
        ] {
            let t = parse_message(raw, Some("mpesa"), &[]);
            assert_eq!(t.status(), "mapped", "money in files itself: {raw}");
            assert_eq!(t.operator(), Some("budget.record_income"));
            assert_eq!(t.parsed_fields().get("direction").and_then(|v| v.as_str()), Some("received"));
        }
    }

    #[test]
    fn the_shipped_set_is_the_expected_shape() {
        // 19 = the original 7, plus 12 shapes learned from his own 2,716
        // real M-Pesa messages on 26 Aug: Fuliza (borrow, interest, repay,
        // statement), Pochi la Biashara (in, moved), M-Shwari (in, out),
        // send-to-a-business, agent withdrawal, balance enquiry, failures.
        assert_eq!(seed_rules("mpesa").len(), 19);
        // 17 = 15 original shapes, + kcb_reversal for refund netting, and
        // + kcb_send_to_mpesa, the real shape his KCB app sends when he moves
        // his own money to his own M-Pesa.
        assert_eq!(seed_rules("kcb").len(), 17);
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

    /// The operators that decide what money was FOR.
    ///
    /// ★★★ This is the real line, and it is not the same line as "inbound or
    /// outbound". Filing money into a category is a human judgment — only a
    /// person knows whether a payment was food or a favour — and nothing may
    /// make that judgment automatically. Everything else about a message can
    /// be read off it: a lender is named in the text, a transfer has the
    /// household at both ends, income is income.
    const CATEGORISING: &[&str] = &["budget.spend", "budget.allocate"];

    #[test]
    fn nothing_files_money_into_a_category_without_being_asked() {
        // ★★★ The money-safety rule. It used to be stated as "the only
        //     auto-applied operator is income", which was the same rule while
        //     income was the only unambiguous shape. §6 settled four more —
        //     an overdraft, a savings transfer, a cash withdrawal, a Pochi —
        //     and none of them involve choosing a pocket. The rule that
        //     actually protects him is this one, so it is written directly
        //     rather than approximated by a proxy that has stopped holding.
        let mapped: Vec<_> =
            all_seed_rules().into_iter().filter(|r| r.operator.is_some()).collect();
        assert!(!mapped.is_empty(), "some rules must map, or the tier is theatre");
        for r in &mapped {
            let op = r.operator.as_deref().unwrap_or_default();
            assert!(
                !CATEGORISING.contains(&op),
                "{} auto-applies {op} — only a person decides what money was for",
                r.id,
            );
        }
    }

    #[test]
    fn a_payment_to_somebody_outside_the_household_always_asks() {
        // ★★★ The half of the old outbound rule that still stands, and the
        //     one that matters: money leaving toward a shop or a person is
        //     exactly the case where the pocket is a judgment. A transfer
        //     between the household's own places is not — the money has not
        //     gone anywhere, and both ends are already known.
        let internal: &[&str] = &["budget.transfer", "budget.repay_debt", "budget.charge_debt"];
        for rule in all_seed_rules() {
            let outbound = rule
                .extract
                .get("direction")
                .and_then(|s| s.value.as_ref())
                .and_then(|v| v.as_str())
                == Some("sent");
            if !outbound {
                continue;
            }
            match rule.operator.as_deref() {
                None => {}
                Some(op) => assert!(
                    internal.contains(&op),
                    "{} routes outbound money to {op} without asking",
                    rule.id,
                ),
            }
        }
    }

    #[test]
    fn every_settled_instrument_shape_names_a_real_operator() {
        // ★★ The other side of the same coin: the shapes §6 settled must
        //    actually route somewhere, or settling them was paperwork.
        let reg = crate::operator::Registry::default();
        for id in [
            "mpesa_fuliza_borrow", "mpesa_fuliza_interest", "mpesa_fuliza_repay",
            "mpesa_mshwari_deposit", "mpesa_mshwari_withdraw",
            "mpesa_withdraw", "mpesa_agent_withdraw",
            "mpesa_pochi_received", "mpesa_pochi_withdraw",
        ] {
            let rule = all_seed_rules().into_iter().find(|r| r.id == id).expect(id);
            let op = rule.operator.as_deref().unwrap_or_else(|| panic!("{id} maps to nothing"));
            assert!(reg.get(op).is_some(), "{id} names {op}, which does not exist");
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
mod transfer_pair_tests {
    //! Two texts, one movement of his own money.
    //!
    //! ★★★ **The join key is the M-PESA ref both sides carry.** Amount and time
    //! alone coincide -- two payments of the same size in the same minute are
    //! ordinary -- but a shared reference is the banks agreeing they are talking
    //! about one event. The KCB side's OWN reference is a different code
    //! entirely, so a rule that captured the leading token would produce two
    //! unrelated messages that never join.
    //!
    //! ★ Real shapes from his handset, with the name replaced and the account
    //! number redacted. The structure and the ref-sharing are what is under
    //! test, and both survive the substitution.
    use super::*;

    // KCB -> M-Pesa, KES 1,500, shared ref UHQB94FQ88.
    const P1_MPESA: &str = "UHQB94FQ88 Confirmed.You have received Ksh1,500.00 from KCB 1 501901 \
        on 26/8/26 at 9:21 PM New M-PESA balance is Ksh1,500.00.";
    const P1_KCB: &str = "MBNHEUDF935FAZOG Completed. Your SEND TO M-PESA request of KES 1,500.00 \
        from 135****140 to 254****143 - PLACEHOLDER NAME at 2026-08-26 09:21:49 PM has been \
        processed successfully. Transaction cost KES 15.00 Incl. Tax Amount KES 1.50. \
        M-PESA REF: UHQB94FQ88.";

    // M-Pesa -> KCB paybill, KES 40,000, shared ref UH1B91GYNW.
    const P2_KCB: &str = "Ksh 40000.00 sent to KCB Pay Bill 522522 for account 135***5140 \
        PLACEHOLDER NAME has been received on 01/08/2026 at 12:21 PM. M-PESA ref UH1B91GYNW.";
    const P2_MPESA: &str = "UH1B91GYNW Confirmed. KSH40,000.00 sent to KCB Paybill AC for account \
        135***5140 on 1/8/26 at 12:21 PM New M-PESA balance is KSH16,877.47. \
        Transaction cost, KSH99.00.";

    fn refv(text: &str, source: &str) -> Option<String> {
        parse_message(text, Some(source), &[])
            .parsed_fields()
            .get("ref")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    fn amount(text: &str, source: &str) -> Option<f64> {
        parse_message(text, Some(source), &[]).parsed_fields().get("amount").and_then(|v| v.as_f64())
    }

    #[test]
    fn both_sides_of_a_kcb_to_mpesa_transfer_carry_the_same_ref() {
        // ★★★ The join, on his real texts. The KCB message leads with its OWN
        //     reference (MBNHEUDF935FAZOG) and carries the shared one at the
        //     end -- capture the wrong token and these never meet.
        assert_eq!(refv(P1_MPESA, "mpesa").as_deref(), Some("UHQB94FQ88"));
        assert_eq!(refv(P1_KCB, "kcb").as_deref(), Some("UHQB94FQ88"));
        assert_eq!(amount(P1_MPESA, "mpesa"), amount(P1_KCB, "kcb"));
    }

    #[test]
    fn the_kcb_side_does_not_take_its_own_reference_as_the_key() {
        // ★★ Stated as its own test because it is the whole failure mode: a
        //    leading all-caps token looks exactly like a reference.
        assert_ne!(refv(P1_KCB, "kcb").as_deref(), Some("MBNHEUDF935FAZOG"));
    }

    #[test]
    fn both_sides_of_an_mpesa_to_kcb_paybill_carry_the_same_ref() {
        assert_eq!(refv(P2_KCB, "kcb").as_deref(), Some("UH1B91GYNW"));
        assert_eq!(refv(P2_MPESA, "mpesa").as_deref(), Some("UH1B91GYNW"));
        assert_eq!(amount(P2_KCB, "kcb"), amount(P2_MPESA, "mpesa"));
    }

    #[test]
    fn the_two_pairs_do_not_join_to_each_other() {
        // ★ Different transfers must stay different. If refs collided the join
        //   would hide a real transaction, which is the dangerous direction.
        assert_ne!(refv(P1_MPESA, "mpesa"), refv(P2_MPESA, "mpesa"));
    }

    #[test]
    fn money_leaving_kcb_asks_rather_than_filing_itself() {
        // ★★★ The money-safety asymmetry, on the new shape. An outbound
        //     transfer is not something to record without him.
        let t = parse_message(P1_KCB, Some("kcb"), &[]);
        assert_eq!(t.status(), "parsed_unmapped");
        assert_eq!(t.parsed_fields().get("direction").and_then(|v| v.as_str()), Some("sent"));
    }
}
