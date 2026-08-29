//! **What one spec may take from another, and at which version** (DSL · §VI).
//!
//! ```text
//!   import  finances@v3  bringing  liquid_non_negative, pocket_non_negative
//! ```
//!
//! ★★★ **An import at `head` is a rule you do not control.** Pull somebody
//! else's invariant unpinned and their next edit silently changes what your
//! Sustain refuses — a household discovering on a Tuesday that a payment it
//! could make last month is now blocked, with nothing in *its* history to
//! explain why. So [`enforce`] refuses `Pin::Head`, and the refusal names that
//! consequence rather than saying "unpinned".
//!
//! ★★★ **There is no wildcard import, because the source can grow.** `bringing
//! *` means a declaration added upstream next year appears in your spec without
//! anybody asking, and the first you hear of it is a refusal you cannot account
//! for. [`Import::brings`] is an explicit list; there is no variant that means
//! everything.
//!
//! ★★★ **A collision is refused and never shadowed.** Two imports bringing the
//! same name is not a precedence question — last-wins would mean the *order of
//! the import lines* decides which rule governs somebody's money, which is a
//! rule nobody wrote. Both sources are named so the author can choose.

use std::collections::{BTreeMap, BTreeSet};

/// Which version of a source a spec is reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pin {
    /// A specific version node.
    At { version: String },
    /// ★★★ Whatever the source's head is right now. **Refused by [`enforce`]** —
    /// see the module header.
    Head,
}

impl Pin {
    pub fn is_pinned(&self) -> bool {
        matches!(self, Self::At { .. })
    }
}

/// One spec taking declarations from another.
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    /// The spec being read.
    pub from: String,
    pub pin: Pin,
    /// ★★★ Exactly what travels. There is no "everything".
    pub brings: Vec<String>,
}

impl Import {
    pub fn new(from: &str, pin: Pin) -> Self {
        Self { from: from.into(), pin, brings: Vec::new() }
    }

    pub fn bringing(mut self, name: &str) -> Self {
        self.brings.push(name.to_string());
        self
    }
}

/// Why a set of imports was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// ★★★ Unpinned.
    Unpinned { from: String },
    /// The same name arrives from two places.
    ///
    /// ★★ Both named. "There is a collision on `liquid_non_negative`" leaves
    /// somebody grepping; naming both sources is the whole of the fix.
    Collision { name: String, from: String, also_from: String },
    /// A spec imports something that imports it back.
    Cycle { path: Vec<String> },
    /// An import that brings nothing.
    ///
    /// ★★ Refused rather than ignored: an import line with an empty list is
    /// either a mistake or a leftover, and both are worth a sentence.
    BringsNothing { from: String },
    /// The named source does not exist.
    NoSuchSource { from: String },
}

impl ImportError {
    pub fn describe(&self) -> String {
        match self {
            Self::Unpinned { from } => format!(
                "'{from}' is imported at head, so its author's next edit silently changes what \
                 this Sustain refuses — and nothing in this Sustain's own history would explain \
                 the change"
            ),
            Self::Collision { name, from, also_from } => format!(
                "'{name}' arrives from both '{from}' and '{also_from}' — the order of the import \
                 lines would decide which rule governs, and nobody wrote that rule"
            ),
            Self::Cycle { path } => {
                format!("these specs import each other: {}", path.join(" → "))
            }
            Self::BringsNothing { from } => {
                format!("'{from}' is imported and nothing is taken from it")
            }
            Self::NoSuchSource { from } => format!("there is no spec called '{from}'"),
        }
    }
}

/// What each known spec imports, for cycle detection.
pub type ImportGraph = BTreeMap<String, Vec<Import>>;

/// **Check one spec's imports.**
///
/// ★★ Every problem is reported, not the first. An author who fixes one pin and
/// is then told about a second has been made to do the work twice.
pub fn enforce(spec: &str, imports: &[Import], graph: &ImportGraph) -> Vec<ImportError> {
    let mut errors = Vec::new();
    let mut seen: BTreeMap<&str, &str> = BTreeMap::new();

    for imp in imports {
        if !imp.pin.is_pinned() {
            errors.push(ImportError::Unpinned { from: imp.from.clone() });
        }
        if imp.brings.is_empty() {
            errors.push(ImportError::BringsNothing { from: imp.from.clone() });
        }
        if !graph.contains_key(imp.from.as_str()) {
            errors.push(ImportError::NoSuchSource { from: imp.from.clone() });
        }
        for name in &imp.brings {
            match seen.get(name.as_str()) {
                Some(first) => errors.push(ImportError::Collision {
                    name: name.clone(),
                    from: first.to_string(),
                    also_from: imp.from.clone(),
                }),
                None => {
                    seen.insert(name, &imp.from);
                }
            }
        }
    }

    if let Some(path) = cycle_from(spec, graph) {
        errors.push(ImportError::Cycle { path });
    }
    errors
}

/// The first import cycle reachable from `spec`, if any.
///
/// ★★ Depth-first with the path carried, so the report is the loop itself
/// rather than the bare fact that one exists — *"A imports B imports A"* is
/// fixable and *"there is a cycle"* is a search.
fn cycle_from(spec: &str, graph: &ImportGraph) -> Option<Vec<String>> {
    fn walk(
        at: &str,
        graph: &ImportGraph,
        stack: &mut Vec<String>,
        done: &mut BTreeSet<String>,
    ) -> Option<Vec<String>> {
        if let Some(pos) = stack.iter().position(|s| s == at) {
            let mut loop_path = stack[pos..].to_vec();
            loop_path.push(at.to_string());
            return Some(loop_path);
        }
        if !done.insert(at.to_string()) {
            return None;
        }
        stack.push(at.to_string());
        if let Some(imports) = graph.get(at) {
            for imp in imports {
                if let Some(found) = walk(&imp.from, graph, stack, done) {
                    return Some(found);
                }
            }
        }
        stack.pop();
        None
    }
    walk(spec, graph, &mut Vec::new(), &mut BTreeSet::new())
}

/// Everything that reaches `spec` through its imports, with where each came
/// from.
///
/// ★★★ Only what was explicitly brought, one hop. A transitive import would
/// mean a name arriving from a spec this author has never read, and *"where did
/// this rule come from"* would have no short answer.
pub fn resolved(imports: &[Import]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for imp in imports {
        for name in &imp.brings {
            out.entry(name.clone()).or_insert_with(|| imp.from.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pinned(from: &str) -> Import {
        Import::new(from, Pin::At { version: "v3".into() })
    }

    fn graph(pairs: &[(&str, Vec<Import>)]) -> ImportGraph {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn an_unpinned_import_is_refused_and_the_refusal_says_why_it_matters() {
        // ★★★ Not "unpinned" — a household discovering on a Tuesday that a
        //     payment it could make last month is now blocked, with nothing in
        //     its own history to explain it.
        let g = graph(&[("household", vec![]), ("finances", vec![])]);
        let e = enforce(
            "household",
            &[Import::new("finances", Pin::Head).bringing("liquid_non_negative")],
            &g,
        );
        assert_eq!(e.len(), 1);
        assert!(e[0].describe().contains("nothing in this Sustain's own history"));
    }

    #[test]
    fn a_pinned_import_is_fine() {
        let g = graph(&[("household", vec![]), ("finances", vec![])]);
        assert!(enforce("household", &[pinned("finances").bringing("liquid_non_negative")], &g)
            .is_empty());
    }

    #[test]
    fn there_is_no_wildcard_to_write() {
        // ★★★ `brings` is a list of names and there is no variant meaning
        //     everything — so a declaration added upstream next year cannot
        //     appear in this spec without somebody asking for it.
        let i = pinned("finances").bringing("a").bringing("b");
        assert_eq!(i.brings, vec!["a", "b"]);
    }

    #[test]
    fn a_collision_is_refused_and_names_both_sources() {
        // ★★★ Not a precedence question. Last-wins would mean the ORDER OF THE
        //     IMPORT LINES decides which rule governs somebody's money.
        let g = graph(&[("household", vec![]), ("a", vec![]), ("b", vec![])]);
        let e = enforce(
            "household",
            &[pinned("a").bringing("liquid_non_negative"), pinned("b").bringing("liquid_non_negative")],
            &g,
        );
        assert_eq!(e.len(), 1);
        let said = e[0].describe();
        assert!(said.contains("'a'") && said.contains("'b'"));
        assert!(said.contains("nobody wrote that rule"));
    }

    #[test]
    fn a_cycle_is_reported_as_the_loop_itself() {
        // ★★ "A imports B imports A" is fixable; "there is a cycle" is a search.
        let g = graph(&[
            ("a", vec![pinned("b").bringing("x")]),
            ("b", vec![pinned("a").bringing("y")]),
        ]);
        let e = enforce("a", &[pinned("b").bringing("x")], &g);
        match e.iter().find(|x| matches!(x, ImportError::Cycle { .. })) {
            Some(ImportError::Cycle { path }) => {
                assert!(path.len() >= 3, "{path:?}");
                assert_eq!(path.first(), path.last());
            }
            other => panic!("expected a cycle: {other:?}"),
        }
    }

    #[test]
    fn a_spec_that_imports_itself_is_a_cycle_too() {
        let g = graph(&[("a", vec![pinned("a").bringing("x")])]);
        let e = enforce("a", &[pinned("a").bringing("x")], &g);
        assert!(e.iter().any(|x| matches!(x, ImportError::Cycle { .. })));
    }

    #[test]
    fn a_long_chain_that_does_not_loop_is_fine() {
        let g = graph(&[
            ("a", vec![pinned("b").bringing("x")]),
            ("b", vec![pinned("c").bringing("y")]),
            ("c", vec![]),
        ]);
        let e = enforce("a", &[pinned("b").bringing("x")], &g);
        assert!(e.is_empty(), "{e:?}");
    }

    #[test]
    fn an_import_that_takes_nothing_is_refused_rather_than_ignored() {
        // ★★ Either a mistake or a leftover, and both are worth a sentence.
        let g = graph(&[("household", vec![]), ("finances", vec![])]);
        let e = enforce("household", &[pinned("finances")], &g);
        assert!(e.iter().any(|x| matches!(x, ImportError::BringsNothing { .. })));
    }

    #[test]
    fn importing_a_spec_nobody_has_is_named_rather_than_silently_empty() {
        let g = graph(&[("household", vec![])]);
        let e = enforce("household", &[pinned("a spec nobody wrote").bringing("x")], &g);
        assert!(e.iter().any(|x| matches!(x, ImportError::NoSuchSource { .. })));
    }

    #[test]
    fn every_problem_is_reported_rather_than_the_first() {
        // ★★ An author who fixes one pin and is then told about a second has
        //    been made to do the work twice.
        let g = graph(&[("household", vec![]), ("a", vec![]), ("b", vec![])]);
        let e = enforce(
            "household",
            &[
                Import::new("a", Pin::Head).bringing("x"),
                Import::new("b", Pin::Head).bringing("x"),
            ],
            &g,
        );
        assert!(e.len() >= 3, "two pins and a collision: {e:?}");
    }

    #[test]
    fn what_a_spec_resolved_and_from_where_is_one_lookup() {
        // ★★★ One hop only. A transitive import would mean a name arriving from
        //     a spec this author has never read, and "where did this rule come
        //     from" would have no short answer.
        let r = resolved(&[
            pinned("finances").bringing("liquid_non_negative"),
            pinned("roster").bringing("member_known"),
        ]);
        assert_eq!(r.get("liquid_non_negative").map(String::as_str), Some("finances"));
        assert_eq!(r.len(), 2);
    }
}
