//! **`resolve(name) → holder`, and one interface over two backends**
//! (Additions · ADD-3).
//!
//! ```text
//!   registry   O(1) lookup, O(n) state at the registry     10²–10⁴
//!   overlay    O(log n) hops, O(log n) state per node      10⁵–10⁶
//! ```
//!
//! ★★★ **The correct design is not registry-or-DHT, it is a hybrid behind one
//! resolution interface** — so the backend changes without touching a single
//! caller. That makes the *interface* the deliverable and the backend an
//! implementation detail, which is the only arrangement in which the DHT can
//! genuinely be a drop-in later.
//!
//! ★★★ **A mock that is indistinguishable from the real thing is how a mock
//! ships.** The overlay is mocked today, and every answer it serves is
//! **labelled as mocked** — because the failure mode here is not the mock being
//! wrong, it is somebody reading a green result and believing the DHT works.
//! [`Resolved::is_trustworthy`] exists so that belief has to be checked rather
//! than assumed.
//!
//! ## What a failure has to distinguish
//!
//! ★★★ **"Not found" conflates two different facts**: nobody holds this name,
//! and the thing that would have known is unreachable. A caller told the first
//! stops looking; a caller told the second retries. So [`Unresolved`] names
//! which happened and **whether the other backend was tried**, because a lookup
//! that failed at the registry and never reached the overlay has not actually
//! answered the question.
//!
//! ## The threshold is a trigger, not a number
//!
//! ★★ The crossover is not "switch at 10⁴". It is *the registry's O(n) state
//! becoming the binding constraint* — [`should_consider_overlay`] reports the
//! condition rather than a constant, so the decision is made on the reason
//! rather than on a number somebody once wrote down.

use std::collections::BTreeMap;

/// Which backend answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// A registry or index. O(1) lookup, O(n) state held centrally.
    Registry,
    /// ★★★ A Kademlia-style overlay. **Mocked today** — the flag rides on every
    /// answer rather than living in a config nobody reads at the call site.
    Overlay { mocked: bool },
}

impl Backend {
    pub fn describe(&self) -> &'static str {
        match self {
            Self::Registry => "the registry",
            Self::Overlay { mocked: true } => "a MOCKED overlay — this is not a real DHT answer",
            Self::Overlay { mocked: false } => "the overlay",
        }
    }
}

/// A name resolved to whoever holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub name: String,
    pub holder: String,
    pub via: Backend,
}

impl Resolved {
    /// ★★★ May this answer be relied on?
    ///
    /// A mocked overlay answer is a *shape*, not a fact. Returning it without
    /// this distinction is how a system ships believing it has a DHT.
    pub fn is_trustworthy(&self) -> bool {
        !matches!(self.via, Backend::Overlay { mocked: true })
    }

    pub fn describe(&self) -> String {
        format!("{} is held by {}, via {}", self.name, self.holder, self.via.describe())
    }
}

/// Why a name did not resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unresolved {
    /// ★★★ Every backend that could have known was asked, and none holds it.
    /// **A caller told this can stop looking.**
    NobodyHoldsIt { asked: Vec<&'static str> },
    /// ★★★ The thing that would have known could not be reached. **A caller
    /// told this should retry** — and conflating it with the above is how a
    /// transient outage becomes a permanent "no such name".
    Unreachable { backend: &'static str, other_tried: bool },
}

impl Unresolved {
    pub fn worth_retrying(&self) -> bool {
        matches!(self, Self::Unreachable { .. })
    }

    pub fn describe(&self) -> String {
        match self {
            Self::NobodyHoldsIt { asked } => {
                format!("nobody holds it — asked {}", asked.join(" and "))
            }
            Self::Unreachable { backend, other_tried: true } => format!(
                "{backend} could not be reached and the other backend did not have it either — \
                 this may still exist"
            ),
            Self::Unreachable { backend, other_tried: false } => format!(
                "{backend} could not be reached and nothing else was asked — this question has \
                 not actually been answered"
            ),
        }
    }
}

/// A registry: fast, central, and O(n) in what it must hold.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    entries: BTreeMap<String, String>,
    reachable: bool,
}

impl Registry {
    pub fn new() -> Self {
        Self { entries: BTreeMap::new(), reachable: true }
    }

    pub fn holding(mut self, name: &str, holder: &str) -> Self {
        self.entries.insert(name.into(), holder.into());
        self
    }

    pub fn unreachable(mut self) -> Self {
        self.reachable = false;
        self
    }

    pub fn size(&self) -> usize {
        self.entries.len()
    }
}

/// The overlay, mocked.
///
/// ★★ It answers from a map like the registry does, and that is exactly why
/// every answer it gives is stamped `mocked`. A mock whose answers are shaped
/// like real ones is useful; a mock whose answers are *labelled* like real ones
/// is a trap.
#[derive(Debug, Clone, Default)]
pub struct MockOverlay {
    entries: BTreeMap<String, String>,
}

impl MockOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn holding(mut self, name: &str, holder: &str) -> Self {
        self.entries.insert(name.into(), holder.into());
        self
    }
}

/// **`resolve(name) → holder`** — the one interface.
///
/// ★★★ A caller passes a name. It does not choose a backend, and there is no
/// parameter through which it could — which is what makes the DHT a drop-in
/// rather than a migration.
pub fn resolve(
    name: &str,
    registry: &Registry,
    overlay: Option<&MockOverlay>,
) -> Result<Resolved, Unresolved> {
    if registry.reachable {
        if let Some(holder) = registry.entries.get(name) {
            return Ok(Resolved {
                name: name.into(),
                holder: holder.clone(),
                via: Backend::Registry,
            });
        }
    }
    if let Some(o) = overlay {
        if let Some(holder) = o.entries.get(name) {
            return Ok(Resolved {
                name: name.into(),
                holder: holder.clone(),
                via: Backend::Overlay { mocked: true },
            });
        }
    }
    if !registry.reachable {
        return Err(Unresolved::Unreachable {
            backend: "the registry",
            other_tried: overlay.is_some(),
        });
    }
    let asked = if overlay.is_some() {
        vec!["the registry", "the overlay"]
    } else {
        vec!["the registry"]
    };
    Err(Unresolved::NobodyHoldsIt { asked })
}

/// **Is it time to think about the overlay?**
///
/// ★★★ Reports the *condition*, not a constant. The crossover is not "switch at
/// ten thousand" — it is the registry's O(n) state becoming the binding
/// constraint, and a decision made on the reason survives a change in hardware
/// where a decision made on a number does not.
pub fn should_consider_overlay(registry: &Registry, state_the_registry_can_hold: usize) -> bool {
    registry.size() > state_the_registry_can_hold
}

/// What each backend costs, so the trade is legible rather than folkloric.
pub fn cost_shape(backend: Backend) -> (&'static str, &'static str) {
    match backend {
        Backend::Registry => ("O(1) lookup", "O(n) state, held in one place"),
        Backend::Overlay { .. } => ("O(log n) hops", "O(log n) state per node"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> Registry {
        Registry::new().holding("the Gachiengu household", "node-a")
    }

    #[test]
    fn a_caller_passes_a_name_and_cannot_choose_a_backend() {
        // ★★★ There is no parameter through which a caller could pick one,
        //     which is what makes the DHT a drop-in rather than a migration.
        let r = resolve("the Gachiengu household", &reg(), None).expect("resolved");
        assert_eq!(r.holder, "node-a");
        assert_eq!(r.via, Backend::Registry);
    }

    #[test]
    fn a_mocked_answer_is_labelled_because_that_is_how_a_mock_ships() {
        // ★★★ The failure mode is not the mock being wrong — it is somebody
        //     reading a green result and believing the DHT works.
        let overlay = MockOverlay::new().holding("a far-off household", "node-z");
        let r = resolve("a far-off household", &reg(), Some(&overlay)).expect("resolved");
        assert!(!r.is_trustworthy());
        assert!(r.describe().contains("not a real DHT answer"));
    }

    #[test]
    fn a_registry_answer_is_trustworthy_and_a_mocked_one_is_not() {
        let overlay = MockOverlay::new().holding("elsewhere", "node-z");
        assert!(resolve("the Gachiengu household", &reg(), Some(&overlay))
            .expect("resolved")
            .is_trustworthy());
        assert!(!resolve("elsewhere", &reg(), Some(&overlay)).expect("resolved").is_trustworthy());
    }

    #[test]
    fn nobody_holds_it_and_nobody_could_be_reached_are_different_answers() {
        // ★★★ A caller told the first stops looking; one told the second
        //     retries. Conflating them turns a transient outage into a
        //     permanent "no such name".
        let absent = resolve("a name nobody registered", &reg(), None).expect_err("no");
        assert!(!absent.worth_retrying());

        let down = resolve("the Gachiengu household", &reg().unreachable(), None).expect_err("no");
        assert!(down.worth_retrying());
    }

    #[test]
    fn a_failure_says_whether_the_other_backend_was_tried() {
        // ★★★ A lookup that failed at the registry and never reached the
        //     overlay has not actually answered the question.
        let alone = resolve("x", &reg().unreachable(), None).expect_err("no");
        assert!(alone.describe().contains("has not actually been answered"));

        let overlay = MockOverlay::new();
        let both = resolve("x", &reg().unreachable(), Some(&overlay)).expect_err("no");
        assert!(both.describe().contains("this may still exist"));
    }

    #[test]
    fn a_not_found_names_what_was_asked() {
        // ★★ "Not found" with no list of who was asked is an answer nobody can
        //    check.
        let overlay = MockOverlay::new();
        match resolve("nowhere", &reg(), Some(&overlay)).expect_err("no") {
            Unresolved::NobodyHoldsIt { asked } => assert_eq!(asked.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_registry_answers_first_because_it_is_the_fast_common_case() {
        // ★★ Both hold the name; the O(1) one wins, and the answer says which
        //    served it.
        let overlay = MockOverlay::new().holding("the Gachiengu household", "node-z");
        let r = resolve("the Gachiengu household", &reg(), Some(&overlay)).expect("resolved");
        assert_eq!(r.holder, "node-a");
        assert_eq!(r.via, Backend::Registry);
    }

    #[test]
    fn the_overlay_still_answers_when_the_registry_is_down() {
        // ★★ Which is the resilience half of the hybrid, and the reason it is a
        //    hybrid rather than a migration.
        let overlay = MockOverlay::new().holding("the Gachiengu household", "node-z");
        let r = resolve("the Gachiengu household", &reg().unreachable(), Some(&overlay))
            .expect("resolved");
        assert_eq!(r.holder, "node-z");
        assert!(!r.is_trustworthy(), "and it is still labelled");
    }

    #[test]
    fn the_threshold_is_a_condition_rather_than_a_constant() {
        // ★★★ Not "switch at ten thousand" — the registry's O(n) state becoming
        //     the binding constraint. A decision made on the reason survives a
        //     change in hardware; one made on a number does not.
        let small = Registry::new().holding("a", "1").holding("b", "2");
        assert!(!should_consider_overlay(&small, 1_000));
        assert!(should_consider_overlay(&small, 1));
    }

    #[test]
    fn the_two_cost_shapes_are_legible_rather_than_folkloric() {
        // ★★ The trade is the reason for the hybrid, so it is written down
        //    where somebody choosing can read it.
        assert_eq!(cost_shape(Backend::Registry).0, "O(1) lookup");
        assert_eq!(cost_shape(Backend::Overlay { mocked: true }).1, "O(log n) state per node");
    }
}
