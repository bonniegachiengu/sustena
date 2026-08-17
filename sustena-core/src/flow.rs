//! `F` — the firewall: what crosses the boundary
//! (Constraint §I + §II · CON-1, CON-2, IMM-1).
//!
//! ```text
//! φ = ⟨ dir ∈ {in, out}, kind, qty, counterparty ⟩,     F(φ, s) → {⊤, ⊥}
//!
//! admit_F(o, s) = admit(o, s) ∧ ⋀_{φ ∈ flows(o,s)} F(φ, s)
//! ```
//!
//! The constraint layer's **third** predicate family, and it gets its own
//! module for the same reason `D` did: `C` is over a state, `D` is over a
//! pair, and `F` is over **neither**.
//!
//! ## ★★ `F` is irreducible to `C` or `D` — and that is provable, not asserted
//!
//! > `F` is not reducible to `C` or `D` either. **Two transitions can have
//! > identical before-and-after states and differ entirely in what crossed the
//! > boundary to produce them** — which is precisely the distinction between
//! > money moving between two of your own pockets and the same amount arriving
//! > from outside.
//!
//! That is the household's real case, and it is the signature proof for this
//! module: 500 moved from `food` to `savings` and 500 sent out plus 500
//! received back produce **byte-identical** before and after states. `C` sees
//! one candidate. `D` sees one pair — and **even conservation passes both**,
//! because both conserve. Only `F` can tell them apart, because only `F` is
//! about the crossing rather than the endpoints.
//!
//! ## ★ A flow is defined relative to `μ`, not independently of it
//!
//! §4D deferred flows here *"explicitly"*, and the boundary it deferred them
//! against is [`crate::boundary`]'s `μ`. So an operator does **not** declare a
//! flow — it declares a [`Movement`], saying what moved and between whom, and
//! [`classify_movement`] asks `μ` whether that movement crossed `B`:
//!
//! - both ends owned → [`Crossing::Internal`], **no flow at all**;
//! - one end outside → a [`Flow`], with `dir` decided by which end;
//! - neither end owned → [`Crossing::Foreign`], reported rather than ignored.
//!
//! This is the right split because an operator should not have to know the
//! boundary to describe what it did, and because *"which side of `B` is this
//! counterparty on"* is exactly the question `μ` exists to answer. Pocket-to-
//! pocket is internal **because `μ` owns both pockets**, not because the
//! operator said so.
//!
//! ## ★ Flows are DECLARED, never inferred from the endpoints
//!
//! > `candidate, flows = o.effect(copy(s), params, ctx)`
//!
//! The effect returns both. That is not an implementation convenience: if
//! flows could be recovered by diffing the endpoints, `F` would be reducible
//! to `D` and this module would not need to exist. The irreducibility case
//! above is the same fact stated from the other side.
//!
//! ## Composition is conjunction, and only conjunction
//!
//! > There is no priority order and no override. A constraint set is a set,
//! > and adding one can only ever **shrink** the admissible region.
//!
//! [`check_flows`] conjoins, returns the **first** rule that refuses, and has
//! no notion of one rule overriding another. Adding a firewall rule can never
//! admit something that was previously refused, and a test asserts that
//! monotonicity directly.
//!
//! ## `F(φ, s)` takes the state, and one rule shape uses it
//!
//! The article gives `F` the state, so [`FlowRule::OnlyKnown`] reads a
//! declared register — *"only send to a counterparty already in your
//! contacts"*. A firewall that could only see the flow would be a static
//! allowlist; the `s` is what lets a rule be about the household's own
//! history.
//!
//! ## ★ The clamp trap — refused at authoring time, not shipped
//!
//! > Capping a transfer's outflow at the available balance while leaving the
//! > inflow untouched **creates money** — it satisfies `C` by violating `D`. A
//! > clamp must be applied to the transition **as a whole**, and any clamp on
//! > a conserved dimension must be rejected at authoring time.
//!
//! So [`FlowRule`] has **no clamp strategy at all**: a firewall rule refuses
//! or admits. Clamping one side of a crossing is precisely the money-minting
//! move the article warns about, and the safest form of "reject at authoring
//! time" is a strategy that cannot be declared. Clamp-on-flows as a *whole-
//! transition* projection is a real, separate piece of work — see the slot in
//! the module's honest limits.
//!
//! ## Honest limits
//!
//! - **Rules are declared shapes, not an expression language.** Four of them,
//!   no `eval`, the same discipline as `predicate`/`transition`. A firewall
//!   needing arbitrary logic is a different row.
//! - **No clamp.** See above — deliberate, and the deliberate part is that it
//!   is unrepresentable rather than merely unimplemented. A whole-transition
//!   clamp is the follow-on.
//! - **`qty` is a number, not a typed quantity.** Conservation's
//!   integer-minor-unit discipline lives in `transition.rs` where the totals
//!   are; a flow carries what the operator declared it moved.

use std::collections::BTreeSet;

use serde_json::Value;
use thiserror::Error;

use crate::boundary::{Boundary, Owned};

/// Which way across `B`.
///
/// **`FlowDirection`, not `Direction`** — [`crate::transition::Direction`] is
/// monotonicity's up/down, a genuinely different axis. Two real notions of
/// direction in one crate, and the newcomer takes the longer name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FlowDirection {
    In,
    Out,
}

impl FlowDirection {
    pub fn name(&self) -> &'static str {
        match self {
            FlowDirection::In => "in",
            FlowDirection::Out => "out",
        }
    }
}

/// What an operator says it moved — **declared, never inferred**.
///
/// Both ends are plain names. Whether either is inside `B` is `μ`'s question,
/// not the operator's, which is what keeps an operator from having to know the
/// boundary to describe its own effect.
#[derive(Debug, Clone, PartialEq)]
pub struct Movement {
    pub kind: String,
    pub qty: f64,
    pub from: String,
    pub to: String,
}

impl Movement {
    pub fn new(kind: &str, qty: f64, from: &str, to: &str) -> Self {
        Self {
            kind: kind.to_string(),
            qty,
            from: from.to_string(),
            to: to.to_string(),
        }
    }
}

/// `φ = ⟨dir, kind, qty, counterparty⟩` — a movement that actually crossed `B`.
#[derive(Debug, Clone, PartialEq)]
pub struct Flow {
    pub dir: FlowDirection,
    pub kind: String,
    pub qty: f64,
    /// The party on the **far** side of the boundary.
    pub counterparty: String,
}

impl Flow {
    pub fn describe(&self) -> String {
        format!(
            "{} {} of {} {}",
            self.qty,
            self.kind,
            self.dir.name(),
            match self.dir {
                FlowDirection::In => format!("from {}", self.counterparty),
                FlowDirection::Out => format!("to {}", self.counterparty),
            }
        )
    }
}

/// What `μ` made of a declared movement.
#[derive(Debug, Clone, PartialEq)]
pub enum Crossing {
    /// Both ends owned. **No flow** — this never touched `B`, and `F` has
    /// nothing to say about it. The pocket-to-pocket case.
    Internal,
    /// One end outside. This is a flow, and `F` judges it.
    Crossed(Flow),
    /// ★ Neither end is owned. Not a flow — it did not cross **this**
    /// boundary — but reported rather than silently dropped, because an
    /// operator declaring a movement between two outside parties is either a
    /// mistake or a thing worth seeing.
    Foreign { from: String, to: String },
}

impl Crossing {
    pub fn flow(&self) -> Option<&Flow> {
        match self {
            Crossing::Crossed(f) => Some(f),
            _ => None,
        }
    }
}

/// Ask `μ` whether a declared movement crossed `B`, and which way.
///
/// **`classify_movement`, not `classify`** — [`crate::detect::classify`] is the
/// severity classification on the Monitor→Controller boundary.
pub fn classify_movement(movement: &Movement, boundary: &Boundary) -> Crossing {
    let from_owned = owned_by(boundary, &movement.from);
    let to_owned = owned_by(boundary, &movement.to);
    match (from_owned, to_owned) {
        (true, true) => Crossing::Internal,
        (true, false) => Crossing::Crossed(Flow {
            dir: FlowDirection::Out,
            kind: movement.kind.clone(),
            qty: movement.qty,
            counterparty: movement.to.clone(),
        }),
        (false, true) => Crossing::Crossed(Flow {
            dir: FlowDirection::In,
            kind: movement.kind.clone(),
            qty: movement.qty,
            counterparty: movement.from.clone(),
        }),
        (false, false) => Crossing::Foreign {
            from: movement.from.clone(),
            to: movement.to.clone(),
        },
    }
}

/// A name is inside `B` if `μ` owns it **either** as an entity or as a path —
/// `Own(Σ) = {x : μ(x) = 1}` ranges over `Entities ∪ Paths`, so both readings
/// are tried before concluding a party is outside.
fn owned_by(boundary: &Boundary, name: &str) -> bool {
    boundary.owns(&Owned::entity(name)) || boundary.owns(&Owned::path(name))
}

/// Classify every declared movement, keeping only the ones that crossed.
pub fn flows_of(movements: &[Movement], boundary: &Boundary) -> Vec<Flow> {
    movements
        .iter()
        .filter_map(|m| classify_movement(m, boundary).flow().cloned())
        .collect()
}

// ---------------------------------------------------------------------------
// F itself
// ---------------------------------------------------------------------------

/// A declared firewall rule.
///
/// ★ There is **no clamp**. A rule refuses or admits, because clamping one
/// side of a crossing is exactly the money-minting move the article warns
/// about — and the safest form of *"rejected at authoring time"* is a strategy
/// nobody can declare.
#[derive(Debug, Clone, PartialEq)]
pub enum FlowRule {
    /// Nothing of this kind crosses in this direction. `None` matches any.
    Forbid {
        id: String,
        dir: Option<FlowDirection>,
        kind: Option<String>,
    },
    /// A counterparty allowlist. Anything outside it is refused.
    OnlyWith {
        id: String,
        dir: Option<FlowDirection>,
        counterparties: BTreeSet<String>,
    },
    /// A cap on a **single** crossing.
    MaxQty {
        id: String,
        dir: Option<FlowDirection>,
        kind: Option<String>,
        limit: f64,
    },
    /// ★ The rule that uses `F`'s state argument: the counterparty must
    /// already appear in a declared register — *"only send to someone in your
    /// contacts"*. A firewall that could only see the flow would be a static
    /// allowlist.
    OnlyKnown {
        id: String,
        dir: Option<FlowDirection>,
        register: String,
    },
}

impl FlowRule {
    pub fn id(&self) -> &str {
        match self {
            FlowRule::Forbid { id, .. }
            | FlowRule::OnlyWith { id, .. }
            | FlowRule::MaxQty { id, .. }
            | FlowRule::OnlyKnown { id, .. } => id,
        }
    }

    fn dir(&self) -> Option<FlowDirection> {
        match self {
            FlowRule::Forbid { dir, .. }
            | FlowRule::OnlyWith { dir, .. }
            | FlowRule::MaxQty { dir, .. }
            | FlowRule::OnlyKnown { dir, .. } => *dir,
        }
    }

    /// Does this rule have anything to say about this flow?
    fn applies_to(&self, flow: &Flow) -> bool {
        if let Some(d) = self.dir() {
            if d != flow.dir {
                return false;
            }
        }
        match self {
            FlowRule::Forbid { kind, .. } | FlowRule::MaxQty { kind, .. } => {
                kind.as_ref().is_none_or(|k| *k == flow.kind)
            }
            _ => true,
        }
    }

    /// `F(φ, s)` for this one rule.
    pub fn holds(&self, flow: &Flow, state: &Value) -> bool {
        if !self.applies_to(flow) {
            return true;
        }
        match self {
            FlowRule::Forbid { .. } => false,
            FlowRule::OnlyWith { counterparties, .. } => {
                counterparties.contains(&flow.counterparty)
            }
            FlowRule::MaxQty { limit, .. } => flow.qty <= *limit,
            FlowRule::OnlyKnown { register, .. } => known(state, register, &flow.counterparty),
        }
    }

    pub fn why(&self, flow: &Flow) -> String {
        match self {
            FlowRule::Forbid { .. } => {
                format!("'{}' forbids this crossing entirely", self.id())
            }
            FlowRule::OnlyWith { counterparties, .. } => format!(
                "'{}' allows only {:?}, and this crossing is with '{}'",
                self.id(),
                counterparties,
                flow.counterparty
            ),
            FlowRule::MaxQty { limit, .. } => format!(
                "'{}' caps a single crossing at {}, and this one is {}",
                self.id(),
                limit,
                flow.qty
            ),
            FlowRule::OnlyKnown { register, .. } => format!(
                "'{}' allows only counterparties already in '{}', and '{}' is not one",
                self.id(),
                register,
                flow.counterparty
            ),
        }
    }
}

fn known(state: &Value, register: &str, who: &str) -> bool {
    let mut cur = state;
    for seg in register.split('.') {
        match cur.get(seg) {
            Some(v) => cur = v,
            None => return false,
        }
    }
    match cur {
        Value::Array(items) => items.iter().any(|v| v.as_str() == Some(who)),
        Value::Object(map) => map.contains_key(who),
        _ => false,
    }
}

/// One refused crossing, and which rule refused it.
#[derive(Debug, Clone, PartialEq, Error)]
#[error("{rule} refused a crossing — {detail}")]
pub struct FlowRefusal {
    pub rule: String,
    pub flow: Flow,
    pub detail: String,
}

/// `⋀_{φ ∈ flows} F(φ, s)` — conjunction, and only conjunction.
///
/// Returns the first refusal. Adding a rule can only ever shrink the
/// admissible region: there is no override, no priority, and no way for one
/// rule to re-admit what another refused.
pub fn check_flows(
    flows: &[Flow],
    rules: &[FlowRule],
    state: &Value,
) -> Result<(), FlowRefusal> {
    for flow in flows {
        for rule in rules {
            if !rule.holds(flow, state) {
                return Err(FlowRefusal {
                    rule: rule.id().to_string(),
                    flow: flow.clone(),
                    detail: rule.why(flow),
                });
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundary::{BoundaryDecl, Scope};
    use serde_json::json;

    fn boundary() -> Boundary {
        BoundaryDecl::new(
            Scope::new()
                .spanning("finances")
                .spanning("roster")
                .with_entity_register("roster"),
        )
        .owning("finances")
        .read(&json!({
            "roster": ["ama"],
            "finances": {"pockets": {"food": {}, "savings": {}}},
            "contacts": ["landlord"]
        }))
    }

    fn state() -> Value {
        json!({"contacts": ["landlord", "employer"]})
    }

    // -- ★ crossing is μ's question ------------------------------------------

    #[test]
    fn pocket_to_pocket_is_internal_and_produces_no_flow() {
        let m = Movement::new("money", 500.0, "finances.pockets.food", "finances.pockets.savings");
        assert_eq!(classify_movement(&m, &boundary()), Crossing::Internal);
        assert!(flows_of(&[m], &boundary()).is_empty());
    }

    #[test]
    fn an_outside_counterparty_makes_it_a_crossing_and_mu_picks_the_direction() {
        let inbound = Movement::new("money", 500.0, "employer", "finances.liquid");
        match classify_movement(&inbound, &boundary()) {
            Crossing::Crossed(f) => {
                assert_eq!(f.dir, FlowDirection::In);
                assert_eq!(f.counterparty, "employer");
            }
            other => panic!("expected a crossing, got {other:?}"),
        }

        let outbound = Movement::new("money", 500.0, "finances.pockets.food", "naivas");
        match classify_movement(&outbound, &boundary()) {
            Crossing::Crossed(f) => {
                assert_eq!(f.dir, FlowDirection::Out);
                assert_eq!(f.counterparty, "naivas");
            }
            other => panic!("expected a crossing, got {other:?}"),
        }
    }

    #[test]
    fn a_movement_between_two_outside_parties_is_reported_not_ignored() {
        let m = Movement::new("money", 5.0, "alice", "bob");
        assert!(matches!(classify_movement(&m, &boundary()), Crossing::Foreign { .. }));
        assert!(flows_of(&[m], &boundary()).is_empty());
    }

    #[test]
    fn an_owned_entity_counts_as_inside_as_well_as_an_owned_path() {
        // Own(Σ) ranges over Entities ∪ Paths, so both readings are tried.
        let m = Movement::new("money", 10.0, "ama", "finances.pockets.food");
        assert_eq!(classify_movement(&m, &boundary()), Crossing::Internal);
    }

    // -- ★★ irreducibility ---------------------------------------------------

    /// ★★ THE PROOF. Identical endpoints, different crossings.
    #[test]
    fn f_distinguishes_two_transitions_c_and_d_cannot() {
        let b = boundary();
        // (a) 500 moves between two of your own pockets.
        let internal = vec![Movement::new(
            "money",
            500.0,
            "finances.pockets.food",
            "finances.pockets.savings",
        )];
        // (b) 500 goes out to a casino and 500 comes back from it. Same
        //     before, same after, same total — and both conserve.
        let round_trip = vec![
            Movement::new("money", 500.0, "finances.pockets.food", "casino"),
            Movement::new("money", 500.0, "casino", "finances.pockets.savings"),
        ];

        let rules = vec![FlowRule::Forbid {
            id: "no_gambling".into(),
            dir: None,
            kind: None,
        }];

        // (a) produces no flows at all, so F is vacuously satisfied.
        let fa = flows_of(&internal, &b);
        assert!(fa.is_empty());
        assert!(check_flows(&fa, &rules, &state()).is_ok());

        // (b) produces two, and F refuses.
        let fb = flows_of(&round_trip, &b);
        assert_eq!(fb.len(), 2);
        assert!(check_flows(&fb, &rules, &state()).is_err());
    }

    // -- the rules -----------------------------------------------------------

    #[test]
    fn only_with_is_an_allowlist_and_max_qty_caps_one_crossing() {
        let b = boundary();
        let out = flows_of(&[Movement::new("money", 900.0, "finances.pockets.food", "naivas")], &b);

        let allow = vec![FlowRule::OnlyWith {
            id: "known_shops".into(),
            dir: Some(FlowDirection::Out),
            counterparties: ["landlord".to_string()].into_iter().collect(),
        }];
        let err = check_flows(&out, &allow, &state()).unwrap_err();
        assert_eq!(err.rule, "known_shops");
        assert!(err.detail.contains("naivas"));

        let cap = vec![FlowRule::MaxQty {
            id: "small_only".into(),
            dir: None,
            kind: Some("money".into()),
            limit: 100.0,
        }];
        assert!(check_flows(&out, &cap, &state()).is_err());
        // A smaller crossing passes the same cap.
        let small = flows_of(&[Movement::new("money", 50.0, "finances.pockets.food", "naivas")], &b);
        assert!(check_flows(&small, &cap, &state()).is_ok());
    }

    /// ★ The rule that uses `F`'s state argument.
    #[test]
    fn only_known_reads_the_register_from_state() {
        let b = boundary();
        let known_party = flows_of(
            &[Movement::new("money", 10.0, "finances.pockets.food", "landlord")],
            &b,
        );
        let stranger = flows_of(
            &[Movement::new("money", 10.0, "finances.pockets.food", "stranger")],
            &b,
        );
        let rules = vec![FlowRule::OnlyKnown {
            id: "contacts_only".into(),
            dir: Some(FlowDirection::Out),
            register: "contacts".into(),
        }];
        assert!(check_flows(&known_party, &rules, &state()).is_ok());
        assert!(check_flows(&stranger, &rules, &state()).is_err());
    }

    #[test]
    fn a_rule_that_does_not_apply_is_silent() {
        let b = boundary();
        let inbound = flows_of(&[Movement::new("money", 5000.0, "employer", "finances.liquid")], &b);
        // An out-only cap says nothing about an in-flow.
        let rules = vec![FlowRule::MaxQty {
            id: "out_cap".into(),
            dir: Some(FlowDirection::Out),
            kind: None,
            limit: 10.0,
        }];
        assert!(check_flows(&inbound, &rules, &state()).is_ok());
    }

    /// ★ Composition is conjunction only — adding a rule can never re-admit.
    #[test]
    fn adding_a_rule_can_only_shrink_the_admissible_region() {
        let b = boundary();
        let out = flows_of(&[Movement::new("money", 50.0, "finances.pockets.food", "naivas")], &b);
        let permissive: Vec<FlowRule> = vec![];
        assert!(check_flows(&out, &permissive, &state()).is_ok());

        let stricter = vec![FlowRule::Forbid {
            id: "no_outflow".into(),
            dir: Some(FlowDirection::Out),
            kind: None,
        }];
        assert!(check_flows(&out, &stricter, &state()).is_err());

        // And a second, more permissive rule cannot undo the first.
        let both = vec![
            stricter[0].clone(),
            FlowRule::OnlyWith {
                id: "allow_naivas".into(),
                dir: Some(FlowDirection::Out),
                counterparties: ["naivas".to_string()].into_iter().collect(),
            },
        ];
        assert!(check_flows(&out, &both, &state()).is_err(), "no override exists");
    }

    #[test]
    fn no_flows_means_f_is_vacuously_satisfied() {
        let rules = vec![FlowRule::Forbid {
            id: "everything".into(),
            dir: None,
            kind: None,
        }];
        assert!(check_flows(&[], &rules, &state()).is_ok());
    }
}
