//! Authored definitions — `Σ` written by a person, checked by the engine.
//!
//! ## ★★★ The safety machinery is the engine's, not this app's
//!
//! Two real checks, and neither is re-implemented here:
//!
//! - [`editing::typecheck`] parses every invariant and **binds it against the
//!   schema**, so a rule referring to a dimension the definition does not
//!   declare is a load-time finding rather than a `None`-comparison at runtime.
//! - [`editing::safe`] evaluates a candidate definition against **every live
//!   instance**, so an edit that would leave a real Sustain outside its own
//!   viable region is refused with the instances it would strand — named.
//!
//! ★★ A definition that does not typecheck is **never persisted**. The store
//! only ever holds definitions the engine has accepted, so loading one cannot
//! surface a rule that was broken when it was written.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use sustena_core::{
    editing::{safe, typecheck, Definition, Instance, Migration},
    schema::{DimType, Schema},
};

/// One dimension a person declared.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DimDecl {
    pub path: String,
    /// `number` | `text` | `bool` | `any`.
    pub kind: String,
    /// ★ Optional bounds for a number. `DimType::Number` carries them, so a
    /// dimension can be constrained at the SCHEMA level as well as by an
    /// invariant — two different questions: what values are representable, and
    /// what values are viable.
    pub lo: Option<f64>,
    pub hi: Option<f64>,
}

/// One rule a person declared.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvariantDecl {
    pub id: String,
    pub expression: String,
}

/// A definition as authored and persisted.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AuthoredDefinition {
    pub id: String,
    pub label: String,
    pub dimensions: Vec<DimDecl>,
    pub operators: Vec<String>,
    pub invariants: Vec<InvariantDecl>,
    /// The opening state a new instance starts from.
    pub opening_state: Value,
}

fn dim_type(d: &DimDecl) -> DimType {
    match d.kind.as_str() {
        "number" => DimType::Number { lo: d.lo, hi: d.hi },
        "text" => DimType::Text,
        "bool" => DimType::Bool,
        _ => DimType::Any,
    }
}

impl AuthoredDefinition {
    /// Build the engine's `Definition` from what was authored.
    pub fn to_definition(&self) -> Definition {
        let mut schema = Schema::new();
        for d in &self.dimensions {
            schema = schema.declare(&d.path, dim_type(d));
        }
        let mut def = Definition::new(schema);
        for op in &self.operators {
            def = def.with_operator(op);
        }
        for inv in &self.invariants {
            def = def.with_invariant(&inv.id, &inv.expression);
        }
        def
    }
}

/// What the engine said about an authored definition.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DefinitionVerdict {
    /// `typecheck` accepted it, and no live instance would be stranded.
    Accepted,
    /// ★ One entry per problem, each naming the invariant and the detail —
    /// `editing::typecheck`'s own words, not a summary of them.
    NotWellTyped { errors: Vec<String> },
    /// ★★ Live Sustains this edit would push outside their viable region —
    /// each with the RULE it would break and the engine's reason. A refusal
    /// that cannot say which sustain and which rule is an alarm, not a
    /// diagnosis.
    WouldStrand { instances: Vec<String> },
}

/// ★★★ Run the engine's real checks over an authored definition.
///
/// `instances` is every live Sustain already on this definition — empty for a
/// brand-new one, which is why creating is always safe and *editing* is where
/// stranding can bite.
pub fn check(authored: &AuthoredDefinition, instances: &[Instance]) -> DefinitionVerdict {
    check_against(authored, instances, &[])
}

/// The same checks, plus the children this definition will aggregate over.
///
/// ★★★ **A parent and its children are one program** (GENOME §IX). An aggregate
/// reading a dimension no child declares does not crash — it totals **zero
/// contributions**, which reads exactly like a household that genuinely has no
/// money, and nothing afterwards can tell the two apart. Checking the parent
/// alone cannot see it, because the mistake is only visible from both sides.
///
/// ★★ Empty children is not a failure. A holon is declared before it is
/// populated, and refusing here would make declaring one impossible until the
/// first member joined.
pub fn check_against(
    authored: &AuthoredDefinition,
    instances: &[Instance],
    children: &[(&str, &sustena_core::Schema)],
) -> DefinitionVerdict {
    let def = authored.to_definition();

    if let Err(errors) = sustena_core::typecheck_holon(&def, children) {
        return DefinitionVerdict::NotWellTyped {
            errors: errors.into_iter().map(|e| format!("{}: {}", e.path, e.detail)).collect(),
        };
    }

    if let Err(errors) = typecheck(&def) {
        return DefinitionVerdict::NotWellTyped {
            errors: errors.into_iter().map(|e| format!("{}: {}", e.path, e.detail)).collect(),
        };
    }

    // ★ `Migration::default()` is the honest starting point: *no migration is
    //   offered*. So `safe` reports every instance the candidate would strand
    //   as-is, rather than one that a migration nobody wrote might have saved.
    if let Err(stranded) = safe(&def, instances, &Migration::default()) {
        return DefinitionVerdict::WouldStrand {
            instances: stranded
                .into_iter()
                .map(|s| format!("{} · {} ({})", s.instance_id, s.invariant_id, s.reason))
                .collect(),
        };
    }

    DefinitionVerdict::Accepted
}
