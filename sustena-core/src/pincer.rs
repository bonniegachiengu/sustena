//! Signal functions and the temporal pincer — the *when to re-plan* layer
//! (Tenet §IX, §X · TEN-8, TEN-9).
//!
//! Everything above this answers **what to compute**. This answers **when to
//! compute it again**, and it invents nothing: it consumes [`crate::signal`]
//! (MUL-15) for firing, [`crate::tenet::backward_induct`] (TEN-4) for the
//! re-plan, [`crate::ensemble`] (TEN-3) for the futures, and the dead drops
//! (TEN-7) that get refreshed.
//!
//! ```text
//! σ_k : S^t → {0,1}                                         §IX
//! Monitor(history, signals): ∀σ, σ(history)==1 → TRIGGER Inversion(σ.linked_action)
//!
//! PROCESS ForwardOperation:                                 §X
//!     WHILE NOT IP(current_state):
//!         a = π(current_state); current_state = T(current_state,a).sample()
//!         Monitor(history, SIGNALS)
//! PROCESS BackwardOperation:
//!     EVERY 2 weeks: Reassess · Update scenario probs · Rerun BackwardInduct
//!                    · Refresh DEAD_DROPS · Recalibrate SIGNALS
//! ```
//!
//! ## A signal is a predicate over the TRAJECTORY, not the state (§IX)
//!
//! `σ_k: S^t → {0,1}` reads the history. [`Trajectory`] is a small temporal
//! layer over the single-state predicates `predicate::check` already
//! evaluates — `Ever`, `Always`, `Never`, `At`, `Count`, and the connectives —
//! which covers §IX's four worked examples without inventing an expression
//! language or reaching for `eval`.
//!
//! ## ★ Three-valued, and this is a correction to the article
//!
//! §IX's pseudocode is two-valued: `IF σ(history) == 1`. Read literally, the
//! example signal *"no customers by week 6"* is **true at week 0** — nothing
//! has happened, so nothing contradicts it — and every warning signal fires on
//! an empty history, triggering a re-inversion before the run has begun.
//!
//! So [`SignalVerdict`] is three-valued: `Holds`, `Fails`, or **`Undetermined`**
//! with the step the window closes at. A signal fires only on `Holds`. The
//! evaluation is early-determining in both directions, LTL-style: `Ever`
//! settles TRUE the moment one observation matches, `Never` settles FALSE the
//! moment one does, and either settles the other way when its window closes.
//! **An unbounded `Never` or `Always` can only ever be falsified, never
//! confirmed** — there is always more history to come — and the code says so
//! rather than quietly treating "not yet contradicted" as "true".
//!
//! ## Why this consumes `signal.rs` rather than calling a callback
//!
//! Not decoration. A warning condition is usually **persistent**: once *"no
//! customers by week 6"* holds, it holds forever. A two-valued monitor with a
//! direct callback would therefore trigger a full re-inversion on **every
//! subsequent forward step**, which is a re-plan storm dressed as vigilance.
//!
//! Each declared signal owns a node in a [`Field`], and firing stimulates it.
//! **The refractory window is the re-plan debounce** — the load-bearing half of
//! §III doing exactly the job it was described for, one layer up. A signal
//! whose predicate holds while its node is refractory is reported as
//! [`MonitorReport::debounced`] rather than silently dropped, so a suppressed
//! trigger is visible. Signals may also be [`SignalMonitor::connect`]ed, in
//! which case one firing relays to its neighbours — a declared way to say
//! *"if this warning fires, look at that one too"*.
//!
//! ## Honest limits
//!
//! - **A signal is only as good as its declared predicate.** Undeclared
//!   warning conditions are not caught, and *"nothing fired"* never means
//!   *"nothing is wrong"*. [`MonitorReport::watched`] carries the names, and
//!   there is no accessor returning the triggers without them — the same
//!   discipline as `InvariantSet::over`.
//! - **The backward recompute is horizon-limited and enumerated-space-bound**,
//!   exactly as TEN-4's sweep is. Re-running it more often does not make it
//!   see further; [`Recompute::horizon`] carries the window.
//! - **"Parallel" is interleaved, deterministically.** Core has no threads. The
//!   two processes are driven in one loop — forward steps, with the backward
//!   recompute on its declared cadence or when a signal fires. That is the
//!   *semantics* of §X; genuine concurrency is an execution-environment
//!   concern, and pretending otherwise here would add nothing but a race.
//! - **The draw is supplied, not generated.** `T(s,a).sample()` takes `u` from
//!   a caller-provided [`Draws`], for the same reason `approval.rs` takes
//!   `now`: no clock, no I/O, no RNG in core. It also makes a run reproducible,
//!   which is what lets a conformance vector pin one.

use std::collections::BTreeSet;

use serde_json::{Map, Value};
use thiserror::Error;

use crate::ensemble::{DeadDropBook, Ensemble, EnsembleAnalysis, EnsembleError, ModelTemplate, RewardBasis};
use crate::kernel::Space;
use crate::signal::{Field, SignalError, SignalSpec};
use crate::tenet::{BellmanSpec, InversionPoint, TenetError};

// ── §IX — signals over the trajectory ───────────────────────────────────────

/// Three-valued, because a bounded claim about history is not yet decided
/// while the window is still open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalVerdict {
    Holds,
    Fails,
    /// Not yet decided. `settles_at` is the history length at which it will be.
    Undetermined { settles_at: Option<usize> },
}

impl SignalVerdict {
    pub fn holds(&self) -> bool {
        matches!(self, SignalVerdict::Holds)
    }

    fn not(self) -> Self {
        match self {
            SignalVerdict::Holds => SignalVerdict::Fails,
            SignalVerdict::Fails => SignalVerdict::Holds,
            u => u,
        }
    }
}

/// `σ_k : S^t → {0,1}` — a predicate over the observed trajectory.
///
/// A thin temporal layer over the single-state predicates already in the crate.
/// `within: None` is an unbounded window, which is exactly why an unbounded
/// `Never`/`Always` can never settle TRUE.
#[derive(Debug, Clone, PartialEq)]
pub enum Trajectory {
    /// ∃ a step in the window where the state predicate holds.
    Ever { within: Option<usize>, state: String },
    /// ∀ steps in the window.
    Always { within: Option<usize>, state: String },
    /// ¬∃ — *"no customers by week 6"*.
    Never { within: Option<usize>, state: String },
    /// The state at exactly this step.
    At { step: usize, state: String },
    /// At least `at_least` steps in the window satisfy the predicate.
    Count { within: Option<usize>, state: String, at_least: usize },
    And(Vec<Trajectory>),
    Or(Vec<Trajectory>),
    Not(Box<Trajectory>),
}

impl Trajectory {
    pub fn ever(state: &str) -> Self {
        Trajectory::Ever { within: None, state: state.to_string() }
    }

    pub fn ever_within(steps: usize, state: &str) -> Self {
        Trajectory::Ever { within: Some(steps), state: state.to_string() }
    }

    pub fn never_within(steps: usize, state: &str) -> Self {
        Trajectory::Never { within: Some(steps), state: state.to_string() }
    }

    /// Evaluate against the observed history so far.
    pub fn evaluate(&self, history: &[Value]) -> Result<SignalVerdict, PincerError> {
        let empty = Map::new();
        let check = |expr: &str, s: &Value| -> Result<bool, PincerError> {
            crate::predicate::check(expr, s, &empty)
                .map(|(ok, _)| ok)
                .map_err(|e| PincerError::BadPredicate { expr: expr.to_string(), detail: e.to_string() })
        };

        // A window is COMPLETE when the history has covered it. An unbounded
        // window never completes, which is the whole reason an unbounded
        // "never" cannot be confirmed.
        let window = |within: &Option<usize>| -> (usize, bool) {
            match within {
                Some(n) => (history.len().min(*n), history.len() >= *n),
                None => (history.len(), false),
            }
        };

        Ok(match self {
            Trajectory::Ever { within, state } => {
                let (upto, complete) = window(within);
                let mut any = false;
                for s in &history[..upto] {
                    if check(state, s)? {
                        any = true;
                        break;
                    }
                }
                if any {
                    SignalVerdict::Holds
                } else if complete {
                    SignalVerdict::Fails
                } else {
                    SignalVerdict::Undetermined { settles_at: *within }
                }
            }
            Trajectory::Never { within, state } => {
                let (upto, complete) = window(within);
                let mut any = false;
                for s in &history[..upto] {
                    if check(state, s)? {
                        any = true;
                        break;
                    }
                }
                if any {
                    SignalVerdict::Fails
                } else if complete {
                    SignalVerdict::Holds
                } else {
                    SignalVerdict::Undetermined { settles_at: *within }
                }
            }
            Trajectory::Always { within, state } => {
                let (upto, complete) = window(within);
                let mut all = true;
                for s in &history[..upto] {
                    if !check(state, s)? {
                        all = false;
                        break;
                    }
                }
                if !all {
                    SignalVerdict::Fails
                } else if complete {
                    SignalVerdict::Holds
                } else {
                    SignalVerdict::Undetermined { settles_at: *within }
                }
            }
            Trajectory::At { step, state } => match history.get(*step) {
                Some(s) => {
                    if check(state, s)? {
                        SignalVerdict::Holds
                    } else {
                        SignalVerdict::Fails
                    }
                }
                None => SignalVerdict::Undetermined { settles_at: Some(step + 1) },
            },
            Trajectory::Count { within, state, at_least } => {
                let (upto, complete) = window(within);
                let mut n = 0usize;
                for s in &history[..upto] {
                    if check(state, s)? {
                        n += 1;
                    }
                }
                let remaining = match within {
                    Some(w) => w.saturating_sub(upto),
                    None => usize::MAX,
                };
                if n >= *at_least {
                    SignalVerdict::Holds
                } else if complete || n.saturating_add(remaining) < *at_least {
                    // Settled early in the negative direction: even if every
                    // remaining step matched, the count could not be reached.
                    SignalVerdict::Fails
                } else {
                    SignalVerdict::Undetermined { settles_at: *within }
                }
            }
            Trajectory::And(parts) => {
                let mut settles = None;
                let mut undetermined = false;
                for p in parts {
                    match p.evaluate(history)? {
                        SignalVerdict::Fails => return Ok(SignalVerdict::Fails),
                        SignalVerdict::Undetermined { settles_at } => {
                            undetermined = true;
                            settles = max_opt(settles, settles_at);
                        }
                        SignalVerdict::Holds => {}
                    }
                }
                if undetermined {
                    SignalVerdict::Undetermined { settles_at: settles }
                } else {
                    SignalVerdict::Holds
                }
            }
            Trajectory::Or(parts) => {
                let mut settles = None;
                let mut undetermined = false;
                for p in parts {
                    match p.evaluate(history)? {
                        SignalVerdict::Holds => return Ok(SignalVerdict::Holds),
                        SignalVerdict::Undetermined { settles_at } => {
                            undetermined = true;
                            settles = max_opt(settles, settles_at);
                        }
                        SignalVerdict::Fails => {}
                    }
                }
                if undetermined {
                    SignalVerdict::Undetermined { settles_at: settles }
                } else {
                    SignalVerdict::Fails
                }
            }
            Trajectory::Not(inner) => inner.evaluate(history)?.not(),
        })
    }
}

fn max_opt(a: Option<usize>, b: Option<usize>) -> Option<usize> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

/// A declared signal (§IX).
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub id: String,
    pub predicate: Trajectory,
    /// The scenario this signal is **evidence for**. Firing reclassifies the
    /// ensemble's base to it — the HMM reading, where an observable emission
    /// infers the latent scenario you are actually in.
    pub reclassifies_to: Option<String>,
    /// §IX's `σ.linked_action` — what a trigger says to re-plan.
    pub linked_action: Option<String>,
}

impl Signal {
    pub fn new(id: &str, predicate: Trajectory) -> Self {
        Self { id: id.to_string(), predicate, reclassifies_to: None, linked_action: None }
    }

    pub fn evidence_for(mut self, scenario: &str) -> Self {
        self.reclassifies_to = Some(scenario.to_string());
        self
    }

    pub fn re_planning(mut self, action: &str) -> Self {
        self.linked_action = Some(action.to_string());
        self
    }
}

/// `TRIGGER Inversion(σ.linked_action)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Trigger {
    pub signal: String,
    pub linked_action: Option<String>,
    pub reclassifies_to: Option<String>,
}

/// One pass of `Monitor(history, signals)`.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorReport {
    pub triggered: Vec<Trigger>,
    /// Predicate holds, but the signal's node is inside its refractory window —
    /// already triggered recently. Reported rather than dropped, so a
    /// suppressed trigger is visible instead of looking like silence.
    pub debounced: Vec<String>,
    /// Not yet decided, with the history length that will decide them.
    pub undetermined: Vec<(String, Option<usize>)>,
    watched: Vec<String>,
}

impl MonitorReport {
    /// **The signals this pass was watching — and only these.**
    ///
    /// Carried with the triggers on purpose. *"Nothing fired"* means *"nothing
    /// among the ones I was told to watch"*, and an undeclared warning
    /// condition is not caught by anything here.
    pub fn watched(&self) -> &[String] {
        &self.watched
    }

    pub fn fired(&self) -> bool {
        !self.triggered.is_empty()
    }

    pub fn describe(&self) -> String {
        if self.triggered.is_empty() {
            format!(
                "no signal fired among the {} declared ({}) — this says nothing about conditions nobody declared",
                self.watched.len(),
                self.watched.join(", ")
            )
        } else {
            format!(
                "{} of {} declared signals fired: {}",
                self.triggered.len(),
                self.watched.len(),
                self.triggered.iter().map(|t| t.signal.as_str()).collect::<Vec<_>>().join(", ")
            )
        }
    }
}

/// `Monitor(history, signals)` — with the Signal primitive underneath.
#[derive(Debug, Clone)]
pub struct SignalMonitor {
    signals: Vec<Signal>,
    field: Field,
}

impl SignalMonitor {
    /// One [`Field`] node per declared signal. The field's refractory window
    /// **is** the re-plan debounce.
    pub fn new(signals: Vec<Signal>, spec: SignalSpec) -> Result<Self, PincerError> {
        if signals.is_empty() {
            return Err(PincerError::NoSignals);
        }
        let mut seen = BTreeSet::new();
        let mut field = Field::new(spec);
        for s in &signals {
            if !seen.insert(s.id.clone()) {
                return Err(PincerError::DuplicateSignal(s.id.clone()));
            }
            field = field.with_node(&s.id);
        }
        Ok(Self { signals, field })
    }

    /// Declare that one signal firing should also rouse another — the relay,
    /// used as *"if this warning fires, look at that one too"*.
    pub fn connect(&mut self, a: &str, b: &str) -> Result<(), PincerError> {
        self.field.connect(a, b).map_err(PincerError::Signal)
    }

    pub fn signals(&self) -> &[Signal] {
        &self.signals
    }

    /// Evaluate every declared signal against the history and fire what holds.
    pub fn monitor(&mut self, history: &[Value]) -> Result<MonitorReport, PincerError> {
        let watched: Vec<String> = self.signals.iter().map(|s| s.id.clone()).collect();
        let mut holding = Vec::new();
        let mut undetermined = Vec::new();

        for s in &self.signals {
            match s.predicate.evaluate(history)? {
                SignalVerdict::Holds => holding.push(s.clone()),
                SignalVerdict::Undetermined { settles_at } => {
                    undetermined.push((s.id.clone(), settles_at))
                }
                SignalVerdict::Fails => {}
            }
        }

        // Drive the field. A signal whose node is refractory is debounced —
        // §III's load-bearing half, doing here exactly the job it was described
        // for: stopping a persistent condition from re-triggering forever.
        // Drive each holding signal at exactly the field's own declared
        // threshold: a signal whose predicate holds is sufficient on its own.
        let theta = self.field.spec().theta_fire;
        for s in &holding {
            self.field.stimulate(&s.id, theta).map_err(PincerError::Signal)?;
        }
        let report = self.field.step();

        let mut triggered = Vec::new();
        let mut debounced = Vec::new();
        for s in &holding {
            if report.fired_at(&s.id) {
                triggered.push(Trigger {
                    signal: s.id.clone(),
                    linked_action: s.linked_action.clone(),
                    reclassifies_to: s.reclassifies_to.clone(),
                });
            } else {
                debounced.push(s.id.clone());
            }
        }
        // A relayed excitation is a genuine trigger too: a connected signal
        // roused by its neighbour fires without its own predicate holding.
        for f in &report.fired {
            if !holding.iter().any(|s| s.id == f.node) {
                if let Some(s) = self.signals.iter().find(|s| s.id == f.node) {
                    triggered.push(Trigger {
                        signal: s.id.clone(),
                        linked_action: s.linked_action.clone(),
                        reclassifies_to: s.reclassifies_to.clone(),
                    });
                }
            }
        }
        triggered.sort_by(|a, b| a.signal.cmp(&b.signal));

        Ok(MonitorReport { triggered, debounced, undetermined, watched })
    }

}

// ── §X — the temporal pincer ────────────────────────────────────────────────

/// Supplies `u ∈ [0,1)` for `T(s,a).sample()`.
///
/// Injected rather than generated: no RNG in core, and a run stays reproducible.
pub trait Draws {
    fn next_draw(&mut self) -> f64;
}

/// A declared sequence of draws, for a reproducible run.
#[derive(Debug, Clone, Default)]
pub struct ScriptedDraws {
    values: Vec<f64>,
    i: usize,
}

impl ScriptedDraws {
    pub fn new(values: Vec<f64>) -> Self {
        Self { values, i: 0 }
    }

    /// Every draw is 0.0 — always the first branch. The deterministic reading.
    pub fn always_first() -> Self {
        Self { values: vec![0.0], i: 0 }
    }
}

impl Draws for ScriptedDraws {
    fn next_draw(&mut self) -> f64 {
        if self.values.is_empty() {
            return 0.0;
        }
        let v = self.values[self.i % self.values.len()];
        self.i += 1;
        v
    }
}

/// When the backward process runs.
#[derive(Debug, Clone, PartialEq)]
pub struct PincerSpec {
    /// §X's *"EVERY 2 weeks"* — in forward steps. `None` disables the cadence,
    /// leaving signals as the only trigger.
    pub cadence: Option<usize>,
    /// Recompute immediately when a signal fires, not only on the cadence.
    pub recompute_on_signal: bool,
    /// A bound on the forward loop. Not a promise it terminates.
    pub max_steps: usize,
}

impl PincerSpec {
    pub fn new(max_steps: usize) -> Self {
        Self { cadence: None, recompute_on_signal: true, max_steps }
    }

    pub fn every(mut self, steps: usize) -> Result<Self, PincerError> {
        if steps == 0 {
            return Err(PincerError::ZeroCadence);
        }
        self.cadence = Some(steps);
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecomputeReason {
    Cadence,
    SignalFired(String),
    /// The opening plan, before any forward step.
    Initial,
}

/// One run of the backward process (§X).
#[derive(Debug, Clone, PartialEq)]
pub struct Recompute {
    pub at_step: usize,
    pub reason: RecomputeReason,
    pub base_before: String,
    pub base_after: String,
    /// Did re-inverting actually change what to do here? The observable proof
    /// that a trigger did something, rather than merely being recorded.
    pub policy_changed: bool,
    pub action_before: Option<String>,
    pub action_after: Option<String>,
    /// Dead drops re-checked against the new analysis. A drop whose node is no
    /// longer a decision node under the new policies is **dropped and named**
    /// rather than silently kept pointing at a state nobody disagrees about.
    pub dead_drops_retired: Vec<String>,
    /// The window this plan is about — re-running does not make it see further.
    pub horizon: usize,
}

/// One forward step (§X).
#[derive(Debug, Clone, PartialEq)]
pub struct ForwardStep {
    pub step: usize,
    pub from: String,
    pub action: String,
    pub to: String,
    /// Which scenario's policy chose it — so a change of belief is legible in
    /// the trace rather than only in the recompute record.
    pub policy_from: String,
    pub monitor: MonitorReport,
}

/// The whole run.
#[derive(Debug, Clone)]
pub struct PincerRun {
    pub steps: Vec<ForwardStep>,
    pub recomputes: Vec<Recompute>,
    pub reached_ip: bool,
    pub history: Vec<Value>,
    pub final_state: String,
}

impl PincerRun {
    /// Recomputes that genuinely changed the action at the state they ran from.
    pub fn effective_recomputes(&self) -> Vec<&Recompute> {
        self.recomputes.iter().filter(|r| r.policy_changed).collect()
    }
}

/// Run the pincer: forward execution and periodic backward recompute,
/// converging at `current_state`.
///
/// The two processes are interleaved in one loop — see the module docs on why
/// that is the semantics of §X rather than a simplification of it.
#[allow(clippy::too_many_arguments)]
pub fn run_pincer(
    ensemble: &Ensemble,
    template: &ModelTemplate,
    space: &Space,
    ip: &InversionPoint,
    bellman: &BellmanSpec,
    monitor: &mut SignalMonitor,
    drops: &mut DeadDropBook,
    spec: &PincerSpec,
    start: &str,
    draws: &mut dyn Draws,
) -> Result<PincerRun, PincerError> {
    if !space.contains_key(start) {
        return Err(PincerError::UnknownState(start.to_string()));
    }

    let analyse = |e: &Ensemble| -> Result<EnsembleAnalysis, PincerError> {
        e.analyse(template, space, ip, bellman, RewardBasis::Declared).map_err(PincerError::Ensemble)
    };

    let mut belief = ensemble.clone();
    let mut analysis = analyse(&belief)?;
    let mut current = start.to_string();
    let mut history: Vec<Value> = vec![space[&current].clone()];

    let mut steps = Vec::new();
    let mut recomputes = vec![Recompute {
        at_step: 0,
        reason: RecomputeReason::Initial,
        base_before: belief.base_name().to_string(),
        base_after: belief.base_name().to_string(),
        policy_changed: false,
        action_before: None,
        action_after: analysis.plan(belief.base_name()).and_then(|p| p.action(0, &current)).map(str::to_string),
        dead_drops_retired: Vec::new(),
        horizon: analysis.horizon(),
    }];

    let mut reached_ip = ip_holds(ip, &current, space)?;

    for step in 0..spec.max_steps {
        if reached_ip {
            break;
        }

        // ── forward: a = π(current), then sample T(current, a) ──────────────
        let base = belief.base_name().to_string();
        let plan = analysis.plan(&base).expect("the base scenario was analysed");
        let Some(action) = plan.action(0, &current).map(str::to_string) else {
            // Terminal by absence — no admissible action here.
            break;
        };
        let model = analysis.model(&base).expect("analysed");
        let dist = model
            .get(&current, &action)
            .ok_or_else(|| PincerError::NoTransition { state: current.clone(), action: action.clone() })?;
        let next = dist.sample(draws.next_draw()).to_string();

        let from = current.clone();
        current = next;
        history.push(
            space
                .get(&current)
                .ok_or_else(|| PincerError::UnknownState(current.clone()))?
                .clone(),
        );

        // ── Monitor(history, SIGNALS) ───────────────────────────────────────
        let report = monitor.monitor(&history)?;
        let fired: Vec<Trigger> = report.triggered.clone();

        steps.push(ForwardStep {
            step: step + 1,
            from,
            action,
            to: current.clone(),
            policy_from: base.clone(),
            monitor: report,
        });

        reached_ip = ip_holds(ip, &current, space)?;

        // ── backward: on a signal, or on the declared cadence ───────────────
        let on_cadence = spec.cadence.is_some_and(|c| (step + 1) % c == 0);
        let trigger = fired.first().cloned();
        let due = (spec.recompute_on_signal && trigger.is_some()) || on_cadence;

        if due && !reached_ip {
            let reason = match (&trigger, spec.recompute_on_signal) {
                (Some(t), true) => RecomputeReason::SignalFired(t.signal.clone()),
                _ => RecomputeReason::Cadence,
            };
            let base_before = belief.base_name().to_string();
            let action_before =
                analysis.plan(&base_before).and_then(|p| p.action(0, &current)).map(str::to_string);

            // Update(scenario_probabilities): a fired signal is evidence for
            // the scenario it is linked to — the HMM reading.
            if let Some(to) = fired.iter().find_map(|t| t.reclassifies_to.clone()) {
                belief = belief.reclassified_to(&to).map_err(PincerError::Ensemble)?;
            }

            // Rerun BackwardInduct with the updated belief.
            analysis = analyse(&belief)?;
            let base_after = belief.base_name().to_string();
            let action_after =
                analysis.plan(&base_after).and_then(|p| p.action(0, &current)).map(str::to_string);

            // Refresh DEAD_DROPS against the new analysis.
            let retired = refresh_dead_drops(drops, &analysis);

            recomputes.push(Recompute {
                at_step: step + 1,
                reason,
                base_before,
                base_after,
                policy_changed: action_before != action_after,
                action_before,
                action_after,
                dead_drops_retired: retired,
                horizon: analysis.horizon(),
            });
        }
    }

    Ok(PincerRun { steps, recomputes, reached_ip, history, final_state: current })
}

/// *Refresh DEAD_DROPS* (§X): a drop whose node is no longer a decision node
/// under the new policies is retired and **named**.
///
/// Silently keeping it would leave a pre-commitment aimed at a state every
/// future now agrees about — a decision waiting for a question that is no
/// longer asked.
fn refresh_dead_drops(book: &mut DeadDropBook, analysis: &EnsembleAnalysis) -> Vec<String> {
    let stale: Vec<String> = book
        .drops()
        .iter()
        .filter(|d| !analysis.is_decision_node(d.t, &d.state))
        .map(|d| d.id.clone())
        .collect();
    book.retain(|d| analysis.is_decision_node(d.t, &d.state));
    stale
}

fn ip_holds(ip: &InversionPoint, state: &str, space: &Space) -> Result<bool, PincerError> {
    let empty = Map::new();
    match ip {
        InversionPoint::Predicate(exprs) => {
            let s = space.get(state).ok_or_else(|| PincerError::UnknownState(state.to_string()))?;
            for e in exprs {
                let (ok, _) = crate::predicate::check(e, s, &empty).map_err(|err| {
                    PincerError::BadPredicate { expr: e.clone(), detail: err.to_string() }
                })?;
                if !ok {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        InversionPoint::Kernel(k) => Ok(k.contains(state)),
        InversionPoint::KernelAnd(k, exprs) => {
            if !k.contains(state) {
                return Ok(false);
            }
            ip_holds(&InversionPoint::Predicate(exprs.clone()), state, space)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Error)]
pub enum PincerError {
    #[error("a monitor needs at least one declared signal — 'nothing fired' from an empty set is not reassurance")]
    NoSignals,
    #[error("signal '{0}' is declared twice")]
    DuplicateSignal(String),
    #[error("a cadence of 0 would recompute on every step and never execute anything")]
    ZeroCadence,
    #[error("'{0}' is not in the enumerated space")]
    UnknownState(String),
    #[error("no transition declared for ({state}, {action})")]
    NoTransition { state: String, action: String },
    #[error("signal predicate '{expr}' could not be evaluated: {detail}")]
    BadPredicate { expr: String, detail: String },
    #[error(transparent)]
    Ensemble(EnsembleError),
    #[error(transparent)]
    Signal(SignalError),
    #[error(transparent)]
    Tenet(TenetError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ensemble::{Prob, Scenario};
    use serde_json::json;

    fn hist(vals: &[f64]) -> Vec<Value> {
        vals.iter().map(|v| json!({ "customers": v })).collect()
    }

    // ── §IX — the trajectory predicate ──────────────────────────────────────

    #[test]
    fn a_bounded_never_is_undetermined_until_its_window_closes() {
        // ★ THE CORRECTION TO THE ARTICLE. §IX's pseudocode is two-valued, so
        // "no customers by week 6" read literally is TRUE on an empty history
        // and every warning fires before the run begins.
        let sigma = Trajectory::never_within(6, "customers >= 1");

        assert_eq!(
            sigma.evaluate(&hist(&[])).unwrap(),
            SignalVerdict::Undetermined { settles_at: Some(6) },
            "nothing has happened yet — that is not the same as the warning being true"
        );
        assert_eq!(
            sigma.evaluate(&hist(&[0.0, 0.0])).unwrap(),
            SignalVerdict::Undetermined { settles_at: Some(6) }
        );
        // Settles TRUE only once the window has genuinely closed.
        assert_eq!(sigma.evaluate(&hist(&[0.0; 6])).unwrap(), SignalVerdict::Holds);
    }

    #[test]
    fn a_never_settles_false_the_moment_it_is_contradicted() {
        let sigma = Trajectory::never_within(6, "customers >= 1");
        assert_eq!(sigma.evaluate(&hist(&[0.0, 3.0])).unwrap(), SignalVerdict::Fails);
    }

    #[test]
    fn an_ever_settles_true_early_and_false_only_at_the_window() {
        let sigma = Trajectory::ever_within(3, "customers >= 1");
        assert_eq!(sigma.evaluate(&hist(&[0.0])).unwrap(), SignalVerdict::Undetermined { settles_at: Some(3) });
        assert_eq!(sigma.evaluate(&hist(&[0.0, 2.0])).unwrap(), SignalVerdict::Holds);
        assert_eq!(sigma.evaluate(&hist(&[0.0, 0.0, 0.0])).unwrap(), SignalVerdict::Fails);
    }

    #[test]
    fn an_unbounded_never_can_only_ever_be_falsified() {
        // There is always more history to come, so "never" unbounded cannot be
        // confirmed. Saying so beats quietly treating not-yet-contradicted as
        // true.
        let sigma = Trajectory::Never { within: None, state: "customers >= 1".into() };
        assert_eq!(sigma.evaluate(&hist(&[0.0; 50])).unwrap(), SignalVerdict::Undetermined { settles_at: None });
        assert_eq!(sigma.evaluate(&hist(&[0.0, 1.0])).unwrap(), SignalVerdict::Fails);
    }

    #[test]
    fn a_count_settles_false_as_soon_as_it_is_unreachable() {
        // 3 of the first 4 steps must match; after two misses it cannot.
        let sigma = Trajectory::Count { within: Some(4), state: "customers >= 1".into(), at_least: 3 };
        assert_eq!(sigma.evaluate(&hist(&[1.0])).unwrap(), SignalVerdict::Undetermined { settles_at: Some(4) });
        assert_eq!(sigma.evaluate(&hist(&[0.0, 0.0])).unwrap(), SignalVerdict::Fails, "2 misses of 4 leaves at most 2");
        assert_eq!(sigma.evaluate(&hist(&[1.0, 1.0, 1.0])).unwrap(), SignalVerdict::Holds);
    }

    #[test]
    fn the_connectives_propagate_undetermined() {
        let known = Trajectory::ever_within(1, "customers >= 1");
        let open = Trajectory::never_within(9, "customers >= 99");

        // AND: one Fails settles it; otherwise an open part keeps it open.
        let a = Trajectory::And(vec![known.clone(), open.clone()]);
        assert!(matches!(a.evaluate(&hist(&[5.0])).unwrap(), SignalVerdict::Undetermined { .. }));
        assert_eq!(
            Trajectory::And(vec![Trajectory::ever_within(1, "customers >= 99"), open.clone()])
                .evaluate(&hist(&[0.0]))
                .unwrap(),
            SignalVerdict::Fails
        );
        // OR: one Holds settles it.
        assert_eq!(Trajectory::Or(vec![known, open]).evaluate(&hist(&[5.0])).unwrap(), SignalVerdict::Holds);
    }

    #[test]
    fn not_swaps_the_two_decided_verdicts_and_leaves_the_third() {
        let open = Trajectory::never_within(9, "customers >= 99");
        assert!(matches!(
            Trajectory::Not(Box::new(open)).evaluate(&hist(&[0.0])).unwrap(),
            SignalVerdict::Undetermined { .. }
        ));
    }

    // ── §IX — Monitor, and the debounce ─────────────────────────────────────

    fn monitor_of() -> SignalMonitor {
        SignalMonitor::new(
            vec![
                Signal::new("warning", Trajectory::never_within(2, "customers >= 1"))
                    .evidence_for("worst")
                    .re_planning("pivot"),
            ],
            SignalSpec::new(1.0, 3).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn a_signal_that_holds_triggers_with_its_linked_action() {
        let mut m = monitor_of();
        let r = m.monitor(&hist(&[0.0, 0.0])).unwrap();
        assert_eq!(r.triggered.len(), 1);
        assert_eq!(r.triggered[0].signal, "warning");
        assert_eq!(r.triggered[0].linked_action.as_deref(), Some("pivot"));
        assert_eq!(r.triggered[0].reclassifies_to.as_deref(), Some("worst"));
    }

    #[test]
    fn a_persistent_condition_is_debounced_by_the_refractory_window() {
        // ★ The load-bearing reason this consumes signal.rs. The warning holds
        // forever once true; without the refractory window it would trigger a
        // full re-inversion on every subsequent step.
        let mut m = monitor_of();
        assert!(m.monitor(&hist(&[0.0, 0.0])).unwrap().fired());

        let second = m.monitor(&hist(&[0.0, 0.0, 0.0])).unwrap();
        assert!(!second.fired(), "the same persistent condition must not re-trigger");
        assert_eq!(second.debounced, ["warning"], "and the suppression is VISIBLE, not silent");
    }

    #[test]
    fn nothing_fired_carries_the_list_it_was_watching() {
        let mut m = monitor_of();
        let r = m.monitor(&hist(&[3.0, 3.0])).unwrap();
        assert!(!r.fired());
        assert_eq!(r.watched(), ["warning"]);
        assert!(
            r.describe().contains("says nothing about conditions nobody declared"),
            "a signal is only as good as its declared predicate: {}",
            r.describe()
        );
    }

    #[test]
    fn an_empty_signal_set_is_refused() {
        assert!(matches!(
            SignalMonitor::new(vec![], SignalSpec::new(1.0, 2).unwrap()),
            Err(PincerError::NoSignals)
        ));
    }

    // ── §X — the pincer ─────────────────────────────────────────────────────

    /// A venture whose early weeks reveal which world it is in.
    fn space() -> Space {
        [("start", 0.0), ("quiet", 0.0), ("busy", 9.0), ("scaled", 20.0), ("safe", 5.0)]
            .into_iter()
            .map(|(id, c)| (id.to_string(), json!({ "customers": c })))
            .collect()
    }

    fn template() -> ModelTemplate {
        ModelTemplate::new()
            .edge(
                "start",
                "push",
                vec![("busy", Prob::param("p_demand")), ("quiet", Prob::one_minus("p_demand"))],
            )
            .certain("start", "hedge", "safe")
            .certain("busy", "scale", "scaled")
            .edge(
                "quiet",
                "push",
                vec![("busy", Prob::param("p_demand")), ("quiet", Prob::one_minus("p_demand"))],
            )
            .certain("quiet", "retrench", "safe")
            .certain("safe", "hold", "safe")
            .certain("scaled", "hold", "scaled")
    }

    fn ensemble() -> Ensemble {
        Ensemble::new(
            vec![
                Scenario::new("good").assigning("p_demand", 0.9),
                Scenario::new("worst").assigning("p_demand", 0.05),
            ],
            "good",
        )
        .unwrap()
    }

    fn bellman() -> BellmanSpec {
        BellmanSpec::new(4, 0.9, 500.0).rewarding("quiet", "push", -60.0).rewarding("start", "hedge", -5.0)
    }

    fn at_scaled() -> InversionPoint<'static> {
        InversionPoint::Predicate(vec!["customers >= 20".into()])
    }

    #[test]
    fn a_fired_signal_reclassifies_and_the_policy_changes() {
        // ★ THE PROOF OF VALUE. The run starts believing `good`. The world
        // hands it quiet weeks; the declared signal reads that history, fires,
        // reclassifies the belief to `worst`, and the re-inverted policy takes
        // a different action than it would have.
        let mut m = SignalMonitor::new(
            vec![Signal::new("demand_warning", Trajectory::never_within(3, "customers >= 1"))
                .evidence_for("worst")
                .re_planning("retrench")],
            SignalSpec::new(1.0, 5).unwrap(),
        )
        .unwrap();
        let mut drops = DeadDropBook::new();
        let mut draws = ScriptedDraws::new(vec![0.99]); // always the unlucky branch

        let run = run_pincer(
            &ensemble(),
            &template(),
            &space(),
            &at_scaled(),
            &bellman(),
            &mut m,
            &mut drops,
            &PincerSpec::new(8),
            "start",
            &mut draws,
        )
        .unwrap();

        let effective: Vec<&Recompute> = run.effective_recomputes();
        assert!(!effective.is_empty(), "a trigger that changed nothing is not a re-inversion");
        let r = effective[0];
        assert!(matches!(r.reason, RecomputeReason::SignalFired(_)));
        assert_eq!(r.base_before, "good");
        assert_eq!(r.base_after, "worst", "the emission inferred the latent scenario");
        assert_ne!(r.action_before, r.action_after, "and the plan genuinely changed");
    }

    #[test]
    fn without_a_signal_firing_the_belief_is_untouched() {
        // The negative case: lucky draws, no warning, no reclassification.
        let mut m = SignalMonitor::new(
            vec![Signal::new("demand_warning", Trajectory::never_within(3, "customers >= 1"))
                .evidence_for("worst")],
            SignalSpec::new(1.0, 5).unwrap(),
        )
        .unwrap();
        let mut drops = DeadDropBook::new();
        let mut draws = ScriptedDraws::always_first(); // always the lucky branch

        let run = run_pincer(
            &ensemble(),
            &template(),
            &space(),
            &at_scaled(),
            &bellman(),
            &mut m,
            &mut drops,
            &PincerSpec::new(8),
            "start",
            &mut draws,
        )
        .unwrap();

        assert!(run.reached_ip, "the good path reaches the inversion point");
        assert!(
            run.recomputes.iter().all(|r| r.base_after == "good"),
            "nothing fired, so nothing was reclassified"
        );
    }

    #[test]
    fn the_cadence_recomputes_even_when_no_signal_fires() {
        // §X's "EVERY 2 weeks" — the backward process runs on its own clock,
        // not only in reaction.
        let mut m = SignalMonitor::new(
            vec![Signal::new("never_fires", Trajectory::ever_within(1, "customers >= 999"))],
            SignalSpec::new(1.0, 2).unwrap(),
        )
        .unwrap();
        let mut drops = DeadDropBook::new();
        let mut draws = ScriptedDraws::new(vec![0.99]);

        let run = run_pincer(
            &ensemble(),
            &template(),
            &space(),
            &at_scaled(),
            &bellman(),
            &mut m,
            &mut drops,
            &PincerSpec::new(6).every(2).unwrap(),
            "start",
            &mut draws,
        )
        .unwrap();

        assert!(
            run.recomputes.iter().any(|r| r.reason == RecomputeReason::Cadence),
            "the backward process must run on the declared cadence too"
        );
    }

    #[test]
    fn the_forward_process_samples_the_declared_transition() {
        // The draw is supplied, so the run is reproducible — the same script
        // walks the same path twice.
        let build = || {
            let mut m = SignalMonitor::new(
                vec![Signal::new("s", Trajectory::ever_within(1, "customers >= 999"))],
                SignalSpec::new(1.0, 2).unwrap(),
            )
            .unwrap();
            let mut d = DeadDropBook::new();
            let mut draws = ScriptedDraws::new(vec![0.99, 0.0, 0.0, 0.0]);
            run_pincer(
                &ensemble(),
                &template(),
                &space(),
                &at_scaled(),
                &bellman(),
                &mut m,
                &mut d,
                &PincerSpec::new(6),
                "start",
                &mut draws,
            )
            .unwrap()
        };
        let a = build();
        let b = build();
        let path = |r: &PincerRun| r.steps.iter().map(|s| s.to.clone()).collect::<Vec<_>>();
        assert_eq!(path(&a), path(&b), "no RNG in core — the same draws walk the same path");
        assert_eq!(a.steps[0].to, "quiet", "0.99 takes the unlucky branch of p_demand=0.9");
    }

    #[test]
    fn the_recompute_carries_the_window_it_is_about() {
        let mut m = monitor_of();
        let mut drops = DeadDropBook::new();
        let mut draws = ScriptedDraws::always_first();
        let run = run_pincer(
            &ensemble(),
            &template(),
            &space(),
            &at_scaled(),
            &bellman(),
            &mut m,
            &mut drops,
            &PincerSpec::new(4),
            "start",
            &mut draws,
        )
        .unwrap();

        assert!(
            run.recomputes.iter().all(|r| r.horizon == 4),
            "re-running the sweep does not make it see further"
        );
    }

    #[test]
    fn an_unknown_start_state_is_refused() {
        let mut m = monitor_of();
        let mut drops = DeadDropBook::new();
        let mut draws = ScriptedDraws::always_first();
        assert!(matches!(
            run_pincer(
                &ensemble(),
                &template(),
                &space(),
                &at_scaled(),
                &bellman(),
                &mut m,
                &mut drops,
                &PincerSpec::new(4),
                "nowhere",
                &mut draws,
            ),
            Err(PincerError::UnknownState(_))
        ));
    }

    #[test]
    fn a_zero_cadence_is_refused() {
        assert!(matches!(PincerSpec::new(4).every(0), Err(PincerError::ZeroCadence)));
    }
}
