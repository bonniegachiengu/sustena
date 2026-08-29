//! **Description length, and when a pattern earns a name** (DSL · GENOME §XI).
//!
//! ```text
//!   ΔL(π) = n · (L(body) − L(call)) − L(def)        lift when ΔL > 0
//! ```
//!
//! ★★★ **Learning is compression, and this is that claim made arithmetic.**
//! A household writes the same rule into six pockets — *never let this go
//! below zero* — and six copies is six places to be wrong. Lifting it into one
//! named definition, called six times, is shorter; and shorter, here, is not an
//! aesthetic. The count IS the evidence: a shape written once is somebody's
//! particular rule, a shape written six times is a concept the household has
//! and has not named yet.
//!
//! ★★★ **`n` is the whole reason this is not just deduplication.** Lifting a
//! pattern used twice usually costs more than it saves — the definition itself
//! has a length, and two call sites may not pay for it. The formula says so
//! rather than leaving it to taste, and the threshold falls out of the numbers
//! instead of being chosen.
//!
//! ## What is measured, and what deliberately is not
//!
//! ★★ **Length is counted over the TYPED AST, never over the text.** Two rules
//! that differ only in whitespace, or in whether somebody wrote `>=` with a
//! space, are the same rule; counting characters would make the formatter part
//! of the arithmetic. Nodes are what a reader has to hold in their head.
//!
//! ★★★ **A proposal is not an edit.** `ΔL > 0` says a lift would be shorter; it
//! does not say the household wants it, that the name is a good one, or that
//! the abstraction is real rather than a coincidence of six pockets that happen
//! to rhyme this month. So this module *proposes*, and the migration predicate
//! and a person stand between the proposal and the definition — the same
//! division `learned.rs` already keeps for parse rules.
//!
//! ★★ **Patterns are matched up to their differing leaves**, which is what
//! makes a lift a *function* rather than a duplicate. `pockets.food.allocated
//! >= 0` and `pockets.rent.allocated >= 0` are one pattern with one hole; two
//! rules with nothing in common are not a pattern at all.

use std::collections::BTreeMap;

use crate::predicate::ast::{Operand, Predicate};
use crate::predicate::{parse_predicate, StatePath};

/// How much of a reader's attention a thing costs.
///
/// ★★ Every node counts one. Weighting — a quantifier is "heavier" than a
/// comparison — would be a second judgment smuggled into a measurement, and the
/// article's `L` is a count.
pub fn length(node: &Predicate) -> usize {
    match node {
        Predicate::Comparison { left, right, .. } => 1 + operand_len(left) + operand_len(right),
        Predicate::Membership { left, right, .. } => 1 + operand_len(left) + operand_len(right),
        Predicate::And(parts) | Predicate::Or(parts) => {
            1 + parts.iter().map(length).sum::<usize>()
        }
        Predicate::Not(inner) => 1 + length(inner),
        Predicate::Quantifier { body, .. } => 2 + length(body),
    }
}

fn operand_len(o: &Operand) -> usize {
    match o {
        Operand::Literal(_) | Operand::Param(_) => 1,
        Operand::Path(p) => p.segments.len(),
        Operand::Aggregate { path, .. } => 1 + path.segments.len(),
        Operand::List(items) => 1 + items.len(),
    }
}

/// A rule with its differing leaves replaced by holes.
///
/// ★★ The shape is a plain string because it is only ever compared for
/// equality and shown to a person. Making it a typed skeleton would be a second
/// AST to keep in step with the first.
fn shape(node: &Predicate) -> String {
    match node {
        Predicate::Comparison { left, op, right } => {
            format!("{} {} {}", shape_operand(left), op.symbol(), shape_operand(right))
        }
        Predicate::Membership { left, negate, right } => format!(
            "{} {}IN {}",
            shape_operand(left),
            if *negate { "NOT " } else { "" },
            shape_operand(right)
        ),
        Predicate::And(parts) => {
            format!("({})", parts.iter().map(shape).collect::<Vec<_>>().join(" AND "))
        }
        Predicate::Or(parts) => {
            format!("({})", parts.iter().map(shape).collect::<Vec<_>>().join(" OR "))
        }
        Predicate::Not(inner) => format!("NOT {}", shape(inner)),
        Predicate::Quantifier { kind, list_path, body } => {
            format!("{} {}: {}", kind.name(), hole_path(list_path), shape(body))
        }
    }
}

fn shape_operand(o: &Operand) -> String {
    match o {
        Operand::Literal(v) => v.to_string(),
        Operand::Param(n) => format!("params.{n}"),
        Operand::Path(p) => hole_path(p),
        Operand::Aggregate { func, path } => format!("{}({})", func.name(), hole_path(path)),
        Operand::List(_) => "[…]".into(),
    }
}

/// A path with its *open-map key* replaced by a hole.
///
/// ★★★ This is the judgment that decides what counts as one pattern. A pocket's
/// NAME is a value the household chose; the segments around it are structure.
/// So `finances.pockets.food.allocated` and `finances.pockets.rent.allocated`
/// differ in a value and agree in shape — one pattern, one hole. Holing every
/// segment would make every rule identical; holing none would make a lift
/// impossible.
///
/// ★★ Only the segment after a `pockets`-like container is holed, and only when
/// the path is long enough for there to BE structure around it. A two-segment
/// path has no interior.
fn hole_path(p: &StatePath) -> String {
    use crate::predicate::PathSegment;
    let names: Vec<&str> = p
        .segments
        .iter()
        .filter_map(|s| match s {
            PathSegment::Name(n) => Some(n.as_str()),
            _ => None,
        })
        .collect();
    if names.len() < 3 {
        return p.raw.clone();
    }
    let mut out: Vec<String> = names.iter().map(|s| (*s).to_string()).collect();
    // The interior segment is the one that varies between siblings.
    out[names.len() - 2] = "·".to_string();
    out.join(".")
}

/// The distinguishing value at the hole, so a lift's call sites are real.
fn hole_value(p: &StatePath) -> Option<String> {
    use crate::predicate::PathSegment;
    let names: Vec<&str> = p
        .segments
        .iter()
        .filter_map(|s| match s {
            PathSegment::Name(n) => Some(n.as_str()),
            _ => None,
        })
        .collect();
    if names.len() < 3 {
        return None;
    }
    Some(names[names.len() - 2].to_string())
}

fn holes(node: &Predicate, out: &mut Vec<String>) {
    match node {
        Predicate::Comparison { left, right, .. }
        | Predicate::Membership { left, right, .. } => {
            for o in [left, right] {
                if let Operand::Path(p) | Operand::Aggregate { path: p, .. } = o {
                    if let Some(v) = hole_value(p) {
                        out.push(v);
                    }
                }
            }
        }
        Predicate::And(parts) | Predicate::Or(parts) => parts.iter().for_each(|p| holes(p, out)),
        Predicate::Not(inner) => holes(inner, out),
        Predicate::Quantifier { body, .. } => holes(body, out),
    }
}

/// A pattern worth naming, with the arithmetic that says so.
#[derive(Debug, Clone, PartialEq)]
pub struct Lift {
    /// The shared shape, with its varying part shown as `·`.
    pub shape: String,
    /// The ids of the rules that share it.
    pub used_by: Vec<String>,
    /// What each of them puts in the hole.
    pub arguments: Vec<String>,
    /// `n`
    pub occurrences: usize,
    /// `L(body)`
    pub body_length: usize,
    /// `ΔL(π) = n(L(body) − L(call)) − L(def)`
    pub saving: isize,
}

/// What one call to a lifted definition costs a reader.
///
/// ★★ A name and its argument. Two nodes, and it is a real cost rather than
/// zero — pretending a call is free is how every abstraction looks worth it.
const CALL_LENGTH: usize = 2;

/// What the definition itself costs, over and above its body.
///
/// ★★ A name, and one parameter. Charging this is what makes `n` matter: a
/// pattern used twice usually does not pay for it, and the formula says so
/// instead of leaving it to taste.
const DEF_OVERHEAD: usize = 2;

/// **Propose every lift the numbers actually support.**
///
/// ★★★ Proposals only. `ΔL > 0` says a lift would be shorter; it does not say
/// the household wants it, that the name would be a good one, or that six
/// pockets rhyming this month is a concept rather than a coincidence. A person
/// and the migration predicate stand between this and the definition — the same
/// division `learned.rs` keeps for parse rules.
///
/// ★★ Sorted by saving, largest first, because a list of proposals nobody
/// ordered is a list somebody reads the least useful end of.
pub fn propose(rules: &[(String, String)]) -> Vec<Lift> {
    let mut groups: BTreeMap<String, (Vec<String>, Vec<String>, usize)> = BTreeMap::new();

    for (id, expression) in rules {
        let Ok(node) = parse_predicate(expression) else { continue };
        let s = shape(&node);
        let mut hs = Vec::new();
        holes(&node, &mut hs);
        // ★★ A rule with no hole is not an instance of anything — it is itself.
        //    Grouping those would propose lifting one rule into a definition
        //    with no parameter, which saves nothing and reads worse.
        if hs.is_empty() {
            continue;
        }
        let entry = groups.entry(s).or_insert_with(|| (Vec::new(), Vec::new(), length(&node)));
        entry.0.push(id.clone());
        entry.1.push(hs.join(", "));
    }

    let mut out: Vec<Lift> = groups
        .into_iter()
        .filter(|(_, (ids, _, _))| ids.len() > 1)
        .map(|(shape, (used_by, arguments, body_length))| {
            let n = used_by.len();
            let saving = (n as isize) * (body_length as isize - CALL_LENGTH as isize)
                - (body_length + DEF_OVERHEAD) as isize;
            Lift { shape, used_by, arguments, occurrences: n, body_length, saving }
        })
        .filter(|l| l.saving > 0)
        .collect();

    out.sort_by(|a, b| b.saving.cmp(&a.saving).then(a.shape.cmp(&b.shape)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(a, b)| ((*a).to_string(), (*b).to_string())).collect()
    }

    #[test]
    fn length_is_counted_over_the_tree_not_the_text() {
        // ★★★ Two rules that differ only in spacing are the same rule.
        //     Counting characters would make the formatter part of the
        //     arithmetic.
        let tight = parse_predicate("finances.liquid.balance>=0").expect("parses");
        let loose = parse_predicate("finances.liquid.balance  >=  0").expect("parses");
        assert_eq!(length(&tight), length(&loose));
    }

    #[test]
    fn six_pockets_saying_the_same_thing_is_a_pattern_with_a_name_missing() {
        // ★★★ The article's own case. Six copies is six places to be wrong, and
        //     the count IS the evidence that this is a concept rather than one
        //     household's particular rule.
        let r = rules(&[
            ("food", "finances.pockets.food.allocated >= 0"),
            ("rent", "finances.pockets.rent.allocated >= 0"),
            ("wifi", "finances.pockets.wifi.allocated >= 0"),
            ("fees", "finances.pockets.fees.allocated >= 0"),
            ("water", "finances.pockets.water.allocated >= 0"),
            ("gas", "finances.pockets.gas.allocated >= 0"),
        ]);
        let lifts = propose(&r);
        assert_eq!(lifts.len(), 1, "{lifts:?}");
        assert_eq!(lifts[0].occurrences, 6);
        assert!(lifts[0].saving > 0);
        assert!(lifts[0].shape.contains('·'), "the varying part is shown: {}", lifts[0].shape);
        assert_eq!(lifts[0].arguments.len(), 6, "and every call site is real");
    }

    #[test]
    fn a_pattern_used_twice_usually_does_not_pay_for_itself() {
        // ★★★ Why `n` is the whole formula and not a rounding detail. The
        //     definition has a length of its own, and two call sites rarely
        //     cover it. The numbers say so rather than taste.
        let r = rules(&[
            ("food", "finances.pockets.food.allocated >= 0"),
            ("rent", "finances.pockets.rent.allocated >= 0"),
        ]);
        assert!(propose(&r).is_empty(), "two is not yet a concept");
    }

    #[test]
    fn one_rule_on_its_own_is_never_a_pattern() {
        let r = rules(&[("food", "finances.pockets.food.allocated >= 0")]);
        assert!(propose(&r).is_empty());
    }

    #[test]
    fn two_rules_with_nothing_in_common_are_not_a_pattern() {
        let r = rules(&[
            ("a", "finances.pockets.food.allocated >= 0"),
            ("b", "ALL finances.pockets[*]: spent >= 0"),
            ("c", "finances.liquid.balance >= 100"),
        ]);
        assert!(propose(&r).is_empty());
    }

    #[test]
    fn rules_that_differ_in_structure_are_different_patterns() {
        // ★★ `>= 0` and `<= 0` are not one shape with a hole in it.
        let r = rules(&[
            ("a", "finances.pockets.food.allocated >= 0"),
            ("b", "finances.pockets.rent.allocated >= 0"),
            ("c", "finances.pockets.wifi.allocated >= 0"),
            ("d", "finances.pockets.gas.allocated <= 0"),
            ("e", "finances.pockets.fees.allocated <= 0"),
            ("f", "finances.pockets.water.allocated <= 0"),
        ]);
        let lifts = propose(&r);
        assert_eq!(lifts.len(), 2, "{lifts:?}");
        assert!(lifts.iter().all(|l| l.occurrences == 3));
    }

    #[test]
    fn a_longer_body_pays_for_itself_sooner() {
        // ★★★ The formula's real content: what is worth naming depends on how
        //     much a reader has to hold, not on how often it appears alone.
        let short = rules(&[
            ("a", "finances.pockets.food.allocated >= 0"),
            ("b", "finances.pockets.rent.allocated >= 0"),
            ("c", "finances.pockets.wifi.allocated >= 0"),
        ]);
        let long = rules(&[
            ("a", "finances.pockets.food.allocated >= 0 AND finances.pockets.food.spent >= 0 AND finances.liquid.balance >= 0"),
            ("b", "finances.pockets.rent.allocated >= 0 AND finances.pockets.rent.spent >= 0 AND finances.liquid.balance >= 0"),
            ("c", "finances.pockets.wifi.allocated >= 0 AND finances.pockets.wifi.spent >= 0 AND finances.liquid.balance >= 0"),
        ]);
        let (s, l) = (propose(&short), propose(&long));
        assert!(!l.is_empty(), "the longer body is worth naming at three");
        if !s.is_empty() {
            assert!(l[0].saving > s[0].saving, "and it saves more");
        }
    }

    #[test]
    fn proposals_come_ordered_by_what_they_save() {
        // ★★ A list nobody ordered is a list somebody reads the wrong end of.
        let mut r = rules(&[
            ("a", "finances.pockets.food.allocated >= 0"),
            ("b", "finances.pockets.rent.allocated >= 0"),
            ("c", "finances.pockets.wifi.allocated >= 0"),
            ("d", "finances.pockets.gas.allocated >= 0"),
        ]);
        r.extend(rules(&[
            ("e", "finances.pockets.food.spent >= 0 AND finances.liquid.balance >= 0"),
            ("f", "finances.pockets.rent.spent >= 0 AND finances.liquid.balance >= 0"),
            ("g", "finances.pockets.wifi.spent >= 0 AND finances.liquid.balance >= 0"),
        ]));
        let lifts = propose(&r);
        assert!(lifts.len() >= 2, "{lifts:?}");
        assert!(lifts[0].saving >= lifts[1].saving);
    }

    #[test]
    fn an_unreadable_rule_is_skipped_rather_than_crashing_the_pass() {
        // ★★ A proposal pass runs over whatever the household has, including a
        //    rule somebody is midway through fixing.
        let r = rules(&[
            ("broken", "finances >>>"),
            ("a", "finances.pockets.food.allocated >= 0"),
            ("b", "finances.pockets.rent.allocated >= 0"),
            ("c", "finances.pockets.wifi.allocated >= 0"),
        ]);
        assert_eq!(propose(&r).len(), 1);
    }

    #[test]
    fn a_proposal_names_which_rules_it_would_replace() {
        // ★★★ It has to be checkable by a person. A proposal that cannot say
        //     which rules it covers is asking for trust it has not earned.
        let r = rules(&[
            ("food", "finances.pockets.food.allocated >= 0"),
            ("rent", "finances.pockets.rent.allocated >= 0"),
            ("wifi", "finances.pockets.wifi.allocated >= 0"),
        ]);
        let lifts = propose(&r);
        assert_eq!(lifts[0].used_by, vec!["food", "rent", "wifi"]);
        assert_eq!(lifts[0].arguments, vec!["food", "rent", "wifi"]);
    }
}
