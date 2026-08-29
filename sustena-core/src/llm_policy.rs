//! **When a model may be asked, and what happens when it may not**
//! (Operative · §XII policy; Capstone §VI.1 duty 7).
//!
//! ```text
//!   allowed(llm.*, τ) ⟺ (∄ o ∈ T covering τ) ∧ (perf(o, τ) < θ_τ)
//!   policy(τ) = ⟨allow, B_τ juul, model, maxtok⟩
//! ```
//!
//! ★★★ **A model is a last resort, not a default.** The guard has two conjuncts
//! and both are refusals: if an Enzyme already covers the task there is nothing
//! to ask about, and if the ordinary path performs above `θ_τ` then asking is
//! spending money to be no better off. A system that reached for a model first
//! would be one where the deterministic path quietly rotted.
//!
//! ★★★ **The guard is an ordinary constraint on the existing seam**, not a new
//! mechanism. It is one more predicate at the same gate that checks every other
//! guard — which is what makes it un-bypassable for the same reason everything
//! else is.
//!
//! ## `maxtok` is not a nicety
//!
//! ★★★ **A budget without a per-call cap is overshot by exactly one call.** `B_τ`
//! says how much may be spent over a period; nothing in it stops a single
//! enormous call from consuming all of it and more, because the cost is only
//! known once the call has already happened. The cap is what makes the budget a
//! bound rather than a hope, and [`Policy`] has no constructor that omits it.
//!
//! ## Deactivation is a policy, not a switch
//!
//! ★★★ **Turning the model off must not be an outage**, so the fallback edge is
//! *declared*. A system whose only answer to "the model is unavailable" is to
//! stop is a system that has made a model a dependency while calling it
//! optional. [`Policy`] therefore requires a [`Fallback`], and there is no
//! variant meaning *nothing*.

/// A class of task a model might be asked about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskClass(String);

impl TaskClass {
    pub fn named(name: &str) -> Self {
        Self(name.to_string())
    }
    pub fn name(&self) -> &str {
        &self.0
    }
}

/// What happens when the model is not asked — for any reason.
///
/// ★★★ Required, and there is no `Nothing`. A declared fallback is what makes
/// deactivation a policy rather than an outage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fallback {
    /// Use the deterministic path that already covers this.
    TheOrdinaryEnzyme { operator: String },
    /// Ask a person.
    ///
    /// ★★ A real fallback, not a failure: surfacing is what this system does
    /// with everything else it cannot decide.
    AskAPerson,
    /// Do nothing, deliberately, and say so.
    ///
    /// ★★ Distinct from an outage because somebody chose it: *this task simply
    /// does not get done when the model is off* is a decision, and one a person
    /// can disagree with.
    DeclineTheTask { because: String },
}

/// The per-task-class policy.
#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    pub task: TaskClass,
    pub allow: bool,
    /// `B_τ` — what may be spent on this class over the period.
    pub budget: u64,
    pub model: String,
    /// ★★★ The enforceable per-call cap. Required.
    pub maxtok: u32,
    /// ★★★ Required. See the module header.
    pub fallback: Fallback,
}

impl Policy {
    /// ★★ Every field is an argument. There is no `Policy::default()`, because
    /// a default budget is a budget nobody set and a default fallback is an
    /// outage nobody noticed.
    pub fn declared(
        task: TaskClass,
        allow: bool,
        budget: u64,
        model: &str,
        maxtok: u32,
        fallback: Fallback,
    ) -> Option<Policy> {
        // ★★★ A cap of zero is not a cap — it is a budget with the bound left
        //     off, which is the exact shape this refuses.
        (maxtok > 0).then(|| Policy {
            task,
            allow,
            budget,
            model: model.into(),
            maxtok,
            fallback,
        })
    }

    /// The most a single call can cost, so the budget cannot be overshot by one.
    pub fn worst_single_call(&self, cost_per_token: u64) -> u64 {
        self.maxtok as u64 * cost_per_token
    }

    /// ★★★ Can this budget actually be overshot?
    ///
    /// Only if a single call could cost more than what is left — which the cap
    /// is there to prevent. Askable, because "we have a budget" is not the same
    /// claim as "the budget binds".
    pub fn budget_binds(&self, cost_per_token: u64) -> bool {
        self.worst_single_call(cost_per_token) <= self.budget
    }
}

/// Why a model was not asked.
#[derive(Debug, Clone, PartialEq)]
pub enum NotAsked {
    /// ★★★ An Enzyme already covers this. There was nothing to ask about.
    AnEnzymeCoversIt { operator: String },
    /// ★★★ The ordinary path is performing well enough. Asking would be
    /// spending money to be no better off.
    TheOrdinaryPathIsGoodEnough { performance: f64, threshold: f64 },
    /// Policy says no for this class.
    NotAllowedForThisClass,
    /// The period's budget is spent.
    BudgetExhausted { spent: u64, budget: u64 },
    /// This one call would exceed the cap.
    OverTheCap { asked: u32, maxtok: u32 },
}

impl NotAsked {
    pub fn describe(&self) -> String {
        match self {
            Self::AnEnzymeCoversIt { operator } => {
                format!("'{operator}' already covers this — there is nothing to ask about")
            }
            Self::TheOrdinaryPathIsGoodEnough { performance, threshold } => format!(
                "the ordinary path scores {performance:.2} against a threshold of {threshold:.2} \
                 — asking would be spending money to be no better off"
            ),
            Self::NotAllowedForThisClass => "policy does not allow a model for this class".into(),
            Self::BudgetExhausted { spent, budget } => {
                format!("{spent} of {budget} already spent this period")
            }
            Self::OverTheCap { asked, maxtok } => format!(
                "{asked} tokens asked against a cap of {maxtok} — a budget without a per-call \
                 cap is overshot by exactly one call"
            ),
        }
    }
}

/// What to do about a task.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Ask the model, within this cap.
    Ask { model: String, maxtok: u32 },
    /// ★★★ Do not — and here is what happens instead. **Never a bare refusal**,
    /// because a refusal with no fallback is an outage.
    Instead { fallback: Fallback, why: NotAsked },
}

impl Decision {
    pub fn asks(&self) -> bool {
        matches!(self, Self::Ask { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Ask { model, maxtok } => format!("ask {model}, at most {maxtok} tokens"),
            Self::Instead { fallback, why } => {
                let what = match fallback {
                    Fallback::TheOrdinaryEnzyme { operator } => format!("use '{operator}'"),
                    Fallback::AskAPerson => "ask a person".to_string(),
                    Fallback::DeclineTheTask { because } => {
                        format!("leave it undone: {because}")
                    }
                };
                format!("{what} — {}", why.describe())
            }
        }
    }
}

/// **`allowed(llm.*, τ)`, and what happens when it is not.**
///
/// ★★★ `covering` and `performance` are supplied because they are facts about
/// the Sustain, not about this policy — and the guard is one more predicate on
/// the ordinary seam rather than a mechanism of its own.
#[allow(clippy::too_many_arguments)]
pub fn decide(
    policy: &Policy,
    covering_operator: Option<&str>,
    ordinary_performance: f64,
    threshold: f64,
    spent_this_period: u64,
    tokens_wanted: u32,
) -> Decision {
    let instead = |why| Decision::Instead { fallback: policy.fallback.clone(), why };

    if let Some(op) = covering_operator {
        return instead(NotAsked::AnEnzymeCoversIt { operator: op.to_string() });
    }
    if ordinary_performance >= threshold {
        return instead(NotAsked::TheOrdinaryPathIsGoodEnough {
            performance: ordinary_performance,
            threshold,
        });
    }
    if !policy.allow {
        return instead(NotAsked::NotAllowedForThisClass);
    }
    if spent_this_period >= policy.budget {
        return instead(NotAsked::BudgetExhausted {
            spent: spent_this_period,
            budget: policy.budget,
        });
    }
    if tokens_wanted > policy.maxtok {
        return instead(NotAsked::OverTheCap { asked: tokens_wanted, maxtok: policy.maxtok });
    }
    Decision::Ask { model: policy.model.clone(), maxtok: policy.maxtok }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task() -> TaskClass {
        TaskClass::named("classify an unfamiliar message")
    }

    fn policy() -> Policy {
        Policy::declared(task(), true, 10_000, "a model", 2_000, Fallback::AskAPerson)
            .expect("a real cap")
    }

    #[test]
    fn a_model_is_a_last_resort_and_an_enzyme_that_covers_it_wins() {
        // ★★★ If an Enzyme already covers the task there is nothing to ask
        //     about. A system that reached for a model first would be one where
        //     the deterministic path quietly rotted.
        let d = decide(&policy(), Some("budget.spend"), 0.1, 0.9, 0, 100);
        assert!(!d.asks());
        assert!(d.describe().contains("nothing to ask about"));
    }

    #[test]
    fn an_ordinary_path_that_is_good_enough_is_not_improved_by_spending_money() {
        // ★★★ The second conjunct. Asking when the deterministic path already
        //     clears θ is spending to be no better off.
        let d = decide(&policy(), None, 0.95, 0.9, 0, 100);
        assert!(!d.asks());
        assert!(d.describe().contains("no better off"));
    }

    #[test]
    fn a_model_is_asked_when_nothing_covers_it_and_the_ordinary_path_is_short() {
        let d = decide(&policy(), None, 0.2, 0.9, 0, 100);
        assert_eq!(d, Decision::Ask { model: "a model".into(), maxtok: 2_000 });
    }

    #[test]
    fn a_budget_without_a_per_call_cap_cannot_be_constructed() {
        // ★★★ A cap of zero is not a cap — it is a budget with the bound left
        //     off, which is the exact shape a budget overshot by one call has.
        assert!(Policy::declared(task(), true, 10_000, "m", 0, Fallback::AskAPerson).is_none());
    }

    #[test]
    fn whether_the_budget_actually_binds_is_askable() {
        // ★★★ "We have a budget" is not the same claim as "the budget binds".
        //     A cap of 2,000 tokens at 10 a token is 20,000 — more than the
        //     whole budget, so one call can blow it.
        let p = policy();
        assert!(p.budget_binds(1), "at 1 per token, the worst call is 2,000 of 10,000");
        assert!(!p.budget_binds(10), "at 10 per token, one call exceeds the whole budget");
        assert_eq!(p.worst_single_call(10), 20_000);
    }

    #[test]
    fn a_call_over_the_cap_is_refused_and_the_reason_says_why_caps_exist() {
        let d = decide(&policy(), None, 0.2, 0.9, 0, 5_000);
        assert!(!d.asks());
        assert!(d.describe().contains("overshot by exactly one call"));
    }

    #[test]
    fn an_exhausted_budget_refuses_and_names_the_numbers() {
        let d = decide(&policy(), None, 0.2, 0.9, 10_000, 100);
        assert!(!d.asks());
        assert!(d.describe().contains("10000 of 10000"));
    }

    #[test]
    fn every_refusal_carries_a_fallback_so_deactivation_is_never_an_outage() {
        // ★★★ A system whose only answer to "the model is unavailable" is to
        //     stop has made a model a dependency while calling it optional.
        for d in [
            decide(&policy(), Some("budget.spend"), 0.1, 0.9, 0, 10),
            decide(&policy(), None, 0.99, 0.9, 0, 10),
            decide(&policy(), None, 0.1, 0.9, 99_999, 10),
            decide(&policy(), None, 0.1, 0.9, 0, 99_999),
        ] {
            match d {
                Decision::Instead { fallback, .. } => assert_eq!(fallback, Fallback::AskAPerson),
                other => panic!("every refusal needs a fallback: {other:?}"),
            }
        }
    }

    #[test]
    fn turning_the_model_off_falls_back_rather_than_failing() {
        // ★★★ Deactivation is a policy, not a switch. `allow: false` produces
        //     an answer, not an error.
        let off = Policy::declared(
            task(),
            false,
            10_000,
            "a model",
            2_000,
            Fallback::TheOrdinaryEnzyme { operator: "vendor.suggest".into() },
        )
        .expect("a real cap");
        let d = decide(&off, None, 0.1, 0.9, 0, 10);
        assert!(!d.asks());
        assert!(d.describe().contains("use 'vendor.suggest'"));
    }

    #[test]
    fn declining_a_task_deliberately_is_a_decision_and_not_an_outage() {
        // ★★ "This simply does not get done when the model is off" is a
        //    choice somebody made and can disagree with, which an outage is not.
        let declining = Policy::declared(
            task(),
            false,
            0,
            "m",
            10,
            Fallback::DeclineTheTask { because: "nobody needs this badly enough".into() },
        )
        .expect("a real cap");
        let d = decide(&declining, None, 0.1, 0.9, 0, 1);
        assert!(d.describe().contains("leave it undone"));
    }

    #[test]
    fn there_is_no_default_policy_to_inherit() {
        // ★★ A default budget is a budget nobody set, and a default fallback is
        //    an outage nobody noticed. Every field is an argument.
        let p = policy();
        assert_eq!(p.budget, 10_000);
        assert_eq!(p.maxtok, 2_000);
    }
}
