//! Consensus without a coordinator — quorum, Paxos, and the two bounds
//! (Multiparty §V · MUL-7, MUL-8, MUL-9).
//!
//! > Now the body must agree on one value — which future, which allocation, who
//! > leads — with no center and unreliable messaging.
//!
//! ## The wall first (MUL-7 · FLP)
//!
//! > Fischer, Lynch & Paterson (1985) proved that in a fully asynchronous
//! > system, *no deterministic protocol* can guarantee consensus if even one
//! > node may fail. So every real system buys its way around FLP with timeouts
//! > or randomization — **never magic**.
//!
//! So this module guarantees **safety** and does not guarantee **liveness**,
//! and the types say which is which. [`propose`] returns a [`Round`], not a
//! chosen value: [`RoundOutcome::Failed`] is a **normal outcome**, not an
//! error. A round can lose its promise to a higher-numbered proposer and get
//! nowhere, and that is the protocol working correctly under FLP rather than
//! something going wrong. Nothing here retries, because retrying is exactly
//! the timeout-or-randomisation the article says has to be *bought* — and it is
//! bought outside core, where there is a clock.
//!
//! ## ★ Safety, made unspellable (MUL-8 · Paxos)
//!
//! > Use overlapping majorities: any two quorums share a node, so two
//! > conflicting decisions can't both be ratified.
//!
//! That property is the whole of Paxos's safety, and it is a property of the
//! **body**, not of the protocol run. So [`Body::new`] **refuses a
//! quorum size that does not guarantee overlap** — `2q > k`. A body from
//! which two disjoint quorums could be drawn cannot be constructed, so the
//! configuration that would let two conflicting values both ratify does not
//! exist to be run.
//!
//! The rest follows §V's protocol exactly:
//!
//! ```text
//! send PREPARE(n) to a quorum
//! IF majority PROMISE(n, accepted?):
//!     v = highest-numbered accepted value seen, else own value
//!     send ACCEPT(n, v) to a quorum
//! IF majority ACCEPTED(n, v):  v is chosen   # any later round re-chooses v
//! ```
//!
//! **The re-choosing is not latched.** It would be easy to have [`Ledger`]
//! remember a chosen value and hand it back, and that would prove nothing —
//! the point is that the *protocol* forces it. A chosen value is computed from
//! acceptor state every time it is asked for, and a later round adopts it
//! because the `v = highest-numbered accepted value seen` rule makes it, not
//! because anything stored it as final.
//!
//! ## The other bound, recorded rather than built (MUL-9 · Byzantine)
//!
//! > Agreement among `k` nodes tolerating `f` traitors is possible iff
//! > `k ≥ 3f + 1`. That bound is the price of trusting a body whose parts might
//! > betray it.
//!
//! **This protocol is not Byzantine fault tolerant**, and
//! [`ByzantineBound::THIS_BUILD_TOLERATES_BYZANTINE`] is a `false` a test
//! asserts. Paxos survives *crash* faults — a node that stops — not a node that
//! lies. [`ByzantineBound`] computes what a BFT protocol on this body
//! *would* tolerate and what a target `f` *would* require, because the article
//! is explicit that the bound is *"the design limit to remember before the
//! federation includes anyone you don't fully trust"* — and a limit you cannot
//! compute is not one you can remember.
//!
//! ## Where this lands in canon
//!
//! > Consensus is the machinery; the session is what runs on it.
//!
//! `CouncilSession` (Symbionts advise, the human decides) is the deliberation
//! layer, and it is not this. What this closes is a different loop:
//! `editing.rs`'s [`crate::editing::CouncilMint`] took `quorum_met` as a
//! **caller-asserted bool**, with a comment naming Multiparty as the
//! composition point. With [`Decision`] it can be **derived** — a `Decision`
//! has private fields and no constructor, and is obtainable only from a
//! [`Ledger`] where acceptors genuinely accepted. A shared definition's
//! rule-change quorum is now computed, not trusted.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use thiserror::Error;

// ── the body, and the safety property ─────────────────────────────────

/// `k ≥ 3f + 1` (Lamport, Shostak & Pease 1982) — recorded, not implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByzantineBound {
    pub nodes: usize,
}

impl ByzantineBound {
    /// **This build is not Byzantine fault tolerant.**
    ///
    /// Paxos survives a node that *stops*, not a node that *lies*. Asserted by
    /// a test, so the claim cannot quietly drift.
    pub const THIS_BUILD_TOLERATES_BYZANTINE: bool = false;

    /// The largest `f` for which `k ≥ 3f+1` holds — what a **BFT** protocol on
    /// this many nodes could tolerate. Not what this one does.
    pub fn traitors_a_bft_protocol_could_tolerate(&self) -> usize {
        self.nodes.saturating_sub(1) / 3
    }

    /// `3f + 1` — how many nodes a body would need to tolerate `f` traitors.
    pub fn nodes_required_for(f: usize) -> usize {
        3 * f + 1
    }

    /// Is this body large enough for a BFT protocol to tolerate `f`?
    pub fn would_admit(&self, f: usize) -> bool {
        self.nodes >= Self::nodes_required_for(f)
    }
}

/// Who is in the body, and how many must accept.
///
/// **Quorum size is declared per body** — a household's threshold is a
/// statement about that household. What is *not* negotiable is the overlap
/// property: see [`Body::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    nodes: BTreeSet<String>,
    quorum_size: usize,
}

impl Body {
    /// **Refuses a quorum size that does not guarantee overlapping majorities.**
    ///
    /// Two quorums of size `q` drawn from `k` nodes must share a member, which
    /// holds exactly when `2q > k`. Below that, two disjoint quorums could each
    /// ratify a different value — the one thing §V's safety rests on not
    /// happening. Refused at construction, so that configuration cannot be run
    /// rather than being run carefully.
    pub fn new(nodes: Vec<String>, quorum_size: usize) -> Result<Self, ConsensusError> {
        let set: BTreeSet<String> = nodes.into_iter().collect();
        let k = set.len();
        if k == 0 {
            return Err(ConsensusError::EmptyBody);
        }
        if quorum_size == 0 || quorum_size > k {
            return Err(ConsensusError::QuorumOutOfRange { quorum: quorum_size, nodes: k });
        }
        if 2 * quorum_size <= k {
            return Err(ConsensusError::QuorumsNeedNotOverlap { quorum: quorum_size, nodes: k });
        }
        Ok(Self { nodes: set, quorum_size })
    }

    /// The smallest quorum that overlaps — `⌊k/2⌋ + 1`.
    pub fn majority(nodes: Vec<String>) -> Result<Self, ConsensusError> {
        let k = nodes.iter().collect::<BTreeSet<_>>().len();
        if k == 0 {
            return Err(ConsensusError::EmptyBody);
        }
        Self::new(nodes, k / 2 + 1)
    }

    pub fn nodes(&self) -> &BTreeSet<String> {
        &self.nodes
    }

    pub fn size(&self) -> usize {
        self.nodes.len()
    }

    pub fn quorum_size(&self) -> usize {
        self.quorum_size
    }

    pub fn contains(&self, id: &str) -> bool {
        self.nodes.contains(id)
    }

    /// The bound this body sits against.
    pub fn byzantine(&self) -> ByzantineBound {
        ByzantineBound { nodes: self.size() }
    }
}

// ── Paxos ───────────────────────────────────────────────────────────────────

/// `n` — globally unique and totally ordered.
///
/// The proposer's id breaks ties, so two proposers cannot mint the same number
/// in the same round. Ordering is `(round, proposer)`, which is what lets
/// "highest-numbered accepted value seen" be a well-defined phrase.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProposalNumber {
    pub round: u64,
    pub proposer: String,
}

impl ProposalNumber {
    pub fn new(round: u64, proposer: &str) -> Self {
        Self { round, proposer: proposer.to_string() }
    }
}

/// One acceptor's state.
#[derive(Debug, Clone, PartialEq, Default)]
struct Acceptor {
    promised: Option<ProposalNumber>,
    accepted: Option<(ProposalNumber, Value)>,
}

/// The reply to `PREPARE(n)`.
#[derive(Debug, Clone, PartialEq)]
pub enum Promise {
    /// Granted, carrying whatever this acceptor has already accepted.
    Granted { accepted: Option<(ProposalNumber, Value)> },
    /// Refused: this acceptor has already promised a higher number.
    Refused { promised: ProposalNumber },
}

/// The reply to `ACCEPT(n, v)`.
#[derive(Debug, Clone, PartialEq)]
pub enum Accepted {
    Ok,
    Refused { promised: ProposalNumber },
}

/// **Proof that a value was chosen by an overlapping-majority quorum.**
///
/// Private fields and no public constructor. Obtainable only from
/// [`Ledger::decision`], which returns `Some` only when acceptors genuinely
/// accepted — which is what lets `CouncilMint` *derive* a quorum instead of
/// being told one was met.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    value: Value,
    number: ProposalNumber,
    quorum: BTreeSet<String>,
    body_size: usize,
}

impl Decision {
    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn number(&self) -> &ProposalNumber {
        &self.number
    }

    /// Exactly which acceptors accepted it — the quorum, named.
    pub fn quorum(&self) -> &BTreeSet<String> {
        &self.quorum
    }

    pub fn accepted_by(&self) -> usize {
        self.quorum.len()
    }

    pub fn body_size(&self) -> usize {
        self.body_size
    }

    pub fn describe(&self) -> String {
        format!(
            "{:?} chosen at round {} by {} of {} ({})",
            self.value,
            self.number.round,
            self.accepted_by(),
            self.body_size,
            self.quorum.iter().cloned().collect::<Vec<_>>().join(", ")
        )
    }
}

/// The acceptor set.
#[derive(Debug, Clone)]
pub struct Ledger {
    body: Body,
    acceptors: BTreeMap<String, Acceptor>,
}

impl Ledger {
    pub fn new(body: Body) -> Self {
        let acceptors =
            body.nodes().iter().map(|id| (id.clone(), Acceptor::default())).collect();
        Self { body, acceptors }
    }

    pub fn body(&self) -> &Body {
        &self.body
    }

    /// `PREPARE(n)` to the named acceptors.
    pub fn prepare(
        &mut self,
        n: &ProposalNumber,
        to: &[&str],
    ) -> Result<Vec<(String, Promise)>, ConsensusError> {
        let mut out = Vec::new();
        for id in to {
            let a = self
                .acceptors
                .get_mut(*id)
                .ok_or_else(|| ConsensusError::NotAMember((*id).to_string()))?;
            match &a.promised {
                Some(p) if p >= n => {
                    out.push(((*id).to_string(), Promise::Refused { promised: p.clone() }))
                }
                _ => {
                    a.promised = Some(n.clone());
                    out.push(((*id).to_string(), Promise::Granted { accepted: a.accepted.clone() }))
                }
            }
        }
        Ok(out)
    }

    /// `ACCEPT(n, v)` to the named acceptors.
    pub fn accept(
        &mut self,
        n: &ProposalNumber,
        v: &Value,
        to: &[&str],
    ) -> Result<Vec<(String, Accepted)>, ConsensusError> {
        let mut out = Vec::new();
        for id in to {
            let a = self
                .acceptors
                .get_mut(*id)
                .ok_or_else(|| ConsensusError::NotAMember((*id).to_string()))?;
            match &a.promised {
                Some(p) if p > n => {
                    out.push(((*id).to_string(), Accepted::Refused { promised: p.clone() }))
                }
                _ => {
                    a.promised = Some(n.clone());
                    a.accepted = Some((n.clone(), v.clone()));
                    out.push(((*id).to_string(), Accepted::Ok))
                }
            }
        }
        Ok(out)
    }

    /// **Computed from acceptor state, never latched.**
    ///
    /// A value is chosen when at least `quorum_size` acceptors hold it. Storing
    /// a "final" value and handing it back would prove nothing about the
    /// protocol; this way, the re-choosing property is something the protocol
    /// produces rather than something the ledger remembers.
    pub fn decision(&self) -> Option<Decision> {
        let mut by_value: BTreeMap<String, (ProposalNumber, BTreeSet<String>)> = BTreeMap::new();
        for (id, a) in &self.acceptors {
            if let Some((n, v)) = &a.accepted {
                let key = v.to_string();
                let e = by_value
                    .entry(key)
                    .or_insert_with(|| (n.clone(), BTreeSet::new()));
                if *n > e.0 {
                    e.0 = n.clone();
                }
                e.1.insert(id.clone());
            }
        }
        for (_, (number, quorum)) in by_value {
            if quorum.len() >= self.body.quorum_size() {
                let value = self
                    .acceptors
                    .values()
                    .filter_map(|a| a.accepted.as_ref())
                    .find(|(n, _)| *n == number)
                    .map(|(_, v)| v.clone())
                    .expect("the number came from an accepted entry");
                return Some(Decision {
                    value,
                    number,
                    quorum,
                    body_size: self.body.size(),
                });
            }
        }
        None
    }

    pub fn chosen(&self) -> Option<Value> {
        self.decision().map(|d| d.value)
    }
}

/// Which phase a round got stuck in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundPhase {
    Prepare,
    Accept,
}

/// How a round ended.
///
/// **`Failed` is a normal outcome, not an error.** FLP says no deterministic
/// protocol guarantees consensus under asynchrony with even one faulty node, so
/// a round that gets nowhere is the protocol behaving correctly. Returning
/// `Err` here would let a caller `?` a lost race into a crash.
#[derive(Debug, Clone, PartialEq)]
pub enum RoundOutcome {
    Chosen(Value),
    Failed { at: RoundPhase, reason: String },
}

/// One proposer's round.
#[derive(Debug, Clone, PartialEq)]
pub struct Round {
    pub number: ProposalNumber,
    /// What this proposer wanted.
    pub own_value: Value,
    /// What it actually proposed — someone else's value if a promise carried a
    /// higher-numbered acceptance. `None` if it never got to ACCEPT.
    pub proposed: Option<Value>,
    pub outcome: RoundOutcome,
}

impl Round {
    /// Did this proposer have to adopt a value it did not want?
    ///
    /// The visible form of *"any later round re-chooses the same v"*.
    pub fn adopted_another_value(&self) -> bool {
        self.proposed.as_ref().is_some_and(|p| *p != self.own_value)
    }

    pub fn succeeded(&self) -> bool {
        matches!(self.outcome, RoundOutcome::Chosen(_))
    }
}

/// Run one proposer's round against a quorum (§V's protocol, exactly).
pub fn propose(
    ledger: &mut Ledger,
    number: ProposalNumber,
    own_value: Value,
    quorum: &[&str],
) -> Result<Round, ConsensusError> {
    if quorum.len() < ledger.body().quorum_size() {
        return Err(ConsensusError::QuorumTooSmall {
            got: quorum.len(),
            need: ledger.body().quorum_size(),
        });
    }

    // ── PREPARE(n) ──────────────────────────────────────────────────────────
    let promises = ledger.prepare(&number, quorum)?;
    let granted: Vec<&(String, Promise)> =
        promises.iter().filter(|(_, p)| matches!(p, Promise::Granted { .. })).collect();

    if granted.len() < ledger.body().quorum_size() {
        return Ok(Round {
            number,
            own_value,
            proposed: None,
            outcome: RoundOutcome::Failed {
                at: RoundPhase::Prepare,
                reason: format!(
                    "{} of {} promised; a higher number holds the rest",
                    granted.len(),
                    ledger.body().quorum_size()
                ),
            },
        });
    }

    // v = highest-numbered accepted value seen, else own value.
    let mut best: Option<(ProposalNumber, Value)> = None;
    for (_, p) in &promises {
        if let Promise::Granted { accepted: Some((n, v)) } = p {
            if best.as_ref().is_none_or(|(bn, _)| n > bn) {
                best = Some((n.clone(), v.clone()));
            }
        }
    }
    let value = best.map(|(_, v)| v).unwrap_or_else(|| own_value.clone());

    // ── ACCEPT(n, v) ────────────────────────────────────────────────────────
    let accepts = ledger.accept(&number, &value, quorum)?;
    let ok = accepts.iter().filter(|(_, a)| *a == Accepted::Ok).count();

    if ok < ledger.body().quorum_size() {
        return Ok(Round {
            number,
            own_value,
            proposed: Some(value),
            outcome: RoundOutcome::Failed {
                at: RoundPhase::Accept,
                reason: format!(
                    "{} of {} accepted; a higher number arrived mid-round",
                    ok,
                    ledger.body().quorum_size()
                ),
            },
        });
    }

    Ok(Round { number, own_value, proposed: Some(value.clone()), outcome: RoundOutcome::Chosen(value) })
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum ConsensusError {
    #[error("a body with no members cannot agree on anything")]
    EmptyBody,
    #[error("a quorum of {quorum} is out of range for {nodes} nodes")]
    QuorumOutOfRange { quorum: usize, nodes: usize },
    #[error("a quorum of {quorum} from {nodes} nodes need not overlap (2q ≤ k) — two disjoint quorums could each ratify a different value, which is the one thing §V's safety rests on not happening")]
    QuorumsNeedNotOverlap { quorum: usize, nodes: usize },
    #[error("'{0}' is not a member of this body")]
    NotAMember(String),
    #[error("a round was sent to {got} acceptors but the quorum is {need}")]
    QuorumTooSmall { got: usize, need: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn five() -> Body {
        Body::majority(
            ["a", "b", "c", "d", "e"].into_iter().map(String::from).collect(),
        )
        .expect("five nodes, majority of three")
    }

    // ── ★ MUL-8 — safety is a property of the MEMBERSHIP ────────────────────

    #[test]
    fn a_quorum_that_need_not_overlap_is_refused() {
        // ★ THE SAFETY PROPERTY, made unspellable. Two quorums of 2 from 5
        // nodes can be disjoint, so each could ratify a different value.
        let err = Body::new(
            ["a", "b", "c", "d", "e"].into_iter().map(String::from).collect(),
            2,
        )
        .unwrap_err();

        assert!(matches!(err, ConsensusError::QuorumsNeedNotOverlap { .. }), "got {err:?}");
        assert!(err.to_string().contains("each ratify a different value"));
    }

    #[test]
    fn a_majority_is_the_smallest_quorum_that_overlaps() {
        assert_eq!(five().quorum_size(), 3, "⌊5/2⌋ + 1");
        // And 3 is genuinely the floor: 2 was refused above.
        assert!(2 * five().quorum_size() > five().size());
    }

    #[test]
    fn an_empty_body_cannot_agree_on_anything() {
        assert!(matches!(Body::new(vec![], 1), Err(ConsensusError::EmptyBody)));
        assert!(matches!(Body::majority(vec![]), Err(ConsensusError::EmptyBody)));
    }

    // ── §V's protocol ───────────────────────────────────────────────────────

    #[test]
    fn a_value_is_chosen_when_a_quorum_accepts_it() {
        let mut l = Ledger::new(five());
        let r = propose(&mut l, ProposalNumber::new(1, "p1"), json!("solar"), &["a", "b", "c"])
            .unwrap();

        assert!(r.succeeded());
        assert_eq!(r.outcome, RoundOutcome::Chosen(json!("solar")));
        assert!(!r.adopted_another_value(), "nothing had been accepted before it");

        let d = l.decision().expect("a quorum accepted");
        assert_eq!(d.value(), &json!("solar"));
        assert_eq!(d.accepted_by(), 3);
        assert_eq!(d.quorum(), &["a", "b", "c"].into_iter().map(String::from).collect());
    }

    #[test]
    fn nothing_is_chosen_before_a_quorum_accepts() {
        let mut l = Ledger::new(five());
        l.prepare(&ProposalNumber::new(1, "p1"), &["a", "b", "c"]).unwrap();
        assert!(l.decision().is_none(), "promises are not acceptances");

        l.accept(&ProposalNumber::new(1, "p1"), &json!("solar"), &["a", "b"]).unwrap();
        assert!(l.decision().is_none(), "two of three is not a quorum");
    }

    // ── ★ two proposers race ────────────────────────────────────────────────

    #[test]
    fn a_later_round_re_chooses_the_same_value() {
        // ★★ THE PROOF OF VALUE, and the heart of §V's safety. A second
        // proposer wants something different and CANNOT have it: the overlap
        // guarantees its quorum contains an acceptor that already took v, and
        // the "highest-numbered accepted value seen" rule makes it adopt v.
        let mut l = Ledger::new(five());
        propose(&mut l, ProposalNumber::new(1, "p1"), json!("solar"), &["a", "b", "c"]).unwrap();

        let r2 = propose(&mut l, ProposalNumber::new(2, "p2"), json!("diesel"), &["c", "d", "e"])
            .unwrap();

        assert!(r2.succeeded());
        assert_eq!(r2.own_value, json!("diesel"), "it wanted something else");
        assert_eq!(
            r2.outcome,
            RoundOutcome::Chosen(json!("solar")),
            "★ and re-chose the value already chosen"
        );
        assert!(r2.adopted_another_value());
        assert_eq!(l.chosen(), Some(json!("solar")), "a chosen value cannot be un-chosen");
    }

    #[test]
    fn the_overlap_is_what_forces_it_and_the_quorums_share_exactly_one_node() {
        // Not a coincidence of the fixture: {a,b,c} ∩ {c,d,e} = {c}, and one
        // shared acceptor is enough, which is precisely why 2q > k suffices.
        let q1: BTreeSet<&str> = ["a", "b", "c"].into_iter().collect();
        let q2: BTreeSet<&str> = ["c", "d", "e"].into_iter().collect();
        let shared: Vec<&&str> = q1.intersection(&q2).collect();
        assert_eq!(shared.len(), 1);

        let mut l = Ledger::new(five());
        propose(&mut l, ProposalNumber::new(1, "p1"), json!("solar"), &["a", "b", "c"]).unwrap();
        // `c` is the only node carrying the acceptance into the second quorum.
        let promises = l.prepare(&ProposalNumber::new(2, "p2"), &["c", "d", "e"]).unwrap();
        let carrying: Vec<&(String, Promise)> = promises
            .iter()
            .filter(|(_, p)| matches!(p, Promise::Granted { accepted: Some(_) }))
            .collect();
        assert_eq!(carrying.len(), 1);
        assert_eq!(carrying[0].0, "c");
    }

    #[test]
    fn re_choosing_is_produced_by_the_protocol_not_remembered_by_the_ledger() {
        // The ledger holds no "final" field. `decision()` recomputes from
        // acceptor state every time, so the property is the protocol's.
        let mut l = Ledger::new(five());
        propose(&mut l, ProposalNumber::new(1, "p1"), json!("solar"), &["a", "b", "c"]).unwrap();
        let first = l.decision().unwrap();
        let second = l.decision().unwrap();
        assert_eq!(first, second, "recomputed, and stable");
    }

    // ── ★ MUL-7 — FLP: a round can fail, and that is correct ────────────────

    #[test]
    fn a_round_can_fail_at_prepare_and_that_is_a_normal_outcome() {
        // ★ FLP. A higher-numbered proposer took the promises first, so this
        // round gets nowhere. Not an Err — a caller `?`-ing a lost race into a
        // crash would be reading a correct protocol as a broken one.
        let mut l = Ledger::new(five());
        l.prepare(&ProposalNumber::new(9, "p9"), &["a", "b", "c", "d", "e"]).unwrap();

        let r = propose(&mut l, ProposalNumber::new(2, "p2"), json!("diesel"), &["a", "b", "c"])
            .unwrap();

        assert!(!r.succeeded());
        match &r.outcome {
            RoundOutcome::Failed { at, reason } => {
                assert_eq!(*at, RoundPhase::Prepare);
                assert!(reason.contains("higher number"));
            }
            other => panic!("expected a failed round, got {other:?}"),
        }
        assert!(r.proposed.is_none(), "it never reached ACCEPT");
        assert!(l.decision().is_none(), "and nothing was chosen");
    }

    #[test]
    fn a_round_can_fail_at_accept_when_a_higher_number_arrives_mid_round() {
        let mut l = Ledger::new(five());
        let n = ProposalNumber::new(1, "p1");
        l.prepare(&n, &["a", "b", "c"]).unwrap();
        // A higher proposer sweeps in between PREPARE and ACCEPT.
        l.prepare(&ProposalNumber::new(5, "p5"), &["a", "b", "c"]).unwrap();

        let accepts = l.accept(&n, &json!("solar"), &["a", "b", "c"]).unwrap();
        assert!(accepts.iter().all(|(_, a)| matches!(a, Accepted::Refused { .. })));
        assert!(l.decision().is_none());
    }

    #[test]
    fn nothing_here_retries() {
        // Buying around FLP is timeouts or randomisation, and both need a
        // clock. Core has none, so a failed round comes back as a failed round
        // and the host decides whether to try again.
        let mut l = Ledger::new(five());
        l.prepare(&ProposalNumber::new(9, "p9"), &["a", "b", "c"]).unwrap();
        let r = propose(&mut l, ProposalNumber::new(1, "p1"), json!("x"), &["a", "b", "c"]).unwrap();
        assert!(!r.succeeded());
        assert_eq!(r.number.round, 1, "the round number is unchanged — nothing escalated it");
    }

    #[test]
    fn a_round_sent_to_fewer_than_a_quorum_is_refused() {
        let mut l = Ledger::new(five());
        assert!(matches!(
            propose(&mut l, ProposalNumber::new(1, "p"), json!("x"), &["a", "b"]),
            Err(ConsensusError::QuorumTooSmall { got: 2, need: 3 })
        ));
    }

    #[test]
    fn a_non_member_cannot_be_asked() {
        let mut l = Ledger::new(five());
        assert!(matches!(
            l.prepare(&ProposalNumber::new(1, "p"), &["a", "outsider", "c"]),
            Err(ConsensusError::NotAMember(_))
        ));
    }

    // ── MUL-9 — the Byzantine bound, recorded ───────────────────────────────

    #[test]
    fn the_byzantine_bound_is_computable_and_this_build_does_not_meet_it() {
        // ★ The article calls it "the design limit to remember before the
        // federation includes anyone you don't fully trust" — and a limit you
        // cannot compute is not one you can remember.
        // clippy is right that this is constant, and that is the point: the
        // assertion exists to FAIL if someone ever flips the const without
        // building the protocol underneath it.
        #[allow(clippy::assertions_on_constants)]
        {
            assert!(
                !ByzantineBound::THIS_BUILD_TOLERATES_BYZANTINE,
                "Paxos survives crashes, not lies"
            );
        }

        let b = five().byzantine();
        assert_eq!(b.nodes, 5);
        assert_eq!(b.traitors_a_bft_protocol_could_tolerate(), 1, "5 ≥ 3·1+1");
        assert!(b.would_admit(1));
        assert!(!b.would_admit(2), "2 traitors would need 7 nodes");
        assert_eq!(ByzantineBound::nodes_required_for(2), 7);
    }

    #[test]
    fn a_household_sized_body_is_the_working_default() {
        // "For Sustena's scale — a household, a small crew, a handful of nodes
        // — plain quorum/Paxos is the working default."
        let household = Body::majority(
            ["bonnie", "cira", "epha"].into_iter().map(String::from).collect(),
        )
        .unwrap();
        assert_eq!(household.quorum_size(), 2);
        assert_eq!(household.byzantine().traitors_a_bft_protocol_could_tolerate(), 0);
    }
}
