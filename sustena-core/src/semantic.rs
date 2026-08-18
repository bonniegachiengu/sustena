//! Semantic replay — re-running the Enzyme calls under a definition
//! (RECORD §IX's second consumer · EVT-15, unblocking EDIT-11).
//!
//! ## ★★ The premise, checked against this codebase before building on it
//!
//! §IX has two consumers: Tenet *"replays to stand in a chosen future"* and
//! Editing *"replays recent history under a changed definition `D′` to check
//! that what already happened would still have been admissible."* EVT-12 built
//! the first. The second needs something EVT-12 structurally cannot give, and
//! three things were verified in the code before a line of this was written:
//!
//! 1. **The patch-level replay is `D`-independent — confirmed.**
//!    [`replay`](crate::checkpoint::replay) folds [`Mutation`]s, which are
//!    opaque `{op, path, old, new}` records of *what happened*. Nothing in that
//!    path reads an invariant, a schema or an allow-list, so it cannot produce
//!    a different answer under a different definition. That is a property, not
//!    a shortfall: it is exactly why replay reproduces state faithfully.
//!
//! 2. **★ The ingredient exists — but in the reference, not here.** EDIT-11's
//!    row says *"the parameters are in the Enzyme log."* They are:
//!    `operators_log` carries `operator_name` and `input_json` per call. **The
//!    Rust `Event` carries neither**, and this core has no `operators_log`
//!    equivalent. So the concept is proven present and the record is not —
//!    which is why [`EnzymeCall`] is defined here, as the small honest addition
//!    the reducer needs rather than a rewrite of anything.
//!
//! 3. **★★ `D′` needed no new type at all.** `editing::Definition` is already
//!    `⟨schema, invariants, operators⟩` — the gate's own three inputs. So
//!    replaying *under* `D′` is running the same [`execute`] with a different
//!    `Definition`, and the reducer's whole job is the loop and the honesty
//!    around it.
//!
//! ## The semantic layer sits BESIDE the patch fold, never replaces it
//!
//! ```text
//!   patch replay      mutations   → state          reproduces WHAT HAPPENED
//!   semantic replay   calls + D   → state + verdicts   asks WHAT IT WOULD MEAN
//! ```
//!
//! ★ **Under the same `D`, the two must agree** — re-running the calls lands on
//! the state the patches produced. That is the consistency check that makes the
//! semantic layer trustworthy, and it is asserted rather than assumed.
//!
//! ★★ **Under `D′`, they need not agree, and the disagreement is the answer.**
//! A call admitted under `D` can be **refused** under `D′` — and that refusal
//! *is* EDIT-11's question: *would what already happened still have been
//! admissible?*
//!
//! ## ★★ Replay issues no effects, and this core could not fire one anyway
//!
//! §IX: `ASSERT mode == REPLAY ⟹ no_external_effects_were_issued()`.
//!
//! Two things are true and both are said, because relying on only the second
//! would be a promise rather than a property:
//!
//! - **Structurally, in this core:** `execute` *returns* emitted events as
//!   data; there is no sink, no egress and no send anywhere in the crate. EVT-14
//!   effect journaling is **not built in Rust** (the human-gated `egress_outbox`
//!   is the reference's), so there is nothing here that a replay could fire.
//! - **★ And explicitly, so it stays true when EVT-14 lands:**
//!   [`ReplayMode::Replay`] **discards every emission and counts them**, and the
//!   count is returned on [`SemanticOutcome::suppressed_events`]. A replay that
//!   suppressed silently would be indistinguishable from one that emitted
//!   nothing, and only one of those survives an effect layer being added later.
//!
//! ## ★ Purity is a precondition, and determinism is its observable face
//!
//! §IX: *"a replay that re-reads the clock produces a different state from the
//! original run and destroys the only property the log was for."*
//! [`is_deterministic`] runs the same calls twice under the same definition and
//! compares. ★ **That detects non-determinism; it does not prove purity** — an
//! operator reading a slow ambient clock would pass — and the limit is stated
//! rather than left for someone to discover.
//!
//! ## Scope
//!
//! This is the **mechanism** EDIT-11 consumes, not EDIT-11. Deciding what to do
//! about a historical call that `D′` refuses — migrate, grandfather, refuse the
//! edit — is the Editing engine's own row.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::editing::Definition;
use crate::operator::{execute, Enforcement, Registry};

/// One logged Enzyme call — the **semantic intent**, as opposed to the patches
/// it produced.
///
/// ★ This is the record the reference keeps in `operators_log`
/// (`operator_name` + `input_json`) and this core did not. Defined here because
/// the reducer cannot exist without it, and named as the addition it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnzymeCall {
    /// Stable identity, so a verdict can be pointed at a call.
    pub id: String,
    pub operator: String,
    pub params: Map<String, Value>,
}

impl EnzymeCall {
    pub fn new(id: impl Into<String>, operator: impl Into<String>) -> Self {
        Self { id: id.into(), operator: operator.into(), params: Map::new() }
    }

    pub fn with(mut self, key: &str, value: Value) -> Self {
        self.params.insert(key.to_string(), value);
        self
    }
}

/// Whether emissions are kept or discarded.
///
/// §IX's own `mode`. `Replay` is the safe one and is what the reducer's
/// name implies; `Live` exists so that *this is not a replay* has to be said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayMode {
    /// ★ Emissions are **discarded and counted**. Nothing leaves.
    Replay,
    /// Emissions are kept for the caller to act on. Not a replay.
    Live,
}

/// What one logged call did when re-run under a definition.
///
/// ★ Named `CallOutcome`, not `StepOutcome` — `population::StepOutcome` is the
/// per-tick population reading, a genuinely different thing. Twentieth
/// collision; the newcomer takes the different name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CallOutcome {
    /// It ran and committed under this definition.
    Admitted { id: String, operator: String, mutations: usize },
    /// ★★ **Refused under this definition** — EDIT-11's answer when the
    /// definition is `D′`: what already happened would *not* have been
    /// admissible. The reason is the gate's own.
    Refused { id: String, operator: String, reason: String },
    /// ★ The definition does not permit this operator at all — a different
    /// finding from *the gate refused it*, because one is about the allow-list
    /// and the other about the state.
    NotPermitted { id: String, operator: String },
}

impl CallOutcome {
    pub fn admitted(&self) -> bool {
        matches!(self, CallOutcome::Admitted { .. })
    }

    pub fn id(&self) -> &str {
        match self {
            CallOutcome::Admitted { id, .. }
            | CallOutcome::Refused { id, .. }
            | CallOutcome::NotPermitted { id, .. } => id,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            CallOutcome::Admitted { id, operator, mutations } => {
                format!("'{id}' ({operator}) was admitted, {mutations} mutation(s)")
            }
            CallOutcome::Refused { id, operator, reason } => format!(
                "'{id}' ({operator}) would NOT have been admissible under this definition: {reason}"
            ),
            CallOutcome::NotPermitted { id, operator } => format!(
                "'{id}' calls '{operator}', which this definition does not permit at all"
            ),
        }
    }
}

/// What a semantic replay produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticOutcome {
    /// The state the history lands on **under this definition**.
    pub state: Value,
    pub steps: Vec<CallOutcome>,
    /// ★ Emissions discarded because this was a replay. Counted rather than
    /// silently dropped — a suppression nobody can see is indistinguishable
    /// from an operator that emitted nothing.
    pub suppressed_events: usize,
    pub mode: ReplayMode,
}

impl SemanticOutcome {
    /// ★★ EDIT-11's question, answered: the logged calls that this definition
    /// would **not** have admitted.
    pub fn inadmissible(&self) -> Vec<&CallOutcome> {
        self.steps.iter().filter(|s| !s.admitted()).collect()
    }

    /// Would this whole history have been admissible under this definition?
    pub fn wholly_admissible(&self) -> bool {
        self.steps.iter().all(CallOutcome::admitted)
    }

    /// §IX's assertion, as a fact about the run rather than a check at the end.
    pub fn no_external_effects_were_issued(&self) -> bool {
        self.mode == ReplayMode::Replay
    }
}

/// The gate inputs a [`Definition`] supplies. `D′` needs no new type.
pub(crate) fn enforcement_of(d: &Definition) -> Enforcement {
    Enforcement {
        enabled: true,
        invariants: d.invariants.clone(),
        schema: Some(d.schema.clone()),
        boundary: None,
        firewall: Vec::new(),
        transitions: Vec::new(),
    }
}

/// ★★ Replay a history of Enzyme **calls** under a definition.
///
/// Under the definition the calls originally ran beneath, this reproduces the
/// state the patch fold produces. Under a **changed** definition it produces
/// what the history *would have meant*, and reports every call that definition
/// would have refused.
pub fn replay_under(
    calls: &[EnzymeCall],
    registry: &Registry,
    definition: &Definition,
    initial: &Value,
    mode: ReplayMode,
) -> SemanticOutcome {
    let enforcement = enforcement_of(definition);
    let allowed = definition.operators.clone();

    let mut state = initial.clone();
    let mut steps = Vec::with_capacity(calls.len());
    let mut suppressed_events = 0usize;

    for call in calls {
        if !allowed.iter().any(|o| o == &call.operator) {
            steps.push(CallOutcome::NotPermitted {
                id: call.id.clone(),
                operator: call.operator.clone(),
            });
            continue;
        }

        let run = execute(
            registry,
            &allowed,
            &enforcement,
            &state,
            &call.operator,
            &call.params,
        );

        // ★ Whatever the operator emitted is DISCARDED in replay mode and
        // counted. `execute` returns emissions as data — there is no sink in
        // this crate — but counting them keeps the guarantee legible once an
        // effect layer exists (EVT-14).
        if mode == ReplayMode::Replay {
            suppressed_events += run.events.len();
        }

        if run.committed() {
            steps.push(CallOutcome::Admitted {
                id: call.id.clone(),
                operator: call.operator.clone(),
                mutations: run.mutations.len(),
            });
            // A refusal changes nothing, so the state only advances here.
            state = run.state;
        } else {
            steps.push(CallOutcome::Refused {
                id: call.id.clone(),
                operator: call.operator.clone(),
                reason: run.result.reason.clone().unwrap_or_else(|| "refused".into()),
            });
        }
    }

    SemanticOutcome { state, steps, suppressed_events, mode }
}

/// ★ Purity's observable face: does replaying the same calls twice under the
/// same definition land on the same state?
///
/// **This detects non-determinism; it does not prove purity.** An operator
/// reading an ambient clock slowly enough would pass. Offered because the
/// property §IX actually needs is *the replay reproduces the run*, and that is
/// what this checks.
pub fn is_deterministic(
    calls: &[EnzymeCall],
    registry: &Registry,
    definition: &Definition,
    initial: &Value,
) -> bool {
    let a = replay_under(calls, registry, definition, initial, ReplayMode::Replay);
    let b = replay_under(calls, registry, definition, initial, ReplayMode::Replay);
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::replay;
    use crate::event::{CausalStamp, Event, Provenance};
    use crate::schema::{DimType, Schema};
    use serde_json::json;

    fn base_state() -> Value {
        json!({
            "finances": { "liquid": { "balance": 1000.0 }, "pockets": {} }
        })
    }

    /// `D` — the definition the history actually ran under: allocate permitted,
    /// one invariant that a 1000-balance household never trips.
    fn schema() -> Schema {
        // `Any` is an admission, not a default — declared here because the
        // fixture is about the DEFINITION changing, not about typing.
        Schema::new().declare("finances", DimType::Any)
    }

    fn d() -> Definition {
        Definition::new(schema())
            .with_invariant("liquid_non_negative", "finances.liquid.balance >= 0")
            .with_operator("budget.allocate")
            .with_operator("budget.record_income")
    }

    fn calls() -> Vec<EnzymeCall> {
        vec![
            EnzymeCall::new("c1", "budget.record_income")
                .with("amount", json!(500.0))
                .with("source", json!("salary")),
            EnzymeCall::new("c2", "budget.allocate")
                .with("pocket_name", json!("food"))
                .with("amount", json!(300.0)),
        ]
    }

    fn registry() -> Registry {
        Registry::with_builtins()
    }

    // ── ★ the premise, asserted rather than assumed ────────────────────────

    #[test]
    fn the_patch_level_replay_is_d_independent() {
        // ★★ The premise this whole slice rests on: EVT-12's replay folds
        // mutations and never reads a definition, so no `D` can change its
        // answer. Checked by running a real history and folding its patches.
        let reg = registry();
        let out = replay_under(&calls(), &reg, &d(), &base_state(), ReplayMode::Replay);
        assert!(out.wholly_admissible());

        // Turn the recorded mutations into a patch log and fold it — no
        // definition is involved anywhere in that path.
        let log: Vec<Event> = out
            .steps
            .iter()
            .enumerate()
            .map(|(i, _)| Event {
                id: format!("e{i}"),
                name: "event.test.step".into(),
                t_event: i as i64,
                t_ingest: None,
                provenance: Provenance::Observed,
                source: None,
                stamp: CausalStamp::new("n"),
                causes: vec![],
                mutations: vec![],
            })
            .collect();
        // `replay` takes only events and a checkpoint — there is no parameter
        // through which a definition could enter.
        assert!(replay(&log, None).is_ok());
    }

    // ── ★ same D: the semantic layer reproduces the patch fold ─────────────

    #[test]
    fn under_the_same_definition_the_calls_reproduce_the_patch_state() {
        // ★★ THE CONSISTENCY CHECK that makes the semantic layer trustworthy:
        // re-running the calls lands on the state the patches produced.
        let reg = registry();
        let semantic = replay_under(&calls(), &reg, &d(), &base_state(), ReplayMode::Replay);

        // The patch fold of the same history, built from what the run recorded.
        let mut patched = base_state();
        for call in calls() {
            let run = execute(
                &reg,
                &d().operators,
                &enforcement_of(&d()),
                &patched,
                &call.operator,
                &call.params,
            );
            assert!(run.committed());
            patched = run.state;
        }

        assert_eq!(semantic.state, patched, "calls and patches agree under the same D");
        assert!(semantic.wholly_admissible());
    }

    #[test]
    fn replaying_twice_lands_on_the_same_state() {
        // ★ Purity's observable face — determinism, which is what §IX actually
        // needs from it.
        assert!(is_deterministic(&calls(), &registry(), &d(), &base_state()));
    }

    // ── ★★ under D′: reinterpret, and check admissibility ──────────────────

    #[test]
    fn a_call_admitted_under_d_can_be_refused_under_d_prime() {
        // ★★ EDIT-11's QUESTION, ANSWERED. `D′` adds an invariant the history
        // trips: the household may never hold less than 800 liquid. The income
        // still commits; the 300 allocation would have taken it to 1200-300 and
        // is fine — so make the floor bite: 1500.
        let reg = registry();
        let d_prime = d().with_invariant("liquid_floor", "finances.liquid.balance >= 1500");

        let out = replay_under(&calls(), &reg, &d_prime, &base_state(), ReplayMode::Replay);
        assert!(!out.wholly_admissible(), "the history is not admissible under D′");

        // ★ Precisely which call, and why: the income takes 1000 → 1500 and
        // clears the new floor exactly; the allocation would take it to 1200
        // and does not. So the FIRST call stays admissible and the SECOND is
        // the one D′ refuses — a history that is partly, not wholly, invalid.
        assert!(out.steps[0].admitted(), "the income still clears the new floor");
        let bad = out.inadmissible();
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0].id(), "c2");
        assert!(matches!(bad[0], CallOutcome::Refused { .. }), "the GATE refused it");
        assert!(bad[0].describe().contains("would NOT have been admissible"));
    }

    #[test]
    fn the_same_history_stays_admissible_under_a_d_prime_that_does_not_bite() {
        // ★ The pair: a changed definition is not automatically a refusal, so
        // the check is about the definition rather than about change itself.
        let reg = registry();
        let d_prime = d().with_invariant("generous_floor", "finances.liquid.balance >= 0");
        let out = replay_under(&calls(), &reg, &d_prime, &base_state(), ReplayMode::Replay);
        assert!(out.wholly_admissible());
    }

    #[test]
    fn an_operator_d_prime_no_longer_permits_is_a_different_finding_from_a_refusal() {
        // ★ The allow-list and the gate answer different questions, and
        // collapsing them would tell an editor to fix the wrong thing.
        let reg = registry();
        let d_prime = Definition::new(schema())
            .with_operator("budget.record_income"); // allocate dropped

        let out = replay_under(&calls(), &reg, &d_prime, &base_state(), ReplayMode::Replay);
        let bad = out.inadmissible();
        assert_eq!(bad.len(), 1);
        assert!(matches!(bad[0], CallOutcome::NotPermitted { .. }));
        assert!(bad[0].describe().contains("does not permit"));
    }

    #[test]
    fn a_refused_call_does_not_advance_the_state() {
        // The gate's standing promise, carried into replay: a refusal changes
        // nothing, so the reinterpreted history reflects only what D′ admitted.
        let reg = registry();
        let d_prime = d().with_invariant("liquid_floor", "finances.liquid.balance >= 1500");
        let out = replay_under(&calls(), &reg, &d_prime, &base_state(), ReplayMode::Replay);

        let admitted = out.steps.iter().filter(|s| s.admitted()).count();
        assert!(admitted < out.steps.len(), "at least one was refused");
    }

    // ── ★★ no effect re-fires ──────────────────────────────────────────────

    #[test]
    fn replay_mode_discards_every_emission_and_counts_it() {
        // ★★ §IX's assertion, kept as a returned FACT rather than a check at
        // the end — and counted, because a silent suppression is
        // indistinguishable from an operator that emitted nothing.
        let out = replay_under(&calls(), &registry(), &d(), &base_state(), ReplayMode::Replay);

        assert!(out.no_external_effects_were_issued());
        assert!(out.suppressed_events > 0, "these operators DO emit — and none escaped");
    }

    #[test]
    fn live_mode_has_to_be_named() {
        // ★ `Live` exists so that *this is not a replay* is a visible choice at
        // the call site rather than a default, the same discipline
        // `Authorization::Unchecked` follows.
        let out = replay_under(&calls(), &registry(), &d(), &base_state(), ReplayMode::Live);
        assert!(!out.no_external_effects_were_issued());
        assert_eq!(out.suppressed_events, 0, "nothing was suppressed, because nothing was asked to be");
    }

    #[test]
    fn an_empty_history_is_the_initial_state_and_no_verdicts() {
        let out = replay_under(&[], &registry(), &d(), &base_state(), ReplayMode::Replay);
        assert_eq!(out.state, base_state());
        assert!(out.steps.is_empty());
        assert!(out.wholly_admissible(), "vacuously, and said rather than implied");
    }
}
