//! **The registry's judgment** — what a package must satisfy to be installed.
//!
//! Arena §§ (the ARENA article's *"the oldest market on Earth has no money, no
//! contracts and no judges"*) meets this codebase's own load gates. The
//! registry's **storage** is the host's, like every other store; what lives
//! here is the one thing that must not be re-implemented per call site: **the
//! decision.**
//!
//! ★★★ **There is no install-specific path, and that is enforced by the type
//! rather than by care.** [`Admitted`] has no public constructor. The only way
//! to obtain one is to call one of the `*_installs` functions below, and each
//! of those calls the **same** gate a locally-authored artifact faces —
//! `editing::typecheck` for a `Definition`, `WidgetSet::load` for a
//! `WidgetDecl`, the live `Registry` for an operator name. A host that wanted
//! to skip the check could not: it has nothing to pass to `install`.
//!
//! ★★★ **Integrity, authenticity and quality are three different questions,
//! and conflating them is how a registry becomes a supply chain attack.**
//! - **Integrity** — are these the bytes that were published? A content hash.
//! - **Authenticity** — did that key publish them? A signature.
//! - **Quality** — is it any good? **PackageProvenance, and it is REPORTED, never
//!   enforced.** A package from a key you trust that does not typecheck is
//!   still refused; a package from a stranger that does typecheck installs.
//!   Trust decides what you *choose*, never what the gate *allows*.
//!
//! The first two are computed in the host (sha2 and ed25519 live there, per
//! ADR-0001); what this module owns is the **vocabulary** — three verdicts that
//! cannot be collapsed into one boolean by a caller in a hurry.
//!
//! ★★ **What a package cannot carry: code.** An [`crate::operator::Registry`]
//! entry holds a Rust function pointer. There is no plugin loader here and no
//! wasm, so an *operator* package is a **name** that must already resolve
//! locally — publishing one does not ship an implementation, and installing
//! one that does not resolve is refused rather than left dangling. Said out
//! loud because a registry that silently accepted an unrunnable operator would
//! be worse than one that had no operator kind at all.

use serde::{Deserialize, Serialize};

use crate::editing::{safe, typecheck, Instance, Migration};
use crate::operator::Registry;
use crate::widget::{WidgetDecl, WidgetSet};
use crate::Definition;

// ---------------------------------------------------------------------------
// Kinds
// ---------------------------------------------------------------------------

/// What a package contains.
///
/// ★★ **Reconciled against the reference, and it does not match — deliberately.**
/// The Python arena's kinds are `operative | operator | spore | widget`. There
/// is no `spore` in this core and no mechanism one would name; `operative`'s
/// publishable part is its `Π`, which is a [`Strategy`](Kind::Strategy) here;
/// and `definition` — the one artifact this node can genuinely install and
/// instantiate from — the reference has no kind for at all, because it never
/// installs anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A publishable `Σ`-template: schema, invariants, operators, opening state.
    Definition,
    /// One curated-UI card.
    Widget,
    /// A **name**, not an implementation. See the module header.
    Operator,
    /// An operative's `Π`. Publishable; see [`InstallVerdict::NotHere`].
    Strategy,
}

impl Kind {
    /// Parse a kind by its own label. ★ `None` rather than a default: a kind
    /// this build does not know is a package it cannot judge, and guessing
    /// would be the one mistake a registry must not make.
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "definition" => Some(Kind::Definition),
            "widget" => Some(Kind::Widget),
            "operator" => Some(Kind::Operator),
            "strategy" => Some(Kind::Strategy),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Kind::Definition => "definition",
            Kind::Widget => "widget",
            Kind::Operator => "operator",
            Kind::Strategy => "strategy",
        }
    }
}

// ---------------------------------------------------------------------------
// The three questions
// ---------------------------------------------------------------------------

/// Are these the bytes that were published?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Integrity {
    /// The content hash matches what the package claims.
    Intact,
    /// It does not. ★ Both values are carried: a mismatch a person cannot
    /// inspect is an alarm rather than a diagnosis.
    Altered { claimed: String, actual: String },
}

/// Did the key it names actually publish it?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Authenticity {
    /// A signature over the content hash verified against the author's key.
    Signed,
    /// ★★ No signature at all — which is **not** the same as a bad one. A
    /// locally-authored package that was never sent anywhere has nothing to
    /// prove; one that arrived from a peer and is unsigned is a warning.
    Unsigned,
    /// A signature that does not verify. Never install this.
    Forged,
}

/// ★★★ **Reported, never enforced.** Everything here is *what is known about
/// where this came from*, and none of it is a permission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Written on this node.
    Authored,
    /// Shipped with the app.
    Bundled,
    /// Arrived from a peer, which is named. ★ Mirrors
    /// [`crate::learning::MemeProvenance::Imported`]'s discipline exactly: the
    /// origin is not optional, so an import can never read as native.
    FromPeer { peer: String },
}

/// What is known about a package before anyone decides anything.
///
/// ★ Named `PackageProvenance` — name-collision rule #46, the newcomer takes
/// the longer name: `event::Provenance` (how a fact reached the log) had it
/// first, and confusing *where an event came from* with *where a package came
/// from* would be a genuinely misleading collision.
///
/// ★★ The three questions travel together and stay separate. A caller that
/// wants one boolean has to choose which one it means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageProvenance {
    /// The author's public key, hex. ★ The identity — a name beside it is a
    /// label.
    pub author: String,
    pub origin: Origin,
    pub integrity: Integrity,
    pub authenticity: Authenticity,
}

impl PackageProvenance {
    /// ★★★ **Safe to install** means intact and not forged — nothing about
    /// quality. An unsigned local package is installable; a tampered or forged
    /// one never is, whoever vouches for it.
    pub fn safe_to_install(&self) -> bool {
        self.integrity == Integrity::Intact && self.authenticity != Authenticity::Forged
    }

    pub fn describe(&self) -> String {
        let integrity = match &self.integrity {
            Integrity::Intact => "bytes intact".to_string(),
            Integrity::Altered { .. } => "BYTES ALTERED since publishing".to_string(),
        };
        let authenticity = match self.authenticity {
            Authenticity::Signed => "signed by its author",
            Authenticity::Unsigned => "unsigned",
            Authenticity::Forged => "SIGNATURE DOES NOT VERIFY",
        };
        let origin = match &self.origin {
            Origin::Authored => "written here".to_string(),
            Origin::Bundled => "shipped with the app".to_string(),
            Origin::FromPeer { peer } => format!("from {peer}"),
        };
        format!("{origin} · {integrity} · {authenticity}")
    }
}

// ---------------------------------------------------------------------------
// The verdict
// ---------------------------------------------------------------------------

/// Proof that the real gate ran and admitted this package.
///
/// ★★★ **No public constructor.** Holding one of these is not a claim that the
/// check passed — it *is* the check having passed, because the only code that
/// can build one is inside this module, and every path that builds one calls
/// the same gate a hand-written artifact faces. This is the curated-UI load
/// gate's own discipline (`LoadedWidget` has no public constructor either),
/// applied to installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admitted {
    kind: Kind,
    id: String,
}

impl Admitted {
    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

/// What the registry decided.
///
/// ★ Named `InstallVerdict` — collision #47: `admission::Verdict` is what the
/// operator gate returns, and one of these is not the other.
#[derive(Debug, Clone, PartialEq)]
pub enum InstallVerdict {
    Admitted(Admitted),
    /// The real gate said no, in its own words.
    Refused {
        /// The rule that refused, by the name the gate itself uses.
        rule: &'static str,
        errors: Vec<String>,
    },
    /// ★★★ **The honest third answer.** Nothing on this node consumes this
    /// kind, so there is no gate to pass and nothing to install into. Not a
    /// failure and not a pass — the same shape as `Skew::Unknown` and
    /// `UrgencyBasis::Undeclared`, for the same reason: a boolean here would
    /// have to lie in one direction.
    NotHere { why: String },
}

impl InstallVerdict {
    pub fn admitted(&self) -> Option<&Admitted> {
        match self {
            InstallVerdict::Admitted(a) => Some(a),
            _ => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            InstallVerdict::Admitted(a) => format!("{} '{}' passes the same gate a local one does", a.kind.label(), a.id),
            InstallVerdict::Refused { rule, errors } => {
                format!("refused [{rule}] — {}", errors.join("; "))
            }
            InstallVerdict::NotHere { why } => format!("nothing here to install it into — {why}"),
        }
    }
}

// ---------------------------------------------------------------------------
// The gates, each the real one
// ---------------------------------------------------------------------------

/// A published `Definition`, against **the gate `author_definition` uses**.
///
/// ★★★ `typecheck` then `safe` — the same two, in the same order, with the
/// same `Migration::default()` honesty (*no migration is offered*). A package
/// gets no gentler treatment than something typed into the Define screen.
pub fn definition_installs(
    id: &str,
    definition: &Definition,
    instances: &[Instance],
) -> InstallVerdict {
    if let Err(errors) = typecheck(definition) {
        return InstallVerdict::Refused {
            rule: "well_typed",
            errors: errors.into_iter().map(|e| format!("{}: {}", e.path, e.detail)).collect(),
        };
    }
    if let Err(stranded) = safe(definition, instances, &Migration::default()) {
        return InstallVerdict::Refused {
            rule: "would_strand",
            errors: stranded
                .into_iter()
                .map(|s| format!("{} · {} ({})", s.instance_id, s.invariant_id, s.reason))
                .collect(),
        };
    }
    InstallVerdict::Admitted(Admitted { kind: Kind::Definition, id: id.to_string() })
}

/// A published `WidgetDecl`, against **the curated-UI load gate**.
///
/// ★★ A widget cannot be typechecked alone: `inputs ⊆ dim(S)` and
/// `emits ⊆ T` are questions about a **target** Sustain. So installing a
/// widget is always installing it *into* a definition, and a package that
/// typechecks against one household may honestly refuse against another.
pub fn widget_installs(decl: WidgetDecl, into: &Definition, registry: &Registry) -> InstallVerdict {
    let id = decl.id.clone();
    match WidgetSet::load(vec![decl], into, registry) {
        Ok(_) => InstallVerdict::Admitted(Admitted { kind: Kind::Widget, id }),
        Err(errors) => InstallVerdict::Refused {
            rule: "widget_well_typed",
            errors: errors.iter().map(|e| e.to_string()).collect(),
        },
    }
}

/// A published operator **name**, against the live registry.
///
/// ★★★ This can never ship an implementation — see the module header. What it
/// checks is the only thing that is checkable: **does this node actually have
/// the operator the package names.** A package naming one it does not have is
/// refused, rather than installed as a reference to nothing.
pub fn operator_installs(name: &str, registry: &Registry) -> InstallVerdict {
    if registry.get(name).is_some() {
        InstallVerdict::Admitted(Admitted { kind: Kind::Operator, id: name.to_string() })
    } else {
        InstallVerdict::Refused {
            rule: "operator_present",
            errors: vec![format!(
                "this node has no operator called '{name}' — a package names an operator, \
                 it cannot carry one, so there is nothing to install"
            )],
        }
    }
}

/// A published strategy `Π`.
///
/// ★★★ **Publishable, and honestly not installable here.** A `StrategyGraph`
/// is run by an `Agent` holding a `MemeLibrary`, and this host wires neither —
/// so installing one would mean writing it somewhere nothing reads. That is
/// the *port a checker to sit beside nothing* mistake, and the answer is the
/// third verdict rather than a pretend success.
pub fn strategy_installs(id: &str) -> InstallVerdict {
    InstallVerdict::NotHere {
        why: format!(
            "'{id}' is a strategy Π; this node has no Agent running a MemeLibrary, so there \
             is nothing here that would ever execute it"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{DimType, Schema};
    use crate::curated::BindingKey;
    use serde_json::json;

    fn good() -> Definition {
        Definition::new(Schema::new().declare("balance", DimType::Number { lo: None, hi: None }))
            .with_invariant("solvent", "balance >= 0")
    }

    fn broken() -> Definition {
        // References a dimension the schema does not declare.
        Definition::new(Schema::new().declare("balance", DimType::Number { lo: None, hi: None }))
            .with_invariant("nonsense", "moisture >= 0")
    }

    // ── the no-bypass property ──────────────────────────────────────────────

    #[test]
    fn an_unknown_kind_is_not_guessed_at() {
        for k in [Kind::Definition, Kind::Widget, Kind::Operator, Kind::Strategy] {
            assert_eq!(Kind::parse(k.label()), Some(k));
        }
        // ★ The reference has a `spore` kind. This build does not, and says so
        //   rather than falling back to something it does know.
        assert_eq!(Kind::parse("spore"), None);
    }

    #[test]
    fn a_package_faces_exactly_the_gate_a_local_artifact_faces() {
        // ★★★ Not "a similar check" — the same functions. If `typecheck`
        //     admits it here, `author_definition` admits it there, and there
        //     is no third path.
        let d = good();
        assert!(typecheck(&d).is_ok());
        assert!(definition_installs("garden", &d, &[]).admitted().is_some());

        let b = broken();
        assert!(typecheck(&b).is_err());
        assert!(definition_installs("garden", &b, &[]).admitted().is_none());
    }

    #[test]
    fn a_broken_package_is_refused_in_the_gates_own_words() {
        let v = definition_installs("garden", &broken(), &[]);
        let InstallVerdict::Refused { rule, errors } = v else { panic!("expected a refusal") };
        assert_eq!(rule, "well_typed");
        assert_eq!(errors.len(), 1);
        // The engine's own detail, not a summary of it.
        assert!(errors[0].contains("moisture"), "{errors:?}");
    }

    #[test]
    fn a_package_that_would_strand_a_live_instance_is_refused() {
        // ★★ The second gate, and the one that only bites on an EDIT: a
        //    definition that typechecks fine can still be inadmissible for a
        //    household already living on it.
        let live = [Instance { id: "s1".into(), state: json!({"balance": -5.0}) }];
        let v = definition_installs("garden", &good(), &live);
        let InstallVerdict::Refused { rule, errors } = v else { panic!("expected a refusal") };
        assert_eq!(rule, "would_strand");
        assert!(errors[0].contains("s1"), "{errors:?}");
    }

    #[test]
    fn admitted_cannot_be_forged() {
        // ★★★ A structural assertion rather than a behavioural one: `Admitted`
        //     has private fields and no constructor, so the ONLY way a host
        //     can hold one is to have run a gate. This test exists to fail if
        //     someone adds a `pub fn new`.
        let v = definition_installs("garden", &good(), &[]);
        let a = v.admitted().expect("admitted");
        assert_eq!(a.kind(), Kind::Definition);
        assert_eq!(a.id(), "garden");
    }

    // ── widgets ─────────────────────────────────────────────────────────────

    #[test]
    fn a_widget_is_checked_against_the_household_it_would_join() {
        let mut d = good();
        d.operators.push("budget.allocate".into());
        let registry = Registry::default();

        let ok = WidgetDecl {
            id: "balance_card".into(),
            inputs: vec!["balance".into()],
            render: "readout".into(),
            emits: vec![],
            binding: BindingKey::Unit,
        };
        assert!(widget_installs(ok, &d, &registry).admitted().is_some());

        // ★ The same package, refused against a household that has no such
        //   dimension — which is honest, not a defect in the package.
        let bad = WidgetDecl {
            id: "moisture_card".into(),
            inputs: vec!["moisture".into()],
            render: "readout".into(),
            emits: vec![],
            binding: BindingKey::Unit,
        };
        let v = widget_installs(bad, &d, &registry);
        let InstallVerdict::Refused { rule, errors } = v else { panic!("expected a refusal") };
        assert_eq!(rule, "widget_well_typed");
        assert!(!errors.is_empty());
    }

    // ── operators and strategies ────────────────────────────────────────────

    #[test]
    fn an_operator_package_names_an_operator_and_cannot_carry_one() {
        let registry = Registry::default();
        // Whatever this build ships is real; something invented is not.
        let v = operator_installs("no.such.operator", &registry);
        let InstallVerdict::Refused { rule, errors } = v else { panic!("expected a refusal") };
        assert_eq!(rule, "operator_present");
        assert!(errors[0].contains("cannot carry one"), "{errors:?}");
    }

    #[test]
    fn a_real_operator_name_resolves() {
        let registry = Registry::default();
        let name = registry.names().first().cloned().expect("this build ships operators");
        assert!(operator_installs(&name, &registry).admitted().is_some());
    }

    #[test]
    fn a_strategy_is_publishable_and_honestly_not_installable_here() {
        // ★★★ The third answer. Not a refusal (nothing is wrong with it) and
        //     not a success (nothing would run it).
        let v = strategy_installs("frugal");
        let InstallVerdict::NotHere { why } = &v else { panic!("expected NotHere, got {v:?}") };
        assert!(why.contains("MemeLibrary"), "{why}");
        assert!(v.admitted().is_none());
        assert!(v.describe().contains("nothing here"));
    }

    // ── the three questions ─────────────────────────────────────────────────

    #[test]
    fn integrity_authenticity_and_quality_are_three_questions() {
        // ★★★ Tampering is fatal whoever vouches for it...
        let tampered = PackageProvenance {
            author: "aa".repeat(32),
            origin: Origin::FromPeer { peer: "bob".into() },
            integrity: Integrity::Altered { claimed: "abc".into(), actual: "def".into() },
            authenticity: Authenticity::Signed,
        };
        assert!(!tampered.safe_to_install());
        assert!(tampered.describe().contains("ALTERED"));

        // ...and a forged signature likewise.
        let forged = PackageProvenance {
            author: "aa".repeat(32),
            origin: Origin::FromPeer { peer: "bob".into() },
            integrity: Integrity::Intact,
            authenticity: Authenticity::Forged,
        };
        assert!(!forged.safe_to_install());

        // ★★ But UNSIGNED is not FORGED. A package written here and never sent
        //    anywhere has nothing to prove, and refusing it would make the
        //    local case impossible.
        let local = PackageProvenance {
            author: "aa".repeat(32),
            origin: Origin::Authored,
            integrity: Integrity::Intact,
            authenticity: Authenticity::Unsigned,
        };
        assert!(local.safe_to_install());
        assert!(local.describe().contains("unsigned"));
    }

    #[test]
    fn provenance_is_not_a_permission() {
        // ★★★ The property that keeps a registry from becoming a supply chain:
        //     a package from the most trusted key alive still faces the gate.
        let impeccable = PackageProvenance {
            author: "aa".repeat(32),
            origin: Origin::Authored,
            integrity: Integrity::Intact,
            authenticity: Authenticity::Signed,
        };
        assert!(impeccable.safe_to_install());
        // And it changes nothing about whether the artifact typechecks.
        assert!(definition_installs("garden", &broken(), &[]).admitted().is_none());
    }

    #[test]
    fn an_imported_origin_can_never_read_as_native() {
        // ★ `MemeProvenance::Imported`'s rule, applied here: the peer is not
        //   optional, so there is no way to represent "arrived from somewhere"
        //   without saying where.
        let p = PackageProvenance {
            author: "aa".repeat(32),
            origin: Origin::FromPeer { peer: "bob.myc".into() },
            integrity: Integrity::Intact,
            authenticity: Authenticity::Signed,
        };
        assert!(p.describe().contains("bob.myc"));
    }
}
