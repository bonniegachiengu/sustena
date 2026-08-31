//! Compile a predicate once, evaluate it many times.
//!
//! ★★★ **The design was already written down; it just was not followed.**
//! `predicate::evaluate` carries the note *"Preferred on a hot path: an
//! invariant is compiled once when its spec loads, then evaluated on every gate
//! check."* But every real evaluation site reached for `check(expr, ..)`, which
//! **parses the string first** — `admissibility::read` on every reading,
//! `World::constraints` on every call, and `World::constraints` now runs on
//! every §III pulse. So a household's rules were being re-parsed continuously
//! to produce an answer that could not change with the parse.
//!
//! ★★ **The correctness half matters more than the speed half.** Re-parsing per
//! evaluation means an unparseable rule is rediscovered *every tick*, and each
//! caller decides for itself what an unreadable rule means — `admissibility`
//! calls it unreadable, `constraints` calls it not-holding. Compiling once
//! makes a syntax error a **property of the rule**, discoverable when it is
//! declared, instead of a surprise re-derived forever.
//!
//! ★ **The failure is cached too, deliberately.** A rule that does not parse
//! will not parse next time either; re-attempting it would be doing work to
//! learn something already known. The error is kept and handed back intact, so
//! nothing is swallowed — the caller still gets the syntax error it would have
//! got from `check`.
//!
//! **Not an optimiser.** It memoises; it does not rewrite, reorder or simplify
//! a predicate. The AST evaluated is exactly the AST `parse_predicate` returns,
//! which is what keeps this indistinguishable from `check` in its answers.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::{Map, Value};

use super::ast::Predicate;
use super::parse::{parse_predicate, SyntaxError};

/// A memoising predicate compiler, shared across threads.
///
/// ★★ Keyed by the expression **text**, because that is the identity a spec
/// gives a rule. Two rules that read the same are the same predicate, and there
/// is nothing else stable to key on — an id belongs to a sustain, not to the
/// expression.
#[derive(Debug, Default)]
pub struct PredicateCache {
    /// `Ok` compiled, `Err` the syntax error — both worth remembering.
    compiled: Mutex<HashMap<String, Result<Predicate, SyntaxError>>>,
}

impl PredicateCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many distinct expressions have been compiled.
    ///
    /// ★ Exists so "compiled once" can be **asserted** rather than assumed. An
    /// optimisation nobody can measure is a claim.
    pub fn len(&self) -> usize {
        self.compiled.lock().expect("cache lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Compile if needed, then hand back a clone of the AST.
    ///
    /// ★ A clone rather than a borrow: holding the lock across an evaluation
    /// would serialise every gate check in the process behind one mutex, which
    /// trades a parse for a queue. The AST is small and evaluation is the part
    /// worth doing concurrently.
    pub fn compile(&self, expr: &str) -> Result<Predicate, SyntaxError> {
        let mut map = self.compiled.lock().expect("cache lock");
        if let Some(known) = map.get(expr) {
            return known.clone();
        }
        let result = parse_predicate(expr);
        map.insert(expr.to_string(), result.clone());
        result
    }

    /// Evaluate an expression against state — the drop-in for
    /// [`super::check`], answering identically and parsing at most once.
    pub fn check(
        &self,
        expr: &str,
        state: &Value,
        params: &Map<String, Value>,
    ) -> Result<(bool, String), SyntaxError> {
        Ok(super::eval::evaluate(&self.compile(expr)?, state, params))
    }

    /// Compile a whole declared set up front, reporting what would not parse.
    ///
    /// ★★★ **This is the point of the cache, not a convenience on it.** A rule
    /// that cannot be read is a fault in the household's own definition, and it
    /// should be found when the definition is loaded — once, by whoever can fix
    /// it — rather than re-discovered on every tick by a surface that can only
    /// display it. Returns `(id, error)` for each rule that will never hold.
    pub fn warm<'a>(
        &self,
        rules: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Vec<(String, SyntaxError)> {
        rules
            .into_iter()
            .filter_map(|(id, expr)| match self.compile(expr) {
                Ok(_) => None,
                Err(e) => Some((id.to_string(), e)),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn state() -> Value {
        json!({ "finances": { "liquid": { "balance": 1200.0 } },
                "pockets": [ { "allocated": 5.0 }, { "allocated": 0.0 } ] })
    }

    const OK_EXPR: &str = "finances.liquid.balance >= 0";
    const BAD_EXPR: &str = "this is (not ) valid";

    #[test]
    fn it_answers_exactly_what_check_answers() {
        // ★★★ The property that makes it safe to substitute. A cache that is
        //     faster and disagrees is not a cache, it is a second evaluator --
        //     and this row exists because two evaluators once disagreed.
        let cache = PredicateCache::new();
        let params = Map::new();
        for expr in [
            OK_EXPR,
            "finances.liquid.balance > 5000",
            "ALL pockets[*].allocated >= 0",
            "SUM(pockets[*].allocated) > 1",
        ] {
            let direct = super::super::check(expr, &state(), &params).expect("parses");
            let cached = cache.check(expr, &state(), &params).expect("parses");
            assert_eq!(direct, cached, "disagreement on {expr}");
        }
    }

    #[test]
    fn the_same_rule_is_compiled_once_however_often_it_is_asked() {
        // ★★ The measurable claim. `World::constraints` runs on every §III
        //    pulse, so this is the difference between parsing a household's
        //    rules once and parsing them forever.
        let cache = PredicateCache::new();
        let params = Map::new();
        for _ in 0..50 {
            cache.check(OK_EXPR, &state(), &params).expect("parses");
        }
        assert_eq!(cache.len(), 1, "fifty evaluations, one compile");
    }

    #[test]
    fn distinct_rules_are_distinct_entries() {
        let cache = PredicateCache::new();
        let params = Map::new();
        cache.check(OK_EXPR, &state(), &params).expect("parses");
        cache.check("finances.liquid.balance > 5000", &state(), &params).expect("parses");
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn a_rule_that_does_not_parse_says_so_every_time_and_is_tried_once() {
        // ★★ The error is cached, and it is still an error. Remembering a
        //    failure must never turn it into a silent success -- the caller
        //    gets the same SyntaxError it would have got from `check`.
        let cache = PredicateCache::new();
        let params = Map::new();
        let direct = super::super::check(BAD_EXPR, &state(), &params);
        for _ in 0..5 {
            let cached = cache.check(BAD_EXPR, &state(), &params);
            assert!(cached.is_err(), "an unparseable rule must not become readable");
            assert_eq!(
                format!("{:?}", cached.unwrap_err()),
                format!("{:?}", direct.as_ref().unwrap_err()),
                "the cached error must be the parser's own",
            );
        }
        assert_eq!(cache.len(), 1, "a rule that cannot parse is not retried");
    }

    #[test]
    fn warming_finds_the_broken_rules_when_the_definition_loads() {
        // ★★★ The correctness half. A household with an unreadable rule should
        //     learn that when the rule is declared -- once, by someone who can
        //     fix it -- not on every tick from a surface that can only show it.
        let cache = PredicateCache::new();
        let broken = cache.warm(vec![
            ("liquid_non_negative", OK_EXPR),
            ("nonsense", BAD_EXPR),
            ("pockets_non_negative", "ALL pockets[*].allocated >= 0"),
        ]);
        assert_eq!(broken.len(), 1, "exactly one rule is unreadable");
        assert_eq!(broken[0].0, "nonsense", "and it is named");
        assert_eq!(cache.len(), 3, "all three were attempted, not just the good ones");
    }

    #[test]
    fn warming_a_sound_definition_reports_nothing() {
        // ★ Empty means every rule can be read -- not that nothing was checked.
        let cache = PredicateCache::new();
        assert!(cache.warm(vec![("a", OK_EXPR)]).is_empty());
    }
}
