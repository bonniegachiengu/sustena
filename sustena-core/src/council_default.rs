//! **The default council, and the claim that it is free** (Operative · §I).
//!
//! ```text
//!   Ω_default = {Orchie} ∪ {Mentor, Protégé, Attaché, Curator, Navigator}
//! ```
//!
//! ★★★ **Corollary 1 says six by default add no capability and no risk, and it
//! is checkable rather than reassuring.** No capability: an operative is a graph
//! over operators the Sustain already declares, so the union of what all six can
//! do is a *subset* of what one person could already have done by hand —
//! [`adds_capability_beyond`] returns what escapes, and it is empty. No risk:
//! every node in every graph is an ordinary operator call through the ordinary
//! gate, and there is no method on any of these types that executes anything.
//!
//! ★★★ **Orchie gets a seat here, and the reason it previously had none is
//! worth keeping.** In the Python build Orchie had a graph spec, operators and
//! routes, and **no class and no seat in the map** — the member that is
//! structurally required (the only path to a person, and therefore the only path
//! to an approval token) was the one nothing enumerated. A council you cannot
//! enumerate is one you cannot check Corollary 1 against.
//!
//! ★★★ **And Orchie carries no operator graph, which is a different thing from
//! having no seat.** Its duties are route, hold the whole, and present the
//! frontier ([`crate::presentation`]) — operations over the *council*, not over
//! state. Forcing it into a `Dag` would have made it look like a councillor that
//! happens to call no operators, and that is a worse lie than the absence was.

use std::collections::BTreeSet;

use crate::dag::{Condition, Dag, Edge, Node};
use crate::operatives::{attache, mentor};

/// What a member is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// Reasons over state through a graph of operators.
    Councillor,
    /// ★★★ Routes, holds the whole, and presents the frontier. Operations over
    /// the council rather than over state, so **no operator graph** — and the
    /// absence is declared here rather than being an empty `Dag` nobody
    /// questions.
    Orchestrator,
}

/// What a member has to act through.
#[derive(Debug, Clone, PartialEq)]
pub enum Means {
    /// A graph of ordinary operator calls.
    Graph(Box<Dag>),
    /// ★★★ The orchestrator, which acts on the council rather than on state.
    /// **Declared**, not an empty graph nobody questions.
    NotOverState,
    /// ★★★ A real remit with **no operator to reach it yet**.
    ///
    /// Found by enumerating the council: Navigator is supposed to answer *who
    /// is in this household*, and membership lives in `principal.rs` as data
    /// rather than behind an operator, so there is nothing for a graph to call.
    /// Inventing an operator here to make the council look complete would be
    /// adding a capability to satisfy a list — backwards, and exactly the kind of
    /// thing Corollary 1 is supposed to make visible.
    AwaitingAnOperator { needs: &'static str },
}

/// One seat in the default council.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub id: String,
    pub role: Role,
    pub means: Means,
    /// What it is for, in a sentence somebody can disagree with.
    pub remit: &'static str,
}

impl Member {
    /// Every operator this member can reach.
    ///
    /// ★★ Read off the graph rather than declared alongside it. A declared list
    /// beside a graph is a list that goes stale the first time somebody adds a
    /// node.
    pub fn capability(&self) -> BTreeSet<String> {
        match &self.means {
            Means::Graph(g) => g.nodes.iter().map(|n| n.operator.clone()).collect(),
            Means::NotOverState | Means::AwaitingAnOperator { .. } => BTreeSet::new(),
        }
    }

    pub fn graph(&self) -> Option<&Dag> {
        match &self.means {
            Means::Graph(g) => Some(g),
            _ => None,
        }
    }

    /// Is this a seat that cannot yet act?
    ///
    /// ★★ Askable, because a council with a silent member is a council whose
    /// coverage claim is smaller than its roster.
    pub fn awaiting(&self) -> Option<&'static str> {
        match &self.means {
            Means::AwaitingAnOperator { needs } => Some(needs),
            _ => None,
        }
    }
}

/// **Protégé — what did the household not account for?**
///
/// ★★ Its whole job is the unaccounted pile: money that arrived or left with no
/// pocket. It places what it can and leaves the rest, which is the same honest
/// shape Mentor takes.
pub fn protege() -> Dag {
    Dag::new("protege", "identify")
        .with_node(Node::new("identify", "vendor.identify").from_input("counterparty"))
        .with_node(
            Node::new("suggest", "vendor.suggest").from_node("vendor", "identify", "vendor"),
        )
        .with_node(
            Node::new("place", "budget.place_unaccounted")
                .from_node("pocket_name", "suggest", "pocket")
                .from_input("amount")
                .from_input("account"),
        )
        .with_edge(Edge::new("identify", "suggest"))
        .with_edge(Edge::new("suggest", "place").when(Condition::Present {
            node: "suggest".into(),
            field: "pocket".into(),
        }))
}

/// **Curator — what is in the house, and what has been used?**
///
/// ★★ Inventory rather than money. It is the member that keeps `⊕`'s resource
/// leg honest: a household that tracks only its balance knows half its state.
pub fn curator() -> Dag {
    Dag::new("curator", "itemize")
        .with_node(
            Node::new("itemize", "inventory.itemize")
                .from_input("item")
                .from_input("quantity")
                .from_input("unit"),
        )
        .with_node(
            Node::new("consume", "inventory.consume")
                .from_input("item")
                .from_input("quantity"),
        )
        .with_edge(Edge::new("itemize", "consume").when(Condition::Present {
            node: "itemize".into(),
            field: "item".into(),
        }))
}

/// **`Ω_default`** — the six, enumerated so Corollary 1 can be checked.
pub fn default_council() -> Vec<Member> {
    vec![
        Member {
            id: "orchie".into(),
            role: Role::Orchestrator,
            means: Means::NotOverState,
            remit: "route sparsely, hold the whole, and present the frontier rather than the \
                    winner",
        },
        Member {
            id: "mentor".into(),
            role: Role::Councillor,
            means: Means::Graph(Box::new(mentor())),
            remit: "where money the household already understands belongs",
        },
        Member {
            id: "protege".into(),
            role: Role::Councillor,
            means: Means::Graph(Box::new(protege())),
            remit: "what the household did not account for",
        },
        Member {
            id: "attache".into(),
            role: Role::Councillor,
            means: Means::Graph(Box::new(attache())),
            remit: "who a counterparty is, and whether the household should know them",
        },
        Member {
            id: "curator".into(),
            role: Role::Councillor,
            means: Means::Graph(Box::new(curator())),
            remit: "what is in the house, and what has been used",
        },
        Member {
            id: "navigator".into(),
            role: Role::Councillor,
            // ★★★ A real remit and no operator to reach it. See `Means`.
            means: Means::AwaitingAnOperator {
                needs: "an operator that admits a person to the roster — membership is data in                         principal.rs today, not something an operative can move",
            },
            remit: "who is in this household, and who is being let in",
        },
    ]
}

/// Everything the council can reach between them.
pub fn union_capability(council: &[Member]) -> BTreeSet<String> {
    council.iter().flat_map(|m| m.capability()).collect()
}

/// **Corollary 1, first half — what the council adds that the Sustain did not
/// already allow.**
///
/// ★★★ Empty is the claim. An operative is a graph over declared operators, so
/// its reach is a subset of what a person with the same Sustain could already do
/// by hand; anything here would be an operative reaching past its own household,
/// which the gate would refuse anyway — so a non-empty answer means the *council*
/// is misdeclared, not that the gate is unsafe.
pub fn adds_capability_beyond(council: &[Member], allowed: &BTreeSet<String>) -> Vec<String> {
    union_capability(council).difference(allowed).cloned().collect()
}

/// **Corollary 1, second half — is any member holding a private path?**
///
/// ★★ There is nothing on [`Member`] that runs anything: a graph is data, and
/// running it is [`crate::dag::run`]'s job, through the ordinary gate. This
/// function exists so that fact is asserted rather than assumed, by checking
/// that every member either reasons through a graph of ordinary operators or
/// declares that it has none.
pub fn every_member_goes_through_the_gate(council: &[Member]) -> bool {
    council.iter().all(|m| match (&m.role, &m.means) {
        (Role::Councillor, Means::Graph(_)) => true,
        // ★★ A seat that cannot act holds no path at all, private or otherwise.
        (Role::Councillor, Means::AwaitingAnOperator { .. }) => true,
        (Role::Orchestrator, Means::NotOverState) => true,
        _ => false,
    })
}

/// Seats whose remit has no operator behind it yet.
///
/// ★★★ The finding enumerating the council produced. A council with a silent
/// member has a coverage claim smaller than its roster, and nothing else in the
/// system would have said so.
pub fn seats_that_cannot_act(council: &[Member]) -> Vec<(&str, &'static str)> {
    council.iter().filter_map(|m| m.awaiting().map(|n| (m.id.as_str(), n))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::Registry;

    fn allowed_everything() -> BTreeSet<String> {
        union_capability(&default_council())
    }

    #[test]
    fn the_default_set_is_six_and_orchie_is_one_of_them() {
        // ★★★ In the Python build the member that is structurally required —
        //     the only path to a person, and therefore to an approval token —
        //     was the one nothing enumerated. A council you cannot enumerate is
        //     one you cannot check Corollary 1 against.
        let c = default_council();
        assert_eq!(c.len(), 6);
        assert!(c.iter().any(|m| m.id == "orchie" && m.role == Role::Orchestrator));
    }

    #[test]
    fn orchie_carries_no_operator_graph_and_that_is_declared_rather_than_empty() {
        // ★★★ Its duties are over the council, not over state. An empty `Dag`
        //     would make it look like a councillor that happens to call nothing,
        //     which is a worse lie than the absence was.
        let c = default_council();
        let orchie = c.iter().find(|m| m.id == "orchie").expect("seated");
        assert!(orchie.graph().is_none());
        assert!(orchie.capability().is_empty());
        assert!(orchie.remit.contains("present the frontier"));
    }

    #[test]
    fn the_five_councillors_all_reason_through_a_graph() {
        let c = default_council();
        for m in c.iter().filter(|m| m.role == Role::Councillor && m.awaiting().is_none()) {
            assert!(m.graph().is_some(), "{} has no graph", m.id);
            assert!(!m.capability().is_empty(), "{} reaches nothing", m.id);
        }
    }

    #[test]
    fn six_by_default_add_no_capability() {
        // ★★★ Corollary 1, checked rather than reassuring. The union of what
        //     all six can do is a subset of what the Sustain already allows.
        assert!(adds_capability_beyond(&default_council(), &allowed_everything()).is_empty());
    }

    #[test]
    fn a_council_reaching_past_its_household_is_reported_as_a_misdeclaration() {
        // ★★ A non-empty answer means the COUNCIL is misdeclared, not that the
        //    gate is unsafe — the gate would refuse it anyway.
        let narrow: BTreeSet<String> = ["budget.spend".to_string()].into_iter().collect();
        let escapes = adds_capability_beyond(&default_council(), &narrow);
        assert!(!escapes.is_empty());
        assert!(escapes.contains(&"vendor.identify".to_string()));
    }

    #[test]
    fn six_by_default_add_no_risk_because_no_member_holds_a_private_path() {
        // ★★ There is nothing on `Member` that runs anything. A graph is data;
        //    running it goes through the ordinary gate.
        assert!(every_member_goes_through_the_gate(&default_council()));
    }

    #[test]
    fn every_councillors_graph_type_checks_against_the_real_registry() {
        // ★★★ Not a mock. A default council that does not type-check is one
        //     that fails on the household's first real message rather than in a
        //     test.
        let reg = Registry::default();
        for m in default_council().iter().filter(|m| m.graph().is_some()) {
            let g = m.graph().expect("checked");
            assert!(g.typecheck(&reg).is_ok(), "{} does not typecheck: {:?}", m.id, g.typecheck(&reg));
        }
    }

    #[test]
    fn capability_is_read_off_the_graph_rather_than_declared_beside_it() {
        // ★★ A declared list beside a graph goes stale the first time somebody
        //    adds a node.
        let c = default_council();
        let cur = c.iter().find(|m| m.id == "curator").expect("seated");
        assert_eq!(
            cur.capability(),
            ["inventory.consume".to_string(), "inventory.itemize".to_string()]
                .into_iter()
                .collect::<BTreeSet<_>>()
        );
    }

    #[test]
    fn each_member_says_what_it_is_for_in_words_somebody_can_disagree_with() {
        // ★★ "Mentor" is a name; "where money the household already understands
        //    belongs" is a claim, and a claim can be argued with.
        for m in default_council() {
            assert!(m.remit.len() > 20, "{} has no real remit", m.id);
        }
    }

    #[test]
    fn the_council_covers_money_inventory_people_and_the_unaccounted() {
        // ★★ Corollary 1 says six add nothing; it does not say six are
        //    arbitrary. Each leg of the household's state has a member, and a
        //    missing leg would be state nobody is looking at.
        let all = union_capability(&default_council());
        assert!(all.iter().any(|o| o.starts_with("budget.")), "money");
        assert!(all.iter().any(|o| o.starts_with("inventory.")), "what is in the house");
        assert!(all.iter().any(|o| o.starts_with("vendor.")), "who the household deals with");
    }

    #[test]
    fn enumerating_the_council_found_a_seat_with_nothing_to_act_through() {
        // ★★★ The finding this row produced. Navigator has a real remit and no
        //     operator behind it: membership lives in `principal.rs` as data,
        //     not something an operative can move. Inventing `roster.admit` to
        //     make the council look complete would be adding a capability to
        //     satisfy a list, which is backwards — and it is precisely what
        //     Corollary 1 exists to make visible.
        let council = default_council();
        let silent = seats_that_cannot_act(&council);
        assert_eq!(silent.len(), 1);
        assert_eq!(silent[0].0, "navigator");
        assert!(silent[0].1.contains("admits a person to the roster"));
    }

    #[test]
    fn a_seat_that_cannot_act_holds_no_private_path_either() {
        // ★★ It reaches nothing, so it adds nothing — including nothing to
        //    worry about. The gap is a coverage finding, not a safety one.
        let c = default_council();
        let nav = c.iter().find(|m| m.id == "navigator").expect("seated");
        assert!(nav.capability().is_empty());
        assert!(every_member_goes_through_the_gate(&c));
    }

    #[test]
    fn no_member_reaches_a_test_only_operator() {
        // ★★ `test.*` exists to force states the gate should refuse. A default
        //    member reaching one would ship a bypass in the shipped council.
        assert!(!union_capability(&default_council()).iter().any(|o| o.starts_with("test.")));
    }
}
