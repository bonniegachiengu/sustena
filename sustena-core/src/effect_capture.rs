//! **Effect-first capture** — `ε → (o, θ)`.
//!
//! A person says what *happened*, not what the system should *do*. This
//! resolves that into a real operator and real params, or asks **one**
//! question with real options.
//!
//! ```text
//! "spent 500 on WiFi"  →  budget.spend(pocket_name=WiFi, amount=500)
//! ```
//!
//! ★★★ **The user speaks in effects; the ENGINE supplies the model's
//! coordinates.** Nobody types a dot-path, a schema or an operator name. A
//! pocket is recovered by matching what the person said against **that
//! Sustain's own live pocket names**, and `θ` is built from the operator's
//! own declared parameter list. The person says a word; the engine resolves the
//! identity.
//!
//! ## ★★★ Deterministic, and never a dead end
//!
//! No LLM, no `eval`. One ordered pass — and the ordering is the design:
//!
//! 1. **Amount**: `known` → the message's own `parsed_fields.amount` → a plain
//!    figure in the narration → **a currency-prefixed figure in the raw
//!    message body**. That last fallback is what lets an *unparsed* capture
//!    still classify with a real amount instead of dead-ending.
//! 2. **Description**: best-effort, never blocks.
//! 3. **Operator narrowing**: a verb hint, else the message's own
//!    `direction` — a real ingest signal, not a guess.
//! 4. **Pocket**: `known` → exactly one live-state match → classification
//!    history, if that pocket still exists → else ask, offering the Sustain's
//!    **own** pocket names.
//! 5. **Amount still missing** → ask for it directly. ★ `options: None` means
//!    *render an input, not buttons* — an amount is not a tap.
//! 6. **More than one operator still viable** → ask which, with at most as
//!    many options as genuinely remain.
//! 7. Build `θ` from the operator's real signature; a still-missing required
//!    field is `CannotInfer` **naming it**.
//!
//! ★★ **A pocket is only asked for when a pocket is relevant.** Income has no
//! `pocket_name` at all, so asking before the direction is resolved would force
//! a person through an irrelevant question before they could say *this was
//! money in*.
//!
//! ## ★★ History pre-fills the tap, never the confirm
//!
//! A repeat merchant skips the *disambiguation*, and the result says so
//! ([`Inference::from_history`]) so a surface can label it and offer a change.
//! It still stops at `Ready`, requiring the same explicit confirmation every
//! capture requires. Nothing about the write path changes.

use std::collections::BTreeMap;

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Which operator a narrated verb points at, by the operator's last segment.
const VERBS: &[(&str, &str)] = &[
    ("spent", "spend"),
    ("paid", "spend"),
    ("bought", "spend"),
    ("spend", "spend"),
    ("allocate", "allocate"),
    ("allocated", "allocate"),
    ("set aside", "allocate"),
    ("put into", "allocate"),
    ("moved", "allocate"),
    ("budget for", "allocate"),
    ("received", "record_income"),
    ("credited", "record_income"),
    ("deposited", "record_income"),
    ("got paid", "record_income"),
    ("income", "record_income"),
    // A refund is money in, but it is not income: it gives back what was
    // spent rather than adding to what was earned.
    ("refund", "unspend"),
    ("refunded", "unspend"),
    ("reversed", "unspend"),
    ("came back", "unspend"),
];

/// The fields this pass can put a question about.
///
/// ★★ Anything else missing is a genuine dead end for that candidate, because
/// there is no question here that would fill it in.
const ASKABLE_FIELDS: &[&str] = &["pocket_name", "amount"];

/// Humanised labels, for the one question that asks *which action*.
fn label_for(operator: &str) -> &str {
    match operator {
        "budget.spend" => "spend it",
        "budget.allocate" => "set it aside",
        "budget.record_income" => "money received",
        "budget.unspend" => "money came back",
        "budget.unallocate" => "take it back out of a pocket",
        other => other,
    }
}

fn amount_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(\d[\d,]*(?:\.\d+)?)").unwrap())
}

/// ★ Deliberately **narrower** than [`extract_amount`]. Run against a full raw
/// SMS body, a bare digit-run match is a real hazard: dates, reference codes
/// and phone fragments are all digits, and the first one found would be wrong.
/// Every real bank message prefixes the transaction amount with `Ksh`/`KES`, so
/// anchoring on that is what makes this safe on unstructured text.
fn currency_amount_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)(?:Ksh|KES)\.?\s*([\d,]+(?:\.\d+)?)").unwrap())
}

fn to_f64(s: &str) -> Option<f64> {
    s.replace(',', "").parse::<f64>().ok()
}

/// A plain figure in a short narration.
pub fn extract_amount(text: &str) -> Option<f64> {
    amount_re().captures(text).and_then(|c| to_f64(c.get(1)?.as_str()))
}

/// A currency-prefixed figure in raw, unstructured text.
pub fn extract_currency_amount(text: &str) -> Option<f64> {
    currency_amount_re().captures(text).and_then(|c| to_f64(c.get(1)?.as_str()))
}

/// What this capture is *about* — the merchant, the counterparty, the words.
///
/// ★ Exposed separately because a caller needs the identical answer **before**
/// calling [`infer`], to look classification history up by it. Two different
/// resolutions would key history on something the inference never saw.
pub fn resolve_description(
    effect_text: Option<&str>,
    parsed_fields: &BTreeMap<String, serde_json::Value>,
    known: &BTreeMap<String, serde_json::Value>,
) -> Option<String> {
    if let Some(d) = known.get("description").and_then(|v| v.as_str()) {
        return Some(d.to_string());
    }
    for key in ["counterparty", "external_ref"] {
        if let Some(v) = parsed_fields.get(key).and_then(|v| v.as_str()) {
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    effect_text.filter(|t| !t.is_empty()).map(str::to_string)
}

/// ★★★ Which of this Sustain's **own** pockets are named in the text.
///
/// The concrete mechanism behind *θ is recovered from state, not typed*: the
/// person says a word that already happens to be one of their pocket names.
pub fn match_pocket_names(text: &str, pockets: &[String]) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let lowered = text.to_lowercase();
    pockets
        .iter()
        .filter(|p| !p.is_empty() && lowered.contains(&p.to_lowercase()))
        .cloned()
        .collect()
}

/// Which candidates a narrated verb points at. ★ Empty means **no hint** — a
/// caller must not read that as elimination.
pub fn narrow_by_verb(text: &str, candidates: &[String]) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let lowered = text.to_lowercase();
    let hinted: Vec<&str> = VERBS
        .iter()
        .filter(|(kw, _)| lowered.contains(kw))
        .map(|(_, suffix)| *suffix)
        .collect();
    if hinted.is_empty() {
        return Vec::new();
    }
    candidates
        .iter()
        .filter(|op| hinted.contains(&op.rsplit('.').next().unwrap_or("")))
        .cloned()
        .collect()
}

/// What an operator really declares. ★ A trait, not a registry dependency:
/// `infer` is about resolution, and which operators exist is the caller's to
/// know — the same seam [`crate::parse_rule::OperatorUniverse`] uses.
pub trait OperatorParams {
    /// Every parameter, and whether it is required (no default).
    fn params(&self, operator: &str) -> Vec<(String, bool)>;
}

/// A plain declared table, for a caller that has one.
#[derive(Debug, Clone, Default)]
pub struct DeclaredParams(pub BTreeMap<String, Vec<(String, bool)>>);

impl OperatorParams for DeclaredParams {
    fn params(&self, operator: &str) -> Vec<(String, bool)> {
        self.0.get(operator).cloned().unwrap_or_default()
    }
}

/// Which required params are still missing.
pub fn missing_required<P: OperatorParams>(
    universe: &P,
    operator: &str,
    facts: &BTreeMap<String, serde_json::Value>,
) -> Vec<String> {
    universe
        .params(operator)
        .into_iter()
        .filter(|(name, required)| *required && !facts.contains_key(name))
        .map(|(name, _)| name)
        .collect()
}

/// ★ An operator whose param names do not line up with the canonical fact keys
/// simply can never be satisfied here — the honest outcome, not a crash.
pub fn required_params_satisfiable<P: OperatorParams>(
    universe: &P,
    operator: &str,
    facts: &BTreeMap<String, serde_json::Value>,
) -> bool {
    let params = universe.params(operator);
    if params.is_empty() {
        return false;
    }
    missing_required(universe, operator, facts).is_empty()
}

/// `θ` — exactly the fields this operator's own signature declares, never more
/// and never invented.
pub fn build_params<P: OperatorParams>(
    universe: &P,
    operator: &str,
    facts: &BTreeMap<String, serde_json::Value>,
) -> BTreeMap<String, serde_json::Value> {
    universe
        .params(operator)
        .into_iter()
        .filter_map(|(name, _)| facts.get(&name).map(|v| (name, v.clone())))
        .collect()
}

/// One tappable answer. ★ Uniform shape, so a surface never special-cases
/// which field is being asked about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

impl Choice {
    fn of(value: &str) -> Self {
        Self { value: value.to_string(), label: value.to_string() }
    }
    fn labelled(value: &str, label: &str) -> Self {
        Self { value: value.to_string(), label: label.to_string() }
    }
}

/// What one inference pass concluded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Inference {
    /// A real operator and real params, awaiting a person's confirmation.
    Ready {
        operator: String,
        params: BTreeMap<String, serde_json::Value>,
        why: String,
        description: Option<String>,
        /// ★★ True when a repeat merchant pre-filled the pocket. Never silent:
        /// a surface labels it and offers a change, and it still stops here.
        from_history: bool,
        history_use_count: Option<u32>,
    },
    /// **One** question, with real options.
    ///
    /// ★ `options: None` means the answer is not a tap — render an input. The
    /// amount is the only field that takes that shape.
    NeedsDisambiguation {
        field: String,
        question: String,
        options: Option<Vec<Choice>>,
        why: String,
    },
    /// Nothing more can be resolved, and it says what is missing.
    CannotInfer { why: String },
}

impl Inference {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }
}

/// Everything one inference pass is given.
#[derive(Debug, Clone, Default)]
pub struct Capture<'a> {
    /// The operators this widget declares it can emit.
    pub candidates: &'a [String],
    /// The Sustain's own live pocket names.
    pub pockets: &'a [String],
    /// A narrated effect, if there was one.
    pub effect_text: Option<&'a str>,
    /// What `τ` understood, if this came from a message.
    pub parsed_fields: BTreeMap<String, serde_json::Value>,
    /// Facts a person has already answered.
    pub known: BTreeMap<String, serde_json::Value>,
    /// A previously-confirmed pocket for this merchant.
    pub history: Option<(String, u32)>,
    /// The full raw message body, for the last-resort amount recovery.
    pub raw_text: Option<&'a str>,
}

/// **`ε → (o, θ)`.** One deterministic pass — see the module docs for the
/// ordering and why each step is where it is.
pub fn infer<P: OperatorParams>(universe: &P, c: &Capture) -> Inference {
    let mut facts = c.known.clone();

    // ★ A human-typed amount arrives from a text input, so normalise rather
    //   than trusting the wire shape. An unparseable typed value is treated as
    //   not-provided (re-asks) rather than crashing downstream.
    if let Some(s) = facts.get("amount").and_then(|v| v.as_str()).map(str::to_string) {
        match to_f64(&s) {
            Some(n) => {
                facts.insert("amount".into(), serde_json::json!(n));
            }
            None => {
                facts.remove("amount");
            }
        }
    }

    // 1 · amount
    if !facts.contains_key("amount") {
        let amount = c
            .parsed_fields
            .get("amount")
            .and_then(|v| v.as_f64())
            .or_else(|| extract_amount(c.effect_text.unwrap_or("")))
            .or_else(|| extract_currency_amount(c.raw_text.unwrap_or("")));
        if let Some(a) = amount {
            facts.insert("amount".into(), serde_json::json!(a));
        }
    }

    // 2 · description
    if !facts.contains_key("description") {
        if let Some(d) = resolve_description(c.effect_text, &c.parsed_fields, &c.known) {
            facts.insert("description".into(), serde_json::json!(d));
        }
    }

    let mut from_history = false;
    let mut history_use_count: Option<u32> = None;

    // 3 · which operator
    let mut ops: Vec<String> = c.candidates.to_vec();
    if let Some(chosen) = facts.remove("operator").and_then(|v| v.as_str().map(str::to_string)) {
        // ★ A person already answered "what should this do?". Their choice wins
        //   outright — no further narrowing, and no re-deriving what they said.
        ops = if c.candidates.contains(&chosen) { vec![chosen] } else { vec![] };
    } else if ops.len() > 1 {
        let mut hinted = narrow_by_verb(c.effect_text.unwrap_or(""), &ops);
        if hinted.is_empty() {
            // A real ingest signal, not a guess: the money already left.
            //
            // ★★★ Money coming IN has two honest destinations, and calling
            // either one the other is a real misstatement. Income adds to what
            // was earned; a refund gives back what was spent and adds to no
            // total at all. Narrowing to both leaves the choice with the
            // person, which is the only one who can tell them apart.
            let direction = c.parsed_fields.get("direction").and_then(|v| v.as_str());
            let suffixes: &[&str] = match direction {
                Some("sent") => &["spend"],
                Some("received") => &["record_income", "unspend"],
                _ => &[],
            };
            if !suffixes.is_empty() {
                hinted = ops
                    .iter()
                    .filter(|op| {
                        op.rsplit('.').next().is_some_and(|tail| suffixes.contains(&tail))
                    })
                    .cloned()
                    .collect();
            }
        }
        // ★ A hint that eliminates EVERY candidate is no hint at all.
        if !hinted.is_empty() {
            ops = hinted;
        }
    }

    // 4 · pocket — only when a pocket is relevant to what remains
    //
    // ★★★ EVERY remaining candidate, not merely one of them. Where the set is
    // mixed -- money in could be income, which has no pocket, or a refund,
    // which has one -- asking for a pocket first is the irrelevant question
    // this module's own note forbids. The operator question comes first, and
    // answering it makes the pocket question either necessary or moot.
    let pocket_relevant = !ops.is_empty()
        && ops.iter().all(|op| universe.params(op).iter().any(|(n, _)| n == "pocket_name"));

    if pocket_relevant && !facts.contains_key("pocket_name") {
        let mut search = String::new();
        if let Some(t) = c.effect_text {
            search.push_str(t);
        }
        if let Some(d) = facts.get("description").and_then(|v| v.as_str()) {
            search.push(' ');
            search.push_str(d);
        }
        let matches = match_pocket_names(&search, c.pockets);
        match matches.len() {
            1 => {
                facts.insert("pocket_name".into(), serde_json::json!(matches[0]));
            }
            0 => {
                // ★★ History only counts if that pocket STILL EXISTS. A stale
                //    mapping to a renamed or deleted pocket is never followed.
                let hist = c
                    .history
                    .as_ref()
                    .filter(|(p, _)| c.pockets.contains(p));
                match hist {
                    Some((p, n)) => {
                        facts.insert("pocket_name".into(), serde_json::json!(p));
                        from_history = true;
                        history_use_count = Some(*n);
                    }
                    None => {
                        // ★ Zero pockets is NOT a dead end: a surface offers a
                        //   "new pocket" affordance over an empty list, which
                        //   creates one through the real gated operator and
                        //   continues into it.
                        let why = if c.pockets.is_empty() {
                            "no pockets exist yet — create one to get started"
                        } else {
                            "couldn't find a pocket name in what you described"
                        };
                        return Inference::NeedsDisambiguation {
                            field: "pocket_name".into(),
                            question: "which pocket does this belong to?".into(),
                            options: Some(c.pockets.iter().map(|p| Choice::of(p)).collect()),
                            why: why.into(),
                        };
                    }
                }
            }
            _ => {
                return Inference::NeedsDisambiguation {
                    field: "pocket_name".into(),
                    question: "which pocket did you mean?".into(),
                    options: Some(matches.iter().map(|p| Choice::of(p)).collect()),
                    why: format!("more than one of your pockets matched: {}", matches.join(", ")),
                }
            }
        }
    }

    // 5 · amount, genuinely last resort
    if !facts.contains_key("amount") {
        return Inference::NeedsDisambiguation {
            field: "amount".into(),
            question: "how much was this for?".into(),
            // ★ Not a tap. A surface renders an input for this one field.
            options: None,
            why: "couldn't find an amount in the message — enter it to continue".into(),
        };
    }

    // 6 · which action, if more than one still fits
    if ops.len() > 1 {
        // ★★★ A candidate is only out of the running if what it lacks can
        // never be asked for. Dropping one merely because a question has not
        // been PUT yet is how a refund became income: `record_income` needs
        // only an amount, `budget.unspend` also needs a pocket, and with the
        // pocket question still ahead of it the refund quietly lost. Missing an
        // askable field means "ask", never "eliminate".
        let satisfiable: Vec<String> = ops
            .iter()
            .filter(|op| {
                missing_required(universe, op, &facts)
                    .iter()
                    .all(|f| ASKABLE_FIELDS.contains(&f.as_str()))
            })
            .cloned()
            .collect();
        if satisfiable.len() == 1 {
            ops = satisfiable;
        } else {
            let offer = if satisfiable.is_empty() { ops.clone() } else { satisfiable };
            return Inference::NeedsDisambiguation {
                field: "operator".into(),
                question: "what should this do?".into(),
                options: Some(
                    offer.iter().map(|op| Choice::labelled(op, label_for(op))).collect(),
                ),
                why: "more than one kind of action would fit this effect".into(),
            };
        }
    }

    let Some(operator) = ops.first().cloned() else {
        return Inference::CannotInfer {
            why: "no candidate operator fits this effect".into(),
        };
    };

    // 7 · θ
    if !required_params_satisfiable(universe, &operator, &facts) {
        return Inference::CannotInfer {
            why: format!(
                "missing required detail: {}",
                missing_required(universe, &operator, &facts).join(", ")
            ),
        };
    }

    let params = build_params(universe, &operator, &facts);
    let described = params
        .iter()
        .map(|(k, v)| format!("{k}={}", render(v)))
        .collect::<Vec<_>>()
        .join(", ");
    let why = if from_history {
        let count = history_use_count
            .map(|n| format!("{n}x before"))
            .unwrap_or_else(|| "before".into());
        format!("usual pocket ({count}) — inferred {operator}({described})")
    } else {
        format!("inferred {operator}({described}) from what you described")
    };

    Inference::Ready {
        operator,
        params,
        why,
        description: facts.get("description").and_then(|v| v.as_str()).map(str::to_string),
        from_history,
        history_use_count: if from_history { history_use_count } else { None },
    }
}

fn render(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn universe() -> DeclaredParams {
        let mut m = BTreeMap::new();
        m.insert(
            "budget.spend".to_string(),
            vec![
                ("pocket_name".to_string(), true),
                ("amount".to_string(), true),
                ("description".to_string(), false),
            ],
        );
        m.insert(
            "budget.allocate".to_string(),
            vec![("pocket_name".to_string(), true), ("amount".to_string(), true)],
        );
        m.insert(
            "budget.unspend".to_string(),
            vec![
                ("pocket_name".to_string(), true),
                ("amount".to_string(), true),
                ("payer".to_string(), false),
            ],
        );
        m.insert(
            "budget.record_income".to_string(),
            vec![
                ("amount".to_string(), true),
                ("source".to_string(), false),
                ("description".to_string(), false),
            ],
        );
        DeclaredParams(m)
    }

    fn pockets() -> Vec<String> {
        ["food", "rent", "WiFi", "SHA"].iter().map(|s| s.to_string()).collect()
    }

    fn candidates() -> Vec<String> {
        ["budget.spend", "budget.allocate"].iter().map(|s| s.to_string()).collect()
    }

    fn capture<'a>(text: &'a str, ops: &'a [String], p: &'a [String]) -> Capture<'a> {
        Capture {
            candidates: ops,
            pockets: p,
            effect_text: Some(text),
            ..Default::default()
        }
    }

    #[test]
    fn a_narrated_effect_resolves_with_no_questions() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let r = infer(&u, &capture("spent 500 on WiFi", &ops, &p));
        match r {
            Inference::Ready { operator, params, .. } => {
                assert_eq!(operator, "budget.spend");
                assert_eq!(params["pocket_name"], json!("WiFi"));
                assert_eq!(params["amount"], json!(500.0));
            }
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn the_pocket_comes_from_live_state_never_from_the_person_typing_a_path() {
        assert_eq!(match_pocket_names("paid SHA HOSPITAL 1200", &pockets()), vec!["SHA"]);
        assert!(match_pocket_names("paid NAIVAS", &pockets()).is_empty());
    }

    #[test]
    fn an_unrecognised_merchant_asks_once_with_the_real_pocket_names() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let mut c = capture("paid NAIVAS SUPERMARKET", &ops, &p);
        c.parsed_fields.insert("amount".into(), json!(680.0));
        match infer(&u, &c) {
            Inference::NeedsDisambiguation { field, options, .. } => {
                assert_eq!(field, "pocket_name");
                let opts = options.expect("real options");
                assert_eq!(opts.len(), 4);
                assert!(opts.iter().any(|o| o.value == "food"));
            }
            other => panic!("expected a question, got {other:?}"),
        }
    }

    #[test]
    fn answering_that_question_resolves_it() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let mut c = capture("paid NAIVAS SUPERMARKET", &ops, &p);
        c.parsed_fields.insert("amount".into(), json!(680.0));
        c.parsed_fields.insert("counterparty".into(), json!("NAIVAS SUPERMARKET"));
        c.parsed_fields.insert("direction".into(), json!("sent"));
        c.known.insert("pocket_name".into(), json!("food"));
        match infer(&u, &c) {
            Inference::Ready { operator, params, description, .. } => {
                assert_eq!(operator, "budget.spend");
                assert_eq!(params["amount"], json!(680.0));
                assert_eq!(params["pocket_name"], json!("food"));
                // ★ Never typed by the person — recovered from the message.
                assert_eq!(description.as_deref(), Some("NAIVAS SUPERMARKET"));
            }
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn ambiguity_between_two_pockets_offers_exactly_those_two() {
        // ★ "foodstuff" contains "food", so a message naming the longer one
        //   genuinely matches both — the real ambiguity, not a contrived one.
        let p: Vec<String> = ["food", "foodstuff"].iter().map(|s| s.to_string()).collect();
        let (u, ops) = (universe(), candidates());
        let mut c = capture("spent 100 on foodstuff", &ops, &p);
        c.parsed_fields.insert("direction".into(), json!("sent"));
        match infer(&u, &c) {
            Inference::NeedsDisambiguation { options, .. } => {
                let opts = options.expect("options");
                assert_eq!(opts.len(), 2, "at most as many as genuinely remain");
            }
            other => panic!("expected a question, got {other:?}"),
        }
    }

    #[test]
    fn an_unparsed_message_still_recovers_its_amount_from_the_raw_text() {
        // ★★★ The dead-end fix. An UNPARSED capture has no parsed_fields at
        //     all, by definition — so there is no direction to narrow on and
        //     the remaining question is "what should this do?". The point is
        //     that it is NOT "no amount could be recovered": the figure came
        //     out of the raw body, and one tap finishes it.
        let (u, ops, p) = (universe(), candidates(), pockets());
        let raw = "Some shape no rule knows. Ksh1,250.00 paid to SOMEWHERE on 1/8/26.";
        let mut c = Capture {
            candidates: &ops,
            pockets: &p,
            effect_text: None,
            raw_text: Some(raw),
            known: [("pocket_name".to_string(), json!("food"))].into_iter().collect(),
            ..Default::default()
        };
        match infer(&u, &c) {
            Inference::NeedsDisambiguation { ref field, .. } => {
                assert_eq!(field, "operator", "the amount is NOT what is missing");
            }
            other => panic!("expected the action question, got {other:?}"),
        }

        // Answer it, and the recovered amount is there.
        c.known.insert("operator".into(), json!("budget.spend"));
        match infer(&u, &c) {
            Inference::Ready { params, .. } => assert_eq!(params["amount"], json!(1250.0)),
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn the_raw_text_fallback_ignores_dates_and_reference_codes() {
        // ★ The narrow regex's whole reason: a bare digit match would take the
        //   reference code or the date.
        let raw = "TGH4A2B9CD Confirmed on 1/8/26 at 11:15 AM. Ksh680.00 paid to NAIVAS.";
        assert_eq!(extract_currency_amount(raw), Some(680.0));
        // The broad one would have taken the first digits it found.
        assert_ne!(extract_amount(raw), Some(680.0));
    }

    #[test]
    fn no_amount_anywhere_asks_for_one_and_is_not_a_dead_end() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let mut c = capture("bought something at the shop", &ops, &p);
        c.known.insert("pocket_name".into(), json!("food"));
        match infer(&u, &c) {
            Inference::NeedsDisambiguation { field, options, .. } => {
                assert_eq!(field, "amount");
                // ★ An input, not buttons.
                assert!(options.is_none());
            }
            other => panic!("expected an amount question, got {other:?}"),
        }
    }

    #[test]
    fn a_typed_amount_arrives_as_a_string_and_is_normalised() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let mut c = capture("bought something", &ops, &p);
        c.known.insert("pocket_name".into(), json!("food"));
        c.known.insert("amount".into(), json!("1,450"));
        match infer(&u, &c) {
            Inference::Ready { params, .. } => assert_eq!(params["amount"], json!(1450.0)),
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn an_unparseable_typed_amount_re_asks_rather_than_crashing() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let mut c = capture("bought something", &ops, &p);
        c.known.insert("pocket_name".into(), json!("food"));
        c.known.insert("amount".into(), json!("not a number"));
        assert!(matches!(
            infer(&u, &c),
            Inference::NeedsDisambiguation { ref field, .. } if field == "amount"
        ));
    }

    /// The classify card's real emits once a refund could be answered at all.
    fn candidates_with_refund() -> Vec<String> {
        ["budget.spend", "budget.allocate", "budget.unspend"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn a_refund_is_never_narrowed_to_a_spend() {
        // ★★★ The bug this closes. A reversal arrives in the same queue as a
        //     purchase, and with only spend and allocate to choose from the
        //     only available answer counted the original charge twice.
        let (u, ops, p) = (universe(), candidates_with_refund(), pockets());
        let mut c = Capture { candidates: &ops, pockets: &p, ..Default::default() };
        c.parsed_fields.insert("amount".into(), json!(500.0));
        c.parsed_fields.insert("direction".into(), json!("received"));
        c.known.insert("pocket_name".into(), json!("food"));
        match infer(&u, &c) {
            Inference::Ready { operator, .. } => assert_eq!(operator, "budget.unspend"),
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn money_in_asks_which_kind_when_both_are_possible() {
        // ★★ Income and a refund are both money in and mean different things:
        //    one adds to what was earned, the other gives back what was spent.
        //    Where both are on offer the person is asked rather than guessed at.
        let u = universe();
        let ops: Vec<String> = ["budget.spend", "budget.record_income", "budget.unspend"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let p = pockets();
        let mut c = Capture { candidates: &ops, pockets: &p, ..Default::default() };
        c.parsed_fields.insert("amount".into(), json!(500.0));
        c.parsed_fields.insert("direction".into(), json!("received"));
        match infer(&u, &c) {
            Inference::NeedsDisambiguation { field, options, .. } => {
                assert_eq!(field, "operator");
                let options = options.expect("a choice between operators is a tap, not an input");
                let vs: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
                assert!(vs.contains(&"budget.record_income"));
                assert!(vs.contains(&"budget.unspend"));
                assert!(!vs.contains(&"budget.spend"), "money in is never a spend");
            }
            other => panic!("expected a question, got {other:?}"),
        }
    }

    #[test]
    fn the_word_refund_alone_is_enough_to_mean_money_came_back() {
        let (u, ops, p) = (universe(), candidates_with_refund(), pockets());
        let mut c = capture("refund of 500 into food", &ops, &p);
        c.known.insert("pocket_name".into(), json!("food"));
        match infer(&u, &c) {
            Inference::Ready { operator, .. } => assert_eq!(operator, "budget.unspend"),
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn direction_sent_narrows_to_a_spend_without_asking() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let mut c = Capture { candidates: &ops, pockets: &p, ..Default::default() };
        c.parsed_fields.insert("amount".into(), json!(300.0));
        c.parsed_fields.insert("direction".into(), json!("sent"));
        c.known.insert("pocket_name".into(), json!("food"));
        match infer(&u, &c) {
            Inference::Ready { operator, .. } => assert_eq!(operator, "budget.spend"),
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn a_pocket_is_never_asked_for_when_income_is_the_answer() {
        // ★★ The real UX flaw this closes: a received message must not be
        //    forced through a pocket question it has no use for.
        let u = universe();
        let ops: Vec<String> = ["budget.spend", "budget.record_income"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let p = pockets();
        let mut c = Capture { candidates: &ops, pockets: &p, ..Default::default() };
        c.parsed_fields.insert("amount".into(), json!(2000.0));
        c.known.insert("operator".into(), json!("budget.record_income"));
        match infer(&u, &c) {
            Inference::Ready { operator, params, .. } => {
                assert_eq!(operator, "budget.record_income");
                assert!(!params.contains_key("pocket_name"));
            }
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn an_explicit_prior_answer_wins_outright() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let mut c = capture("spent 500 on WiFi", &ops, &p);
        // The verb says spend; the person said allocate. The person wins.
        c.known.insert("operator".into(), json!("budget.allocate"));
        match infer(&u, &c) {
            Inference::Ready { operator, .. } => assert_eq!(operator, "budget.allocate"),
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn history_prefills_the_tap_but_still_stops_at_ready() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let mut c = capture("paid NAIVAS", &ops, &p);
        c.parsed_fields.insert("amount".into(), json!(680.0));
        c.parsed_fields.insert("direction".into(), json!("sent"));
        c.history = Some(("food".to_string(), 3));
        match infer(&u, &c) {
            Inference::Ready { params, from_history, history_use_count, why, .. } => {
                assert_eq!(params["pocket_name"], json!("food"));
                assert!(from_history, "a pre-fill is never silent");
                assert_eq!(history_use_count, Some(3));
                assert!(why.contains("3x before"), "{why}");
            }
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn a_stale_history_pocket_is_never_followed() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let mut c = capture("paid NAIVAS", &ops, &p);
        c.parsed_fields.insert("amount".into(), json!(680.0));
        c.parsed_fields.insert("direction".into(), json!("sent"));
        // A pocket that has since been renamed or deleted.
        c.history = Some(("groceries".to_string(), 9));
        assert!(matches!(
            infer(&u, &c),
            Inference::NeedsDisambiguation { ref field, .. } if field == "pocket_name"
        ));
    }

    #[test]
    fn a_text_match_beats_history() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let mut c = capture("spent 200 on rent", &ops, &p);
        c.history = Some(("food".to_string(), 9));
        match infer(&u, &c) {
            Inference::Ready { params, from_history, .. } => {
                assert_eq!(params["pocket_name"], json!("rent"));
                assert!(!from_history, "what was said beats what was remembered");
            }
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn theta_carries_exactly_what_the_operator_declares_and_no_more() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let mut c = capture("spent 500 on WiFi", &ops, &p);
        c.parsed_fields.insert("phone".into(), json!("254712345678"));
        c.parsed_fields.insert("balance_after".into(), json!(9000.0));
        match infer(&u, &c) {
            Inference::Ready { params, .. } => {
                let keys: Vec<&String> = params.keys().collect();
                assert!(keys.iter().all(|k| ["pocket_name", "amount", "description"].contains(&k.as_str())),
                    "invented a param: {keys:?}");
            }
            other => panic!("expected ready, got {other:?}"),
        }
    }

    #[test]
    fn an_operator_with_unknown_params_cannot_be_satisfied_which_is_honest() {
        let u = universe();
        let ops = vec!["nobody.knows".to_string()];
        let p = pockets();
        let mut c = Capture { candidates: &ops, pockets: &p, ..Default::default() };
        c.known.insert("amount".into(), json!(5.0));
        assert!(matches!(infer(&u, &c), Inference::CannotInfer { .. }));
    }

    #[test]
    fn zero_pockets_asks_rather_than_dead_ending() {
        let (u, ops) = (universe(), candidates());
        let empty: Vec<String> = vec![];
        let mut c = capture("bought bread", &ops, &empty);
        c.parsed_fields.insert("amount".into(), json!(50.0));
        match infer(&u, &c) {
            Inference::NeedsDisambiguation { field, why, options, .. } => {
                assert_eq!(field, "pocket_name");
                assert!(why.contains("no pockets exist yet"), "{why}");
                assert_eq!(options.unwrap().len(), 0, "an empty list, not an absence");
            }
            other => panic!("expected a question, got {other:?}"),
        }
    }

    #[test]
    fn inference_is_reproducible() {
        let (u, ops, p) = (universe(), candidates(), pockets());
        let c = capture("spent 500 on WiFi", &ops, &p);
        assert_eq!(infer(&u, &c), infer(&u, &c));
    }
}
