//! **Correction-learning** — a human's confirmed classification becomes a rule.
//!
//! A person classifies a message the transducer did not understand. That
//! correction is the most valuable signal the system will ever get about a
//! message shape, and it should not have to be given twice.
//!
//! ```text
//! raw message + the (o, θ) a human confirmed  ──►  a candidate ParseRule
//! ```
//!
//! ## ★★★ Template synthesis, NOT generalisation
//!
//! This is not an LLM and not a learner. It is **conservative to the point of
//! being boring**: the confirmed amount's own text is located inside the raw
//! message and parameterised, a leading reference code is parameterised if
//! there is one, and **everything else is kept as a literal anchor**. The
//! candidate matches this shape and shapes that differ only in the amount.
//!
//! ★★ That is deliberately narrow. A rule that generalised further would start
//! matching messages nobody confirmed anything about — and a false match here
//! is a wrong entry in a ledger, which is exactly the failure the whole
//! transducer is arranged around. The cost of being too narrow is that a person
//! corrects a second variant once; the cost of being too broad is silent, and
//! financial.
//!
//! ★ If the confirmed amount cannot be found in the raw text at all, this
//! returns `None` rather than a rule anchored on nothing. A useless rule is
//! worse than no rule: it occupies precedence and matches by accident.
//!
//! ## ★★★ The money-safety asymmetry is inherited, not re-decided
//!
//! A learned **income** rule may be `Mapped` — money arriving is unambiguous,
//! and the same shape next month means the same thing. A learned **spend**
//! rule is `ParsedUnmapped` and carries no operator at all: *which pocket* is a
//! decision that belongs to a person every time, and a rule that auto-applied
//! it would be spending on their behalf because they once classified something
//! similar.
//!
//! ## ★★ Verified before a human ever sees it
//!
//! [`verify_candidate`] is `V(r)`, two conditions, both required:
//!
//! 1. **`Γ ⊢ r`** and the rule matches **its own inducing example**. A rule
//!    that does not recognise the message that produced it is broken by
//!    construction.
//! 2. **Regression**: it must not fire on a message an existing rule already
//!    handles. A correction that quietly captured its neighbours' messages
//!    would be a regression nobody asked for.

use std::collections::BTreeMap;

use regex::Regex;
use std::sync::OnceLock;

use crate::parse_rule::{
    apply_rule, typecheck_rule, FieldKind, FieldSpec, OperatorUniverse, ParseRule, ParseRuleTrust,
    RuleStatus,
};

/// A leading transaction reference — `TGH4A2B9CD Confirmed…`.
fn leading_ref_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^([A-Z0-9]{8,12})\b").unwrap())
}

/// How a confirmed float might literally appear in the raw text.
///
/// ★ The raw message carries the ORIGINAL formatting — thousands separators, a
/// trailing `.00` — not a float's own rendering. Tried in order; the first one
/// actually present wins.
fn amount_texts(amount: f64) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if !out.contains(&s) {
            out.push(s);
        }
    };
    if amount.fract() == 0.0 && amount.abs() < 9.0e15 {
        let i = amount as i64;
        push(format!("{}.00", with_commas(i)));
        push(with_commas(i));
        push(format!("{i}.00"));
        push(format!("{i}"));
    } else {
        push(with_commas_f(amount));
        push(format!("{amount:.2}"));
    }
    out
}

fn with_commas(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

fn with_commas_f(v: f64) -> String {
    let whole = v.trunc() as i64;
    let cents = ((v.abs().fract() * 100.0).round()) as i64;
    format!("{}.{:02}", with_commas(whole), cents)
}

/// **Synthesise a candidate rule from one confirmed correction.**
///
/// `None` when the confirmed amount's own text cannot be located in the raw
/// message — see the module docs for why that is a refusal rather than a
/// looser rule.
pub fn synthesize_from_correction(
    source: &str,
    raw_text: &str,
    operator: &str,
    params: &BTreeMap<String, serde_json::Value>,
    id: &str,
) -> Option<ParseRule> {
    let amount = params.get("amount")?.as_f64()?;

    let mut pattern = regex::escape(raw_text);
    let mut extract: BTreeMap<String, FieldSpec> = BTreeMap::new();

    if let Some(m) = leading_ref_re().captures(raw_text) {
        let escaped_ref = regex::escape(m.get(1)?.as_str());
        if pattern.contains(&escaped_ref) {
            pattern = pattern.replacen(&escaped_ref, r"(?P<ref>[A-Z0-9]{8,12})", 1);
            extract.insert(
                "ref".into(),
                FieldSpec { kind: FieldKind::Text, group: Some("ref".into()), value: None },
            );
        }
    }

    let mut found = false;
    for candidate in amount_texts(amount) {
        let escaped = regex::escape(&candidate);
        if pattern.contains(&escaped) {
            pattern = pattern.replacen(&escaped, r"(?P<amount>[\d,]+\.?\d*)", 1);
            extract.insert(
                "amount".into(),
                FieldSpec { kind: FieldKind::Amount, group: Some("amount".into()), value: None },
            );
            found = true;
            break;
        }
    }
    if !found {
        // ★ Nothing to anchor a generalisable rule on. A rule that matched only
        //   this exact text would occupy precedence and teach nothing.
        return None;
    }

    // ★★★ The asymmetry, inherited rather than re-decided here.
    let is_income = operator == "budget.record_income";

    let mut rule_params: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    if is_income {
        for (k, v) in params {
            // The confirmed amount becomes a reference; everything else the
            // person confirmed is carried verbatim.
            let is_the_amount = k == "amount" && v.as_f64() == Some(amount);
            rule_params.insert(
                k.clone(),
                if is_the_amount { serde_json::json!("$amount") } else { v.clone() },
            );
        }
    }

    Some(ParseRule {
        id: id.to_string(),
        source: source.to_string(),
        version: 1,
        pattern,
        extract,
        status: if is_income { RuleStatus::Mapped } else { RuleStatus::ParsedUnmapped },
        operator: if is_income { Some(operator.to_string()) } else { None },
        params: rule_params,
        flags: vec![],
        reason_template: if is_income {
            None
        } else {
            Some(
                "Recognised from a message you classified before — still needs a pocket decision."
                    .into(),
            )
        },
        trust: ParseRuleTrust::UserCorrected,
        provenance: "human_correction".into(),
        examples: vec![raw_text.to_string()],
    })
}

/// Why a candidate was refused.
///
/// ★ Named the long way: `Refusal` is taken. Collision 44, same house rule —
/// the newcomer takes the longer name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LearningRefusal {
    NotWellTyped(Vec<String>),
    /// It does not recognise the message that produced it.
    NoFidelity,
    /// It fires on a message an existing rule already handles.
    Regression { existing_rule: String, example: String },
}

impl std::fmt::Display for LearningRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotWellTyped(errors) => write!(f, "Γ ⊢ r failed: {}", errors.join("; ")),
            Self::NoFidelity => {
                write!(f, "the candidate does not match its own inducing example")
            }
            Self::Regression { existing_rule, example } => write!(
                f,
                "the candidate would also match a message '{existing_rule}' already handles: {example}"
            ),
        }
    }
}

/// **`V(r)`** — both conditions, both required. See the module docs.
pub fn verify_candidate<U: OperatorUniverse>(
    candidate: &ParseRule,
    inducing_example: &str,
    existing: &[ParseRule],
    universe: &U,
) -> Result<(), LearningRefusal> {
    let errors = typecheck_rule(candidate, universe);
    if !errors.is_empty() {
        // ★ Short-circuit: an ill-typed rule cannot safely be RUN, so the
        //   fidelity check below must not be attempted on it.
        return Err(LearningRefusal::NotWellTyped(errors));
    }

    if apply_rule(candidate, inducing_example).is_none() {
        return Err(LearningRefusal::NoFidelity);
    }

    for rule in existing {
        for example in &rule.examples {
            if example == inducing_example {
                continue;
            }
            if apply_rule(candidate, example).is_some() {
                return Err(LearningRefusal::Regression {
                    existing_rule: rule.id.clone(),
                    example: example.clone(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_rule::NoOperators;
    use crate::transducer::seed_rules;
    use serde_json::json;

    struct Universe;
    impl OperatorUniverse for Universe {
        fn has(&self, operator: &str) -> bool {
            matches!(operator, "budget.record_income" | "budget.spend")
        }
        fn params_of(&self, _operator: &str) -> Option<Vec<String>> {
            Some(vec![
                "amount".into(),
                "source".into(),
                "frequency".into(),
                "pocket_name".into(),
                "description".into(),
            ])
        }
    }

    fn params(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    const UNSEEN: &str =
        "ZZ99XYZ123 Confirmed. Ksh2,500.00 has been moved to your wallet by CHAMA GROUP on 1/8/26.";

    #[test]
    fn a_correction_becomes_a_rule_that_matches_the_message_that_taught_it() {
        let r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0)), ("source", json!("Chama"))]),
            "learned_1",
        )
        .expect("synthesised");
        assert!(verify_candidate(&r, UNSEEN, &[], &Universe).is_ok());
        let t = apply_rule(&r, UNSEEN).expect("matches");
        assert_eq!(t.operator(), Some("budget.record_income"));
        assert_eq!(t.operator_params()["amount"], json!(2500.0));
        assert_eq!(t.operator_params()["source"], json!("Chama"));
    }

    #[test]
    fn the_learned_rule_generalises_over_the_amount_and_nothing_else() {
        let r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0))]),
            "learned_2",
        )
        .expect("synthesised");

        // ★ The same shape with a DIFFERENT amount matches, and extracts it.
        let next_month = UNSEEN.replace("2,500.00", "4,100.00");
        let t = apply_rule(&r, &next_month).expect("matches the next one");
        assert_eq!(t.operator_params()["amount"], json!(4100.0));

        // ★★ A different shape does NOT. Being narrow is the design.
        assert!(apply_rule(&r, "Ksh2,500.00 paid to SOMEWHERE ELSE entirely").is_none());
    }

    #[test]
    fn a_leading_reference_is_parameterised_so_the_next_one_still_matches() {
        let r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0))]),
            "learned_3",
        )
        .expect("synthesised");
        let different_ref = UNSEEN.replace("ZZ99XYZ123", "AB12CD34EF");
        assert!(apply_rule(&r, &different_ref).is_some(), "a new ref must not break it");
    }

    #[test]
    fn a_learned_spend_never_auto_applies() {
        // ★★★ The asymmetry. Money out is a person's decision every time.
        let r = synthesize_from_correction(
            "mpesa",
            "QQ11WW22EE Confirmed. Ksh680.00 paid to NAIVAS on 1/8/26.",
            "budget.spend",
            &params(&[("amount", json!(680.0)), ("pocket_name", json!("food"))]),
            "learned_4",
        )
        .expect("synthesised");
        assert_eq!(r.status, RuleStatus::ParsedUnmapped);
        assert!(r.operator.is_none(), "a learned spend carries no operator at all");
        assert!(r.params.is_empty(), "and no pocket");
        assert!(r.reason_template.is_some(), "it says why it still needs a person");
    }

    #[test]
    fn a_learned_rule_is_marked_as_the_person_s_own() {
        let r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0))]),
            "learned_5",
        )
        .unwrap();
        assert_eq!(r.trust, ParseRuleTrust::UserCorrected);
        assert_eq!(r.provenance, "human_correction");
        assert_eq!(r.examples, vec![UNSEEN.to_string()]);
    }

    #[test]
    fn an_amount_that_is_not_in_the_text_synthesises_nothing() {
        // ★ A rule anchored on nothing would occupy precedence and match by
        //   accident. None is the honest answer.
        assert!(synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(999.0))]),
            "learned_6",
        )
        .is_none());
        // And so is a correction with no amount at all.
        assert!(synthesize_from_correction("kcb", UNSEEN, "budget.spend", &params(&[]), "x")
            .is_none());
    }

    #[test]
    fn a_candidate_that_would_capture_an_existing_rules_message_is_refused() {
        // ★★ The regression condition. A hand-made over-broad candidate that
        //    matches a shipped rule's own example must not be accepted.
        let shipped = seed_rules("mpesa");
        let example = shipped
            .iter()
            .find(|r| !r.examples.is_empty())
            .and_then(|r| r.examples.first().cloned())
            .expect("a shipped example");

        let mut greedy = shipped[0].clone();
        greedy.id = "greedy".into();
        greedy.pattern = "Confirmed".into();
        greedy.status = RuleStatus::ParsedUnmapped;
        greedy.operator = None;
        greedy.params.clear();
        greedy.extract.clear();
        greedy.examples = vec![];

        let err = verify_candidate(&greedy, "something else entirely Confirmed", shipped, &Universe)
            .unwrap_err();
        match err {
            LearningRefusal::Regression { example: e, .. } => assert_eq!(e, example),
            other => panic!("expected a regression refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_candidate_that_does_not_match_its_own_example_is_refused() {
        let mut r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0))]),
            "learned_7",
        )
        .unwrap();
        r.pattern = "something else".into();
        assert_eq!(
            verify_candidate(&r, UNSEEN, &[], &Universe).unwrap_err(),
            LearningRefusal::NoFidelity
        );
    }

    #[test]
    fn an_ill_typed_candidate_is_refused_before_it_is_ever_run() {
        // ★ Short-circuit: a broken pattern must not reach the fidelity check.
        let mut r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0))]),
            "learned_8",
        )
        .unwrap();
        r.pattern = "(unclosed".into();
        assert!(matches!(
            verify_candidate(&r, UNSEEN, &[], &Universe).unwrap_err(),
            LearningRefusal::NotWellTyped(_)
        ));
    }

    #[test]
    fn a_learned_income_rule_is_refused_where_the_operator_does_not_exist() {
        // ★ The typecheck is the caller's universe, not a global assumption.
        let r = synthesize_from_correction(
            "kcb",
            UNSEEN,
            "budget.record_income",
            &params(&[("amount", json!(2500.0))]),
            "learned_9",
        )
        .unwrap();
        assert!(matches!(
            verify_candidate(&r, UNSEEN, &[], &NoOperators).unwrap_err(),
            LearningRefusal::NotWellTyped(_)
        ));
    }

    #[test]
    fn amount_text_candidates_cover_how_a_real_message_writes_a_figure() {
        assert_eq!(amount_texts(2500.0), vec!["2,500.00", "2,500", "2500.00", "2500"]);
        assert_eq!(amount_texts(680.0), vec!["680.00", "680"]);
        assert_eq!(amount_texts(1234.56), vec!["1,234.56", "1234.56"]);
    }

    #[test]
    fn synthesis_is_reproducible() {
        let p = params(&[("amount", json!(2500.0))]);
        let a = synthesize_from_correction("kcb", UNSEEN, "budget.record_income", &p, "id");
        let b = synthesize_from_correction("kcb", UNSEEN, "budget.record_income", &p, "id");
        assert_eq!(a, b, "no clock, no randomness — the id is the caller's");
    }
}
