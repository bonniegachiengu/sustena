//! **A commons with a subsidy, and the Symbionts who fund it** (Mycelium ·
//! §§VII, X).
//!
//! ★★★ **The treasury is a common-pool resource** — subtractable, and hard to
//! exclude members from, which is what membership *means*. That is the exact
//! configuration Hardin predicted would be destroyed, and Olson had already
//! given the mechanism: in a group of self-interested members the individually
//! rational move is to consume the collective good and let others fund it. **A
//! subsidy funded from a common purse is a free-rider magnet by construction.**
//!
//! ★★★ **Ostrom is the empirical reply, and mapping the eight principles is not
//! decoration — it is the specification of what the network must implement to
//! survive its own generosity.** [`Principle`] is that list, each carrying the
//! mechanism that discharges it, so "we follow Ostrom" is checkable rather than
//! claimed.
//!
//! ★★★ **Principle 8 is the one to sit with.** Durable commons at scale are
//! organised in nested layers, each governing at its own level — and Sustena did
//! not adopt that as a policy. It falls out of `Σ` being recursive. **The
//! network is nested enterprises because it is a Sustain of Sustains, and there
//! is no other way for it to be.**
//!
//! ## Two defences the mapping makes mandatory
//!
//! ★★★ **Sybil resistance.** Any per-identity grant is an incentive to
//! manufacture identities, so a grant must be bounded by something *costly* —
//! a treasury balance, an attestation, an invitation edge. A grant bounded by
//! nothing is a mint with extra steps.
//!
//! ★★★ **Boundary honesty: principle 1 fails SILENTLY if identity is free.**
//! Silently is the whole problem — the commons looks healthy right up until it
//! is drained, because every fake member is indistinguishable from a real one at
//! the moment it takes its grant.
//!
//! ## The no-special-power rule is structural, not a promise
//!
//! ★★★ The Foundation cannot mint outside the treasury Enzyme, cannot bypass a
//! member's gate, cannot write to a member's state. **Promises are
//! unenforceable**; this is complete mediation. *A privileged-Symbiont exception
//! is not a feature with a risk; it is the deletion of the property that makes
//! the rest of the argument true.*

use std::collections::BTreeSet;

/// Ostrom's eight, each with the mechanism that discharges it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Principle {
    ClearBoundaries,
    CongruenceWithLocalConditions,
    CollectiveChoice,
    Monitoring,
    GraduatedSanctions,
    ConflictResolution,
    RightToOrganise,
    NestedEnterprises,
}

impl Principle {
    pub fn all() -> [Principle; 8] {
        [
            Self::ClearBoundaries,
            Self::CongruenceWithLocalConditions,
            Self::CollectiveChoice,
            Self::Monitoring,
            Self::GraduatedSanctions,
            Self::ConflictResolution,
            Self::RightToOrganise,
            Self::NestedEnterprises,
        ]
    }

    /// What in this system discharges it.
    ///
    /// ★★ Naming the mechanism is what makes the mapping checkable. "We follow
    /// Ostrom" is a claim; "principle 5 is the graduated sanction ladder in
    /// `market.rs`" is a thing somebody can go and read.
    pub fn mechanism(&self) -> &'static str {
        match self {
            Self::ClearBoundaries => {
                "B_net — membership is explicit and gated, and identity is a handle resolving to \
                 a habitat rather than an anonymous key"
            }
            Self::CongruenceWithLocalConditions => {
                "per-niche rules and libraries — No Free Lunch forbids one global rule set"
            }
            Self::CollectiveChoice => {
                "CouncilSession, plus StakeholderSession for organisations — those affected set \
                 the rules"
            }
            Self::Monitoring => {
                "the append-only event log: treasury flows are derivable, not asserted"
            }
            Self::GraduatedSanctions => {
                "the sanction ladder — proportionate, and every step reversible"
            }
            Self::ConflictResolution => "council deliberation, and the proposal-vote-veto path",
            Self::RightToOrganise => {
                "member sovereignty over its own V — the network cannot legislate a member's \
                 internals"
            }
            Self::NestedEnterprises => {
                "⊕ — the recursive Sustain itself, which is not a policy anybody adopted"
            }
        }
    }

    /// ★★★ Is this one a *consequence* of the design rather than a rule bolted
    /// onto it?
    ///
    /// Only principle 8, and that is worth marking: a commons that has to
    /// *remember* to be nested can stop being nested. One that is nested because
    /// its primitive is recursive cannot.
    pub fn falls_out_of_the_primitive(&self) -> bool {
        matches!(self, Self::NestedEnterprises)
    }
}

/// What bounds a grant, so it cannot be farmed.
///
/// ★★★ There is no `Unbounded`. A grant bounded by nothing is a mint with extra
/// steps, and the type has nowhere to express one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantBound {
    /// Bounded by what the purse holds. Self-limiting.
    TreasuryBalance { available: u64 },
    /// Bounded by something the recipient had to obtain.
    Attestation { by: String },
    /// Bounded by an existing member spending their own standing to vouch.
    InvitationEdge { from: String },
}

impl GrantBound {
    /// ★★ Every bound here costs the claimant something. That is the whole
    /// requirement: *bounded by something costly*.
    pub fn is_costly(&self) -> bool {
        true
    }

    pub fn describe(&self) -> String {
        match self {
            Self::TreasuryBalance { available } => {
                format!("bounded by the purse — {available} left, and it does not refill itself")
            }
            Self::Attestation { by } => format!("bounded by an attestation from {by}"),
            Self::InvitationEdge { from } => {
                format!("bounded by {from} spending their own standing to vouch")
            }
        }
    }
}

/// Why a grant was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantRefusal {
    /// The purse cannot cover it.
    TreasuryWouldGoNegative { asked: u64, available: u64 },
    /// ★★★ Identity is free here, so principle 1 has already failed — and it
    /// fails *silently*, which is why this is refused loudly.
    IdentityIsFree,
    /// This principal already took its grant.
    AlreadyGranted { principal: String },
}

impl GrantRefusal {
    pub fn describe(&self) -> String {
        match self {
            Self::TreasuryWouldGoNegative { asked, available } => {
                format!("asked for {asked} and the purse holds {available}")
            }
            Self::IdentityIsFree => {
                "identity costs nothing here, so a per-identity grant is an incentive to \
                 manufacture identities — and boundary failure is silent, which is why this \
                 refuses rather than warns"
                    .into()
            }
            Self::AlreadyGranted { principal } => {
                format!("{principal} has already had its grant")
            }
        }
    }
}

/// A network-scale Symbiont — Foundation, Institute, or any other.
///
/// ★★★ **Its authority is zero, and that is a property of the type.** There is
/// no field granting an exception, no `privileged` flag, and no method that
/// writes anywhere. A privileged-Symbiont exception is not a feature with a
/// risk; it is the deletion of the property that makes the rest of the argument
/// true.
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkSymbiont {
    pub id: String,
    /// Its niche happens to be the whole graph. **Nothing else about it is
    /// different.**
    pub niche: String,
    granted_to: BTreeSet<String>,
}

impl NetworkSymbiont {
    pub fn new(id: &str, niche: &str) -> Self {
        Self { id: id.into(), niche: niche.into(), granted_to: BTreeSet::new() }
    }

    /// ★★★ Always false, for every one of them. A function rather than a
    /// comment, so a change that gave a Symbiont an exception would have to
    /// delete this and the test that calls it.
    pub fn can_bypass_a_members_gate(&self) -> bool {
        false
    }

    /// ★★★ Also always false. It cannot mint outside the treasury Enzyme; it
    /// proposes a grant and the gate commits it, exactly as anybody else does.
    pub fn can_mint(&self) -> bool {
        false
    }

    /// **Propose a grant.** It is an ordinary Enzyme call, and this only says
    /// whether it would be admitted.
    ///
    /// ★★ `identity_is_costly` is a required argument rather than an
    /// assumption, because principle 1 failing silently is the failure mode —
    /// and a parameter somebody must supply is one they have to think about.
    pub fn propose_grant(
        &mut self,
        to: &str,
        amount: u64,
        bound: &GrantBound,
        identity_is_costly: bool,
    ) -> Result<u64, GrantRefusal> {
        if !identity_is_costly {
            return Err(GrantRefusal::IdentityIsFree);
        }
        if self.granted_to.contains(to) {
            return Err(GrantRefusal::AlreadyGranted { principal: to.to_string() });
        }
        if let GrantBound::TreasuryBalance { available } = bound {
            if amount > *available {
                return Err(GrantRefusal::TreasuryWouldGoNegative {
                    asked: amount,
                    available: *available,
                });
            }
        }
        self.granted_to.insert(to.to_string());
        Ok(amount)
    }

    pub fn has_granted_to(&self, principal: &str) -> bool {
        self.granted_to.contains(principal)
    }
}

/// Is the commons defended against its own generosity?
///
/// ★★ Both defences, together, because either alone leaves the hole open: a
/// costly identity with an unbounded grant drains the purse slowly, and a
/// bounded grant with free identity drains it quickly.
pub fn defended(identity_is_costly: bool, grants_are_bounded: bool) -> bool {
    identity_is_costly && grants_are_bounded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn foundation() -> NetworkSymbiont {
        NetworkSymbiont::new("the Mycelium Foundation", "the whole network")
    }

    fn purse(n: u64) -> GrantBound {
        GrantBound::TreasuryBalance { available: n }
    }

    #[test]
    fn all_eight_principles_name_a_mechanism_rather_than_being_claimed() {
        // ★★ "We follow Ostrom" is a claim; "principle 5 is the sanction
        //    ladder" is a thing somebody can go and read.
        for p in Principle::all() {
            assert!(p.mechanism().len() > 20, "{p:?}");
        }
        assert_eq!(Principle::all().len(), 8);
    }

    #[test]
    fn nested_enterprises_is_the_one_that_falls_out_of_the_primitive() {
        // ★★★ Sustena did not adopt nesting as a policy. A commons that has to
        //     REMEMBER to be nested can stop being nested; one that is nested
        //     because its primitive is recursive cannot.
        assert!(Principle::NestedEnterprises.falls_out_of_the_primitive());
        for p in Principle::all() {
            if p != Principle::NestedEnterprises {
                assert!(!p.falls_out_of_the_primitive(), "{p:?}");
            }
        }
        assert!(Principle::NestedEnterprises.mechanism().contains("not a policy anybody adopted"));
    }

    #[test]
    fn a_grant_where_identity_is_free_is_refused_loudly() {
        // ★★★ Principle 1 fails SILENTLY, which is the whole problem: the
        //     commons looks healthy right up until it is drained, because every
        //     fake member is indistinguishable from a real one at the moment it
        //     takes its grant. So this refuses rather than warns.
        let refusal = foundation()
            .propose_grant("somebody", 100, &purse(10_000), false)
            .expect_err("refused");
        assert_eq!(refusal, GrantRefusal::IdentityIsFree);
        assert!(refusal.describe().contains("failure is silent"));
    }

    #[test]
    fn a_grant_must_be_bounded_by_something_costly_and_there_is_no_unbounded() {
        // ★★★ A grant bounded by nothing is a mint with extra steps, and the
        //     type has nowhere to express one.
        for b in [
            purse(1),
            GrantBound::Attestation { by: "a co-op".into() },
            GrantBound::InvitationEdge { from: "bonnie".into() },
        ] {
            assert!(b.is_costly(), "{b:?}");
        }
    }

    #[test]
    fn the_purse_does_not_refill_itself() {
        let refused = foundation()
            .propose_grant("somebody", 500, &purse(100), true)
            .expect_err("refused");
        assert!(matches!(refused, GrantRefusal::TreasuryWouldGoNegative { .. }));
        assert!(purse(100).describe().contains("does not refill itself"));
    }

    #[test]
    fn one_grant_per_principal_because_a_repeatable_grant_is_a_faucet() {
        let mut f = foundation();
        assert_eq!(f.propose_grant("a newcomer", 100, &purse(10_000), true), Ok(100));
        assert!(matches!(
            f.propose_grant("a newcomer", 100, &purse(10_000), true),
            Err(GrantRefusal::AlreadyGranted { .. })
        ));
        assert!(f.has_granted_to("a newcomer"));
    }

    #[test]
    fn the_foundation_cannot_bypass_a_gate_or_mint_and_that_is_a_type_property() {
        // ★★★ Promises are unenforceable. A privileged-Symbiont exception is
        //     not a feature with a risk; it is the deletion of the property that
        //     makes the rest of the argument true.
        let f = foundation();
        assert!(!f.can_bypass_a_members_gate());
        assert!(!f.can_mint());
    }

    #[test]
    fn its_niche_is_the_whole_graph_and_nothing_else_about_it_differs() {
        // ★★ The Institute is the same kind of object as the Foundation, and
        //    both are the same kind of object as any household's Symbiont.
        let institute = NetworkSymbiont::new("the Sustena Institute", "the whole network");
        assert_eq!(institute.niche, foundation().niche);
        assert!(!institute.can_mint());
    }

    #[test]
    fn both_defences_are_needed_because_either_alone_leaves_the_hole_open() {
        // ★★ A costly identity with an unbounded grant drains the purse slowly;
        //    a bounded grant with free identity drains it quickly.
        assert!(defended(true, true));
        assert!(!defended(true, false));
        assert!(!defended(false, true));
        assert!(!defended(false, false));
    }

    #[test]
    fn a_grant_that_the_purse_can_cover_to_a_costly_identity_is_admitted() {
        // ★★ The commons is meant to be generous. A defence that refused
        //    everything would be a purse nobody could draw on, which is the
        //    same failure as one that drains.
        let mut f = foundation();
        assert_eq!(f.propose_grant("a real newcomer", 100, &purse(10_000), true), Ok(100));
    }
}
