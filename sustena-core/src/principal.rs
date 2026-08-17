//! Authorization — the `permitted(α, o, Σ)` conjunct (TOLERANCE / the Immune
//! paper, §I and §V).
//!
//! The paper extends `admit()` with authorization:
//!
//! ```text
//!   admit(α, o, s) ⟺ auth(α) ∧ permitted(α, o, Σ) ∧ g_o(s)
//!                     ∧ o(s) ∈ V ∧ D(s, o(s)) ∧ ⋀ F(φ, s)
//! ```
//!
//! Six conjuncts, read as six distinct refusals: *not known · not you · not now
//! · not ever · not that way · not across this wall*. None reduces to another.
//!
//! In particular **`auth` and `permitted` are separate**. Knowing who someone
//! is tells you nothing about what they may do, and a system that conflates the
//! two has an authorization model with exactly one tier. `auth` is the host's
//! job — it owns sessions and tokens. `permitted` is this module's.
//!
//! ## What was actually there
//!
//! The paper's audit is blunt: `min_privilege` is declared on every operator,
//! accepted by the decorator, stored — and *read nowhere*. `access_policy.
//! owner_ids` is templated into both shipped specs and consulted by no
//! authorization decision. The one real check is single-owner, at the route
//! layer, comparing against the sustain's *creator*. So `permitted` was "a
//! field, not a check", and there was no multi-principal model in force at all.
//!
//! ## Authority attaches to the EDGE, not to the person
//!
//! The declared-but-unbuilt model is the right one, so it is what is built
//! here: privilege lives on the **membership edge** between a principal and a
//! sustain. The same person may own their habitat, contribute to a homestead,
//! and observe an organisation read-only — three edges, three authorities, one
//! person. A privilege attached to the person could not express that.
//!
//! [`Skin`] is a named bundle of edge privileges — role-based access control,
//! with `min_privilege` as the per-action binding. It is resolved through a
//! [`SkinRegistry`], and where an edge carries both a tier and a skin the
//! **weaker** of the two wins: a bundle meant to restrict an edge must not be
//! able to widen it. An edge naming a skin nothing resolves is **refused**
//! rather than quietly demoted to its raw tier.
//!
//! ## Ambient authority, and where it stops being right
//!
//! Everything in this module answers *may this principal act here* from
//! **ambient** authority: the principal is named, and all of what it holds is
//! in force. That is correct when the principal is the one deciding, and it is
//! precisely wrong when a **deputy** acts on its behalf over inputs it did not
//! choose. See [`crate::capability`] for the confused-deputy case and the form
//! of authority that travels *with* a request instead.
//!
//! ## Defence in depth is `⊕`, and it is monotone
//!
//! ```text
//!   admit*(α, o, s) = ⋀ over H ∈ path(Σ) of admit_H(α, o, s)
//! ```
//!
//! Permission across a nesting is the **conjunction** of every gate on the
//! path. Because `∧` is monotone, adding a containing sustain can only shrink
//! what a principal may do — never widen it. A pocket inside a treasury inside
//! a household is three boundaries and three gates.
//!
//! That is why [`effective_privilege`] takes the WEAKEST authority along the
//! path rather than the strongest: defence in depth as an algebraic property
//! rather than an aspiration.
//!
//! ## The honest limit
//!
//! Harrison, Ruzzo and Ullman proved the safety question — can a right ever
//! leak to a subject — undecidable in the general access-matrix model. An
//! authorization model therefore cannot be verified for what it permits over
//! all futures; it can only be made small enough to reason about. This one is
//! deliberately small.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Access tier. **Lower is more authority** — 0 is owner.
///
/// Matches the reference declaration's own comment
/// (`0=owner, 1=member, ...`), so existing `min_privilege` values keep their
/// meaning rather than silently inverting.
pub type Tier = u8;

pub const TIER_OWNER: Tier = 0;
pub const TIER_MEMBER: Tier = 1;
pub const TIER_CONTRIBUTOR: Tier = 2;
pub const TIER_OBSERVER: Tier = 3;

/// A named bundle of edge privileges — RBAC, with `min_privilege` binding per
/// action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skin {
    pub name: String,
    pub tier: Tier,
}

/// Authority a principal holds **on one relationship** with one sustain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipEdge {
    pub principal: String,
    pub sustain: String,
    pub tier: Tier,
    #[serde(default)]
    pub skin: Option<String>,
}

/// Every membership edge in force.
#[derive(Debug, Clone, Default)]
pub struct Memberships {
    edges: BTreeMap<(String, String), MembershipEdge>,
}

impl Memberships {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grant(&mut self, edge: MembershipEdge) {
        self.edges
            .insert((edge.principal.clone(), edge.sustain.clone()), edge);
    }

    pub fn revoke(&mut self, principal: &str, sustain: &str) {
        self.edges.remove(&(principal.to_string(), sustain.to_string()));
    }

    pub fn edge(&self, principal: &str, sustain: &str) -> Option<&MembershipEdge> {
        self.edges.get(&(principal.to_string(), sustain.to_string()))
    }

    pub fn len(&self) -> usize {
        self.edges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }
}

/// Why authorization refused. Distinct variants because the paper is explicit
/// that these are genuinely different questions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Denial {
    /// *Not known* — no membership edge exists between this principal and this
    /// sustain at all. Distinct from having one with too little authority:
    /// "you are not a member here" and "you are a member who may not do this"
    /// are different answers and deserve different words.
    NoEdge { principal: String, sustain: String },
    /// *Not you* — an edge exists, but its authority is insufficient.
    InsufficientTier {
        principal: String,
        sustain: String,
        held: Tier,
        required: Tier,
    },
    /// An edge names a skin that no registry resolves.
    ///
    /// **Fail-closed, deliberately.** The tempting alternative is to fall back
    /// to the edge's own `tier`, and that is exactly how a typo'd role name
    /// becomes owner-by-accident: the bundle that was meant to *restrict* the
    /// edge silently stops applying, and nothing says so. IMM-4's rule — an
    /// unreadable rule refuses, it is never silently skipped — is the same rule
    /// one layer over.
    UnresolvedSkin {
        principal: String,
        sustain: String,
        skin: String,
    },
    /// *Not that object* — a capability was presented that designates a
    /// different sustain. **This is the confused deputy's refusal**: the
    /// authority in force did not name the thing the request named.
    NotDesignated { held: String, attempted: String },
    /// A capability designates the right object but does not carry this
    /// operator. Distinct from insufficient tier: narrowing rights and
    /// weakening authority are different attenuations and different repairs.
    RightNotHeld { sustain: String, operator: String },
}

impl std::fmt::Display for Denial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Denial::NoEdge { principal, sustain } => write!(
                f,
                "principal '{principal}' has no membership of sustain '{sustain}'"
            ),
            Denial::InsufficientTier { principal, sustain, held, required } => write!(
                f,
                "principal '{principal}' holds tier {held} on sustain '{sustain}', \
                 but this operation requires tier {required} or stronger"
            ),
            Denial::UnresolvedSkin { principal, sustain, skin } => write!(
                f,
                "principal '{principal}' holds sustain '{sustain}' through skin \
                 '{skin}', which no skin registry resolves"
            ),
            Denial::NotDesignated { held, attempted } => write!(
                f,
                "the capability presented designates sustain '{held}', \
                 not '{attempted}'"
            ),
            Denial::RightNotHeld { sustain, operator } => write!(
                f,
                "the capability presented over sustain '{sustain}' does not \
                 carry operator '{operator}'"
            ),
        }
    }
}

/// The skins in force — named bundles of edge privilege, i.e. RBAC.
///
/// Deliberately a lookup and nothing more. `Skin` shipped with the edge model
/// as a declared field that nothing read — the same *"a field, not a check"*
/// defect this module's own header criticises `min_privilege` for. This is what
/// reads it.
#[derive(Debug, Clone, Default)]
pub struct SkinRegistry {
    skins: BTreeMap<String, Skin>,
}

impl SkinRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// No skins defined. An edge naming one will be refused, not ignored.
    pub fn empty() -> Self {
        Self::default()
    }

    /// A shared empty registry, for the common case of a host that declares no
    /// skins at all. Saves every such call site keeping one alive purely to
    /// borrow from.
    pub fn none() -> &'static SkinRegistry {
        static NONE: std::sync::OnceLock<SkinRegistry> = std::sync::OnceLock::new();
        NONE.get_or_init(SkinRegistry::default)
    }

    pub fn define(&mut self, skin: Skin) {
        self.skins.insert(skin.name.clone(), skin);
    }

    pub fn get(&self, name: &str) -> Option<&Skin> {
        self.skins.get(name)
    }

    pub fn len(&self) -> usize {
        self.skins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skins.is_empty()
    }
}

/// The principal's effective authority across a whole nesting path.
///
/// `path` runs from the outermost containing sustain to the target. The result
/// is the WEAKEST authority held anywhere along it, because permission is the
/// conjunction of every gate on the path and `∧` is monotone: adding a
/// containing sustain can only shrink what a principal may do.
///
/// A missing edge anywhere on the path denies outright. There is no "inherit
/// from the parent" rule — that would widen authority by nesting, which is
/// exactly the direction the algebra forbids.
pub fn effective_privilege(
    memberships: &Memberships,
    principal: &str,
    path: &[String],
) -> Result<Tier, Denial> {
    effective_privilege_with(memberships, &SkinRegistry::empty(), principal, path)
}

/// [`effective_privilege`], with the skins in force.
///
/// An edge may carry a skin (a named RBAC bundle) as well as its own tier.
/// Where both are present the **weaker** wins — `max`, the same weakest-link
/// algebra the path walk uses, for the same reason: a bundle meant to restrict
/// an edge must not be able to widen it, and an edge must not be able to
/// escape its bundle.
///
/// An edge naming a skin the registry cannot resolve is **refused**
/// ([`Denial::UnresolvedSkin`]), never quietly demoted to its raw tier.
pub fn effective_privilege_with(
    memberships: &Memberships,
    skins: &SkinRegistry,
    principal: &str,
    path: &[String],
) -> Result<Tier, Denial> {
    if path.is_empty() {
        return Err(Denial::NoEdge {
            principal: principal.to_string(),
            sustain: "<empty path>".to_string(),
        });
    }

    let mut weakest = TIER_OWNER;
    for sustain in path {
        match memberships.edge(principal, sustain) {
            None => {
                return Err(Denial::NoEdge {
                    principal: principal.to_string(),
                    sustain: sustain.clone(),
                })
            }
            Some(edge) => {
                // Higher number = less authority, so max() is the weakest link.
                weakest = weakest.max(edge.tier);
                if let Some(name) = &edge.skin {
                    match skins.get(name) {
                        Some(skin) => weakest = weakest.max(skin.tier),
                        None => {
                            return Err(Denial::UnresolvedSkin {
                                principal: principal.to_string(),
                                sustain: sustain.clone(),
                                skin: name.clone(),
                            })
                        }
                    }
                }
            }
        }
    }
    Ok(weakest)
}

/// `permitted(α, o, Σ)` — may this principal perform this operation here?
///
/// `required` is the operator's declared `min_privilege`. A principal is
/// permitted when the authority it holds is at least as strong as the operation
/// demands — that is, a *numerically lower or equal* tier.
pub fn permitted(
    memberships: &Memberships,
    principal: &str,
    path: &[String],
    required: Tier,
) -> Result<(), Denial> {
    permitted_with(memberships, &SkinRegistry::empty(), principal, path, required)
}

/// [`permitted`], with the skins in force.
pub fn permitted_with(
    memberships: &Memberships,
    skins: &SkinRegistry,
    principal: &str,
    path: &[String],
    required: Tier,
) -> Result<(), Denial> {
    let held = effective_privilege_with(memberships, skins, principal, path)?;
    if held <= required {
        Ok(())
    } else {
        Err(Denial::InsufficientTier {
            principal: principal.to_string(),
            sustain: path.last().cloned().unwrap_or_default(),
            held,
            required,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(principal: &str, sustain: &str, tier: Tier) -> MembershipEdge {
        MembershipEdge {
            principal: principal.into(),
            sustain: sustain.into(),
            tier,
            skin: None,
        }
    }

    fn setup() -> Memberships {
        let mut m = Memberships::new();
        m.grant(edge("bonnie", "household", TIER_OWNER));
        m.grant(edge("bonnie", "habitat", TIER_OWNER));
        m.grant(edge("cira", "household", TIER_MEMBER));
        m.grant(edge("cira", "habitat", TIER_OBSERVER));
        m
    }

    #[test]
    fn authority_attaches_to_the_edge_not_the_person() {
        let m = setup();
        // The same person, two relationships, two different authorities.
        assert_eq!(m.edge("cira", "household").unwrap().tier, TIER_MEMBER);
        assert_eq!(m.edge("cira", "habitat").unwrap().tier, TIER_OBSERVER);
    }

    #[test]
    fn an_owner_may_perform_a_member_level_operation() {
        let m = setup();
        assert!(permitted(&m, "bonnie", &["household".into()], TIER_MEMBER).is_ok());
    }

    #[test]
    fn an_observer_may_not_perform_a_member_level_operation() {
        let m = setup();
        match permitted(&m, "cira", &["habitat".into()], TIER_MEMBER) {
            Err(Denial::InsufficientTier { held, required, .. }) => {
                assert_eq!(held, TIER_OBSERVER);
                assert_eq!(required, TIER_MEMBER);
            }
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn no_membership_is_a_different_answer_from_too_little_authority() {
        let m = setup();
        match permitted(&m, "stranger", &["household".into()], TIER_OBSERVER) {
            Err(Denial::NoEdge { .. }) => {}
            other => panic!("expected NoEdge, got {other:?}"),
        }
    }

    #[test]
    fn nesting_can_only_shrink_authority_never_widen_it() {
        // Owner of the habitat, but only an observer of the household that
        // contains it. Acting on the habitat THROUGH the household is bounded
        // by the weaker link.
        let mut m = Memberships::new();
        m.grant(edge("epha", "household", TIER_OBSERVER));
        m.grant(edge("epha", "habitat", TIER_OWNER));

        let alone = effective_privilege(&m, "epha", &["habitat".into()]).unwrap();
        assert_eq!(alone, TIER_OWNER);

        let nested =
            effective_privilege(&m, "epha", &["household".into(), "habitat".into()]).unwrap();
        assert_eq!(nested, TIER_OBSERVER, "the conjunction takes the weakest link");

        assert!(permitted(&m, "epha", &["habitat".into()], TIER_MEMBER).is_ok());
        assert!(
            permitted(&m, "epha", &["household".into(), "habitat".into()], TIER_MEMBER).is_err(),
            "nesting must not widen what a principal may do"
        );
    }

    #[test]
    fn a_gap_anywhere_on_the_path_denies_outright() {
        // No inherit-from-parent rule: that would widen authority by nesting.
        let mut m = Memberships::new();
        m.grant(edge("kui", "household", TIER_OWNER));
        assert!(matches!(
            effective_privilege(&m, "kui", &["household".into(), "habitat".into()]),
            Err(Denial::NoEdge { .. })
        ));
    }

    #[test]
    fn revoking_an_edge_removes_the_authority_it_carried() {
        let mut m = setup();
        assert!(permitted(&m, "cira", &["household".into()], TIER_MEMBER).is_ok());
        m.revoke("cira", "household");
        assert!(matches!(
            permitted(&m, "cira", &["household".into()], TIER_MEMBER),
            Err(Denial::NoEdge { .. })
        ));
    }

    // ── skins: the named bundles, now actually read ──────────────────────────

    fn skinned(principal: &str, sustain: &str, tier: Tier, skin: &str) -> MembershipEdge {
        MembershipEdge {
            principal: principal.into(),
            sustain: sustain.into(),
            tier,
            skin: Some(skin.into()),
        }
    }

    #[test]
    fn a_skin_restricts_an_edge_that_would_otherwise_be_stronger() {
        let mut m = Memberships::new();
        m.grant(skinned("frankie", "household", TIER_OWNER, "guest"));
        let mut s = SkinRegistry::new();
        s.define(Skin { name: "guest".into(), tier: TIER_OBSERVER });

        // The edge says owner; the bundle says observer. The weaker wins.
        assert_eq!(
            effective_privilege_with(&m, &s, "frankie", &["household".into()]).unwrap(),
            TIER_OBSERVER
        );
        assert!(permitted_with(&m, &s, "frankie", &["household".into()], TIER_MEMBER).is_err());
    }

    #[test]
    fn a_skin_cannot_widen_an_edge_it_is_attached_to() {
        let mut m = Memberships::new();
        m.grant(skinned("kui", "household", TIER_OBSERVER, "admin"));
        let mut s = SkinRegistry::new();
        s.define(Skin { name: "admin".into(), tier: TIER_OWNER });

        // A bundle must not be a promotion route.
        assert_eq!(
            effective_privilege_with(&m, &s, "kui", &["household".into()]).unwrap(),
            TIER_OBSERVER
        );
    }

    #[test]
    fn an_unresolvable_skin_refuses_rather_than_falling_back_to_the_raw_tier() {
        let mut m = Memberships::new();
        // Owner tier, but the bundle that was meant to restrict it is missing.
        // Falling back to the tier would silently promote.
        m.grant(skinned("mum", "household", TIER_OWNER, "gest"));
        let s = SkinRegistry::empty();

        assert!(matches!(
            permitted_with(&m, &s, "mum", &["household".into()], TIER_OWNER),
            Err(Denial::UnresolvedSkin { .. })
        ));
    }

    #[test]
    fn an_edge_without_a_skin_is_unaffected_by_the_registry() {
        // The pre-skin behaviour is preserved exactly: every existing edge
        // carries `skin: None` and must decide identically.
        let m = setup();
        let mut s = SkinRegistry::new();
        s.define(Skin { name: "guest".into(), tier: TIER_OBSERVER });

        assert_eq!(
            effective_privilege(&m, "cira", &["household".into()]).unwrap(),
            effective_privilege_with(&m, &s, "cira", &["household".into()]).unwrap(),
        );
    }

    #[test]
    fn the_default_operator_requirement_admits_members_but_not_observers() {
        // min_privilege defaults to 1 (member) on every declared operator.
        let m = setup();
        assert!(permitted(&m, "cira", &["household".into()], TIER_MEMBER).is_ok());
        assert!(permitted(&m, "cira", &["habitat".into()], TIER_MEMBER).is_err());
    }
}
