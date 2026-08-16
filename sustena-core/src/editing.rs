//! Edit authority — the gate on the definition (Editing §VI · Capstone §VI.1 duty 5).
//!
//! ```text
//! admit(e, ⟨D,s⟩) ⟺ tok(α,e) ∧ ⊢e(D) ok ∧ Safe(e,μ) ∧ applicable(e,D)
//!                    authority    §II typing   §III migration    guard
//! ```
//!
//! The correspondence with §4E is exact, and worth reading before the code:
//! **typing is the *not ever*** — a fact about the document. **Migration is
//! `o(s) ∈ A`** — a fact about the things that exist. **The token is *not
//! you*.**
//!
//! ## The meta-Sustain
//!
//! Σ↑ has state space 𝒟 (definitions), Enzymes the edit taxonomy of §II, and
//!
//! ```text
//! V↑ = { D' ∈ 𝒟 : ⊢D' ok ∧ ∀i ∈ inst: μ(sᵢ) ∈ V_D' }
//! ```
//!
//! `admit(e, ·)` **is** §4E's gate on Σ↑, so §4E's induction applies unchanged:
//! start from a well-typed definition under which every instance is viable,
//! admit only gated edits, and **every definition the system ever holds has
//! that property**. The editing engine is not a subsystem beside the system;
//! it is the system, with `𝒮 = 𝒟`.
//!
//! ## Authority does not lift from below
//!
//! This is §VI's load-bearing point and the reason this module exists at all.
//! `V↑` is defined *over the instances*, so one move in Σ↑ can affect every
//! instance at once while a move in any single Σ affects one.
//!
//! > Permission to spend from a pocket is not permission to redefine what a
//! > pocket is.
//!
//! So the edit token is **not** derived from object-level privilege, and it is
//! **not** an ownership check. For a [`Governance::Shared`] definition it can
//! be minted **only** by a council decision — an object-level transition needs
//! one actor, a meta-level transition needs a quorum. *"Changing the rules is
//! harder than following them"* becomes a structural property rather than a
//! convention.
//!
//! ## What is unrepresentable here
//!
//! - **An edit reaching live without authority.** [`EditEffect::Live`] carries
//!   the token. Unlike the operator gate, this one has **no `Unchecked`
//!   variant at all** — there is no prior behaviour to preserve, so no bypass
//!   needed a name, so none exists.
//! - **A shared definition edited by one actor.** [`EditToken::mint`] will not
//!   produce a token for [`Governance::Shared`] from a principal; it requires a
//!   [`CouncilMint`], which in turn cannot be built from a proposal that did
//!   not pass **and** did not meet quorum.
//! - **A token for a different edit.** As in [`crate::approval`], the binding
//!   is kept structurally and compared by equality, not by digest.
//!
//! ## Scope, stated rather than implied
//!
//! `Safe(e, μ)` is built here **for `μ = id`** — the *compatible* case, which
//! is the same classification the reference engine makes (`compatible`, or
//! refused). A real migration function has no representation anywhere yet;
//! that is EDIT-12 (expand–migrate–contract), a separate and larger piece, and
//! [`Migration`] names it as a composed dependency rather than pretending.
//!
//! The taxonomy covers the **structural**, **regional** and **transitional**
//! families. `ModifyOp(o, g', ε')` and the peripheral family
//! (`SetUtility`, `RebindWidget`) are deliberately absent: guards and effects
//! are Rust functions in this core, not data, and there is no utility or widget
//! model to edit. A variant whose typing obligation could not be discharged
//! would be a stub wearing a type's clothes.

use serde_json::Value;
use thiserror::Error;

use crate::council::ProposalStatus;
use crate::predicate::{self, parse_predicate};
use crate::schema::{bind, DimType, Schema, TypeError};

/// A definition **D** — the state of the meta-Sustain.
///
/// Only the parts this core can actually reason about: the declared state
/// space, the invariants that cut `V` out of it, and the operator allow-list.
/// The allow-list is genuinely data here (the gate reads it on every call), so
/// transitional edits are checkable; operator *bodies* are not.
#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    pub schema: Schema,
    /// `(id, expression)` — the same shape the operator gate consumes.
    pub invariants: Vec<(String, String)>,
    /// The operators this definition permits, by name.
    pub operators: Vec<String>,
}

impl Definition {
    pub fn new(schema: Schema) -> Self {
        Self { schema, invariants: vec![], operators: vec![] }
    }

    pub fn with_invariant(mut self, id: &str, expression: &str) -> Self {
        self.invariants.push((id.to_string(), expression.to_string()));
        self
    }

    pub fn with_operator(mut self, name: &str) -> Self {
        self.operators.push(name.to_string());
        self
    }

    fn has_invariant(&self, id: &str) -> bool {
        self.invariants.iter().any(|(i, _)| i == id)
    }
}

/// The typed edit taxonomy (§II), in the three families this core can type.
///
/// > Edits are not an open set of arbitrary functions.
///
/// An untyped edit — *"here is a new JSON document, please use this one"* — has
/// no kind, and therefore no per-kind rule can fire. That is precisely the gap
/// this enum closes: the reference engine rebuilds a whole candidate spec and
/// diffs nothing, so it cannot tell a `DropInv` from an `AddInv`.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    // ── Structural — acting on 𝒮 ────────────────────────────────────────────
    AddDim { name: String, ty: DimType, default: Value },
    RetypeDim { name: String, ty: DimType },
    RetireDim { name: String },

    // ── Regional — acting on Inv, and therefore on V ────────────────────────
    AddInv { id: String, expression: String },
    DropInv { id: String },
    ModifyInv { id: String, expression: String },

    // ── Transitional — acting on T (the declared allow-list) ────────────────
    AddOp { name: String },
    RetireOp { name: String },
}

impl Edit {
    /// A short stable name, for vectors and messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Edit::AddDim { .. } => "AddDim",
            Edit::RetypeDim { .. } => "RetypeDim",
            Edit::RetireDim { .. } => "RetireDim",
            Edit::AddInv { .. } => "AddInv",
            Edit::DropInv { .. } => "DropInv",
            Edit::ModifyInv { .. } => "ModifyInv",
            Edit::AddOp { .. } => "AddOp",
            Edit::RetireOp { .. } => "RetireOp",
        }
    }

    /// `applicable(e, D)` — the per-kind guard.
    ///
    /// Adding a dimension that already exists is not a stricter edit, it is a
    /// confused one; dropping an invariant that was never there is not a
    /// loosening, it is a mistake. Each kind carries its own applicability
    /// obligation, which is what "typed" buys.
    pub fn applicable(&self, d: &Definition) -> Result<(), EditError> {
        let declared = |name: &str| d.schema.top_level().any(|n| n == name);
        match self {
            Edit::AddDim { name, .. } => {
                if declared(name) {
                    return Err(EditError::NotApplicable {
                        kind: self.kind(),
                        reason: format!("dimension '{name}' is already declared"),
                    });
                }
            }
            Edit::RetypeDim { name, .. } | Edit::RetireDim { name } => {
                if !declared(name) {
                    return Err(EditError::NotApplicable {
                        kind: self.kind(),
                        reason: format!("dimension '{name}' is not declared"),
                    });
                }
            }
            Edit::AddInv { id, .. } => {
                if d.has_invariant(id) {
                    return Err(EditError::NotApplicable {
                        kind: self.kind(),
                        reason: format!("invariant '{id}' already exists"),
                    });
                }
            }
            Edit::DropInv { id } | Edit::ModifyInv { id, .. } => {
                if !d.has_invariant(id) {
                    return Err(EditError::NotApplicable {
                        kind: self.kind(),
                        reason: format!("invariant '{id}' does not exist"),
                    });
                }
            }
            Edit::AddOp { name } => {
                if d.operators.iter().any(|o| o == name) {
                    return Err(EditError::NotApplicable {
                        kind: self.kind(),
                        reason: format!("operator '{name}' is already declared"),
                    });
                }
            }
            Edit::RetireOp { name } => {
                if !d.operators.iter().any(|o| o == name) {
                    return Err(EditError::NotApplicable {
                        kind: self.kind(),
                        reason: format!("operator '{name}' is not declared"),
                    });
                }
            }
        }
        Ok(())
    }

    /// `e(D)` — produce the candidate definition. Pure; nothing is committed.
    pub fn apply(&self, d: &Definition) -> Definition {
        let mut out = d.clone();
        match self {
            Edit::AddDim { name, ty, .. } => {
                out.schema = out.schema.clone().declare(name, ty.clone());
            }
            Edit::RetypeDim { name, ty } => {
                out.schema = out.schema.clone().declare(name, ty.clone());
            }
            Edit::RetireDim { name } => {
                out.schema = out.schema.without(name);
            }
            Edit::AddInv { id, expression } => {
                out.invariants.push((id.clone(), expression.clone()));
            }
            Edit::DropInv { id } => {
                out.invariants.retain(|(i, _)| i != id);
            }
            Edit::ModifyInv { id, expression } => {
                for (i, expr) in out.invariants.iter_mut() {
                    if i == id {
                        *expr = expression.clone();
                    }
                }
            }
            Edit::AddOp { name } => out.operators.push(name.clone()),
            Edit::RetireOp { name } => out.operators.retain(|o| o != name),
        }
        out
    }

    /// `e⁻¹` — the inverse edit, derived against the definition the edit was
    /// applied TO (Editing §V).
    ///
    /// > The taxonomy is closed under inversion by construction:
    /// > `AddDim⁻¹ = RetireDim`, `DropInv⁻¹ = AddInv`,
    /// > `ModifyOp(o,g',ε')⁻¹ = ModifyOp(o,g,ε)`.
    ///
    /// This is the half the article calls **nearly always achievable** — it
    /// restores the *document*. The other half, `μ⁻¹(μ(s)) = s`, is where
    /// honesty is required and lives in [`crate::version`].
    ///
    /// The parent definition is needed because half the taxonomy is only
    /// invertible against what was there before: undoing a `DropInv` means
    /// knowing the expression that was dropped. That is the pre-image, read
    /// from the value the edit was applied to rather than inferred.
    pub fn inverse(&self, parent: &Definition) -> Result<Edit, Irreversible> {
        let missing = |what: String| Irreversible { kind: self.kind(), detail: what };
        Ok(match self {
            Edit::AddDim { name, .. } => Edit::RetireDim { name: name.clone() },

            // Restoring a retired dimension needs its declared type back. The
            // TYPE is in the parent schema; the DEFAULT is not part of a
            // Definition at all, which is exactly the lossy seam §V is about —
            // see `version::Rollback`, which reports it rather than papering
            // over it.
            Edit::RetireDim { name } => {
                let ty = parent
                    .schema
                    .dimensions
                    .get(name)
                    .cloned()
                    .ok_or_else(|| missing(format!("dimension '{name}' is not in the parent")))?;
                Edit::AddDim { name: name.clone(), ty, default: Value::Null }
            }

            Edit::RetypeDim { name, .. } => {
                let ty = parent
                    .schema
                    .dimensions
                    .get(name)
                    .cloned()
                    .ok_or_else(|| missing(format!("dimension '{name}' is not in the parent")))?;
                Edit::RetypeDim { name: name.clone(), ty }
            }

            Edit::AddInv { id, .. } => Edit::DropInv { id: id.clone() },

            Edit::DropInv { id } | Edit::ModifyInv { id, .. } => {
                let expression = parent
                    .invariants
                    .iter()
                    .find(|(i, _)| i == id)
                    .map(|(_, e)| e.clone())
                    .ok_or_else(|| missing(format!("invariant '{id}' is not in the parent")))?;
                match self {
                    Edit::DropInv { .. } => Edit::AddInv { id: id.clone(), expression },
                    _ => Edit::ModifyInv { id: id.clone(), expression },
                }
            }

            Edit::AddOp { name } => Edit::RetireOp { name: name.clone() },
            Edit::RetireOp { name } => Edit::AddOp { name: name.clone() },
        })
    }

    /// Whether undoing this edit can restore live instances exactly — i.e.
    /// whether `μ` was injective for this kind.
    ///
    /// `RetireDim` forgets: the dimension's declared default is not part of a
    /// `Definition`, so undoing it cannot say what new instances should start
    /// with unless the log or an explicit pre-image supplies it. Everything
    /// else in this taxonomy is document-only and loses nothing.
    pub fn forgets(&self) -> Option<&str> {
        match self {
            Edit::RetireDim { name } => Some(name),
            _ => None,
        }
    }

    /// Whether this kind can strand a live instance.
    ///
    /// §IV's asymmetry: **loosening is free by proposition** — a weaker `V`
    /// cannot strand a state already inside a stronger one — so only edits that
    /// tighten or reshape need the `|inst| × |Inv|` scan. The reference engine
    /// pays for the scan on every edit because it has no `e` to classify.
    ///
    /// Only three kinds can strand, and the reason is exact: `Safe` evaluates
    /// the *candidate's invariants* against live instance states, so an edit
    /// strands only if it **adds or tightens an invariant**, or changes what a
    /// value means. Adding or retiring a dimension, dropping an invariant, and
    /// adding or retiring an operator all leave every existing state exactly as
    /// admissible as it was.
    ///
    /// This is what makes Expand–Migrate–Contract's `e₊` **auto-safe with no
    /// check** (§VIII) rather than merely cheap — see [`crate::migrate`].
    pub fn can_strand(&self) -> bool {
        matches!(
            self,
            Edit::AddInv { .. } | Edit::ModifyInv { .. } | Edit::RetypeDim { .. }
        )
    }
}

/// The migration function μ.
///
/// §III classifies every edit as *compatible*, *migratable* or *rejected*.
/// `Identity` decides the first and the third; [`Migration::Apply`] is the
/// **migratable** case, and it is what turns today's discipline from
/// *refuse-if-unsafe* into *migrate* (EDIT-12, shipped 2026-08-12 —
/// see [`crate::migrate`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Migration {
    /// `μ = id`. The instance is judged exactly as it stands.
    #[default]
    Identity,
    /// A declared μ. `Safe(e, μ)` judges **`μ(sᵢ)`**, not `sᵢ` — which is the
    /// whole difference between refusing an edit and carrying the instances
    /// into it.
    Apply(crate::migrate::Mu),
}

/// One instance, as the migration predicate needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub id: String,
    pub state: Value,
}

/// A live instance that would be stranded — the witness set of §III, with the
/// instance and the rule both named.
///
/// A refusal that cannot say *which* sustain and *which* rule is an alarm, not
/// a diagnosis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stranded {
    pub instance_id: String,
    pub invariant_id: String,
    pub expression: String,
    pub reason: String,
}

/// An edit whose inverse could not be derived against the given parent.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{kind} cannot be inverted here: {detail}")]
pub struct Irreversible {
    pub kind: &'static str,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("{kind} is not applicable to this definition: {reason}")]
    NotApplicable { kind: &'static str, reason: String },

    #[error("the edited definition is not well-typed: {0}")]
    NotWellTyped(String),

    #[error("this edit would strand {} live sustain(s) outside their viable region", .0.len())]
    WouldStrand(Vec<Stranded>),

    #[error("{0}")]
    Unauthorized(#[from] AuthorityError),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthorityError {
    #[error("approval is for a different edit: approved '{approved}', attempted '{attempted}'")]
    WrongEdit { approved: String, attempted: String },

    #[error("edit approval was given by '{approved}', but '{acting}' is acting")]
    WrongPrincipal { approved: String, acting: String },

    #[error("edit approval expired at {expiry} (now {now})")]
    Expired { expiry: u64, now: u64 },

    #[error("'{principal}' does not hold this definition")]
    NotTheOwner { principal: String },

    #[error("this definition is shared, so a meta-level edit needs a council decision, not one actor — permission to spend from a pocket is not permission to redefine what a pocket is")]
    SharedNeedsCouncil,

    #[error("the council did not pass this edit (status {status})")]
    CouncilDidNotPass { status: &'static str },

    #[error("the council decision did not meet quorum")]
    QuorumNotMet,
}

/// Who holds a definition — and therefore what it takes to change one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Governance {
    /// One actor holds it. An object-level actor and a meta-level actor happen
    /// to coincide, which is the *only* case where they may.
    SoleOwner(String),
    /// Shared. A meta-level move needs a quorum, never an actor — see
    /// [`CouncilMint`].
    Shared,
}

/// Proof that a council decided an edit.
///
/// The only way to obtain one is [`CouncilMint::from_decision`], which refuses
/// a proposal that did not pass **and** refuses one that did not meet quorum.
/// Private fields, no public constructor: a shared definition cannot be edited
/// by asserting that a council agreed.
///
/// **The composition point.** Quorum *arithmetic* — who counted, against what
/// threshold, under which overlapping-majority rule — is Multiparty's (§4B,
/// M-MUL), and is not built. So `quorum_met` is supplied by the caller and
/// refused when false. That makes the dependency a compile-time obligation the
/// host cannot skip silently, rather than a check this module pretends to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CouncilMint {
    proposal_id: String,
}

impl CouncilMint {
    pub fn from_decision(
        proposal_id: &str,
        status: ProposalStatus,
        quorum_met: bool,
    ) -> Result<Self, AuthorityError> {
        if status != ProposalStatus::Passed {
            return Err(AuthorityError::CouncilDidNotPass { status: status.as_str() });
        }
        if !quorum_met {
            return Err(AuthorityError::QuorumNotMet);
        }
        Ok(Self { proposal_id: proposal_id.to_string() })
    }

    pub fn proposal_id(&self) -> &str {
        &self.proposal_id
    }
}

/// How the authority for an edit was established.
#[derive(Debug, Clone)]
pub enum EditAuthority<'a> {
    /// A single actor, admissible only against [`Governance::SoleOwner`].
    Owner(&'a str),
    /// A council decision — the only route for a shared definition.
    Council(&'a CouncilMint),
}

/// `tok(α, e)` — a bound, expiring authority to make **one specific edit**.
///
/// No public constructor and no public fields: reachable only through
/// [`EditToken::mint`], which is where §VI's "authority does not lift from
/// below" is enforced.
// Not `Eq`: an `AddDim` carries a default `Value`, and JSON numbers are not
// totally equal. `PartialEq` is what the binding comparison needs anyway.
#[derive(Debug, Clone, PartialEq)]
pub struct EditToken {
    edit: Edit,
    principal: String,
    proposal_id: Option<String>,
    expiry: u64,
}

impl EditToken {
    /// Mint an authority token, or explain why this actor cannot have one.
    ///
    /// The whole of §VI's asymmetry lives in this match: a sole-owner
    /// definition takes an actor, a shared one takes a quorum, and nothing
    /// converts one into the other.
    pub fn mint(
        governance: &Governance,
        authority: &EditAuthority,
        edit: Edit,
        expiry: u64,
    ) -> Result<Self, AuthorityError> {
        match (governance, authority) {
            (Governance::SoleOwner(owner), EditAuthority::Owner(principal)) => {
                if owner != principal {
                    return Err(AuthorityError::NotTheOwner {
                        principal: (*principal).to_string(),
                    });
                }
                Ok(Self { edit, principal: (*principal).to_string(), proposal_id: None, expiry })
            }
            // An actor, however privileged at the object level, cannot mint a
            // meta-level token for a shared definition.
            (Governance::Shared, EditAuthority::Owner(_)) => {
                Err(AuthorityError::SharedNeedsCouncil)
            }
            (_, EditAuthority::Council(mint)) => Ok(Self {
                edit,
                principal: format!("council:{}", mint.proposal_id()),
                proposal_id: Some(mint.proposal_id().to_string()),
                expiry,
            }),
        }
    }

    pub fn edit(&self) -> &Edit {
        &self.edit
    }
    pub fn principal(&self) -> &str {
        &self.principal
    }
    pub fn proposal_id(&self) -> Option<&str> {
        self.proposal_id.as_deref()
    }
    pub fn expiry(&self) -> u64 {
        self.expiry
    }

    /// The authority term itself: does this token admit *this* edit, by *this*
    /// actor, *now*?
    pub fn validate(
        &self,
        attempted: &Edit,
        acting: Option<&str>,
        now: u64,
    ) -> Result<(), AuthorityError> {
        if &self.edit != attempted {
            return Err(AuthorityError::WrongEdit {
                approved: self.edit.kind().to_string(),
                attempted: attempted.kind().to_string(),
            });
        }
        // A council-minted token is not held by any one person, so there is no
        // acting principal to match it against.
        if let (Some(acting), None) = (acting, &self.proposal_id) {
            if acting != self.principal {
                return Err(AuthorityError::WrongPrincipal {
                    approved: self.principal.clone(),
                    acting: acting.to_string(),
                });
            }
        }
        if now > self.expiry {
            return Err(AuthorityError::Expired { expiry: self.expiry, now });
        }
        Ok(())
    }
}

/// Whether an edit is real, and — if it is — what authorises it.
///
/// **There is no `Unchecked` variant.** The operator gate needed one to keep
/// R1 parity; the meta-gate is new, so no bypass had to be preserved, and none
/// exists. An unauthorised edit reaching live is not refused here — it cannot
/// be written down.
#[derive(Debug, Clone)]
pub enum EditEffect<'a> {
    /// Type-check and migration-check without authority. Answers "would this
    /// edit be safe?" without being permission to make it.
    Sandbox,
    Live { token: &'a EditToken, now: u64 },
}

/// `⊢ D ok` — the typing judgment, over the **whole** definition.
///
/// > Well-typedness is a property of the whole definition, not of the diff.
///
/// `RetireDim(d)` touches one line and can invalidate six invariants that never
/// mentioned it. There is no local check, so this runs on `D'` entire, every
/// time — affordable exactly because the predicate fragment is small and its
/// checking is linear.
pub fn typecheck(d: &Definition) -> Result<(), Vec<TypeError>> {
    let mut errors = Vec::new();
    for (id, expression) in &d.invariants {
        match parse_predicate(expression) {
            Err(e) => errors.push(TypeError {
                path: id.clone(),
                detail: format!("invariant does not parse: {e}"),
            }),
            Ok(p) => {
                for mut err in bind(&p, &d.schema) {
                    err.detail = format!("invariant '{id}': {}", err.detail);
                    errors.push(err);
                }
            }
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

/// `Safe(e, μ) ⟺ ∀i ∈ inst(D): μ(sᵢ) ∈ V_D'`
///
/// Every live instance's **actual current state** is evaluated against every
/// **candidate** invariant. A candidate that will not compile is reported as a
/// violation against every instance rather than skipped — fail-safe defaults,
/// the §4E principle, applied at the right place. Skipping is how a gate fails
/// open.
pub fn safe(
    candidate: &Definition,
    instances: &[Instance],
    migration: &Migration,
) -> Result<(), Vec<Stranded>> {
    let empty = serde_json::Map::new();
    let mut stranded = Vec::new();

    for instance in instances {
        // `Safe(e, μ) ⟺ ∀i: μ(sᵢ) ∈ V_D'` — the check is on the MIGRATED
        // state. Judging `sᵢ` when a μ was declared would refuse edits the
        // migration was written precisely to make safe.
        let judged = match migration {
            Migration::Identity => instance.state.clone(),
            Migration::Apply(mu) => match mu.apply(&instance.state) {
                Ok(s) => s,
                Err(why) => {
                    stranded.push(Stranded {
                        instance_id: instance.id.clone(),
                        invariant_id: "<migration>".to_string(),
                        expression: "μ".to_string(),
                        reason: format!("migration could not be applied: {why}"),
                    });
                    continue;
                }
            },
        };
        for (id, expression) in &candidate.invariants {
            match predicate::check(expression, &judged, &empty) {
                Err(e) => stranded.push(Stranded {
                    instance_id: instance.id.clone(),
                    invariant_id: id.clone(),
                    expression: expression.clone(),
                    reason: format!("candidate invariant could not be evaluated: {e}"),
                }),
                Ok((false, reason)) => stranded.push(Stranded {
                    instance_id: instance.id.clone(),
                    invariant_id: id.clone(),
                    expression: expression.clone(),
                    reason,
                }),
                Ok((true, _)) => {}
            }
        }
    }
    if stranded.is_empty() { Ok(()) } else { Err(stranded) }
}

/// What an admitted edit produced.
#[derive(Debug, Clone)]
pub struct EditAdmission {
    /// The candidate definition. Present whether or not it was admitted, so a
    /// sandbox run can show what *would* have happened.
    pub candidate: Definition,
    pub verdict: Result<(), EditError>,
}

impl EditAdmission {
    pub fn admitted(&self) -> bool {
        self.verdict.is_ok()
    }
}

/// `admit(e, ⟨D,s⟩)` — the meta-gate, all four terms.
///
/// Order is chosen so the most useful answer comes first and nothing expensive
/// runs for an edit that was never going to be admitted:
///
/// 1. **`tok(α,e)`** — authority. Independent of state; no reason to type-check
///    an edit this actor may not make.
/// 2. **`applicable(e,D)`** — the guard, against the *old* definition.
/// 3. **`⊢e(D) ok`** — typing, over the whole candidate.
/// 4. **`Safe(e,μ)`** — migration, the `|inst| × |Inv|` scan, skipped entirely
///    for a kind that cannot strand (§IV's asymmetry).
pub fn admit_edit(
    definition: &Definition,
    edit: &Edit,
    instances: &[Instance],
    migration: &Migration,
    effect: &EditEffect,
) -> EditAdmission {
    let refuse = |candidate: Definition, err: EditError| EditAdmission {
        candidate,
        verdict: Err(err),
    };

    // ── tok(α, e) ────────────────────────────────────────────────────────────
    if let EditEffect::Live { token, now } = effect {
        let acting = if token.proposal_id().is_some() { None } else { Some(token.principal()) };
        if let Err(e) = token.validate(edit, acting, *now) {
            return refuse(definition.clone(), EditError::Unauthorized(e));
        }
    }

    // ── applicable(e, D) ─────────────────────────────────────────────────────
    if let Err(e) = edit.applicable(definition) {
        return refuse(definition.clone(), e);
    }

    let candidate = edit.apply(definition);

    // ── ⊢ e(D) ok ────────────────────────────────────────────────────────────
    if let Err(errors) = typecheck(&candidate) {
        let joined = errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; ");
        return refuse(candidate, EditError::NotWellTyped(joined));
    }

    // ── Safe(e, μ) ───────────────────────────────────────────────────────────
    // Loosening is free by proposition: a weaker V cannot strand a state
    // already inside a stronger one.
    if edit.can_strand() {
        if let Err(stranded) = safe(&candidate, instances, migration) {
            return refuse(candidate, EditError::WouldStrand(stranded));
        }
    }

    EditAdmission { candidate, verdict: Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn schema() -> Schema {
        Schema::new().declare(
            "moisture",
            DimType::Record {
                fields: BTreeMap::from([("level".into(), DimType::Number { lo: None, hi: None })]),
            },
        )
    }

    fn definition() -> Definition {
        Definition::new(schema()).with_operator("edit.state_patch")
    }

    fn instances(levels: &[(&str, f64)]) -> Vec<Instance> {
        levels
            .iter()
            .map(|(id, v)| Instance { id: (*id).to_string(), state: json!({"moisture": {"level": v}}) })
            .collect()
    }

    fn owner_token(edit: Edit) -> EditToken {
        EditToken::mint(
            &Governance::SoleOwner("bonnie".into()),
            &EditAuthority::Owner("bonnie"),
            edit,
            100,
        )
        .expect("the owner may mint")
    }

    fn live<'a>(token: &'a EditToken, now: u64) -> EditEffect<'a> {
        EditEffect::Live { token, now }
    }

    // ── authority (§VI) ──────────────────────────────────────────────────────

    #[test]
    fn the_owner_of_an_unshared_definition_may_edit_it() {
        let e = Edit::AddInv { id: "floor".into(), expression: "moisture.level >= 0".into() };
        let t = owner_token(e.clone());
        let out = admit_edit(&definition(), &e, &instances(&[("a", 5.0)]), &Migration::Identity, &live(&t, 10));
        assert!(out.admitted(), "{:?}", out.verdict);
        assert_eq!(out.candidate.invariants.len(), 1);
    }

    #[test]
    fn someone_who_does_not_hold_the_definition_gets_no_token() {
        let e = Edit::DropInv { id: "floor".into() };
        let got = EditToken::mint(
            &Governance::SoleOwner("bonnie".into()),
            &EditAuthority::Owner("cira"),
            e,
            100,
        );
        assert!(matches!(got, Err(AuthorityError::NotTheOwner { .. })));
    }

    #[test]
    fn a_shared_definition_cannot_be_edited_by_one_actor() {
        // The load-bearing point of §VI: permission to spend from a pocket is
        // not permission to redefine what a pocket is.
        let e = Edit::DropInv { id: "floor".into() };
        let got = EditToken::mint(&Governance::Shared, &EditAuthority::Owner("bonnie"), e, 100);
        assert_eq!(got.unwrap_err(), AuthorityError::SharedNeedsCouncil);
    }

    #[test]
    fn a_shared_definition_is_edited_by_a_council_decision() {
        let mint = CouncilMint::from_decision("prop-1", ProposalStatus::Passed, true).unwrap();
        let e = Edit::AddInv { id: "floor".into(), expression: "moisture.level >= 0".into() };
        let t = EditToken::mint(&Governance::Shared, &EditAuthority::Council(&mint), e.clone(), 100)
            .expect("a passed, quorate council decision mints the token");
        assert_eq!(t.proposal_id(), Some("prop-1"));

        let out = admit_edit(&definition(), &e, &instances(&[("a", 5.0)]), &Migration::Identity, &live(&t, 10));
        assert!(out.admitted());
    }

    #[test]
    fn a_council_that_did_not_pass_mints_nothing() {
        for status in [ProposalStatus::InVoting, ProposalStatus::Failed, ProposalStatus::Deferred] {
            let got = CouncilMint::from_decision("p", status, true);
            assert!(matches!(got, Err(AuthorityError::CouncilDidNotPass { .. })), "{status:?}");
        }
    }

    #[test]
    fn quorum_is_required_and_is_the_composition_point() {
        // Quorum arithmetic belongs to Multiparty and is not built; refusing
        // when it is not asserted keeps the dependency honest rather than
        // pretending this module counted anything.
        let got = CouncilMint::from_decision("p", ProposalStatus::Passed, false);
        assert_eq!(got.unwrap_err(), AuthorityError::QuorumNotMet);
    }

    #[test]
    fn a_token_for_one_edit_does_not_admit_another() {
        let approved = Edit::AddInv { id: "floor".into(), expression: "moisture.level >= 0".into() };
        let t = owner_token(approved);
        let attempted = Edit::DropInv { id: "floor".into() };
        assert!(matches!(
            t.validate(&attempted, Some("bonnie"), 10),
            Err(AuthorityError::WrongEdit { .. })
        ));
    }

    #[test]
    fn an_edit_approval_expires() {
        let e = Edit::AddInv { id: "f".into(), expression: "moisture.level >= 0".into() };
        let t = owner_token(e.clone());
        assert_eq!(t.validate(&e, Some("bonnie"), 100), Ok(()), "expiry is inclusive");
        assert!(matches!(t.validate(&e, Some("bonnie"), 101), Err(AuthorityError::Expired { .. })));
    }

    #[test]
    fn a_sandbox_edit_needs_no_authority_but_still_meets_every_other_term() {
        let e = Edit::AddInv { id: "floor".into(), expression: "moisture.level >= 0".into() };

        let ok = admit_edit(&definition(), &e, &instances(&[("a", 5.0)]), &Migration::Identity, &EditEffect::Sandbox);
        assert!(ok.admitted(), "no token needed to ask whether an edit would be safe");

        // ...and it is still refused when it would strand an instance.
        let bad = admit_edit(&definition(), &e, &instances(&[("a", -5.0)]), &Migration::Identity, &EditEffect::Sandbox);
        assert!(!bad.admitted(), "the sandbox answers honestly, it does not answer yes");
    }

    // ── applicable(e, D) — the guard ─────────────────────────────────────────

    #[test]
    fn dropping_an_invariant_that_does_not_exist_is_not_a_loosening() {
        let e = Edit::DropInv { id: "nope".into() };
        let t = owner_token(e.clone());
        let out = admit_edit(&definition(), &e, &[], &Migration::Identity, &live(&t, 1));
        assert!(matches!(out.verdict, Err(EditError::NotApplicable { .. })));
    }

    #[test]
    fn adding_a_dimension_that_already_exists_is_refused() {
        let e = Edit::AddDim { name: "moisture".into(), ty: DimType::Any, default: json!({}) };
        let t = owner_token(e.clone());
        let out = admit_edit(&definition(), &e, &[], &Migration::Identity, &live(&t, 1));
        assert!(matches!(out.verdict, Err(EditError::NotApplicable { .. })));
    }

    #[test]
    fn retiring_an_operator_that_was_never_declared_is_refused() {
        let e = Edit::RetireOp { name: "budget.allocate".into() };
        let t = owner_token(e.clone());
        let out = admit_edit(&definition(), &e, &[], &Migration::Identity, &live(&t, 1));
        assert!(matches!(out.verdict, Err(EditError::NotApplicable { .. })));
    }

    // ── ⊢ e(D) ok — typing over the WHOLE definition (§II) ───────────────────

    #[test]
    fn an_invariant_over_an_undeclared_dimension_is_a_typing_error() {
        let e = Edit::AddInv { id: "bad".into(), expression: "nutrients.level >= 0".into() };
        let t = owner_token(e.clone());
        let out = admit_edit(&definition(), &e, &[], &Migration::Identity, &live(&t, 1));
        assert!(matches!(out.verdict, Err(EditError::NotWellTyped(_))), "{:?}", out.verdict);
    }

    #[test]
    fn retiring_a_dimension_invalidates_an_invariant_that_never_mentioned_the_edit() {
        // The reason typing is whole-definition and not a diff: one line out,
        // and a rule written long ago stops making sense.
        let d = definition().with_invariant("floor", "moisture.level >= 0");
        let e = Edit::RetireDim { name: "moisture".into() };
        let t = owner_token(e.clone());
        let out = admit_edit(&d, &e, &[], &Migration::Identity, &live(&t, 1));
        assert!(matches!(out.verdict, Err(EditError::NotWellTyped(_))), "{:?}", out.verdict);
    }

    #[test]
    fn an_invariant_that_will_not_parse_is_a_typing_error_not_a_skip() {
        let e = Edit::AddInv { id: "broken".into(), expression: "this is (not ) valid".into() };
        let t = owner_token(e.clone());
        let out = admit_edit(&definition(), &e, &[], &Migration::Identity, &live(&t, 1));
        assert!(matches!(out.verdict, Err(EditError::NotWellTyped(_))));
    }

    // ── Safe(e, μ) — the migration predicate (§III) ──────────────────────────

    #[test]
    fn an_edit_that_would_strand_a_live_instance_is_refused_and_names_it() {
        let e = Edit::AddInv { id: "floor".into(), expression: "moisture.level >= 0".into() };
        let t = owner_token(e.clone());
        let out = admit_edit(&definition(), &e, &instances(&[("ok", 5.0), ("bad", -3.0)]),
                             &Migration::Identity, &live(&t, 1));
        match out.verdict {
            Err(EditError::WouldStrand(v)) => {
                assert_eq!(v.len(), 1, "only the failing instance is a witness");
                assert_eq!(v[0].instance_id, "bad");
                assert_eq!(v[0].invariant_id, "floor");
                assert!(!v[0].reason.is_empty(), "a refusal must say why");
            }
            other => panic!("expected WouldStrand, got {other:?}"),
        }
    }

    #[test]
    fn the_same_edit_succeeds_once_the_instance_is_back_inside_the_region() {
        let e = Edit::AddInv { id: "floor".into(), expression: "moisture.level >= 0".into() };
        let t = owner_token(e.clone());
        let out = admit_edit(&definition(), &e, &instances(&[("a", 0.0)]), &Migration::Identity, &live(&t, 1));
        assert!(out.admitted(), "the boundary itself is inside the region");
    }

    #[test]
    fn loosening_is_free_and_skips_the_scan() {
        // A DropInv cannot strand anything, so it is admitted even against an
        // instance that violates a DIFFERENT surviving rule — because that
        // instance was already outside, and this edit did not put it there.
        let d = definition()
            .with_invariant("floor", "moisture.level >= 0")
            .with_invariant("ceiling", "moisture.level <= 10");
        let e = Edit::DropInv { id: "ceiling".into() };
        assert!(!e.can_strand());
        let t = owner_token(e.clone());
        let out = admit_edit(&d, &e, &instances(&[("already_bad", -1.0)]), &Migration::Identity, &live(&t, 1));
        assert!(out.admitted(), "a weaker V cannot strand a state already inside a stronger one");
    }

    #[test]
    fn a_no_instance_definition_is_editable_freely() {
        let e = Edit::AddInv { id: "floor".into(), expression: "moisture.level >= 0".into() };
        let t = owner_token(e.clone());
        let out = admit_edit(&definition(), &e, &[], &Migration::Identity, &live(&t, 1));
        assert!(out.admitted(), "nothing exists to strand");
    }

    // ── the meta-Sustain induction ───────────────────────────────────────────

    #[test]
    fn only_gated_edits_keep_every_definition_well_typed_with_instances_viable() {
        // §4E's induction on Σ↑: start inside V↑, admit only gated edits, stay
        // inside V↑. Walked here rather than asserted.
        let mut d = definition();
        let live_instances = instances(&[("a", 5.0)]);
        assert!(typecheck(&d).is_ok());

        for e in [
            Edit::AddInv { id: "floor".into(), expression: "moisture.level >= 0".into() },
            Edit::AddOp { name: "budget.summary".into() },
            Edit::AddInv { id: "ceiling".into(), expression: "moisture.level <= 10".into() },
        ] {
            let t = owner_token(e.clone());
            let out = admit_edit(&d, &e, &live_instances, &Migration::Identity, &live(&t, 1));
            assert!(out.admitted(), "{:?}", out.verdict);
            d = out.candidate;
            assert!(typecheck(&d).is_ok(), "⊢D ok holds at every step");
            assert!(safe(&d, &live_instances, &Migration::Identity).is_ok(), "instances stay viable");
        }
        assert_eq!(d.invariants.len(), 2);
        assert_eq!(d.operators.len(), 2);
    }

    #[test]
    fn a_refused_edit_leaves_the_definition_untouched() {
        let d = definition();
        let e = Edit::AddInv { id: "floor".into(), expression: "moisture.level >= 0".into() };
        let t = owner_token(e.clone());
        let out = admit_edit(&d, &e, &instances(&[("bad", -3.0)]), &Migration::Identity, &live(&t, 1));
        assert!(!out.admitted());
        // The candidate is returned for display, but D itself is a separate
        // value the caller still holds, unchanged.
        assert_eq!(d.invariants.len(), 0, "the live definition is not mutated by a refusal");
    }
}
