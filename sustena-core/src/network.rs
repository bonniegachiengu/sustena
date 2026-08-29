//! **The network is a Sustain, and knowing who is in it** (Mycelium · §§I, IV).
//!
//! ```text
//!   Σ_net = ⟨B_net, S_net, V_net, T_net, ⊕⟩
//!   probe → ack? ALIVE : k witnesses → ack? ALIVE : SUSPECT
//! ```
//!
//! ★★★ **`Σ_net` is not a new kind of thing**, and this module deliberately
//! introduces no `Network` type. The network is a [`crate::sigma::Sigma`] whose
//! members are Sustains, and `⊕` folds a homestead into it exactly as it folds a
//! habitat into a homestead. A distinct type would be the claim that the largest
//! scale is special, and the whole article is that it is not.
//!
//! ★★★ **Membership is a gated transition, not a fact of deployment** — it is
//! what makes "the network" a definite object rather than a mood. `join` and
//! `leave` are ordinary Enzymes: gated, logged, reversible.
//!
//! ## Failure is indistinguishable from slowness
//!
//! ★★★ **A suspicion is a tunable false-positive rate, never a fact.** No
//! failure detector achieves perfect accuracy in an asynchronous system
//! (Chandra & Toueg): the useful class gets there only *eventually*. So there is
//! no `Dead` variant reachable from a probe — [`Health::Suspect`] is refutable
//! by the target, and **every membership design must say what it does when it is
//! wrong**, which is why the posture is carried on the reading rather than left
//! to a caller's judgement.
//!
//! ★★★ **One silent probe is not evidence.** SWIM's indirect probe through `k`
//! witnesses exists because a single timeout says as much about the prober's
//! own network as about the target. Suspecting on one missed ack would make a
//! congested minute look like a departure.
//!
//! ## What federation forbids
//!
//! ★★★ **No global lock, no global clock, no global view** — the engineering
//! consequence of a federation being autonomous domains under a shared protocol
//! with no shared administrator. So nothing here reads a clock, and every
//! function takes a **local** view: a member that has not heard of another
//! member is an ordinary state, not an error.
//!
//! ★★ **Sovereignty.** The network may observe a member, aggregate it, price it
//! and refuse it service; it may not reach past that member's gate. Only the
//! member knows its own `V`, so only the member enforces it — the same
//! end-to-end argument that makes delisting not deletion.

use std::collections::BTreeSet;

/// What one member currently believes about another.
///
/// ★★★ There is no `Dead`. A crash and a slow link are indistinguishable from
/// outside, and a variant asserting death would be a claim no asynchronous
/// system can make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    /// It answered, directly or through a witness.
    Alive,
    /// ★★★ Nobody could reach it. **Refutable by the target**, and carrying the
    /// posture: how many witnesses were asked, and what this observer does when
    /// it turns out to be wrong.
    Suspect { witnesses_asked: usize, refutable: bool },
    /// It said it was leaving.
    ///
    /// ★★ Distinct from `Suspect` because it is a *statement* rather than an
    /// inference — the one departure that is a fact.
    Departed,
    /// ★★ Never heard of. Not a fault: in a federation with no global view, a
    /// member this one has not met is ordinary.
    Unknown,
}

impl Health {
    pub fn reachable(&self) -> bool {
        matches!(self, Self::Alive)
    }

    /// ★★★ May this reading be acted on as though the member were gone?
    ///
    /// Only a stated departure. A suspicion may be *shown to a person* and may
    /// change routing, and it may not be treated as death — that is what
    /// "tunable false-positive rate" means in practice.
    pub fn is_a_fact(&self) -> bool {
        matches!(self, Self::Departed)
    }

    pub fn describe(&self, who: &str) -> String {
        match self {
            Self::Alive => format!("{who} answered"),
            Self::Suspect { witnesses_asked, .. } => format!(
                "{who} did not answer, and neither did {witnesses_asked} witness(es) reach it — \
                 suspected, not declared: it can still refute this"
            ),
            Self::Departed => format!("{who} said it was leaving"),
            Self::Unknown => format!("this member has never met {who}"),
        }
    }
}

/// What one member can see. **Local by construction.**
#[derive(Debug, Clone, Default)]
pub struct View {
    known: BTreeSet<String>,
    departed: BTreeSet<String>,
}

impl View {
    pub fn new() -> Self {
        Self::default()
    }

    /// `join` — a gated transition, not a fact of deployment.
    pub fn joined(mut self, member: &str) -> Self {
        self.known.insert(member.to_string());
        self.departed.remove(member);
        self
    }

    /// `leave` — **disaggregation**: only the edge is destroyed.
    ///
    /// ★★★ The leaving member keeps its entire state. This module has no method
    /// that touches a member's state at all, which is the holon invariant
    /// applied to the network: *the whole is real, and it never erases the parts
    /// inside it.*
    pub fn left(mut self, member: &str) -> Self {
        self.known.remove(member);
        self.departed.insert(member.to_string());
        self
    }

    pub fn members(&self) -> Vec<&str> {
        self.known.iter().map(String::as_str).collect()
    }

    pub fn size(&self) -> usize {
        self.known.len()
    }

    fn has_met(&self, member: &str) -> bool {
        self.known.contains(member) || self.departed.contains(member)
    }
}

/// How many witnesses to ask before suspecting.
///
/// ★★ Declared, because it is the knob that trades detection speed against the
/// false-positive rate, and a knob nobody chose is a rate nobody chose.
pub const WITNESSES: usize = 3;

/// **One SWIM probe round.**
///
/// ★★★ `direct_ack` and `witness_acks` are supplied rather than fetched: this
/// module decides *what a silence means*, and finding out is the host's job.
/// That is also what keeps it testable without a network — and what keeps a
/// clock out of it.
pub fn probe(
    view: &View,
    target: &str,
    direct_ack: bool,
    witness_acks: &[bool],
) -> Health {
    if view.departed.contains(target) {
        return Health::Departed;
    }
    if !view.has_met(target) {
        return Health::Unknown;
    }
    if direct_ack {
        return Health::Alive;
    }
    // ★★★ One silent probe is not evidence — a single timeout says as much
    //     about this prober's own network as about the target.
    if witness_acks.iter().any(|a| *a) {
        return Health::Alive;
    }
    Health::Suspect { witnesses_asked: witness_acks.len(), refutable: true }
}

/// A member refuting a suspicion about itself.
///
/// ★★★ The half that makes a suspicion a suspicion. A detector whose output
/// could not be argued with would not have a false-positive *rate*; it would
/// have victims.
pub fn refute(health: &Health) -> Health {
    match health {
        Health::Suspect { .. } => Health::Alive,
        other => other.clone(),
    }
}

/// What the network is permitted to do about a member.
///
/// ★★★ Observe, aggregate, price, refuse service — and **not** reach past the
/// member's gate. Only the member knows its own `V`, so only the member
/// enforces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPower {
    Observe,
    Aggregate,
    Price,
    RefuseService,
}

impl NetworkPower {
    pub fn all() -> [NetworkPower; 4] {
        [Self::Observe, Self::Aggregate, Self::Price, Self::RefuseService]
    }

    /// ★★★ Always false, for every power there is. A function rather than a
    /// comment, so a future variant that reached into a member would have to
    /// delete this and the test that calls it.
    pub fn reaches_past_a_members_gate(&self) -> bool {
        false
    }
}

/// The invariants of `V_net`, as a checkable list.
///
/// ★★ Named rather than assumed: "the network has invariants" is a sentence,
/// and a list somebody can run is a design.
#[derive(Debug, Clone, PartialEq)]
pub struct NetInvariants {
    pub pawa_conserved: bool,
    pub treasury_non_negative: bool,
    pub membership_acyclic: bool,
}

impl NetInvariants {
    pub fn holds(&self) -> bool {
        self.pawa_conserved && self.treasury_non_negative && self.membership_acyclic
    }

    /// Which ones are broken, so a report names them.
    pub fn broken(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if !self.pawa_conserved {
            out.push("pawa is not conserved under transfer");
        }
        if !self.treasury_non_negative {
            out.push("the treasury is negative");
        }
        if !self.membership_acyclic {
            out.push("membership has a cycle");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_network() -> View {
        View::new().joined("the Gachiengu household").joined("the Otieno household")
    }

    #[test]
    fn membership_is_what_makes_the_network_a_definite_object() {
        // ★★ Not a fact of deployment: a machine that is running is not
        //    thereby a member, and "the network" without a boundary is a mood.
        let v = a_network();
        assert_eq!(v.size(), 2);
        assert_eq!(probe(&View::new(), "somebody", false, &[]), Health::Unknown);
    }

    #[test]
    fn a_member_this_one_has_never_met_is_ordinary_and_not_a_fault() {
        // ★★★ No global view. In a federation, not knowing about somebody is a
        //     state rather than an error.
        let h = probe(&a_network(), "a stranger", false, &[false; WITNESSES]);
        assert_eq!(h, Health::Unknown);
        assert!(h.describe("a stranger").contains("never met"));
    }

    #[test]
    fn one_silent_probe_is_not_evidence() {
        // ★★★ A single timeout says as much about this prober's own network as
        //     about the target. Suspecting on it would make a congested minute
        //     look like a departure.
        let alive_via_witness = probe(&a_network(), "the Otieno household", false, &[false, true, false]);
        assert_eq!(alive_via_witness, Health::Alive);
    }

    #[test]
    fn a_suspicion_is_never_a_fact_and_there_is_no_dead_to_reach() {
        // ★★★ A crash and a slow link are indistinguishable from outside, and a
        //     variant asserting death would be a claim no asynchronous system
        //     can make.
        let h = probe(&a_network(), "the Otieno household", false, &[false; WITNESSES]);
        match &h {
            Health::Suspect { witnesses_asked, refutable } => {
                assert_eq!(*witnesses_asked, WITNESSES);
                assert!(refutable);
            }
            other => panic!("{other:?}"),
        }
        assert!(!h.is_a_fact());
        assert!(h.describe("them").contains("it can still refute this"));
    }

    #[test]
    fn a_suspicion_can_be_refuted_which_is_what_makes_it_one() {
        // ★★★ A detector whose output could not be argued with would not have a
        //     false-positive rate; it would have victims.
        let suspected = probe(&a_network(), "the Otieno household", false, &[false; WITNESSES]);
        assert_eq!(refute(&suspected), Health::Alive);
    }

    #[test]
    fn only_a_stated_departure_is_a_fact() {
        // ★★ The one departure that is not an inference.
        let v = a_network().left("the Otieno household");
        let h = probe(&v, "the Otieno household", false, &[]);
        assert_eq!(h, Health::Departed);
        assert!(h.is_a_fact());
        assert!(!probe(&a_network(), "the Otieno household", true, &[]).is_a_fact());
    }

    #[test]
    fn refuting_something_that_was_not_a_suspicion_changes_nothing() {
        // ★★ A member cannot talk its way out of having said it was leaving.
        assert_eq!(refute(&Health::Departed), Health::Departed);
        assert_eq!(refute(&Health::Unknown), Health::Unknown);
    }

    #[test]
    fn leaving_destroys_the_edge_and_nothing_else() {
        // ★★★ Disaggregation: the leaving member keeps its entire state. This
        //     module has no method that touches a member's state at all — the
        //     holon invariant applied to the network.
        let v = a_network().left("the Otieno household");
        assert_eq!(v.size(), 1);
        assert_eq!(v.members(), vec!["the Gachiengu household"]);
    }

    #[test]
    fn a_departed_member_can_rejoin() {
        // ★★ `join` and `leave` are reversible Enzymes, not one-way doors.
        let v = a_network().left("the Otieno household").joined("the Otieno household");
        assert_eq!(v.size(), 2);
        assert_eq!(probe(&v, "the Otieno household", true, &[]), Health::Alive);
    }

    #[test]
    fn no_network_power_reaches_past_a_members_gate() {
        // ★★★ Observe, aggregate, price, refuse service — and nothing else.
        //     Only the member knows its own V, so only the member enforces it.
        for p in NetworkPower::all() {
            assert!(!p.reaches_past_a_members_gate(), "{p:?}");
        }
    }

    #[test]
    fn the_network_invariants_are_a_list_somebody_can_run() {
        // ★★ "The network has invariants" is a sentence; a list that names what
        //    is broken is a design.
        let sound = NetInvariants {
            pawa_conserved: true,
            treasury_non_negative: true,
            membership_acyclic: true,
        };
        assert!(sound.holds());
        let broken = NetInvariants { pawa_conserved: false, ..sound };
        assert!(!broken.holds());
        assert_eq!(broken.broken(), vec!["pawa is not conserved under transfer"]);
    }

    #[test]
    fn nothing_here_reads_a_clock() {
        // ★★★ No global clock is one of the three things federation forbids, so
        //     `probe` takes the acks rather than waiting for them: this module
        //     decides what a silence MEANS, and finding out is the host's job.
        //     That is also why it is testable without a network.
        let quiet = probe(&a_network(), "the Otieno household", false, &[false, false, false]);
        assert!(matches!(quiet, Health::Suspect { .. }));
    }
}
