//! Capabilities — authority that travels **with** the request (the Immune
//! paper, §IV; Dennis & Van Horn 1966; Hardy 1988; Miller, Yee & Shapiro 2003).
//!
//! ## What this is for, and what it is not
//!
//! [`crate::principal`] already builds `permitted(α, o, Σ)`: authority on the
//! **membership edge**, checked as a conjunct of `admit()`. That answers *may
//! this principal act here* — and it answers it from **ambient** authority: the
//! principal id is presented, and everything that principal holds anywhere is
//! in force for the whole call.
//!
//! Ambient authority is correct when the principal is the one deciding. It is
//! precisely wrong when a **deputy** is acting on their behalf.
//!
//! ## The confused deputy (Hardy 1988)
//!
//! Hardy's compiler held a licence to write the billing file, and a caller who
//! could not write it supplied that filename as an *output* argument. The
//! compiler applied its own authority to a name someone else chose. Nothing was
//! forged; every check passed. The deputy was not compromised — it was
//! *confused* about whose authority it was exercising.
//!
//! Sustena's Symbionts are deputies by construction. An operative acts on a
//! person's authority over inputs it did not choose: a parsed SMS, an Arena
//! artefact, a network event. The moment the advisory layer can write, "which
//! sustain" arrives from untrusted text while the authority in force is the
//! whole of what the person holds. That is Hardy's shape exactly, and it is the
//! predictable first exploit rather than a hypothetical one.
//!
//! ## The fix: designation and authority in one token
//!
//! Dennis and Van Horn's capability is a single unforgeable thing that names an
//! **object** and carries the **rights** over it. The caller hands the deputy a
//! capability instead of a name, so the deputy cannot reach anything the caller
//! could not have reached itself. Designation and authorisation stop being two
//! separate steps that can disagree.
//!
//! Miller, Yee and Shapiro's sharpening is the one that matters here: the
//! property is **no ambient authority**, not "tokens exist". A capability
//! system that still consults an ambient table on the side has not fixed
//! anything. So [`Authorization::Capability`] carries **no `&Memberships`** —
//! the ambient set is not merely unconsulted on that path, it is *absent*, and
//! a fallback cannot be written without changing the type.
//!
//! [`Authorization::Capability`]: crate::operator::Authorization::Capability
//!
//! ## Unforgeable, structurally rather than cryptographically
//!
//! A [`Capability`] has no public fields, no public constructor, no `From`, and
//! **no `Deserialize`**. There are exactly two ways to hold one: [`issue`],
//! which consults a real membership edge, and [`Capability::attenuate`], from
//! one you already hold. You cannot write one down. A doctest that fails to
//! compile proves the `Deserialize` half rather than asserting it.
//!
//! [`issue`]: Capability::issue
//!
//! ★ **The honest limit, named rather than discovered later.** That is
//! unforgeability *in process*. A capability that must cross a wire needs a MAC
//! or a signature, which needs a key and probably a random nonce — and
//! ADR-0001 keeps RNG and I/O out of this core. So the cross-boundary form is a
//! **host-layer slot**, the same relocation CTL-6 took, and it is a slot rather
//! than a gap: in-process attenuation is where the confused-deputy fix actually
//! has to hold, because the deputy runs in-process.
//!
//! ## Attenuation only, and it refuses rather than clamps
//!
//! `attenuate` can weaken a capability and cannot strengthen one. Two of the
//! three ways to strengthen are **unrepresentable**: there is no method that
//! widens rights, and [`Attenuation`] carries no sustain, so **redesignating a
//! capability at a different object cannot be expressed at all** — which is the
//! confused deputy's move, removed from the vocabulary.
//!
//! The third, asking for a stronger tier, is representable because a caller may
//! sincerely ask; it is **refused and named** ([`Amplification`]) rather than
//! silently bounded. Silently clamping a request to what was allowed is the
//! trap [`crate::admission::Strategy`] warns about in the constraint layer: it
//! hides the fact that someone asked for more than they had.
//!
//! ## Economy of mechanism (Harrison, Ruzzo & Ullman 1976)
//!
//! HRU proved the safety question — can right `r` ever leak to subject `s` —
//! undecidable in the general access-matrix model. So an authorization model
//! cannot be verified for what it permits over all futures; it can only be made
//! small enough to reason about. This one is four fields and two operations,
//! and it stays that way on purpose. It is the same conclusion Rice's theorem
//! forced on the constraint layer, reached from a different direction.

use std::collections::BTreeSet;

use crate::principal::{Denial, Memberships, SkinRegistry, Tier};

/// Which operations a capability carries.
///
/// `All` is bounded by the capability's tier and by the sustain's own
/// allow-list — it is *everything this authority reaches*, never everything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rights {
    /// Every operator the designated sustain allows, subject to the tier.
    All,
    /// Only these, by name.
    Only(BTreeSet<String>),
}

impl Rights {
    /// A convenience for the common `Only` case.
    pub fn only<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Rights::Only(names.into_iter().map(Into::into).collect())
    }

    pub fn carries(&self, operator: &str) -> bool {
        match self {
            Rights::All => true,
            Rights::Only(set) => set.contains(operator),
        }
    }

    /// Is `self` at least as narrow as `other`? Used to reject amplification.
    fn within(&self, other: &Rights) -> bool {
        match (self, other) {
            (_, Rights::All) => true,
            (Rights::All, Rights::Only(_)) => false,
            (Rights::Only(mine), Rights::Only(theirs)) => mine.is_subset(theirs),
        }
    }
}

/// A request to weaken a capability.
///
/// ★ It carries **no sustain**. Redesignating a capability at a different
/// object is not a narrowing, it is the confused deputy's own move, and it is
/// left out of the vocabulary rather than checked for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Attenuation {
    /// Weaken to this tier. `None` keeps the current one.
    pub tier: Option<Tier>,
    /// Narrow to these rights. `None` keeps the current ones.
    pub rights: Option<Rights>,
}

impl Attenuation {
    pub fn to_tier(tier: Tier) -> Self {
        Attenuation { tier: Some(tier), rights: None }
    }

    pub fn to_rights(rights: Rights) -> Self {
        Attenuation { tier: None, rights: Some(rights) }
    }

    pub fn and_tier(mut self, tier: Tier) -> Self {
        self.tier = Some(tier);
        self
    }

    pub fn and_rights(mut self, rights: Rights) -> Self {
        self.rights = Some(rights);
        self
    }
}

/// A request that would have made a capability stronger. Refused, not clamped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Amplification {
    /// Asked for more authority than the parent capability holds.
    Tier { held: Tier, requested: Tier },
    /// Asked for an operator the parent capability does not carry.
    Rights,
}

impl std::fmt::Display for Amplification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Amplification::Tier { held, requested } => write!(
                f,
                "cannot attenuate to tier {requested} from tier {held}: \
                 attenuation weakens, it does not strengthen"
            ),
            Amplification::Rights => write!(
                f,
                "cannot attenuate to rights the held capability does not carry"
            ),
        }
    }
}

/// τ — an unforgeable designation of one sustain plus the rights over it.
///
/// No public fields, no public constructor, no `From`, no `Deserialize`. The
/// only ways to hold one are [`Capability::issue`] and
/// [`Capability::attenuate`].
///
/// A capability cannot be conjured from JSON:
///
/// ```compile_fail
/// use sustena_core::capability::Capability;
/// // Capability derives no Deserialize, so there is no wire form to forge.
/// let _forged: Capability = serde_json::from_str(
///     r#"{"sustain":"household","tier":0,"rights":"All","issued_to":"attacker"}"#,
/// )
/// .unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    sustain: String,
    tier: Tier,
    rights: Rights,
    issued_to: String,
}

impl Capability {
    /// Mint a capability from a **real membership edge**.
    ///
    /// The edge is consulted, so a capability cannot designate authority nobody
    /// holds: issuing is a *transfer* of authority that already exists, never a
    /// mint of new authority. (IMM-13 makes the same distinction about grants,
    /// for the same reason.)
    ///
    /// The tier is the one the edge carries — issuing does not choose it. To
    /// hand out less than you hold, [`attenuate`] the result.
    ///
    /// [`attenuate`]: Capability::attenuate
    pub fn issue(
        memberships: &Memberships,
        skins: &SkinRegistry,
        principal: &str,
        sustain: &str,
    ) -> Result<Capability, Denial> {
        // Reuse `permitted`'s own path walk rather than reading the edge
        // directly: a capability must not be able to route around the
        // weakest-link rule that governs the ambient path.
        let tier = crate::principal::effective_privilege_with(
            memberships,
            skins,
            principal,
            std::slice::from_ref(&sustain.to_string()),
        )?;
        Ok(Capability {
            sustain: sustain.to_string(),
            tier,
            rights: Rights::All,
            issued_to: principal.to_string(),
        })
    }

    /// Mint a capability over a whole nesting path, bounded by the weakest link.
    ///
    /// The designated object is the **target** (the last element), because that
    /// is what the holder may act on; the tier is the conjunction across the
    /// path, so nesting can only shrink what the capability carries.
    pub fn issue_across(
        memberships: &Memberships,
        skins: &SkinRegistry,
        principal: &str,
        path: &[String],
    ) -> Result<Capability, Denial> {
        let tier = crate::principal::effective_privilege_with(memberships, skins, principal, path)?;
        let sustain = path.last().cloned().unwrap_or_default();
        Ok(Capability { sustain, tier, rights: Rights::All, issued_to: principal.to_string() })
    }

    /// Weaken this capability. Never strengthens it; refuses if asked to.
    pub fn attenuate(&self, request: &Attenuation) -> Result<Capability, Amplification> {
        let tier = match request.tier {
            None => self.tier,
            // Lower number is MORE authority, so a lower request is an
            // amplification.
            Some(t) if t < self.tier => {
                return Err(Amplification::Tier { held: self.tier, requested: t })
            }
            Some(t) => t,
        };

        let rights = match &request.rights {
            None => self.rights.clone(),
            Some(r) if !r.within(&self.rights) => return Err(Amplification::Rights),
            Some(r) => r.clone(),
        };

        Ok(Capability {
            sustain: self.sustain.clone(),
            tier,
            rights,
            issued_to: self.issued_to.clone(),
        })
    }

    /// The object this capability designates.
    pub fn sustain(&self) -> &str {
        &self.sustain
    }

    pub fn tier(&self) -> Tier {
        self.tier
    }

    pub fn rights(&self) -> &Rights {
        &self.rights
    }

    /// Whose authority this carries. Kept for accountability: a capability is a
    /// transfer of someone's authority, and the log should be able to say
    /// whose.
    pub fn issued_to(&self) -> &str {
        &self.issued_to
    }

    /// `permitted(α, o, Σ)` under a capability rather than ambient authority.
    ///
    /// Three refusals, kept apart because they send an operator to different
    /// repairs: the capability does not designate this sustain (the confused
    /// deputy's refusal), it does not carry this operator, or it does not carry
    /// enough authority.
    pub fn permits(&self, sustain: &str, operator: &str, required: Tier) -> Result<(), Denial> {
        if self.sustain != sustain {
            return Err(Denial::NotDesignated {
                held: self.sustain.clone(),
                attempted: sustain.to_string(),
            });
        }
        if !self.rights.carries(operator) {
            return Err(Denial::RightNotHeld {
                sustain: sustain.to_string(),
                operator: operator.to_string(),
            });
        }
        if self.tier > required {
            return Err(Denial::InsufficientTier {
                principal: self.issued_to.clone(),
                sustain: sustain.to_string(),
                held: self.tier,
                required,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::principal::{MembershipEdge, TIER_CONTRIBUTOR, TIER_MEMBER, TIER_OBSERVER, TIER_OWNER};

    fn edge(principal: &str, sustain: &str, tier: Tier) -> MembershipEdge {
        MembershipEdge {
            principal: principal.into(),
            sustain: sustain.into(),
            tier,
            skin: None,
        }
    }

    fn setup() -> (Memberships, SkinRegistry) {
        let mut m = Memberships::new();
        m.grant(edge("bonnie", "household", TIER_OWNER));
        m.grant(edge("bonnie", "habitat", TIER_OWNER));
        (m, SkinRegistry::empty())
    }

    #[test]
    fn issuing_transfers_authority_that_exists_and_cannot_mint_any() {
        let (m, s) = setup();
        let cap = Capability::issue(&m, &s, "bonnie", "habitat").unwrap();
        assert_eq!(cap.tier(), TIER_OWNER);

        // Nobody may issue over a sustain they hold no edge on.
        assert!(matches!(
            Capability::issue(&m, &s, "stranger", "habitat"),
            Err(Denial::NoEdge { .. })
        ));
    }

    #[test]
    fn attenuation_weakens_and_refuses_to_strengthen() {
        let (m, s) = setup();
        let owner = Capability::issue(&m, &s, "bonnie", "habitat").unwrap();

        let weaker = owner.attenuate(&Attenuation::to_tier(TIER_CONTRIBUTOR)).unwrap();
        assert_eq!(weaker.tier(), TIER_CONTRIBUTOR);

        // Refused, and named — not silently bounded back to what was held.
        match weaker.attenuate(&Attenuation::to_tier(TIER_OWNER)) {
            Err(Amplification::Tier { held, requested }) => {
                assert_eq!(held, TIER_CONTRIBUTOR);
                assert_eq!(requested, TIER_OWNER);
            }
            other => panic!("expected refusal, got {other:?}"),
        }
    }

    #[test]
    fn rights_narrow_and_cannot_widen() {
        let (m, s) = setup();
        let all = Capability::issue(&m, &s, "bonnie", "habitat").unwrap();

        let narrowed = all
            .attenuate(&Attenuation::to_rights(Rights::only(["budget.spend"])))
            .unwrap();
        assert!(narrowed.rights().carries("budget.spend"));
        assert!(!narrowed.rights().carries("budget.allocate"));

        assert_eq!(
            narrowed.attenuate(&Attenuation::to_rights(Rights::All)),
            Err(Amplification::Rights),
        );
        assert_eq!(
            narrowed.attenuate(&Attenuation::to_rights(Rights::only(["budget.allocate"]))),
            Err(Amplification::Rights),
        );
    }

    #[test]
    fn a_capability_cannot_be_redesignated_at_another_object() {
        let (m, s) = setup();
        let cap = Capability::issue(&m, &s, "bonnie", "habitat").unwrap();

        // There is no API to change the object — `Attenuation` carries no
        // sustain — so the only thing left to assert is that the designation
        // survives every weakening.
        let weakened = cap
            .attenuate(&Attenuation::to_tier(TIER_OBSERVER).and_rights(Rights::only(["budget.summary"])))
            .unwrap();
        assert_eq!(weakened.sustain(), "habitat");
        assert!(weakened.permits("household", "budget.summary", TIER_OBSERVER).is_err());
    }

    #[test]
    fn permits_distinguishes_its_three_refusals() {
        let (m, s) = setup();
        let cap = Capability::issue(&m, &s, "bonnie", "habitat")
            .unwrap()
            .attenuate(&Attenuation::to_tier(TIER_CONTRIBUTOR).and_rights(Rights::only(["budget.spend"])))
            .unwrap();

        assert!(matches!(
            cap.permits("household", "budget.spend", TIER_CONTRIBUTOR),
            Err(Denial::NotDesignated { .. })
        ));
        assert!(matches!(
            cap.permits("habitat", "budget.allocate", TIER_CONTRIBUTOR),
            Err(Denial::RightNotHeld { .. })
        ));
        assert!(matches!(
            cap.permits("habitat", "budget.spend", TIER_MEMBER),
            Err(Denial::InsufficientTier { .. })
        ));
        assert!(cap.permits("habitat", "budget.spend", TIER_CONTRIBUTOR).is_ok());
    }

    #[test]
    fn issuing_across_a_path_takes_the_weakest_link() {
        let mut m = Memberships::new();
        m.grant(edge("epha", "household", TIER_OBSERVER));
        m.grant(edge("epha", "habitat", TIER_OWNER));
        let s = SkinRegistry::empty();

        let direct = Capability::issue(&m, &s, "epha", "habitat").unwrap();
        assert_eq!(direct.tier(), TIER_OWNER);

        let nested = Capability::issue_across(
            &m,
            &s,
            "epha",
            &["household".to_string(), "habitat".to_string()],
        )
        .unwrap();
        assert_eq!(nested.sustain(), "habitat");
        assert_eq!(nested.tier(), TIER_OBSERVER, "nesting can only shrink");
    }
}
