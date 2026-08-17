//! What to DO when a definition edit would strand something (EDIT-11; the
//! Editing paper / SLOPE, §I–III, consuming EVT-15's [`crate::semantic`]).
//!
//! ## The mechanism was already here; this is the decision
//!
//! Two halves existed before this module and neither is rebuilt:
//!
//! - [`crate::editing::safe`] — the migration predicate `Safe(e,μ) ⟺ ∀i ∈
//!   inst(D): μ(sᵢ) ∈ V_{D′}`, over every live instance's **current state**,
//!   with μ applied first and a witness set naming instance *and* rule.
//! - [`crate::semantic::replay_under`] — EVT-15's semantic reducer, which
//!   re-runs a **history** of Enzyme calls through the real gate under a chosen
//!   definition.
//!
//! What was missing is the wiring between them and the choice that follows.
//!
//! ## ★★ States and histories are different questions
//!
//! The crux, and the reason this row exists at all:
//!
//! > An instance can sit at a perfectly valid state that it reached through a
//! > step the new definition would have refused.
//!
//! `safe` asks *is this instance viable under `D′` now*. `replay_under` asks
//! *would each thing that already happened still have been allowed*. A balance
//! that dipped to 300 on its way to 1000 passes a `>= 500` floor as a state and
//! fails it as a history — and the two answers send an editor to different
//! places, so [`EditImpact`] keeps them in **separate lists** rather than
//! summing them into a count of problems.
//!
//! ## The three outcomes, offered rather than chosen
//!
//! §III's classification — compatible, migratable, rejected — becomes three
//! named remedies once something *is* stranded:
//!
//! 1. [`Remedy::Migrate`] — transform the instances to fit `D′` (Expand–
//!    Migrate–Contract, already built: [`crate::migrate`]).
//! 2. [`Remedy::Grandfather`] — the old actions stay valid under the old `D`;
//!    only **new** actions must meet `D′`.
//! 3. [`Remedy::Refuse`] — block the edit.
//!
//! ★★ **Two of them are conditionally unavailable, and the conditions are the
//! substance rather than bookkeeping.**
//!
//! **Migration cannot fix a history.** μ transforms the state an instance is
//! *at*; it cannot reach back and make a past call admissible. So a `Migrate`
//! offered against history stranding always carries what it leaves unresolved.
//! That is the states-versus-histories distinction showing up a second time —
//! in the *remedies* rather than the checks — and it is why collapsing the two
//! lists would have produced a surface that quietly lies.
//!
//! **Grandfathering is unsound while the state is stranded.** Scoping `D′` to
//! new actions only helps if the instance is somewhere `D′` permits; if its
//! current state violates the new rules, the very next call refuses anyway and
//! nothing has been solved. So `Grandfather` is offered only when the state
//! check is clear.
//!
//! ## No default, deliberately
//!
//! There is no method here that returns *the* remedy. [`EditImpact::remedies`]
//! returns every one that applies, each carrying its own evidence and its own
//! residue, and the caller chooses — the same discipline OPV-29 follows for the
//! Pareto frontier, where no method returns a single option because collapsing
//! needs a declared rule and must report what it hid. A silent default here
//! would be a policy decision hidden in a library.
//!
//! ## Honest scope
//!
//! EVT-15's limit carries unchanged: a `D′` replay checks **schema, invariants
//! and the allow-list**, because those are what a [`Definition`] carries. `μ`
//! (the boundary) and `F` (the firewall) are **not** part of a definition, so
//! the history check is complete for the definition's own contract and says
//! nothing about a past crossing. Named here rather than left to be discovered
//! from an empty result.

use serde_json::Value;

use crate::editing::{safe, Definition, Instance, Migration, Stranded};
use crate::operator::Registry;
use crate::semantic::{replay_under, CallOutcome, EnzymeCall, ReplayMode};

/// One instance's history, as the semantic check needs it.
///
/// ★ The genesis state is carried explicitly rather than assumed empty: an
/// instance created from a template starts at its `default_state`, and
/// replaying from `{}` would refuse calls that were fine at the time for a
/// reason that has nothing to do with `D′`.
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceHistory {
    pub id: String,
    /// `s₀` — where this instance's log begins.
    pub genesis: Value,
    /// The logged Enzyme calls, in order.
    pub calls: Vec<EnzymeCall>,
}

impl InstanceHistory {
    pub fn new(id: impl Into<String>, genesis: Value) -> Self {
        InstanceHistory { id: id.into(), genesis, calls: Vec::new() }
    }

    pub fn then(mut self, call: EnzymeCall) -> Self {
        self.calls.push(call);
        self
    }
}

/// A past action `D′` would not have admitted — the witness, with the instance
/// and the call both named.
///
/// Mirrors [`Stranded`]'s discipline deliberately: a refusal that cannot say
/// *which* instance and *which* call is an alarm, not a diagnosis.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryStranded {
    pub instance_id: String,
    /// The gate's own verdict, kept whole. `Refused` and `NotPermitted` stay
    /// distinct here for the same reason EVT-15 split them: one is about the
    /// state, the other about the allow-list.
    pub outcome: CallOutcome,
}

impl HistoryStranded {
    pub fn describe(&self) -> String {
        format!("instance '{}': {}", self.instance_id, self.outcome.describe())
    }
}

/// Everything a candidate definition would strand, in two lists that are never
/// summed together.
#[derive(Debug, Clone, PartialEq)]
pub struct EditImpact {
    pub candidate: Definition,
    /// `∀i: μ(sᵢ) ∈ V_{D′}` — the **state** question ([`safe`]).
    pub state_stranded: Vec<Stranded>,
    /// *Would each past action still have been admissible* — the **history**
    /// question ([`replay_under`]).
    pub history_stranded: Vec<HistoryStranded>,
}

impl EditImpact {
    /// Nothing stranded either way.
    pub fn is_clear(&self) -> bool {
        self.state_stranded.is_empty() && self.history_stranded.is_empty()
    }

    /// ★ The distinction, made queryable: viable now, but it got here through a
    /// step `D′` would have refused. This is the case the pre-existing
    /// state-only check cannot see, and the reason EDIT-11 is not already done.
    pub fn histories_only(&self) -> bool {
        self.state_stranded.is_empty() && !self.history_stranded.is_empty()
    }

    /// Stranded where it stands, whatever its history says.
    pub fn states_only(&self) -> bool {
        !self.state_stranded.is_empty() && self.history_stranded.is_empty()
    }

    /// Instances named by either check, deduplicated — for a caller that wants
    /// the blast radius before it wants the detail.
    pub fn affected_instances(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .state_stranded
            .iter()
            .map(|s| s.instance_id.clone())
            .chain(self.history_stranded.iter().map(|h| h.instance_id.clone()))
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// ★★ Every remedy that genuinely applies, each with its evidence and its
    /// residue. **There is no method that returns one of these.**
    ///
    /// An impact that is clear returns an empty vector: nothing is stranded, so
    /// there is nothing to decide, and offering `Refuse` for a safe edit would
    /// be noise dressed as caution.
    pub fn remedies(&self, migration: &Migration) -> Vec<Remedy> {
        if self.is_clear() {
            return Vec::new();
        }

        let mut out = Vec::new();

        // ── MIGRATE ──────────────────────────────────────────────────────────
        // Offered only when a μ was actually declared. A `Migrate` suggested
        // with no migration to apply would be advice, not a remedy — and the
        // caller would have to invent the transform this surface is supposed to
        // be reporting on.
        if let Migration::Apply(_) = migration {
            out.push(Remedy::Migrate {
                // `safe` already judged the MIGRATED state, so anything still
                // in `state_stranded` is what this μ does not fix.
                unresolved_states: self.state_stranded.clone(),
                // ★★ Always the whole list: μ transforms where an instance IS,
                // and cannot reach back to make a past call admissible.
                unresolved_histories: self.history_stranded.clone(),
            });
        }

        // ── GRANDFATHER ──────────────────────────────────────────────────────
        // ★★ Sound only while the state check is clear. Scoping `D′` to new
        // actions does not help an instance already sitting somewhere `D′`
        // forbids: the next call refuses regardless, so offering it there would
        // be offering a remedy that cannot work.
        if self.state_stranded.is_empty() && !self.history_stranded.is_empty() {
            out.push(Remedy::Grandfather {
                histories_kept: self.history_stranded.clone(),
            });
        }

        // ── REFUSE ───────────────────────────────────────────────────────────
        // Always available whenever anything is stranded, and — importantly —
        // sometimes the ONLY one, which is exactly the case a silent default
        // would have papered over.
        out.push(Remedy::Refuse {
            state_stranded: self.state_stranded.clone(),
            history_stranded: self.history_stranded.clone(),
        });

        out
    }
}

/// What an editor may do about a stranding. Offered, never chosen here.
///
/// ★ Deliberately implements no `Default` and has no ordering that could read
/// as a recommendation: which of these is right is a policy question about a
/// particular household or business, and a library that picked one would be
/// making that decision invisibly.
#[derive(Debug, Clone, PartialEq)]
pub enum Remedy {
    /// Transform the instances to fit `D′` — §III's *migratable* case, using
    /// the Expand–Migrate–Contract machinery already built.
    Migrate {
        /// What this μ still does not fix. Empty when the migration clears the
        /// state check entirely.
        unresolved_states: Vec<Stranded>,
        /// ★★ Always everything the history check found. Migration moves the
        /// present; it cannot make the past admissible.
        unresolved_histories: Vec<HistoryStranded>,
    },
    /// Old actions stay valid under the old `D`; only new actions must meet
    /// `D′`. Offered only when the state check is clear — see [`EditImpact::remedies`].
    Grandfather {
        /// Exactly what is being kept on the old terms. Grandfathering without
        /// naming what it covers is amnesty by shrug.
        histories_kept: Vec<HistoryStranded>,
    },
    /// Block the edit. Always available, and sometimes the only one.
    Refuse {
        state_stranded: Vec<Stranded>,
        history_stranded: Vec<HistoryStranded>,
    },
}

impl Remedy {
    pub fn kind(&self) -> &'static str {
        match self {
            Remedy::Migrate { .. } => "migrate",
            Remedy::Grandfather { .. } => "grandfather",
            Remedy::Refuse { .. } => "refuse",
        }
    }

    /// Would taking this remedy leave something unaddressed?
    ///
    /// `Refuse` never does — blocking the edit resolves the stranding by not
    /// creating it. `Grandfather` never does either, because it is only offered
    /// when the states are clear and it covers every stranded history by
    /// definition. `Migrate` frequently does, and that is the honest finding.
    pub fn leaves_residue(&self) -> bool {
        match self {
            Remedy::Migrate { unresolved_states, unresolved_histories } => {
                !unresolved_states.is_empty() || !unresolved_histories.is_empty()
            }
            Remedy::Grandfather { .. } | Remedy::Refuse { .. } => false,
        }
    }
}

/// Assess a candidate definition against both the live states and the logged
/// histories.
///
/// Read-only: [`safe`] evaluates predicates and [`replay_under`] runs in
/// [`ReplayMode::Replay`], which discards and counts every emission. Nothing
/// here can commit, and nothing here can fire an external effect.
///
/// ★ The two checks are run **independently and both to completion** — a state
/// stranding does not short-circuit the history scan. An editor deciding
/// between remedies needs the whole picture, and stopping at the first problem
/// is how a caller ends up fixing one thing at a time without ever seeing that
/// `Refuse` was the only real option.
/// ★ Named `assess_edit`, not `assess` — `cynefin::assess` is the domain
/// reading, a genuinely different thing. Twenty-first collision; the newcomer
/// takes the longer name.
pub fn assess_edit(
    candidate: &Definition,
    instances: &[Instance],
    histories: &[InstanceHistory],
    registry: &Registry,
    migration: &Migration,
) -> EditImpact {
    let state_stranded = safe(candidate, instances, migration).err().unwrap_or_default();

    let mut history_stranded = Vec::new();
    for history in histories {
        let outcome = replay_under(
            &history.calls,
            registry,
            candidate,
            &history.genesis,
            ReplayMode::Replay,
        );
        for step in outcome.inadmissible() {
            history_stranded.push(HistoryStranded {
                instance_id: history.id.clone(),
                outcome: step.clone(),
            });
        }
    }

    EditImpact { candidate: candidate.clone(), state_stranded, history_stranded }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::Registry;
    use crate::schema::{DimType, Schema};
    use serde_json::json;

    fn schema() -> Schema {
        Schema::new().declare("finances", DimType::Any)
    }

    fn base() -> Definition {
        Definition::new(schema()).with_operator("budget.record_income")
    }

    /// `D′` adds a floor the current state clears and the journey did not.
    fn with_floor() -> Definition {
        base().with_invariant("floor", "finances.liquid.balance >= 500")
    }

    fn genesis() -> Value {
        json!({"finances": {"liquid": {"balance": 0.0}, "income": {"monthly_total": 0.0, "sources": []}, "pockets": {}}})
    }

    fn income(id: &str, amount: f64) -> EnzymeCall {
        EnzymeCall::new(id, "budget.record_income")
            .with("amount", json!(amount))
            .with("source", json!("wages"))
            .with("entry_id", json!(id))
    }

    /// 0 → 300 → 1000. The dip is the whole point.
    fn dipping_history() -> InstanceHistory {
        InstanceHistory::new("i1", genesis())
            .then(income("c1", 300.0))
            .then(income("c2", 700.0))
    }

    fn landed_state() -> Value {
        json!({"finances": {"liquid": {"balance": 1000.0}}})
    }

    #[test]
    fn a_history_is_refused_that_the_current_state_passes() {
        // ★★ THE CRUX. Balance 1000 clears a >= 500 floor as a state. The path
        // to it went through 300, which does not. The state-only check the
        // engine already had cannot see this.
        let reg = Registry::default();
        let instances = vec![Instance { id: "i1".into(), state: landed_state() }];
        let impact = assess_edit(
            &with_floor(),
            &instances,
            &[dipping_history()],
            &reg,
            &Migration::Identity,
        );

        assert!(impact.state_stranded.is_empty(), "the state is viable under D′");
        assert_eq!(impact.history_stranded.len(), 1, "exactly the dipping call");
        assert!(impact.histories_only(), "this is the case EDIT-11 exists for");

        match &impact.history_stranded[0].outcome {
            CallOutcome::Refused { id, reason, .. } => {
                assert_eq!(id, "c1", "the FIRST call is the one that dipped");
                assert!(reason.contains("floor"), "the rule is named: {reason}");
            }
            other => panic!("expected a gate refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_history_that_never_dipped_is_clear() {
        // Same floor, same endpoint, different journey: 1000 in one call.
        let reg = Registry::default();
        let history = InstanceHistory::new("i1", genesis()).then(income("c1", 1000.0));
        let instances = vec![Instance { id: "i1".into(), state: landed_state() }];

        let impact = assess_edit(&with_floor(), &instances, &[history], &reg, &Migration::Identity);
        assert!(impact.is_clear(), "nothing to decide");
        assert!(impact.remedies(&Migration::Identity).is_empty(), "and nothing offered");
    }

    #[test]
    fn grandfather_is_offered_when_only_histories_are_stranded() {
        let reg = Registry::default();
        let instances = vec![Instance { id: "i1".into(), state: landed_state() }];
        let impact = assess_edit(
            &with_floor(),
            &instances,
            &[dipping_history()],
            &reg,
            &Migration::Identity,
        );

        let kinds: Vec<_> = impact
            .remedies(&Migration::Identity)
            .iter()
            .map(Remedy::kind)
            .collect();
        assert_eq!(kinds, vec!["grandfather", "refuse"]);
    }

    #[test]
    fn grandfather_is_withheld_while_the_state_itself_is_stranded() {
        // ★★ Scoping D′ to new actions cannot help an instance already sitting
        // somewhere D′ forbids — the next call refuses anyway.
        let reg = Registry::default();
        let stranded_now = json!({"finances": {"liquid": {"balance": 100.0}}});
        let instances = vec![Instance { id: "i1".into(), state: stranded_now }];

        let impact = assess_edit(
            &with_floor(),
            &instances,
            &[dipping_history()],
            &reg,
            &Migration::Identity,
        );
        assert!(!impact.state_stranded.is_empty());

        let kinds: Vec<_> = impact
            .remedies(&Migration::Identity)
            .iter()
            .map(Remedy::kind)
            .collect();
        assert_eq!(kinds, vec!["refuse"], "refuse is the only one that works here");
    }

    #[test]
    fn migration_never_clears_a_stranded_history() {
        // ★★ The distinction, showing up in the remedies rather than the
        // checks: μ moves where an instance IS. It cannot make a past call
        // admissible, so a Migrate always carries the history residue.
        use crate::migrate::Mu;
        let reg = Registry::default();
        let instances = vec![Instance { id: "i1".into(), state: landed_state() }];
        let mu = Migration::Apply(Mu::new());

        let impact = assess_edit(&with_floor(), &instances, &[dipping_history()], &reg, &mu);
        let remedies = impact.remedies(&mu);

        let migrate = remedies
            .iter()
            .find(|r| r.kind() == "migrate")
            .expect("a declared μ offers migration");
        match migrate {
            Remedy::Migrate { unresolved_states, unresolved_histories } => {
                assert!(unresolved_states.is_empty(), "μ leaves the states fine here");
                assert_eq!(unresolved_histories.len(), 1, "and cannot touch the history");
            }
            other => panic!("{other:?}"),
        }
        assert!(migrate.leaves_residue());
    }

    #[test]
    fn migration_is_not_offered_when_none_was_declared() {
        // Suggesting `Migrate` with no μ in hand would be advice, not a remedy.
        let reg = Registry::default();
        let stranded_now = json!({"finances": {"liquid": {"balance": 100.0}}});
        let instances = vec![Instance { id: "i1".into(), state: stranded_now }];
        let impact = assess_edit(&with_floor(), &instances, &[], &reg, &Migration::Identity);

        assert!(impact.states_only());
        let kinds: Vec<_> = impact.remedies(&Migration::Identity).iter().map(Remedy::kind).collect();
        assert_eq!(kinds, vec!["refuse"]);
    }

    #[test]
    fn a_dropped_operator_is_reported_as_not_permitted_not_as_refused() {
        // The allow-list and the gate are different findings, and the split
        // EVT-15 made survives the wiring.
        let reg = Registry::default();
        let dropped = Definition::new(schema()); // no operators at all
        let impact = assess_edit(&dropped, &[], &[dipping_history()], &reg, &Migration::Identity);

        assert_eq!(impact.history_stranded.len(), 2, "both calls lose their operator");
        assert!(impact
            .history_stranded
            .iter()
            .all(|h| matches!(h.outcome, CallOutcome::NotPermitted { .. })));
    }

    #[test]
    fn both_lists_are_filled_and_neither_short_circuits() {
        let reg = Registry::default();
        let stranded_now = json!({"finances": {"liquid": {"balance": 100.0}}});
        let instances = vec![Instance { id: "i1".into(), state: stranded_now }];

        let impact = assess_edit(
            &with_floor(),
            &instances,
            &[dipping_history()],
            &reg,
            &Migration::Identity,
        );
        assert!(!impact.state_stranded.is_empty(), "state scan ran");
        assert!(!impact.history_stranded.is_empty(), "history scan ran too");
        assert_eq!(impact.affected_instances(), vec!["i1".to_string()]);
    }

    #[test]
    fn there_is_no_single_recommended_remedy() {
        // Structural, not a convention: `remedies` is the only accessor, and it
        // returns a Vec. Nothing here collapses the choice.
        let reg = Registry::default();
        let instances = vec![Instance { id: "i1".into(), state: landed_state() }];
        let impact = assess_edit(
            &with_floor(),
            &instances,
            &[dipping_history()],
            &reg,
            &Migration::Identity,
        );
        assert!(impact.remedies(&Migration::Identity).len() > 1);
    }

    #[test]
    fn assessing_leaves_the_instance_state_untouched() {
        let reg = Registry::default();
        let before = landed_state();
        let instances = vec![Instance { id: "i1".into(), state: before.clone() }];
        let _ = assess_edit(&with_floor(), &instances, &[dipping_history()], &reg, &Migration::Identity);
        assert_eq!(instances[0].state, before);
    }
}
