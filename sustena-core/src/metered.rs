//! **Everything that spends, spends the same meter** (Operator §VIII;
//! Monitor and Controller Additions).
//!
//! Three rows, one idea: once computation has a price, the price is available
//! everywhere a decision is made — so watching, surfacing and intervening all
//! become economic questions rather than only statistical ones.
//!
//! ## Declared and metered are two different numbers
//!
//! ★★★ **A declared cost is a promise made before the run; a metered cost is a
//! measurement taken after it.** Gas is quoted up front so a caller can decide
//! whether to proceed; the odometer says what actually happened. Collapsing them
//! loses the ability to notice that an operator consistently costs more than it
//! says — which is precisely the signal worth having.
//!
//! ★★★ **Compute is work, not wall-clock.** A reproducible proxy means the same
//! run costs the same everywhere, which is why the EVM charges declared gas per
//! opcode rather than measuring seconds. A price that varied with how busy the
//! machine was would make an audit impossible and a budget meaningless.
//!
//! ★★★ **A refused call costs zero.** The gate ran and nothing was committed, so
//! there is nothing to meter — and charging for refusals would make the gate a
//! revenue source, which is the worst possible incentive to attach to a thing
//! whose job is to say no.
//!
//! ## Looking is not free
//!
//! ★★★ **Surfacing spends a working-memory slot AND pawa**, so the CUSUM
//! crossing is an economic decision rather than only a statistical one. A
//! detector tuned purely on statistics will happily spend a person's attention
//! on a signal that was not worth the interruption — and attention is the
//! scarcer of the two.
//!
//! ## The cheaper equal-gain intervention wins
//!
//! ★★★ Worth is **Lyapunov gain net of pawa**. Two interventions that close the
//! same distance to `V` are not equally good if one costs four times as much,
//! and without the meter there was no way to say so.

/// What an operator says it will cost, before it runs.
///
/// ★★ A promise, and promises can be wrong. That is the point of keeping it
/// separate from the measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Declared(pub u64);

/// What it actually cost, after.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metered {
    pub compute: u64,
    pub storage: u64,
}

impl Metered {
    /// ★★★ Nothing at all — the shape of a refused call.
    pub fn nothing() -> Self {
        Self { compute: 0, storage: 0 }
    }

    pub fn is_nothing(&self) -> bool {
        self.compute == 0 && self.storage == 0
    }

    pub fn pawa(&self, kappa_c: f64, kappa_s: f64) -> f64 {
        kappa_c * self.compute as f64 + kappa_s * self.storage as f64
    }
}

/// How a declared price compares to what was actually spent.
#[derive(Debug, Clone, PartialEq)]
pub enum Quote {
    /// It cost about what it said.
    Honest { declared: u64, actual: f64 },
    /// ★★★ It consistently costs more than it says. **The signal worth having**,
    /// and one a single collapsed number could not produce.
    UnderQuoted { declared: u64, actual: f64, by: f64 },
    /// It costs less than it says. Not a fault, and still worth knowing: an
    /// over-quoted operator makes every budget built on it too conservative.
    OverQuoted { declared: u64, actual: f64 },
}

impl Quote {
    pub fn describe(&self, operator: &str) -> String {
        match self {
            Self::Honest { declared, .. } => format!("{operator} costs about the {declared} it quotes"),
            Self::UnderQuoted { declared, actual, by } => format!(
                "{operator} quotes {declared} and costs {actual:.1} — {by:.0}% more than it says, \
                 so every budget built on its quote is short"
            ),
            Self::OverQuoted { declared, actual } => format!(
                "{operator} quotes {declared} and costs {actual:.1} — not a fault, but every \
                 budget built on its quote is too conservative"
            ),
        }
    }
}

/// How far a quote may drift before it is worth saying something.
///
/// ★★ Declared rather than assumed: measurement noise is real, and a comparison
/// with no tolerance reports every operator as dishonest on its first run.
pub const QUOTE_TOLERANCE: f64 = 0.10;

/// **Compare the promise to the measurement.**
pub fn check_quote(declared: Declared, actual: f64) -> Quote {
    let d = declared.0 as f64;
    if d <= 0.0 {
        return Quote::Honest { declared: declared.0, actual };
    }
    let ratio = (actual - d) / d;
    if ratio > QUOTE_TOLERANCE {
        Quote::UnderQuoted { declared: declared.0, actual, by: ratio * 100.0 }
    } else if ratio < -QUOTE_TOLERANCE {
        Quote::OverQuoted { declared: declared.0, actual }
    } else {
        Quote::Honest { declared: declared.0, actual }
    }
}

/// What a refused call costs.
///
/// ★★★ Always nothing. A function rather than a comment, so a change that
/// charged for refusals would have to delete this and the test that calls it —
/// and charging for refusals would make the gate a revenue source, which is the
/// worst incentive to attach to a thing whose job is to say no.
pub fn cost_of_a_refusal() -> Metered {
    Metered::nothing()
}

/// What surfacing one thing costs.
///
/// ★★★ Two currencies, and the scarcer one is not the money. A person has about
/// four working-memory slots; they have rather more than four pawa.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interruption {
    pub slots: usize,
    pub pawa: f64,
}

/// **Is a detection worth surfacing?**
///
/// ★★★ The CUSUM crossing is a *statistical* fact; surfacing is an *economic*
/// decision. A detector tuned purely on statistics will spend a person's
/// attention on a signal that was not worth the interruption — so the crossing
/// is necessary and not sufficient.
///
/// ★★ `value_if_acted_on` is supplied by the caller because only the caller
/// knows what the finding is worth. A module that guessed would be putting a
/// number on somebody else's Tuesday.
pub fn worth_surfacing(
    crossed: bool,
    value_if_acted_on: f64,
    cost: Interruption,
    pawa_per_slot: f64,
) -> bool {
    crossed && value_if_acted_on > cost.pawa + cost.slots as f64 * pawa_per_slot
}

/// How much a slot is worth, expressed in pawa.
///
/// ★★★ Required rather than defaulted, and the reason is uncomfortable: putting
/// a price on attention is a judgement about how much somebody's focus is worth,
/// and a default would be this module making that judgement for them.
pub fn attention_is_priced(pawa_per_slot: f64) -> bool {
    pawa_per_slot > 0.0
}

/// The worth of an intervention: Lyapunov gain, net of what it costs.
///
/// ★★★ Two interventions that close the same distance to `V` are not equally
/// good if one costs four times as much — and before the meter there was no way
/// to say so.
pub fn worth(lyapunov_gain: f64, pawa: f64, lambda: f64) -> f64 {
    lyapunov_gain - lambda * pawa
}

/// Of several interventions, the one worth most. Ties broken by name so the
/// answer does not move between runs.
pub fn best(options: &[(String, f64, f64)], lambda: f64) -> Option<&(String, f64, f64)> {
    options.iter().reduce(|a, b| {
        let (wa, wb) = (worth(a.1, a.2, lambda), worth(b.1, b.2, lambda));
        match wb.partial_cmp(&wa) {
            Some(std::cmp::Ordering::Greater) => b,
            Some(std::cmp::Ordering::Equal) if b.0 < a.0 => b,
            _ => a,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_call_costs_nothing() {
        // ★★★ The gate ran and nothing committed, so there is nothing to meter
        //     — and charging for refusals would make the gate a revenue source,
        //     which is the worst incentive to attach to a thing whose job is to
        //     say no.
        assert!(cost_of_a_refusal().is_nothing());
        assert_eq!(cost_of_a_refusal().pawa(1.0, 0.01), 0.0);
    }

    #[test]
    fn an_operator_that_costs_more_than_it_quotes_is_the_signal_worth_having() {
        // ★★★ And it is one a single collapsed number could not produce: with
        //     only a measurement there is nothing to compare it against.
        let q = check_quote(Declared(10), 18.0);
        assert!(matches!(q, Quote::UnderQuoted { .. }));
        assert!(q.describe("budget.spend").contains("every budget built on its quote is short"));
    }

    #[test]
    fn an_over_quoted_operator_is_not_a_fault_and_is_still_worth_knowing() {
        // ★★ Every budget built on its quote is too conservative, which costs
        //    somebody something even though nothing is broken.
        let q = check_quote(Declared(100), 40.0);
        assert!(matches!(q, Quote::OverQuoted { .. }));
        assert!(q.describe("x").contains("not a fault"));
    }

    #[test]
    fn a_tolerance_exists_so_the_first_run_does_not_call_everything_dishonest() {
        // ★★ Measurement noise is real, and a comparison with no tolerance
        //    reports every operator as under-quoting the moment it varies.
        assert!(matches!(check_quote(Declared(100), 105.0), Quote::Honest { .. }));
        assert!(matches!(check_quote(Declared(100), 95.0), Quote::Honest { .. }));
    }

    #[test]
    fn compute_is_work_rather_than_wall_clock() {
        // ★★★ The same run costs the same everywhere. A price that varied with
        //     how busy the machine was would make an audit impossible and a
        //     budget meaningless — so the metered value has no time in it at
        //     all, and there is nowhere to put one.
        let a = Metered { compute: 7, storage: 542 };
        let b = Metered { compute: 7, storage: 542 };
        assert_eq!(a.pawa(1.0, 0.01), b.pawa(1.0, 0.01));
    }

    #[test]
    fn a_crossing_is_necessary_and_not_sufficient_for_an_interruption() {
        // ★★★ The CUSUM crossing is a statistical fact; surfacing is an
        //     economic decision. A detector tuned purely on statistics will
        //     spend a person's attention on something that was not worth it.
        let small = Interruption { slots: 1, pawa: 2.0 };
        assert!(!worth_surfacing(true, 3.0, small, 5.0), "crossed, and not worth the slot");
        assert!(worth_surfacing(true, 50.0, small, 5.0), "crossed, and worth it");
        assert!(!worth_surfacing(false, 50.0, small, 5.0), "not crossed at all");
    }

    #[test]
    fn attention_is_the_scarcer_of_the_two_currencies() {
        // ★★ A person has about four working-memory slots and rather more than
        //    four pawa, so the slot term is what usually decides.
        let cheap_in_money = Interruption { slots: 2, pawa: 0.1 };
        assert!(!worth_surfacing(true, 20.0, cheap_in_money, 20.0), "the slots cost more");
    }

    #[test]
    fn pricing_attention_is_a_judgement_somebody_has_to_make() {
        // ★★★ Required rather than defaulted, and the reason is uncomfortable:
        //     a default would be this module deciding how much somebody's focus
        //     is worth.
        assert!(attention_is_priced(5.0));
        assert!(!attention_is_priced(0.0));
    }

    #[test]
    fn of_two_interventions_that_close_the_same_distance_the_cheaper_wins() {
        // ★★★ Before the meter there was no way to say so.
        let options = vec![
            ("expensive".to_string(), 10.0, 40.0),
            ("cheap".to_string(), 10.0, 5.0),
        ];
        assert_eq!(best(&options, 0.1).map(|o| o.0.as_str()), Some("cheap"));
    }

    #[test]
    fn a_much_better_outcome_still_beats_a_cheaper_worse_one() {
        // ★★ Worth is gain NET of cost, not cost alone. An intervention that
        //    closes four times the distance is allowed to cost more.
        let options = vec![
            ("does little, cheaply".to_string(), 1.0, 0.0),
            ("does a lot".to_string(), 40.0, 20.0),
        ];
        assert_eq!(best(&options, 0.1).map(|o| o.0.as_str()), Some("does a lot"));
    }

    #[test]
    fn a_tie_resolves_the_same_way_every_time() {
        // ★★ A ranking that moves between runs on identical inputs is one
        //    people stop trusting.
        let options =
            vec![("b".to_string(), 5.0, 1.0), ("a".to_string(), 5.0, 1.0)];
        assert_eq!(best(&options, 0.1).map(|o| o.0.as_str()), Some("a"));
    }

    #[test]
    fn nothing_to_choose_between_is_no_choice() {
        assert!(best(&[], 0.1).is_none());
    }
}
