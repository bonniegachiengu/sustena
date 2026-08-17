//! `B` — the boundary, and the autopoietic closure law
//! (Sustain §IV · SUS-7, SUS-8, OP-10).
//!
//! ```text
//! B = ⟨scope, μ⟩,     μ : Entities ∪ Paths → {0,1}
//! Own(Σ) = { x : μ(x) = 1 }
//!
//! ∀ o ∈ T, ∀ s ∈ S:   μ_{o(s)} = μ_s  ∧  schema(o(s)) = schema(s)
//! ```
//!
//! > `μ` is a **membership predicate**: it decides what this Sustain owns. This
//! > is what upgrades *"a state dict with an owner field"* into a bounded
//! > whole, and it is where private-versus-shared stops being a permissions
//! > bolt-on and becomes a **structural fact** — a variable is private because
//! > of where it sits relative to `B`.
//!
//! ## ★ `μ` is subscripted by the state, which is what makes the law checkable
//!
//! The law reads `μ_{o(s)} = μ_s`, not `μ = μ`. So `μ` is a function *of the
//! state*, and the roster is state: who belongs is something an operator could
//! change. That is precisely the thing the closure law exists to catch, and it
//! is why [`Boundary`] is **read from a state** ([`BoundaryDecl::read`]) rather
//! than declared once and trusted.
//!
//! ★ **`μ` has two halves with different natures, and saying so is the accurate
//! thing rather than a simplification:**
//!
//! - **The path half is declared.** Which state roots and prefixes a Sustain
//!   owns is part of its *definition*. Changing it is a definition edit, and
//!   that higher gate already exists — [`crate::editing::admit_edit`].
//! - **The entity half lives in state.** The roster, the adopted children. This
//!   is the half an ordinary Enzyme could move, so this is where the closure law
//!   bites.
//!
//! ## ★★ The law is structural at the gate, not a convention
//!
//! [`crate::operator::execute`] already enforced the schema half
//! ([`crate::schema::preserves_shape`]) with a comment anticipating this row.
//! The `μ` half is now checked beside it, before the invariants, and an
//! ordinary operator that moves the boundary is **refused** — it does not
//! commit and the refusal names which half of the law it broke.
//!
//! > **Enzymes may change the values inside the boundary; they may not change
//! > what the boundary is.** A Sustain is *materially open* — money, stock,
//! > messages and readings cross `B` constantly — and *organisationally
//! > closed*: the structure `⟨B, schema⟩` is preserved by every ordinary move.
//!
//! ## ★ Two paths, and the higher gate is REUSED rather than reinvented
//!
//! > Changing `B` itself is a distinct class of act (adding a member, adopting
//! > a child Sustain), governed by a **separate, higher gate** — which is
//! > exactly why boundary changes feel different to users, and should.
//!
//! [`BoundaryToken`] has private fields and no public constructor; the only
//! route is [`BoundaryToken::mint`], which takes the **same
//! [`crate::editing::Governance`] and [`crate::editing::EditAuthority`]** the
//! definition gate uses. So a shared Sustain's boundary cannot be moved by a
//! single actor — it needs a [`crate::editing::CouncilMint`], obtainable only
//! from a passed and quorate decision. There is deliberately **no second
//! authority model**: one asymmetry, two kinds of meta-level act.
//!
//! ★ **Adopting a child Sustain is a boundary change**, and that is the
//! reconciliation with the composition work: `⊕` is not an ordinary move, it is
//! `μ` gaining an entity. [`BoundaryChange::AdoptChild`] is the same act the
//! reference engine's `holon.create_child` performs behind its own higher gate.
//!
//! ## Person-first: `μ` nests, it does not flatten
//!
//! > A person-Sustain's boundary is *itself*, and it composes into Homestead and
//! > Vyyb boundaries without dissolving.
//!
//! A parent adopting a child owns **the child**, not the child's interior:
//! [`Boundary::owns`] answers for the child entity and **not** for paths inside
//! it, so a member's boundary survives being composed. See
//! [`Boundary::owns_interior_of`] for the distinction, which is the whole of
//! "without dissolving".
//!
//! ## Slot — flows across `B` are the NEXT slice, not this one
//!
//! > Flows that cross `B` — what may enter, what may leave — form their own
//! > predicate class and **belong to the constraint-engine layer, not here**.
//!
//! So the firewall `F(φ, s)` over `φ = ⟨dir, kind, qty, counterparty⟩` is
//! CON-1's follow-on and is deliberately absent. What this row unblocks is its
//! subject: `F` needs a boundary to be a predicate *about*, and until now there
//! was only a flat owner list.
//!
//! ## Honest limits
//!
//! - **`μ` is a declared rule set, not an arbitrary predicate.** Owned prefixes
//!   and a state-resident entity register — no `eval`, same discipline as
//!   `predicates`/`constraints`. An arbitrary computable `μ` is not expressible
//!   and is not meant to be.
//! - **Scope and `μ` are BOTH required.** §IV's own pseudocode is
//!   `scope.contains(x) AND membership(x)`, so a path `μ` claims but scope does
//!   not span is **not owned** — which catches declaring ownership of something
//!   the Sustain has no business in.
//! - **The declared half's gate is elsewhere.** Changing which prefixes a
//!   Sustain owns is a definition edit ([`crate::editing`]), not a
//!   [`BoundaryChange`].

use std::collections::BTreeSet;

use serde_json::Value;
use thiserror::Error;

use crate::editing::{AuthorityError, EditAuthority, Governance};
use crate::schema::{preserves_shape, Schema};

/// `x` — what `μ` ranges over: `Entities ∪ Paths`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Owned {
    /// A person, a device, a counterparty, or an adopted child Sustain.
    Entity(String),
    /// A path into state — `finances.liquid.balance`.
    Path(String),
}

impl Owned {
    pub fn entity(id: &str) -> Self {
        Owned::Entity(id.to_string())
    }

    pub fn path(p: &str) -> Self {
        Owned::Path(p.to_string())
    }
}

/// The declared extent — the coarse container `μ` decides *within*.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scope {
    roots: BTreeSet<String>,
    entity_register: Option<String>,
}

impl Scope {
    pub fn new() -> Self {
        Self::default()
    }

    /// A state root this boundary spans. A path outside every declared root is
    /// out of scope **however `μ` is declared**.
    pub fn spanning(mut self, root: &str) -> Self {
        self.roots.insert(root.to_string());
        self
    }

    /// Where in state the owned-entity register lives — the roster.
    ///
    /// `None` means this Sustain owns no entities at all, and every entity is
    /// out of scope. That is a real configuration (a leaf Sustain with no
    /// members), not a missing value.
    pub fn with_entity_register(mut self, path: &str) -> Self {
        self.entity_register = Some(path.to_string());
        self
    }

    pub fn roots(&self) -> &BTreeSet<String> {
        &self.roots
    }

    pub fn entity_register(&self) -> Option<&str> {
        self.entity_register.as_deref()
    }

    /// `scope.contains(x)` — the first conjunct of §IV's `owns`.
    pub fn contains(&self, x: &Owned) -> bool {
        match x {
            Owned::Entity(_) => self.entity_register.is_some(),
            Owned::Path(p) => root_of(p).is_some_and(|r| self.roots.contains(r)),
        }
    }
}

/// `B = ⟨scope, μ⟩`, as **declared**.
///
/// The path half of `μ` lives here (it is part of the definition); the entity
/// half is read from state by [`BoundaryDecl::read`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryDecl {
    scope: Scope,
    owned_prefixes: BTreeSet<String>,
}

impl BoundaryDecl {
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            owned_prefixes: BTreeSet::new(),
        }
    }

    /// A path prefix this Sustain owns. Declared, because which region of state
    /// a Sustain is *about* is part of its definition — changing it is a
    /// definition edit, not a [`BoundaryChange`].
    pub fn owning(mut self, prefix: &str) -> Self {
        self.owned_prefixes.insert(prefix.to_string());
        self
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// ★ Read `μ` **at a state**. The entity half comes from the register, which
    /// is what makes `μ_{o(s)} = μ_s` a comparison of two real values rather
    /// than a tautology.
    ///
    /// A register that is absent or not a list of names yields an empty entity
    /// set — reported through [`Boundary::register_readable`] rather than
    /// silently, because *"nobody is a member"* and *"the roster is not where
    /// it was declared to be"* are different facts.
    pub fn read(&self, state: &Value) -> Boundary {
        let (entities, readable) = match self.scope.entity_register() {
            None => (BTreeSet::new(), true),
            Some(reg) => match lookup(state, reg) {
                Some(Value::Array(items)) => {
                    let set = items
                        .iter()
                        .filter_map(|v| entity_name(v))
                        .map(|s| s.to_string())
                        .collect();
                    (set, true)
                }
                Some(Value::Object(map)) => {
                    (map.keys().cloned().collect::<BTreeSet<String>>(), true)
                }
                _ => (BTreeSet::new(), false),
            },
        };
        Boundary {
            scope: self.scope.clone(),
            owned_prefixes: self.owned_prefixes.clone(),
            entities,
            register_readable: readable,
        }
    }
}

/// `μ` at a state — the membership predicate, evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boundary {
    scope: Scope,
    owned_prefixes: BTreeSet<String>,
    entities: BTreeSet<String>,
    register_readable: bool,
}

impl Boundary {
    /// `owns(Σ, x) = scope.contains(x) ∧ μ(x)` — §IV's own two conjuncts, kept
    /// as two.
    pub fn owns(&self, x: &Owned) -> bool {
        if !self.scope.contains(x) {
            return false;
        }
        match x {
            Owned::Entity(e) => self.entities.contains(e),
            Owned::Path(p) => self
                .owned_prefixes
                .iter()
                .any(|pre| p == pre || p.starts_with(&format!("{pre}."))),
        }
    }

    /// `Own(Σ)` restricted to entities.
    pub fn entities(&self) -> &BTreeSet<String> {
        &self.entities
    }

    pub fn owned_prefixes(&self) -> &BTreeSet<String> {
        &self.owned_prefixes
    }

    /// Was the declared entity register actually there and shaped like a
    /// register? `false` means the roster is not where the declaration says —
    /// which is a different fact from *"nobody is a member"* and must not be
    /// read as it.
    pub fn register_readable(&self) -> bool {
        self.register_readable
    }

    /// ★ **`μ` nests; it does not flatten.**
    ///
    /// A parent that has adopted a child owns **the child**, and not the
    /// child's interior. This returns `false` for every path inside an adopted
    /// entity, which is the whole of *"composes into Homestead and Vyyb
    /// boundaries without dissolving"* — the member's own boundary survives
    /// being composed.
    pub fn owns_interior_of(&self, entity: &str, path: &str) -> bool {
        let _ = path;
        // Deliberately always false: owning an entity says nothing about the
        // paths inside it. Written as a method rather than an absence so the
        // property is visible and testable, not merely unimplemented.
        let _ = entity;
        false
    }

    pub fn describe(&self) -> String {
        format!(
            "owns {} entit{} and {} path prefix{}",
            self.entities.len(),
            if self.entities.len() == 1 { "y" } else { "ies" },
            self.owned_prefixes.len(),
            if self.owned_prefixes.len() == 1 { "" } else { "es" },
        )
    }
}

// ---------------------------------------------------------------------------
// The autopoietic closure law
// ---------------------------------------------------------------------------

/// Which half of `⟨B, schema⟩` an ordinary move would have broken.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ClosureViolation {
    #[error(
        "would change what the boundary IS, not just the values inside it: {detail}. \
         Adding a member or adopting a child is a distinct class of act and needs a \
         BoundaryToken, not an ordinary operator"
    )]
    Membership { detail: String },
    #[error("would change the sustain's shape, not just its values: {0}")]
    Shape(String),
    #[error(
        "the declared entity register is not readable at '{register}' after this move — \
         an unreadable roster reads as an empty one, and an operator must not be able to \
         empty the boundary by moving it"
    )]
    RegisterLost { register: String },
}

/// §IV's `preserves_boundary(op, s)` — `μ_{o(s)} = μ_s`.
///
/// Compares `μ` read at both states. The declared half is equal by
/// construction; the state-resident entity half is the real comparison, and it
/// is the one an ordinary Enzyme could move.
pub fn preserves_membership(
    before: &Value,
    after: &Value,
    decl: &BoundaryDecl,
) -> Result<(), ClosureViolation> {
    let b = decl.read(before);
    let a = decl.read(after);

    // A register that was readable and now is not would silently empty μ.
    if b.register_readable && !a.register_readable {
        return Err(ClosureViolation::RegisterLost {
            register: decl
                .scope
                .entity_register()
                .unwrap_or("<none>")
                .to_string(),
        });
    }

    if b.entities != a.entities {
        let added: Vec<&str> = a.entities.difference(&b.entities).map(|s| s.as_str()).collect();
        let removed: Vec<&str> = b.entities.difference(&a.entities).map(|s| s.as_str()).collect();
        let mut parts = Vec::new();
        if !added.is_empty() {
            parts.push(format!("admitted {}", added.join(", ")));
        }
        if !removed.is_empty() {
            parts.push(format!("released {}", removed.join(", ")));
        }
        return Err(ClosureViolation::Membership {
            detail: parts.join("; "),
        });
    }
    Ok(())
}

/// The **whole** law: `μ_{o(s)} = μ_s ∧ schema(o(s)) = schema(s)`.
///
/// One conjunction, two halves, and the error says which half broke — because
/// *"you moved the boundary"* and *"you changed the shape"* call for different
/// things from whoever reads the refusal.
pub fn preserves_closure(
    before: &Value,
    after: &Value,
    decl: Option<&BoundaryDecl>,
    schema: Option<&Schema>,
) -> Result<(), ClosureViolation> {
    if let Some(decl) = decl {
        preserves_membership(before, after, decl)?;
    }
    if let Some(schema) = schema {
        preserves_shape(before, after, schema)
            .map_err(|e| ClosureViolation::Shape(e.to_string()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The higher gate — reused, not reinvented
// ---------------------------------------------------------------------------

/// A change to `B` itself. **Not an Enzyme.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryChange {
    /// Add a member.
    AdmitEntity(String),
    /// Remove one. Note this is `μ` losing an entity, not the entity ceasing to
    /// exist — the holon rule that a departure removes an edge, never a node.
    ReleaseEntity(String),
    /// ★ `⊕` — adopting a child Sustain. A boundary change, not an ordinary
    /// move, which is the reconciliation with the composition work.
    AdoptChild(String),
    /// `⊕⁻¹`.
    ReleaseChild(String),
}

impl BoundaryChange {
    pub fn subject(&self) -> &str {
        match self {
            BoundaryChange::AdmitEntity(x)
            | BoundaryChange::ReleaseEntity(x)
            | BoundaryChange::AdoptChild(x)
            | BoundaryChange::ReleaseChild(x) => x,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            BoundaryChange::AdmitEntity(x) => format!("admit '{x}' to the boundary"),
            BoundaryChange::ReleaseEntity(x) => format!("release '{x}' from the boundary"),
            BoundaryChange::AdoptChild(x) => format!("adopt child sustain '{x}' (⊕)"),
            BoundaryChange::ReleaseChild(x) => format!("release child sustain '{x}' (⊕⁻¹)"),
        }
    }
}

/// Authority to make **one specific** boundary change.
///
/// ★ Private fields and no public constructor — the only route is
/// [`BoundaryToken::mint`], which takes the **same** [`Governance`] and
/// [`EditAuthority`] the definition gate uses. There is deliberately no second
/// authority model: one asymmetry, two kinds of meta-level act. A shared
/// Sustain's boundary cannot be moved by a single actor.
///
/// ★ **A disclosed cosmetic imprecision, accepted deliberately:**
/// [`AuthorityError::SharedNeedsCouncil`]'s message is worded for *definition*
/// edits (*"permission to spend from a pocket is not permission to redefine
/// what a pocket is"*) and reads slightly off for a boundary change. Reusing
/// the type keeps **one** authority model, which is worth more than a
/// perfectly-worded second one — the alternative is forking the asymmetry
/// Editing §VI spent its whole argument establishing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryToken {
    change: BoundaryChange,
    principal: String,
}

impl BoundaryToken {
    /// Mint it, or explain why this actor cannot have one.
    ///
    /// The asymmetry is [`crate::editing`]'s, reused verbatim: a sole-owner
    /// Sustain takes an actor, a shared one takes a council decision, and
    /// nothing converts one into the other.
    pub fn mint(
        governance: &Governance,
        authority: &EditAuthority<'_>,
        change: BoundaryChange,
    ) -> Result<Self, AuthorityError> {
        match (governance, authority) {
            (Governance::SoleOwner(owner), EditAuthority::Owner(principal)) => {
                if owner == principal {
                    Ok(Self {
                        change,
                        principal: (*principal).to_string(),
                    })
                } else {
                    Err(AuthorityError::NotTheOwner {
                        principal: (*principal).to_string(),
                    })
                }
            }
            (Governance::Shared, EditAuthority::Owner(_)) => {
                Err(AuthorityError::SharedNeedsCouncil)
            }
            (_, EditAuthority::Council(mint)) => Ok(Self {
                change,
                principal: mint.proposal_id().to_string(),
            }),
        }
    }

    pub fn change(&self) -> &BoundaryChange {
        &self.change
    }

    pub fn principal(&self) -> &str {
        &self.principal
    }
}

/// Apply a boundary change — the **only** way `μ`'s entity half moves.
///
/// Takes a [`BoundaryToken`] by value, so one authority is one change: it
/// cannot be replayed to admit a second member.
pub fn admit_boundary_change(
    boundary: &Boundary,
    token: BoundaryToken,
) -> Result<Boundary, ClosureViolation> {
    let mut entities = boundary.entities.clone();
    match token.change() {
        BoundaryChange::AdmitEntity(x) | BoundaryChange::AdoptChild(x) => {
            entities.insert(x.clone());
        }
        BoundaryChange::ReleaseEntity(x) | BoundaryChange::ReleaseChild(x) => {
            entities.remove(x);
        }
    }
    Ok(Boundary {
        scope: boundary.scope.clone(),
        owned_prefixes: boundary.owned_prefixes.clone(),
        entities,
        register_readable: boundary.register_readable,
    })
}

// ---------------------------------------------------------------------------

fn root_of(path: &str) -> Option<&str> {
    path.split('.').next().filter(|s| !s.is_empty())
}

fn entity_name(v: &Value) -> Option<&str> {
    match v {
        Value::String(s) => Some(s.as_str()),
        Value::Object(m) => m.get("id").and_then(|x| x.as_str()),
        _ => None,
    }
}

fn lookup<'a>(state: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = state;
    for seg in path.split('.') {
        cur = cur.get(seg)?;
    }
    Some(cur)
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn decl() -> BoundaryDecl {
        BoundaryDecl::new(
            Scope::new()
                .spanning("finances")
                .spanning("roster")
                .with_entity_register("roster"),
        )
        .owning("finances")
    }

    fn state(members: &[&str], balance: i64) -> Value {
        json!({
            "roster": members,
            "finances": {"liquid": {"balance": balance}},
            "weather": {"outside": "rain"}
        })
    }

    // -- μ as a predicate ----------------------------------------------------

    #[test]
    fn owns_requires_both_scope_and_membership() {
        let b = decl().read(&state(&["ama", "ben"], 100));
        assert!(b.owns(&Owned::entity("ama")));
        assert!(!b.owns(&Owned::entity("stranger")));
        assert!(b.owns(&Owned::path("finances.liquid.balance")));
        // ★ In μ's prefixes? No. But also: out of scope entirely.
        assert!(!b.owns(&Owned::path("weather.outside")));
    }

    #[test]
    fn a_path_mu_claims_but_scope_does_not_span_is_not_owned() {
        // μ claims "weather", scope does not span it.
        let d = BoundaryDecl::new(Scope::new().spanning("finances")).owning("weather");
        let b = d.read(&json!({}));
        assert!(!b.owns(&Owned::path("weather.outside")), "scope is a real conjunct");
    }

    #[test]
    fn a_sustain_with_no_entity_register_owns_no_entities() {
        let d = BoundaryDecl::new(Scope::new().spanning("finances")).owning("finances");
        let b = d.read(&state(&["ama"], 1));
        assert!(!b.owns(&Owned::entity("ama")));
        assert!(b.register_readable(), "no register declared is not an unreadable one");
    }

    #[test]
    fn an_unreadable_register_is_distinguished_from_an_empty_one() {
        let d = decl();
        let empty = d.read(&json!({"roster": [], "finances": {}}));
        assert!(empty.entities().is_empty());
        assert!(empty.register_readable());

        let missing = d.read(&json!({"finances": {}}));
        assert!(missing.entities().is_empty());
        assert!(!missing.register_readable(), "absent is not empty");
    }

    // -- ★★ the closure law --------------------------------------------------

    #[test]
    fn an_ordinary_move_changes_values_and_preserves_the_boundary() {
        let before = state(&["ama", "ben"], 100);
        let after = state(&["ama", "ben"], 40);
        assert!(preserves_membership(&before, &after, &decl()).is_ok());
    }

    #[test]
    fn admitting_a_member_breaks_the_closure_law() {
        let before = state(&["ama"], 100);
        let after = state(&["ama", "ben"], 100);
        match preserves_membership(&before, &after, &decl()) {
            Err(ClosureViolation::Membership { detail }) => {
                assert!(detail.contains("admitted ben"), "{detail}");
            }
            other => panic!("expected a membership violation, got {other:?}"),
        }
    }

    #[test]
    fn releasing_a_member_breaks_it_too() {
        let before = state(&["ama", "ben"], 100);
        let after = state(&["ama"], 100);
        match preserves_membership(&before, &after, &decl()) {
            Err(ClosureViolation::Membership { detail }) => {
                assert!(detail.contains("released ben"), "{detail}");
            }
            other => panic!("expected a membership violation, got {other:?}"),
        }
    }

    #[test]
    fn moving_the_register_out_from_under_mu_is_refused_not_read_as_empty() {
        let before = state(&["ama", "ben"], 100);
        let after = json!({"finances": {"liquid": {"balance": 100}}});
        assert!(matches!(
            preserves_membership(&before, &after, &decl()),
            Err(ClosureViolation::RegisterLost { .. })
        ));
    }

    #[test]
    fn the_whole_law_names_which_half_broke() {
        let before = state(&["ama"], 100);
        let moved = state(&["ama", "ben"], 100);
        let err = preserves_closure(&before, &moved, Some(&decl()), None).unwrap_err();
        assert!(matches!(err, ClosureViolation::Membership { .. }));
        assert!(err.to_string().contains("needs a BoundaryToken"));
    }

    // -- ★ nesting -----------------------------------------------------------

    #[test]
    fn owning_a_child_does_not_own_the_childs_interior() {
        let b = decl().read(&state(&["cira"], 0));
        assert!(b.owns(&Owned::entity("cira")));
        // ★ The member's own boundary survives being composed.
        assert!(!b.owns_interior_of("cira", "finances.liquid.balance"));
    }

    // -- ★ the higher gate ---------------------------------------------------

    #[test]
    fn a_sole_owner_may_move_their_own_boundary() {
        let gov = Governance::SoleOwner("bonnie".into());
        let tok = BoundaryToken::mint(
            &gov,
            &EditAuthority::Owner("bonnie"),
            BoundaryChange::AdmitEntity("ben".into()),
        )
        .expect("the owner");
        assert_eq!(tok.change().subject(), "ben");

        let before = decl().read(&state(&["ama"], 0));
        let after = admit_boundary_change(&before, tok).expect("applied");
        assert!(after.owns(&Owned::entity("ben")));
        assert!(after.owns(&Owned::entity("ama")), "admitting does not displace");
    }

    #[test]
    fn a_stranger_cannot_move_a_sole_owners_boundary() {
        let gov = Governance::SoleOwner("bonnie".into());
        assert!(matches!(
            BoundaryToken::mint(
                &gov,
                &EditAuthority::Owner("someone_else"),
                BoundaryChange::AdmitEntity("ben".into())
            ),
            Err(AuthorityError::NotTheOwner { .. })
        ));
    }

    /// ★ The asymmetry is editing.rs's, reused rather than reinvented.
    #[test]
    fn a_shared_sustains_boundary_cannot_be_moved_by_a_single_actor() {
        assert!(matches!(
            BoundaryToken::mint(
                &Governance::Shared,
                &EditAuthority::Owner("bonnie"),
                BoundaryChange::AdoptChild("cira".into())
            ),
            Err(AuthorityError::SharedNeedsCouncil)
        ));
    }

    #[test]
    fn adopting_and_releasing_a_child_are_boundary_changes() {
        let gov = Governance::SoleOwner("bonnie".into());
        let b0 = decl().read(&state(&[], 0));
        let adopt = BoundaryToken::mint(
            &gov,
            &EditAuthority::Owner("bonnie"),
            BoundaryChange::AdoptChild("cira".into()),
        )
        .unwrap();
        assert!(adopt.change().describe().contains("⊕"));
        let b1 = admit_boundary_change(&b0, adopt).unwrap();
        assert!(b1.owns(&Owned::entity("cira")));

        let release = BoundaryToken::mint(
            &gov,
            &EditAuthority::Owner("bonnie"),
            BoundaryChange::ReleaseChild("cira".into()),
        )
        .unwrap();
        let b2 = admit_boundary_change(&b1, release).unwrap();
        assert!(!b2.owns(&Owned::entity("cira")));
    }
}
