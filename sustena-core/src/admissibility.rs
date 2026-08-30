//! **`admissible_global = ⋀_{H ∈ path} admit_H`** (Constraint · §VII).
//!
//! ```text
//!   a habitat  ⊂  a household  ⊂  a village
//!       └── its own gate ──┴── its parent's ──┴── and its parent's
//! ```
//!
//! ★★★ **Only the immediate parent was ever asked.** A habitat's spend was
//! checked against its own laws and its household's, and a village-level rule
//! two edges up simply did not apply — silently, with nothing anywhere saying
//! a level had been skipped. A holon three deep is not exotic; it is the shape
//! the whole model is named after.
//!
//! ★★★ **Refusal only on a NEWLY-CAUSED breach.** A household already over its
//! own limit does not get to freeze every member until somebody fixes it. That
//! would be the opposite of person-first: the one household that most needs its
//! members to be able to act would be the one that stopped them. So a level
//! refuses only where its rule held *before* and does not hold *after* — an
//! already-broken rule blocks nothing, and an action that IMPROVES a broken
//! rule is admitted even though the rule still fails.
//!
//! ★★★ **Advisory by default, binding only where a household said so.** A
//! parent's rule about the total is *information* travelling upward; making it
//! authority travelling downward is a decision each household makes per rule.
//! An advisory breach is reported and never refuses, which is the difference
//! between a system that tells you something and one that takes your money's
//! movement away from you.
//!
//! ## Two properties worth having as tests rather than intentions
//!
//! ★★ **Monotone.** Adding a level to the path can only refuse more, never
//! fewer — conjunction has no other option, but a composition that shortcut on
//! the first refusal and forgot to keep looking would quietly lose the rest.
//!
//! ★★ **Order-independent.** `⋀` is commutative, so walking the path from the
//! root down or from the leaf up gives the same verdict. If it did not, the
//! answer would depend on an implementation detail nobody declared.
//!
//! ★★ **It names the refuser.** "Refused" without a level and a rule is an
//! alarm; a person needs to know whose law it was to know who to talk to.

use serde_json::Value;

use crate::predicate::{eval::evaluate, parse_predicate};

/// Whether a level's rule may stop a member, or only speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// Reported, never refuses. The default, on purpose.
    Advisory,
    /// May refuse a member's action, on a newly-caused breach only.
    Binding,
}

/// One rule a level holds about what its members add up to.
#[derive(Debug, Clone, PartialEq)]
pub struct BindingRule {
    pub id: String,
    pub expression: String,
    pub authority: Authority,
}

impl BindingRule {
    pub fn advisory(id: &str, expression: &str) -> Self {
        Self { id: id.into(), expression: expression.into(), authority: Authority::Advisory }
    }
    pub fn binding(id: &str, expression: &str) -> Self {
        Self { id: id.into(), expression: expression.into(), authority: Authority::Binding }
    }
}

/// One holon on the path, with what it would read before and after.
///
/// ★★ The readings are supplied rather than computed here. Recomputing a
/// roll-up needs every child's state, which is the host's to hold; this module
/// composes verdicts and does not reach for data.
#[derive(Debug, Clone)]
pub struct Level {
    pub sustain_id: String,
    pub rules: Vec<BindingRule>,
    /// The level's reading as things stand.
    pub before: Value,
    /// Its reading if the member's action were committed.
    pub after: Value,
}

/// A rule that a level found broken.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub sustain_id: String,
    pub rule: String,
    pub expression: String,
    /// Why, in the evaluator's own words.
    pub reason: String,
    /// Did this action CAUSE the breach, or was it already there?
    pub newly_caused: bool,
    /// ★★★ The rule could not be READ, which is a different thing from a rule
    /// that was already broken — and a test caught them being conflated.
    ///
    /// An unreadable rule evaluates false both before and after, so it looks
    /// exactly like a pre-existing breach and would have been waved through as
    /// "not newly caused". That is a gate failing open at the worst possible
    /// moment: right after somebody mistyped a law.
    pub unreadable: bool,
    pub authority: Authority,
}

/// What the whole path decided.
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalVerdict {
    /// Every level that refused. Empty ⟺ admitted.
    pub refusals: Vec<Finding>,
    /// Breaches that spoke without refusing — advisory rules, and rules that
    /// were already broken before this action.
    pub notes: Vec<Finding>,
}

impl GlobalVerdict {
    pub fn admitted(&self) -> bool {
        self.refusals.is_empty()
    }

    /// The first level to refuse, deepest-declared first.
    ///
    /// ★★ "Refused" without a level and a rule is an alarm rather than a
    /// diagnosis — a person needs to know whose law it was.
    pub fn refused_by(&self) -> Option<&Finding> {
        self.refusals.first()
    }

    pub fn describe(&self) -> String {
        match self.refused_by() {
            None => "admitted along the whole path".into(),
            Some(f) => format!(
                "'{}' refused it: {} ({})",
                f.sustain_id, f.reason, f.rule
            ),
        }
    }
}

/// Three outcomes, because *broken document* is not *violated law*.
enum Reading {
    Held,
    Failed(String),
    /// ★★★ CON-5's fail-safe default, and DSL-5's distinction arriving here
    ///     too: a rule that cannot be compiled is not a rule that is false.
    Unreadable(String),
}

fn read(expression: &str, state: &Value) -> Reading {
    match parse_predicate(expression) {
        Err(e) => Reading::Unreadable(format!("this rule cannot be read: {e}")),
        Ok(node) => match evaluate(&node, state, &serde_json::Map::new()) {
            (true, _) => Reading::Held,
            (false, why) => Reading::Failed(why),
        },
    }
}

/// **Compose admissibility along the whole membership path.**
///
/// ★★★ Every level, not the nearest one. Stopping at the first refusal would
/// be cheaper and would lose the rest of the findings — and a person told about
/// one broken rule, who fixes it and is then told about a second, has been made
/// to do the work twice.
pub fn admissible_along(path: &[Level]) -> GlobalVerdict {
    let mut refusals = Vec::new();
    let mut notes = Vec::new();

    for level in path {
        for rule in &level.rules {
            let (reason, unreadable) = match read(&rule.expression, &level.after) {
                Reading::Held => continue,
                Reading::Failed(why) => (why, false),
                Reading::Unreadable(why) => (why, true),
            };
            // ★★ An unreadable rule has no "before" to compare against — it was
            //    never evaluated, so calling it pre-existing would be inventing
            //    a history for it.
            let newly_caused =
                !unreadable && matches!(read(&rule.expression, &level.before), Reading::Held);

            let finding = Finding {
                sustain_id: level.sustain_id.clone(),
                rule: rule.id.clone(),
                expression: rule.expression.clone(),
                reason,
                newly_caused,
                unreadable,
                authority: rule.authority,
            };

            // ★★★ Refuse where this action caused it, or where the rule cannot
            //     be read at all — and only where the household made the rule
            //     binding. An already-broken rule freezing every member would
            //     punish exactly the household that most needs its members able
            //     to act; an unreadable one waved through would be the gate
            //     failing open right after somebody mistyped a law.
            if rule.authority == Authority::Binding && (newly_caused || unreadable) {
                refusals.push(finding);
            } else {
                notes.push(finding);
            }
        }
    }

    GlobalVerdict { refusals, notes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn level(id: &str, rules: Vec<BindingRule>, before: f64, after: f64) -> Level {
        Level {
            sustain_id: id.into(),
            rules,
            before: json!({"total": before}),
            after: json!({"total": after}),
        }
    }

    #[test]
    fn a_path_where_every_level_is_content_admits() {
        let path = vec![
            level("village", vec![BindingRule::binding("cap", "total <= 100000")], 5000.0, 5500.0),
            level("household", vec![BindingRule::binding("cap", "total <= 10000")], 5000.0, 5500.0),
        ];
        assert!(admissible_along(&path).admitted());
    }

    #[test]
    fn a_level_two_edges_up_is_asked_at_all() {
        // ★★★ The gap this closes. Only the immediate parent was ever asked, so
        //     a village-level rule simply did not apply — silently, with
        //     nothing saying a level had been skipped.
        let path = vec![
            level("village", vec![BindingRule::binding("cap", "total <= 6000")], 5000.0, 7000.0),
            level("household", vec![BindingRule::binding("cap", "total <= 10000")], 5000.0, 7000.0),
        ];
        let v = admissible_along(&path);
        assert!(!v.admitted());
        assert_eq!(v.refused_by().unwrap().sustain_id, "village");
    }

    #[test]
    fn an_already_broken_rule_does_not_freeze_every_member() {
        // ★★★ The opposite of person-first would be the household that most
        //     needs its members able to act being the one that stops them.
        let path = vec![level(
            "household",
            vec![BindingRule::binding("cap", "total <= 1000")],
            5000.0,
            5001.0,
        )];
        let v = admissible_along(&path);
        assert!(v.admitted(), "already over, so this action did not cause it");
        assert_eq!(v.notes.len(), 1, "and it is still reported");
        assert!(!v.notes[0].newly_caused);
    }

    #[test]
    fn improving_a_broken_rule_is_admitted_even_though_it_still_fails() {
        let path = vec![level(
            "household",
            vec![BindingRule::binding("cap", "total <= 1000")],
            5000.0,
            2000.0,
        )];
        assert!(admissible_along(&path).admitted());
    }

    #[test]
    fn advisory_speaks_and_never_refuses() {
        // ★★★ The difference between a system that tells you something and one
        //     that takes your money's movement away from you.
        let path = vec![level(
            "household",
            vec![BindingRule::advisory("cap", "total <= 1000")],
            500.0,
            5000.0,
        )];
        let v = admissible_along(&path);
        assert!(v.admitted());
        assert_eq!(v.notes.len(), 1);
        assert!(v.notes[0].newly_caused, "it did cause it — and still does not refuse");
    }

    #[test]
    fn adding_a_level_can_only_refuse_more() {
        // ★★ Conjunction has no other option, but a composition that shortcut
        //    on the first refusal and forgot to keep looking would lose the
        //    rest.
        let household =
            level("household", vec![BindingRule::binding("cap", "total <= 10000")], 100.0, 200.0);
        let strict =
            level("village", vec![BindingRule::binding("cap", "total <= 150")], 100.0, 200.0);

        assert!(admissible_along(std::slice::from_ref(&household)).admitted());
        assert!(!admissible_along(&[household, strict]).admitted());
    }

    #[test]
    fn walking_the_path_either_way_round_gives_one_answer() {
        // ★★ `⋀` is commutative. If this were not true the answer would depend
        //    on an implementation detail nobody declared.
        let a = level("village", vec![BindingRule::binding("cap", "total <= 150")], 100.0, 200.0);
        let b = level("household", vec![BindingRule::binding("cap", "total <= 180")], 100.0, 200.0);
        let down = admissible_along(&[a.clone(), b.clone()]);
        let up = admissible_along(&[b, a]);
        assert_eq!(down.admitted(), up.admitted());
        assert_eq!(down.refusals.len(), up.refusals.len());
    }

    #[test]
    fn every_level_is_reported_not_just_the_first_to_refuse() {
        // ★★★ A person told about one broken rule, who fixes it and is then
        //     told about a second, has been made to do the work twice.
        let path = vec![
            level("village", vec![BindingRule::binding("cap", "total <= 150")], 100.0, 200.0),
            level("household", vec![BindingRule::binding("cap", "total <= 160")], 100.0, 200.0),
        ];
        assert_eq!(admissible_along(&path).refusals.len(), 2);
    }

    #[test]
    fn the_refusal_says_whose_law_it_was() {
        let path = vec![level(
            "the village",
            vec![BindingRule::binding("village_cap", "total <= 150")],
            100.0,
            200.0,
        )];
        let v = admissible_along(&path);
        let said = v.describe();
        assert!(said.contains("the village"), "{said}");
        assert!(said.contains("village_cap"), "{said}");
    }

    #[test]
    fn an_unreadable_rule_is_not_mistaken_for_an_already_broken_one() {
        // ★★★ The trap a test caught: an unreadable rule evaluates false both
        //     before and after, so it looks exactly like a pre-existing breach
        //     and would have been waved through as "not newly caused".
        let path = vec![level(
            "household",
            vec![BindingRule::binding("broken", "total >>>")],
            1.0,
            2.0,
        )];
        let v = admissible_along(&path);
        assert!(v.refused_by().unwrap().unreadable);
        assert!(!v.refused_by().unwrap().newly_caused, "it has no before to have been caused by");
    }

    #[test]
    fn an_unreadable_advisory_rule_still_only_speaks() {
        // ★★ Advisory never refuses. It was never a gate decision, so failing
        //    to read it cannot fail one open.
        let path = vec![level(
            "household",
            vec![BindingRule::advisory("broken", "total >>>")],
            1.0,
            2.0,
        )];
        let v = admissible_along(&path);
        assert!(v.admitted());
        assert!(v.notes[0].unreadable);
    }

    #[test]
    fn a_rule_that_cannot_be_read_refuses_rather_than_passing() {
        // ★★★ CON-5's fail-safe default. A constraint that fails to compile
        //     must not silently admit — that is a gate failing open, and it
        //     fails open exactly when somebody has just mistyped a law.
        let path = vec![level("household", vec![BindingRule::binding("broken", "total >>>")], 1.0, 2.0)];
        let v = admissible_along(&path);
        assert!(!v.admitted());
        assert!(v.refused_by().unwrap().reason.contains("cannot be read"));
    }

    #[test]
    fn an_empty_path_admits() {
        // ★★ A Sustain with no parents is not a Sustain nobody checked — its
        //    own gate ran before this was ever called.
        assert!(admissible_along(&[]).admitted());
    }
}
