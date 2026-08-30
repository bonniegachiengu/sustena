//! **`Def(θ)` — a definition as a typed function into Sustains**
//! (DSL · GENOME §VII).
//!
//! ```text
//!   Γ ⊢ Def(θ) : Sustain        Γ ⊢ v : τ_θ
//!   ───────────────────────────────────────    Def(v) is well-typed
//!            (substitution lemma)
//! ```
//!
//! ★★★ **The whole value is "checked once".** A parameterised definition is
//! type-checked one time, against the *declared types* of its parameters. After
//! that, every instantiation with a well-typed `θ` is sound **by the lemma**,
//! not by re-running the checker and hoping. Ten thousand households from one
//! template cost one check.
//!
//! ## What this replaces, and why the difference is not cosmetic
//!
//! ★★★ Today a template says `{{role_in_family}}` and substitution is textual:
//! a token nobody supplied **survives as the literal string `{{role_in_family}}`**
//! and lands in real state. It has happened — a household carried that exact
//! string as somebody's role. Textual substitution has no theorem behind it, so
//! there is no step at which the mistake can be caught; the first thing that
//! notices is a person reading their own profile.
//!
//! A declared parameter has three properties textual substitution cannot have:
//!
//! - **An unsupplied parameter is refused**, at instantiation, naming itself.
//! - **A wrongly-typed argument is refused** — `limit: "soon"` where a number
//!   was declared is caught before a Sustain exists, not by an invariant later
//!   comparing a number to a word.
//! - **A token nobody declared is refused at AUTHOR time**, which is the
//!   `role_in_family` case exactly: the mistake was never in the argument, it
//!   was in a template referring to something it never declared.
//!
//! ★★ **The token syntax is unchanged** — `{{name}}`, the same shape
//! [`crate::strategy`] already calibrates with. Two spellings of one idea would
//! be worse than the textual substitution this replaces.
//!
//! ## The one real subtlety: a token is not always a string
//!
//! ★★★ `"{{limit}}"` **is** the parameter — if `limit` is `5000`, the result is
//! the number `5000`, not the text `"5000"`. A template that produced a string
//! there would declare a number dimension and then fill it with text, and the
//! schema would refuse the Sustain the template was written to build.
//! `"pocket for {{name}}"` is different: the token sits inside other text, so
//! the result is text. The rule is *whole-value token substitutes, embedded
//! token interpolates*, and it is the difference between a template that works
//! and one that type-errors on its own output.

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use thiserror::Error;

use crate::schema::{conforms, DimType, Schema, TypeError};

/// One parameter a definition declares.
#[derive(Debug, Clone, PartialEq)]
pub struct DefParam {
    pub name: String,
    pub ty: DimType,
    /// Used when `θ` omits it. `None` makes the parameter required.
    ///
    /// ★★ A default is part of the DECLARATION, so it is type-checked once
    /// along with everything else. A default that does not fit its own
    /// parameter is a template that is wrong before anyone uses it.
    pub default: Option<Value>,
}

impl DefParam {
    pub fn required(name: &str, ty: DimType) -> Self {
        Self { name: name.into(), ty, default: None }
    }

    pub fn optional(name: &str, ty: DimType, default: Value) -> Self {
        Self { name: name.into(), ty, default: Some(default) }
    }
}

/// Why a parameterised definition, or an instantiation of one, was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParamError {
    #[error("the template uses '{{{{{token}}}}}' at {at}, which it never declares as a parameter")]
    Undeclared { token: String, at: String },
    #[error("'{name}' is declared twice")]
    Duplicate { name: String },
    #[error("'{name}' has a default that does not fit it: {detail}")]
    BadDefault { name: String, detail: String },
    #[error("'{name}' was not supplied, and has no default")]
    Missing { name: String },
    #[error("'{name}' was given {detail}")]
    WrongType { name: String, detail: String },
    #[error("'{name}' was supplied but the template declares no such parameter")]
    Unexpected { name: String },
}

/// A definition together with the parameters it takes.
///
/// ★★ Deliberately a wrapper rather than fields on `Definition`. Most
/// definitions take no parameters at all, and a `Definition` that carried an
/// always-empty parameter list would invite every reader to wonder what it was
/// for.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameterised {
    pub params: Vec<DefParam>,
    /// The opening state, with `{{token}}`s still in it.
    pub opening_state: Value,
}

impl Parameterised {
    pub fn new(opening_state: Value) -> Self {
        Self { params: Vec::new(), opening_state }
    }

    pub fn taking(mut self, param: DefParam) -> Self {
        self.params.push(param);
        self
    }

    fn declared(&self, name: &str) -> Option<&DefParam> {
        self.params.iter().find(|p| p.name == name)
    }

    /// **Check once.** After this, any well-typed `θ` instantiates soundly.
    ///
    /// ★★★ Three findings, and they are genuinely different mistakes:
    /// a parameter declared twice (which of them wins?), a default that does
    /// not fit its own parameter (wrong before anyone uses it), and a token the
    /// template never declared — the `role_in_family` case, where the mistake
    /// was never in anybody's argument.
    ///
    /// ★★ The schema is checked against too, so a template cannot promise a
    /// Sustain it could not build: every dimension the opening state fills must
    /// be one the schema declares, once the tokens are of a known type.
    pub fn typecheck(&self, schema: &Schema) -> Result<(), Vec<ParamError>> {
        let mut errors = Vec::new();

        let mut seen: BTreeMap<&str, ()> = BTreeMap::new();
        for p in &self.params {
            if seen.insert(p.name.as_str(), ()).is_some() {
                errors.push(ParamError::Duplicate { name: p.name.clone() });
            }
            if let Some(d) = &p.default {
                if let Some(detail) = conforms(d, &p.ty) {
                    errors.push(ParamError::BadDefault { name: p.name.clone(), detail });
                }
            }
        }

        // Every token the template mentions must be declared.
        for (token, at) in tokens_in(&self.opening_state, "") {
            if self.declared(&token).is_none() {
                errors.push(ParamError::Undeclared { token, at });
            }
        }

        // ★★ With every token declared, the template's own SHAPE can be checked
        //    against the schema by standing in a value of each declared type —
        //    which is what makes "checked once" cover the result and not just
        //    the arguments.
        if errors.is_empty() {
            let witness = self.witness();
            if let Ok(shape) = self.substitute(&witness) {
                for e in crate::schema::validate(&shape, schema) {
                    errors.push(ParamError::WrongType {
                        name: e.path,
                        detail: e.detail,
                    });
                }
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    /// A stand-in argument of each declared type — never a real one.
    ///
    /// ★★ Only the TYPES matter for the shape check, so this uses a canonical
    /// value per type rather than the defaults. Using the defaults would make
    /// the check pass or fail depending on values a caller is free to replace.
    fn witness(&self) -> Map<String, Value> {
        self.params
            .iter()
            .map(|p| (p.name.clone(), canonical(&p.ty)))
            .collect()
    }

    /// **Instantiate.** `θ` in, a concrete opening state out.
    ///
    /// ★★★ Refuses rather than substitutes-what-it-can. A half-filled template
    /// is the textual behaviour this exists to end: it produced a Sustain that
    /// looked built and carried `{{role_in_family}}` in it.
    pub fn instantiate(&self, theta: &Map<String, Value>) -> Result<Value, Vec<ParamError>> {
        let mut errors = Vec::new();
        let mut resolved: Map<String, Value> = Map::new();

        for p in &self.params {
            match theta.get(&p.name).or(p.default.as_ref()) {
                None => errors.push(ParamError::Missing { name: p.name.clone() }),
                Some(v) => match conforms(v, &p.ty) {
                    Some(detail) => {
                        errors.push(ParamError::WrongType { name: p.name.clone(), detail })
                    }
                    None => {
                        resolved.insert(p.name.clone(), v.clone());
                    }
                },
            }
        }
        // ★★ An argument nobody declared is a mistake worth naming. Silently
        //    ignoring it is how a typo in a caller goes unnoticed for months.
        for name in theta.keys() {
            if self.declared(name).is_none() {
                errors.push(ParamError::Unexpected { name: name.clone() });
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }
        self.substitute(&resolved)
    }

    fn substitute(&self, values: &Map<String, Value>) -> Result<Value, Vec<ParamError>> {
        let mut errors = Vec::new();
        let out = fill(&self.opening_state, values, "", &mut errors);
        if errors.is_empty() { Ok(out) } else { Err(errors) }
    }
}

/// A canonical value of a type, for the shape check.
fn canonical(ty: &DimType) -> Value {
    match ty {
        DimType::Number { lo, .. } => Value::from(lo.unwrap_or(0.0)),
        DimType::Text => Value::from(""),
        DimType::Bool => Value::from(false),
        DimType::List { .. } => Value::Array(vec![]),
        DimType::Record { .. } | DimType::Map { .. } => Value::Object(Map::new()),
        DimType::Any => Value::Null,
    }
}

/// `{{name}}`, if this string is exactly one token.
fn whole_token(s: &str) -> Option<&str> {
    let t = s.trim();
    t.strip_prefix("{{").and_then(|r| r.strip_suffix("}}")).map(str::trim)
}

/// Every token the template mentions, with where it was found.
fn tokens_in(value: &Value, at: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    match value {
        Value::String(s) => {
            if let Some(t) = whole_token(s) {
                out.push((t.to_string(), at.to_string()));
            } else {
                for t in embedded(s) {
                    out.push((t, at.to_string()));
                }
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                out.extend(tokens_in(v, &format!("{at}[{i}]")));
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                let path = if at.is_empty() { k.clone() } else { format!("{at}.{k}") };
                out.extend(tokens_in(v, &path));
            }
        }
        _ => {}
    }
    out
}

/// Tokens sitting inside other text.
fn embedded(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        out.push(after[..end].trim().to_string());
        rest = &after[end + 2..];
    }
    out
}

/// Replace every token. Whole-value tokens substitute; embedded ones
/// interpolate.
fn fill(
    value: &Value,
    values: &Map<String, Value>,
    at: &str,
    errors: &mut Vec<ParamError>,
) -> Value {
    match value {
        Value::String(s) => {
            // ★★★ The subtlety. `"{{limit}}"` IS the parameter, so a number
            //     stays a number; a template that turned it into text would
            //     fill a number dimension with a word and the schema would
            //     refuse the very Sustain the template exists to build.
            if let Some(token) = whole_token(s) {
                return match values.get(token) {
                    Some(v) => v.clone(),
                    None => {
                        errors.push(ParamError::Undeclared {
                            token: token.to_string(),
                            at: at.to_string(),
                        });
                        Value::Null
                    }
                };
            }
            let mut out = s.clone();
            for token in embedded(s) {
                let Some(v) = values.get(&token) else {
                    errors.push(ParamError::Undeclared {
                        token: token.clone(),
                        at: at.to_string(),
                    });
                    continue;
                };
                let text = match v {
                    Value::String(t) => t.clone(),
                    other => other.to_string(),
                };
                out = out.replace(&format!("{{{{{token}}}}}"), &text);
            }
            Value::String(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(i, v)| fill(v, values, &format!("{at}[{i}]"), errors))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| {
                    let path = if at.is_empty() { k.clone() } else { format!("{at}.{k}") };
                    (k.clone(), fill(v, values, &path, errors))
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Every error, as one readable list.
pub fn describe(errors: &[ParamError]) -> Vec<TypeError> {
    errors
        .iter()
        .map(|e| TypeError { path: "definition".into(), detail: e.to_string() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn number() -> DimType {
        DimType::Number { lo: None, hi: None }
    }

    fn habitat_schema() -> Schema {
        Schema::new()
            .declare(
                "identity",
                DimType::Record {
                    fields: [
                        ("name".to_string(), DimType::Text),
                        ("role_in_family".to_string(), DimType::Text),
                    ]
                    .into_iter()
                    .collect(),
                },
            )
            .declare(
                "finances",
                DimType::Record {
                    fields: [("limit".to_string(), number())].into_iter().collect(),
                },
            )
    }

    fn habitat() -> Parameterised {
        Parameterised::new(json!({
            "identity": {"name": "{{name}}", "role_in_family": "{{role_in_family}}"},
            "finances": {"limit": "{{limit}}"}
        }))
        .taking(DefParam::required("name", DimType::Text))
        .taking(DefParam::optional("role_in_family", DimType::Text, json!("")))
        .taking(DefParam::required("limit", number()))
    }

    fn theta(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
    }

    #[test]
    fn a_well_formed_template_checks_once() {
        assert!(habitat().typecheck(&habitat_schema()).is_ok());
    }

    #[test]
    fn a_token_the_template_never_declared_is_caught_at_author_time() {
        // ★★★ The `role_in_family` bug, at the only place it could ever have
        //     been caught. The mistake was never in anybody's argument — it was
        //     a template referring to something it never declared, and textual
        //     substitution has no step at which to notice.
        let broken = Parameterised::new(json!({"identity": {"role_in_family": "{{role_in_family}}"}}));
        let errors = broken.typecheck(&habitat_schema()).expect_err("must refuse");
        assert!(
            matches!(&errors[0], ParamError::Undeclared { token, .. } if token == "role_in_family"),
            "{errors:?}",
        );
        assert!(errors[0].to_string().contains("identity.role_in_family"), "and says where");
    }

    #[test]
    fn an_unbound_token_can_no_longer_survive_as_a_literal() {
        // ★★★ The behaviour this replaces, asserted directly: the old textual
        //     pass produced a real Sustain carrying the string
        //     "{{role_in_family}}" as somebody's role.
        let broken = Parameterised::new(json!({"identity": {"role_in_family": "{{whoops}}"}}));
        assert!(broken.typecheck(&habitat_schema()).is_err());
        let out = broken.instantiate(&theta(&[]));
        assert!(out.is_err(), "and instantiation refuses too: {out:?}");
    }

    #[test]
    fn instantiating_fills_every_token() {
        let out = habitat()
            .instantiate(&theta(&[
                ("name", json!("Aida")),
                ("role_in_family", json!("sister")),
                ("limit", json!(5000.0)),
            ]))
            .expect("instantiates");
        assert_eq!(out.pointer("/identity/name"), Some(&json!("Aida")));
        assert_eq!(out.pointer("/identity/role_in_family"), Some(&json!("sister")));
    }

    #[test]
    fn a_whole_token_keeps_its_own_type() {
        // ★★★ `"{{limit}}"` IS the parameter. A template that turned it into
        //     text would fill a number dimension with a word, and the schema
        //     would refuse the very Sustain the template exists to build.
        let out = habitat()
            .instantiate(&theta(&[("name", json!("Aida")), ("limit", json!(5000.0))]))
            .expect("instantiates");
        assert_eq!(out.pointer("/finances/limit"), Some(&json!(5000.0)));
        assert!(out.pointer("/finances/limit").unwrap().is_number());
    }

    #[test]
    fn a_token_inside_other_text_interpolates_instead() {
        let p = Parameterised::new(json!({"label": "pocket for {{name}}"}))
            .taking(DefParam::required("name", DimType::Text));
        let out = p.instantiate(&theta(&[("name", json!("Aida"))])).expect("instantiates");
        assert_eq!(out.pointer("/label"), Some(&json!("pocket for Aida")));
    }

    #[test]
    fn an_omitted_parameter_takes_its_default() {
        let out = habitat()
            .instantiate(&theta(&[("name", json!("Aida")), ("limit", json!(0.0))]))
            .expect("instantiates");
        assert_eq!(
            out.pointer("/identity/role_in_family"),
            Some(&json!("")),
            "a clean empty string, never the raw token",
        );
    }

    #[test]
    fn a_required_parameter_nobody_supplied_is_refused_by_name() {
        let errors = habitat()
            .instantiate(&theta(&[("limit", json!(0.0))]))
            .expect_err("must refuse");
        assert!(matches!(&errors[0], ParamError::Missing { name } if name == "name"), "{errors:?}");
    }

    #[test]
    fn an_argument_of_the_wrong_type_is_refused_before_a_sustain_exists() {
        // ★★ Otherwise the first thing that notices is an invariant comparing
        //    a number to a word, reported as a violated law.
        let errors = habitat()
            .instantiate(&theta(&[("name", json!("Aida")), ("limit", json!("soon"))]))
            .expect_err("must refuse");
        assert!(
            matches!(&errors[0], ParamError::WrongType { name, .. } if name == "limit"),
            "{errors:?}",
        );
    }

    #[test]
    fn an_argument_nobody_declared_is_named_rather_than_ignored() {
        // ★★ Silently dropping it is how a typo in a caller goes unnoticed.
        let errors = habitat()
            .instantiate(&theta(&[
                ("name", json!("Aida")),
                ("limit", json!(0.0)),
                ("lmit", json!(1.0)),
            ]))
            .expect_err("must refuse");
        assert!(matches!(&errors[0], ParamError::Unexpected { name } if name == "lmit"));
    }

    #[test]
    fn a_default_that_does_not_fit_its_own_parameter_is_caught_once() {
        let p = Parameterised::new(json!({"finances": {"limit": "{{limit}}"}}))
            .taking(DefParam::optional("limit", number(), json!("soon")));
        let errors = p.typecheck(&habitat_schema()).expect_err("must refuse");
        assert!(matches!(&errors[0], ParamError::BadDefault { name, .. } if name == "limit"));
    }

    #[test]
    fn a_template_that_could_not_build_its_own_sustain_is_refused() {
        // ★★★ "Checked once" has to cover the RESULT, not only the arguments.
        //     A template promising a Sustain the schema would reject is wrong
        //     before anybody instantiates it.
        let p = Parameterised::new(json!({"finances": {"limit": "{{label}}"}}))
            .taking(DefParam::required("label", DimType::Text));
        let errors = p.typecheck(&habitat_schema()).expect_err("must refuse");
        assert!(
            errors.iter().any(|e| matches!(e, ParamError::WrongType { .. })),
            "{errors:?}",
        );
    }

    #[test]
    fn checked_once_means_any_well_typed_argument_is_sound() {
        // ★★★ The substitution lemma, as a property rather than a claim. One
        //     check, then ten thousand households — and none of them re-check.
        let template = habitat();
        let schema = habitat_schema();
        template.typecheck(&schema).expect("checked once");

        for (name, role, limit) in [
            ("Aida", "sister", 5000.0),
            ("", "", 0.0),
            ("a very long name indeed", "the one who cooks", 999_999.0),
        ] {
            let out = template
                .instantiate(&theta(&[
                    ("name", json!(name)),
                    ("role_in_family", json!(role)),
                    ("limit", json!(limit)),
                ]))
                .expect("well-typed θ instantiates");
            assert!(
                crate::schema::validate(&out, &schema).is_empty(),
                "and the result conforms without being re-checked: {out}",
            );
        }
    }
}
