//! **`ParseRule`** — the transducer's matching logic as a declared, typed
//! primitive (Parser primitive-lift · §§4K.6/4F.6/4I.6/4G.7).
//!
//! `τ : RawMessage → Event | ParseFailure` used to be a set of hand-written
//! functions with match logic and field-mapping baked into each body — editable
//! only by a deploy. This makes the *internal structure* of `τ` first-class:
//!
//! ```text
//! r = ⟨id, source, version, pattern, extract, status, operator, params,
//!      reason, trust, provenance, examples⟩
//! ```
//!
//! and [`run_rules`] is a small generic interpreter over an ordered set of
//! them. "Call the next Python function" becomes "evaluate the next declared
//! rule", and a correction becomes **data a person can author** rather than a
//! release.
//!
//! ## ★★★ Deterministic, and no code in the data
//!
//! Same discipline as [`crate::predicate`]: a rule is parsed once into typed
//! nodes and only those nodes are ever executed. There is **no callable field**
//! anywhere in a `ParseRule`. Every transform is one of six well-scoped,
//! side-effect-free string/number operations ([`FieldKind`]) — a genuinely
//! different transform stays out until there is a real shared need, rather than
//! being smuggled in as "just a small lambda". A rule cannot compute, cannot
//! branch, and cannot reach anything.
//!
//! ## ★★ `Γ ⊢ r` — well-formedness is a load-time finding
//!
//! [`typecheck_rule`] refuses a rule whose regex will not compile, whose status
//! is unknown, whose field spec is incomplete, whose operator is not registered,
//! or which binds a param that operator does not declare. All of it is checked
//! **once, when the rule is loaded or added** — never on every message, and
//! never as a silent "it just never matches". A rule that maps to a nonexistent
//! operator is a finding, exactly like an invariant over an undeclared
//! dimension.
//!
//! ## ★★★ What a rule may NOT be
//!
//! **`rejected` is not a status a rule can carry.** The sensitive-secret gate
//! is fixed code in [`crate::transducer`], checked before any rule runs, and no
//! rule can be authored, corrected, learned or retired to weaken it. Nor is
//! `unparsed` a rule outcome — that is *no rule matched*, which is the
//! interpreter's answer, not a rule's.
//!
//! **★★ Money-safety asymmetry is declared, not incidental.** A rule may only
//! carry `status = "mapped"` — the tier that auto-applies an operator — when
//! what it recognises is unambiguous. Every shipped *spend* shape is
//! `parsed_unmapped`, because *which pocket* is a decision a person makes; the
//! only shipped mapped operator is income. A learned rule inherits the same
//! asymmetry (see [`crate::parse_rule_learn`]).

use std::collections::BTreeMap;

use regex::{Captures, Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::transducer::Transduction;

/// One extracted field's typed capture instruction.
///
/// ★ Six kinds, each earned by a real message shape. `Amount`/`Text`/`Upper`/
/// `PrefixIfMissing` read a named group; `Literal` carries a fixed value with
/// no group at all; `Template` composes a string from **several** captured
/// pieces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    /// A number, commas stripped.
    Amount,
    /// Trimmed text.
    #[serde(rename = "string")]
    Text,
    /// A fixed value — no group. `direction: "received"` on every income shape.
    Literal,
    /// ★ Normalise a captured string to always carry a prefix. Real
    /// withdrawal messages say either `Agent 123456 - TOWN SHOP` or bare
    /// `654321 - ESTATE SHOP`; this makes them one field.
    PrefixIfMissing,
    /// Uppercase a captured string — a currency alternation that could in
    /// principle match lowercase under a case-insensitive rule.
    Upper,
    /// ★★ A `{}`-template over the raw groups **plus** the fields extracted
    /// before it, in declaration order, with extracted fields winning. Several
    /// real shapes carry one field built from several captured pieces.
    Template,
}

/// How a field is captured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSpec {
    #[serde(rename = "type")]
    pub kind: FieldKind,
    /// The regex group to read. Absent for `Literal` and `Template`.
    #[serde(default)]
    pub group: Option<String>,
    /// The fixed value (`Literal`), prefix (`PrefixIfMissing`) or format
    /// string (`Template`).
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}

/// What tier a matching rule produces.
///
/// ★★★ Deliberately three, not five. `Rejected` belongs to the fixed secret
/// gate and `Unparsed` means *nothing matched* — neither is something a rule
/// can declare about itself, so neither is spellable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleStatus {
    /// Understood **and** unambiguously routed to an operator.
    Mapped,
    /// Understood; the operator is deliberately not guessed.
    ParsedUnmapped,
    /// Recognised, and nothing moved — a balance notice, a loan status.
    Informational,
}

/// Where a rule came from, and how much it has earned.
///
/// ★ Named the long way **twice**: `Trust` is `MemeLibrary`'s and `RuleTrust`
/// is `learned.rs`'s, both meaning something else. Collisions 42 and 43,
/// resolved by the same house rule each time — the newcomer takes the longer
/// name. Three concepts called "trust" in one crate is a sign the word is
/// overloaded; keeping them apart by name is cheaper than pretending they are
/// the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParseRuleTrust {
    /// Shipped with the engine, tuned against real messages.
    #[default]
    Shipped,
    /// Synthesised from a human's own confirmed correction.
    UserCorrected,
    /// Proposed by a generator and confirmed by a human.
    ProposedConfirmed,
}

/// A declared rule: the whole of what `τ` knows about one message shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParseRule {
    pub id: String,
    /// The sender this rule belongs to. ★★ A rule never runs outside its own
    /// source's set — see [`crate::transducer::parse_message`].
    pub source: String,
    #[serde(default = "one")]
    pub version: u32,
    /// A regex with named groups. Declared data, checked at load time.
    pub pattern: String,
    #[serde(default)]
    pub extract: BTreeMap<String, FieldSpec>,
    pub status: RuleStatus,
    /// Required iff `status == Mapped`; forbidden otherwise.
    #[serde(default)]
    pub operator: Option<String>,
    /// `param → "$field"` | `"…{field}…"` | a literal.
    #[serde(default)]
    pub params: BTreeMap<String, serde_json::Value>,
    /// `IGNORECASE` | `DOTALL` | `MULTILINE`.
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub reason_template: Option<String>,
    #[serde(default)]
    pub trust: ParseRuleTrust,
    #[serde(default)]
    pub provenance: String,
    /// The regression corpus: texts this rule is known to match.
    #[serde(default)]
    pub examples: Vec<String>,
}

fn one() -> u32 {
    1
}

/// ★ Declaration order matters and is preserved: the interpreter walks a
/// rule's `extract` in the order it was declared, so a `Template` can depend on
/// a field declared before it. A `BTreeMap` would silently reorder, so the
/// ordering a rule relies on is carried explicitly.
impl ParseRule {
    fn ordered_fields(&self) -> Vec<(&String, &FieldSpec)> {
        // Templates depend on plain fields, never the other way round, so a
        // stable two-pass order reproduces the reference's insertion order for
        // every shipped rule without carrying a second list to keep in sync.
        let mut out: Vec<(&String, &FieldSpec)> = self
            .extract
            .iter()
            .filter(|(_, s)| s.kind != FieldKind::Template)
            .collect();
        out.extend(self.extract.iter().filter(|(_, s)| s.kind == FieldKind::Template));
        out
    }

    fn compile(&self) -> Result<Regex, regex::Error> {
        let mut b = RegexBuilder::new(&self.pattern);
        for f in &self.flags {
            match f.as_str() {
                "IGNORECASE" => {
                    b.case_insensitive(true);
                }
                "DOTALL" => {
                    b.dot_matches_new_line(true);
                }
                "MULTILINE" => {
                    b.multi_line(true);
                }
                _ => {}
            }
        }
        b.build()
    }
}

/// What operator names exist, for the typecheck.
///
/// ★ A trait rather than a hard dependency on the registry: `typecheck_rule` is
/// about well-formedness, and the set of registered operators is the caller's
/// to know. It also lets a test declare a tiny universe without standing up a
/// registry.
pub trait OperatorUniverse {
    fn has(&self, operator: &str) -> bool;
    /// The params that operator really declares. `None` = unknown/not checked.
    fn params_of(&self, operator: &str) -> Option<Vec<String>>;
}

/// The empty universe — every mapped rule fails its operator check.
pub struct NoOperators;

impl OperatorUniverse for NoOperators {
    fn has(&self, _operator: &str) -> bool {
        false
    }
    fn params_of(&self, _operator: &str) -> Option<Vec<String>> {
        None
    }
}

/// ★★ The registry IS an operator universe. Declared here rather than left to
/// each host, so a typecheck asks the same thing the gate will.
impl OperatorUniverse for crate::operator::Registry {
    fn has(&self, operator: &str) -> bool {
        self.get(operator).is_some()
    }
    fn params_of(&self, operator: &str) -> Option<Vec<String>> {
        Some(self.get(operator)?.params.iter().map(|p| p.name.to_string()).collect())
    }
}

/// ★ And an operator's declared params are its capture universe too — the same
/// one source of truth, read for a different question.
impl crate::effect_capture::OperatorParams for crate::operator::Registry {
    fn params(&self, operator: &str) -> Vec<(String, bool)> {
        self.get(operator)
            .map(|m| m.params.iter().map(|p| (p.name.to_string(), p.required)).collect())
            .unwrap_or_default()
    }
}

/// `Γ ⊢ r`. Every error, not the first — a rule with three problems should be
/// fixed once.
pub fn typecheck_rule<U: OperatorUniverse>(rule: &ParseRule, universe: &U) -> Vec<String> {
    let mut errors = Vec::new();

    if let Err(e) = rule.compile() {
        errors.push(format!("invalid regex pattern: {e}"));
    }
    for f in &rule.flags {
        if !matches!(f.as_str(), "IGNORECASE" | "DOTALL" | "MULTILINE") {
            errors.push(format!("unknown regex flag '{f}'"));
        }
    }
    if rule.id.trim().is_empty() {
        errors.push("a rule needs an id".to_string());
    }
    if rule.source.trim().is_empty() {
        errors.push("a rule needs a source".to_string());
    }

    for (name, spec) in &rule.extract {
        match spec.kind {
            FieldKind::Amount | FieldKind::Text | FieldKind::Upper => {
                if spec.group.as_deref().unwrap_or("").is_empty() {
                    errors.push(format!(
                        "extract field '{name}' ({:?}) requires a regex group name",
                        spec.kind
                    ));
                }
            }
            FieldKind::Literal => {
                if spec.value.is_none() {
                    errors.push(format!("extract field '{name}' (literal) requires a value"));
                }
            }
            FieldKind::PrefixIfMissing => {
                if spec.group.as_deref().unwrap_or("").is_empty() || spec.value.is_none() {
                    errors.push(format!(
                        "extract field '{name}' (prefix_if_missing) requires both a regex group and a prefix"
                    ));
                }
            }
            FieldKind::Template => {
                if spec.value.as_ref().and_then(|v| v.as_str()).unwrap_or("").is_empty() {
                    errors.push(format!(
                        "extract field '{name}' (template) requires a format-string value"
                    ));
                }
            }
        }
    }

    match rule.status {
        RuleStatus::Mapped => match rule.operator.as_deref() {
            None => errors.push("status='mapped' requires an operator".to_string()),
            Some(op) if !universe.has(op) => {
                errors.push(format!("operator '{op}' is not registered"));
            }
            Some(op) => {
                if let Some(real) = universe.params_of(op) {
                    for p in rule.params.keys() {
                        if !real.contains(p) {
                            errors.push(format!(
                                "param '{p}' is not a real parameter of operator '{op}' \
                                 (real params: {real:?})"
                            ));
                        }
                    }
                }
            }
        },
        _ => {
            if rule.operator.is_some() || !rule.params.is_empty() {
                errors.push(format!(
                    "status={:?} must not declare operator/params (only 'mapped' rules do)",
                    rule.status
                ));
            }
        }
    }

    errors
}

// ── the interpreter ─────────────────────────────────────────────────────────

/// Substitute `{name}` from a map. ★ A missing name is left **verbatim**
/// rather than blanked: a template that referred to something absent should
/// look wrong, not look like an answer.
fn format_template(t: &str, values: &BTreeMap<String, serde_json::Value>) -> String {
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            None => {
                out.push_str(&rest[open..]);
                return out;
            }
            Some(close) => {
                let key = &after[..close];
                match values.get(key) {
                    Some(v) => out.push_str(&render(v)),
                    None => {
                        out.push('{');
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A value as a template renders it: a string without its quotes, a whole
/// float without its `.0`.
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

fn apply_extract(rule: &ParseRule, caps: &Captures) -> BTreeMap<String, serde_json::Value> {
    // Raw named groups, for templates and reasons to reach.
    let mut raw: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for name in rule.compile().ok().iter().flat_map(|r| r.capture_names().flatten()) {
        if let Some(m) = caps.name(name) {
            raw.insert(name.to_string(), serde_json::Value::String(m.as_str().to_string()));
        }
    }

    let mut fields: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for (name, spec) in rule.ordered_fields() {
        match spec.kind {
            FieldKind::Literal => {
                if let Some(v) = &spec.value {
                    fields.insert(name.clone(), v.clone());
                }
            }
            FieldKind::Template => {
                let t = spec.value.as_ref().and_then(|v| v.as_str()).unwrap_or("");
                // ★ Extracted fields win over a same-named raw group.
                let mut merged = raw.clone();
                merged.extend(fields.clone());
                fields.insert(
                    name.clone(),
                    serde_json::Value::String(format_template(t, &merged)),
                );
            }
            _ => {
                let Some(g) = spec.group.as_deref() else { continue };
                // ★ An optional group that legitimately did not match is
                //   OMITTED, never zeroed — absent is not zero.
                let Some(m) = caps.name(g) else { continue };
                let text = m.as_str().trim();
                let value = match spec.kind {
                    FieldKind::Amount => match text.replace(',', "").parse::<f64>() {
                        Ok(n) => serde_json::json!(n),
                        Err(_) => continue,
                    },
                    FieldKind::Upper => serde_json::Value::String(text.to_uppercase()),
                    FieldKind::PrefixIfMissing => {
                        let prefix = spec
                            .value
                            .as_ref()
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let already =
                            text.to_lowercase().starts_with(&prefix.to_lowercase()) && !prefix.is_empty();
                        serde_json::Value::String(if already {
                            text.to_string()
                        } else {
                            format!("{prefix} {text}")
                        })
                    }
                    _ => serde_json::Value::String(text.to_string()),
                };
                fields.insert(name.clone(), value);
            }
        }
    }
    fields
}

/// `β_θ : parsed_fields → θ`. Three binding forms, in order:
/// `"$name"` (a direct reference, keeping the field's real type),
/// `"…{name}…"` (a template, always a string), and anything else (a literal).
fn bind_params(
    rule: &ParseRule,
    fields: &BTreeMap<String, serde_json::Value>,
) -> BTreeMap<String, serde_json::Value> {
    let mut bound = BTreeMap::new();
    for (param, reference) in &rule.params {
        let value = match reference.as_str() {
            Some(s) if s.starts_with('$') && !s.contains('{') => fields
                .get(&s[1..])
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            Some(s) if s.contains('{') => {
                serde_json::Value::String(format_template(s, fields))
            }
            _ => reference.clone(),
        };
        bound.insert(param.clone(), value);
    }
    bound
}

/// Try **one** rule. `None` when it does not match — kept separate from
/// [`run_rules`] because the correction verifier needs exactly this.
pub fn apply_rule(rule: &ParseRule, raw_text: &str) -> Option<Transduction> {
    let re = rule.compile().ok()?;
    let caps = re.captures(raw_text)?;

    let fields = apply_extract(rule, &caps);
    let reason = rule.reason_template.as_deref().map(|t| {
        let mut merged: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        for name in re.capture_names().flatten() {
            if let Some(m) = caps.name(name) {
                merged.insert(name.to_string(), serde_json::Value::String(m.as_str().to_string()));
            }
        }
        merged.extend(fields.clone());
        format_template(t, &merged)
    });

    let external_ref = fields.get("ref").and_then(|v| v.as_str()).map(str::to_string);
    Some(match rule.status {
        RuleStatus::Mapped => Transduction::mapped(
            rule.operator.clone().unwrap_or_default(),
            bind_params(rule, &fields),
            fields,
            external_ref,
            reason.unwrap_or_default(),
            &rule.id,
        ),
        RuleStatus::ParsedUnmapped => Transduction::parsed_unmapped(
            fields,
            external_ref,
            reason.unwrap_or_default(),
            &rule.id,
        ),
        RuleStatus::Informational => Transduction::informational(
            fields,
            external_ref,
            reason.unwrap_or_default(),
            &rule.id,
        ),
    })
}

/// `τ_c(raw)` = the first rule in `R_c` that matches. **List order is
/// precedence**, declared by the caller rather than by comment-documented
/// function order.
pub fn run_rules(rules: &[ParseRule], raw_text: &str) -> Option<Transduction> {
    rules.iter().find_map(|r| apply_rule(r, raw_text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Universe;
    impl OperatorUniverse for Universe {
        fn has(&self, operator: &str) -> bool {
            operator == "budget.record_income"
        }
        fn params_of(&self, _operator: &str) -> Option<Vec<String>> {
            Some(vec!["amount".into(), "source".into(), "frequency".into()])
        }
    }

    fn rule(pattern: &str) -> ParseRule {
        ParseRule {
            id: "t".into(),
            source: "test".into(),
            version: 1,
            pattern: pattern.into(),
            extract: BTreeMap::new(),
            status: RuleStatus::ParsedUnmapped,
            operator: None,
            params: BTreeMap::new(),
            flags: vec![],
            reason_template: None,
            trust: ParseRuleTrust::Shipped,
            provenance: "test".into(),
            examples: vec![],
        }
    }

    fn spec(kind: FieldKind, group: Option<&str>, value: Option<serde_json::Value>) -> FieldSpec {
        FieldSpec { kind, group: group.map(str::to_string), value }
    }

    #[test]
    fn a_bad_regex_is_a_load_time_finding() {
        let errors = typecheck_rule(&rule("(unclosed"), &Universe);
        assert!(errors.iter().any(|e| e.contains("invalid regex")), "{errors:?}");
    }

    #[test]
    fn a_mapped_rule_must_name_a_registered_operator() {
        let mut r = rule("x");
        r.status = RuleStatus::Mapped;
        assert!(typecheck_rule(&r, &Universe)
            .iter()
            .any(|e| e.contains("requires an operator")));
        r.operator = Some("no.such.operator".into());
        assert!(typecheck_rule(&r, &Universe)
            .iter()
            .any(|e| e.contains("is not registered")));
    }

    #[test]
    fn a_mapped_rule_may_not_bind_a_param_the_operator_does_not_declare() {
        let mut r = rule("x");
        r.status = RuleStatus::Mapped;
        r.operator = Some("budget.record_income".into());
        r.params.insert("nonsense".into(), json!("$amount"));
        let errors = typecheck_rule(&r, &Universe);
        assert!(errors.iter().any(|e| e.contains("not a real parameter")), "{errors:?}");
    }

    #[test]
    fn only_a_mapped_rule_may_declare_an_operator() {
        let mut r = rule("x");
        r.operator = Some("budget.record_income".into());
        assert!(typecheck_rule(&r, &Universe)
            .iter()
            .any(|e| e.contains("must not declare operator")));
    }

    #[test]
    fn an_incomplete_field_spec_is_refused() {
        let mut r = rule("x");
        r.extract.insert("a".into(), spec(FieldKind::Amount, None, None));
        r.extract.insert("b".into(), spec(FieldKind::Literal, None, None));
        r.extract.insert("c".into(), spec(FieldKind::Template, None, None));
        let errors = typecheck_rule(&r, &Universe);
        assert_eq!(errors.len(), 3, "{errors:?}");
    }

    #[test]
    fn an_amount_is_parsed_with_its_commas_stripped() {
        let mut r = rule(r"Ksh(?P<amt>[\d,]+\.?\d*)");
        r.extract.insert("amount".into(), spec(FieldKind::Amount, Some("amt"), None));
        let t = apply_rule(&r, "Ksh1,234.50 paid").expect("matches");
        assert_eq!(t.parsed_fields().get("amount"), Some(&json!(1234.5)));
    }

    #[test]
    fn an_optional_group_that_did_not_match_is_omitted_not_zeroed() {
        let mut r = rule(r"A(?P<x>\d+)(?: cost (?P<cost>\d+))?");
        r.extract.insert("x".into(), spec(FieldKind::Amount, Some("x"), None));
        r.extract.insert("cost".into(), spec(FieldKind::Amount, Some("cost"), None));
        let t = apply_rule(&r, "A5").expect("matches");
        assert_eq!(t.parsed_fields().get("x"), Some(&json!(5.0)));
        assert!(!t.parsed_fields().contains_key("cost"), "absent is not zero");
    }

    #[test]
    fn a_literal_needs_no_group_at_all() {
        let mut r = rule("hello");
        r.extract.insert("direction".into(), spec(FieldKind::Literal, None, Some(json!("received"))));
        let t = apply_rule(&r, "well hello there").expect("matches");
        assert_eq!(t.parsed_fields().get("direction"), Some(&json!("received")));
    }

    #[test]
    fn prefix_if_missing_normalises_only_when_it_is_missing() {
        let mut r = rule(r"to (?P<who>.+)$");
        r.extract
            .insert("agent".into(), spec(FieldKind::PrefixIfMissing, Some("who"), Some(json!("Agent"))));
        assert_eq!(
            apply_rule(&r, "to 654321 - ESTATE SHOP").unwrap().parsed_fields()["agent"],
            json!("Agent 654321 - ESTATE SHOP")
        );
        assert_eq!(
            apply_rule(&r, "to Agent 123456 - TOWN SHOP").unwrap().parsed_fields()["agent"],
            json!("Agent 123456 - TOWN SHOP")
        );
    }

    #[test]
    fn upper_normalises_a_currency() {
        let mut r = rule(r"(?P<cur>kes|usd)");
        r.extract.insert("currency".into(), spec(FieldKind::Upper, Some("cur"), None));
        assert_eq!(apply_rule(&r, "kes 5").unwrap().parsed_fields()["currency"], json!("KES"));
    }

    #[test]
    fn a_template_composes_from_several_captured_pieces() {
        let mut r = rule(r"loan (?P<n>\d+) repaid");
        r.extract.insert("loan_number".into(), spec(FieldKind::Text, Some("n"), None));
        r.extract.insert(
            "counterparty".into(),
            spec(FieldKind::Template, None, Some(json!("KCB Mobile Loan {loan_number} repayment"))),
        );
        let t = apply_rule(&r, "loan 8891 repaid").expect("matches");
        assert_eq!(t.parsed_fields()["counterparty"], json!("KCB Mobile Loan 8891 repayment"));
    }

    #[test]
    fn a_reason_may_reference_a_group_never_promoted_to_a_field() {
        let mut r = rule(r"overdue since (?P<date>\S+)");
        r.reason_template = Some("this loan has been overdue since {date}".into());
        let t = apply_rule(&r, "overdue since 01/08/2026").expect("matches");
        assert_eq!(t.reason(), "this loan has been overdue since 01/08/2026");
    }

    #[test]
    fn params_bind_by_reference_by_template_and_by_literal() {
        let mut r = rule(r"Ksh(?P<amt>\d+) from (?P<name>\w+)");
        r.status = RuleStatus::Mapped;
        r.operator = Some("budget.record_income".into());
        r.extract.insert("amount".into(), spec(FieldKind::Amount, Some("amt"), None));
        r.extract.insert("counterparty".into(), spec(FieldKind::Text, Some("name"), None));
        r.params.insert("amount".into(), json!("$amount"));
        r.params.insert("source".into(), json!("M-Pesa: {counterparty}"));
        r.params.insert("frequency".into(), json!("once"));
        let t = apply_rule(&r, "Ksh900 from JANE").expect("matches");
        let p = t.operator_params();
        // ★ A `$` reference keeps the real type; a template is always a string.
        assert_eq!(p["amount"], json!(900.0));
        assert_eq!(p["source"], json!("M-Pesa: JANE"));
        assert_eq!(p["frequency"], json!("once"));
    }

    #[test]
    fn list_order_is_precedence() {
        let mut general = rule(r"paid to (?P<who>.+)");
        general.id = "general".into();
        let mut specific = rule(r"paid to (?P<who>\w+) for account (?P<acct>\w+)");
        specific.id = "specific".into();
        let text = "paid to SAFARICOM for account 12345";
        // Specific first wins; general first swallows the account.
        assert_eq!(
            run_rules(&[specific.clone(), general.clone()], text).unwrap().parser_name(),
            "specific"
        );
        assert_eq!(run_rules(&[general, specific], text).unwrap().parser_name(), "general");
    }

    #[test]
    fn nothing_matching_is_none_not_a_wrong_answer() {
        assert!(run_rules(&[rule("zzz")], "hello").is_none());
    }

    #[test]
    fn flags_are_honoured() {
        let mut r = rule("hello");
        r.flags = vec!["IGNORECASE".into()];
        assert!(apply_rule(&r, "HELLO").is_some());
        r.flags = vec![];
        assert!(apply_rule(&r, "HELLO").is_none());
    }

    #[test]
    fn a_template_leaves_an_unknown_name_verbatim() {
        // ★ Looking wrong beats looking like an answer.
        let mut r = rule("x");
        r.extract.insert(
            "t".into(),
            spec(FieldKind::Template, None, Some(json!("a {nope} b"))),
        );
        assert_eq!(apply_rule(&r, "x").unwrap().parsed_fields()["t"], json!("a {nope} b"));
    }
}
