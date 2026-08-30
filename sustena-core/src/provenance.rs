//! **The thread from a device to a decision** (Capstone · §VI.5 — SEAM 5).
//!
//! ```text
//!   device token ──▶ event principal ──▶ approval decision
//! ```
//!
//! > *"Break the thread anywhere and the audit becomes a list of assertions."*
//!
//! ★★★ **The three links exist separately and were never joined.** A device is
//! registered ([`crate::device`]), an event carries a source and a principal
//! ([`crate::event`], [`crate::principal`]), an approval binds a principal to an
//! act ([`crate::approval`]). Each is sound on its own, and *"Bonnie approved
//! this payment"* is still worthless if nobody can say which device reported the
//! transaction he approved it against — because then the thing he approved is
//! only an assertion that a transaction happened.
//!
//! ★★★ **So a broken thread is a named outcome, never a slightly weaker one.**
//! [`Provenance::Broken`] carries **where** it broke, and there is no method that
//! reads a complete-looking answer out of it. A partial thread that could be
//! read as attribution is exactly how an audit becomes a list of assertions
//! while looking like a record.
//!
//! ★★★ **A missing link is not a probable one.** An event with no recorded
//! source is not *probably the phone*; it is unattributed, and everything
//! downstream of that point is unattributed with it. The strength of the thread
//! is the strength of its weakest link, and **the weakest link's identity is the
//! useful output** — "we cannot say which device" is actionable, and a
//! confidence score is not.

use std::collections::BTreeMap;

/// Where a thread came apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Break {
    /// The event records no source, so nothing reported it that we can name.
    NoDevice { event: String },
    /// A source is named but no device by that id is registered.
    ///
    /// ★★ Different from `NoDevice` and worth its own variant: one is a gap in
    /// the record, the other is a record pointing at something that does not
    /// exist, and only the second suggests somebody should go looking.
    UnknownDevice { event: String, source: String },
    /// The device is known but nobody is bound to it.
    DeviceWithoutAPrincipal { device: String },
    /// The event names no principal, so nothing acted as anybody.
    NoPrincipal { event: String },
    /// Nothing approved this, and it needed approving.
    NoApproval { event: String },
    /// An approval exists and was granted by somebody other than the principal
    /// the event acted as.
    ///
    /// ★★★ The one that matters most, because it is the only break that is not
    /// an absence. A thread that runs end to end through two *different* people
    /// looks complete from either end.
    ApprovedBySomebodyElse { event: String, acted_as: String, approved_by: String },
}

impl Break {
    pub fn describe(&self) -> String {
        match self {
            Self::NoDevice { event } => {
                format!("{event} records no source — nothing we can name reported it")
            }
            Self::UnknownDevice { event, source } => format!(
                "{event} says it came from '{source}', and no device by that name is registered"
            ),
            Self::DeviceWithoutAPrincipal { device } => {
                format!("'{device}' is registered and nobody is bound to it")
            }
            Self::NoPrincipal { event } => format!("{event} acted as nobody"),
            Self::NoApproval { event } => format!("{event} was never approved"),
            Self::ApprovedBySomebodyElse { event, acted_as, approved_by } => format!(
                "{event} acted as '{acted_as}' and was approved by '{approved_by}' — the thread \
                 runs end to end through two different people, which looks complete from either \
                 end"
            ),
        }
    }
}

/// What can be said about where one act came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// Every link held.
    Complete { device: String, principal: String, approval: String },
    /// ★★★ It did not. **There is no method that reads a partial attribution
    /// out of this** — a thread that could be half-read is how an audit becomes
    /// a list of assertions while looking like a record.
    Broken { at: Break },
}

impl Provenance {
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete { .. })
    }

    /// Who this act is attributable to. `None` when the thread broke.
    ///
    /// ★★ An `Option`, so an unattributed act cannot be reported as somebody's
    /// by a caller that forgot to check.
    pub fn attributable_to(&self) -> Option<&str> {
        match self {
            Self::Complete { principal, .. } => Some(principal),
            Self::Broken { .. } => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Complete { device, principal, approval } => format!(
                "reported by '{device}', acted as '{principal}', approved as '{approval}'"
            ),
            Self::Broken { at } => format!("unattributed: {}", at.describe()),
        }
    }
}

/// One act, as the audit sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Act {
    pub event: String,
    /// The source id the event recorded, if any.
    pub source: Option<String>,
    /// The principal the event acted as, if any.
    pub acted_as: Option<String>,
    /// The approval that admitted it, and who granted it.
    pub approval: Option<(String, String)>,
    /// Did this act need approving at all?
    ///
    /// ★★ A read needs no approval, and demanding one would make every ordinary
    /// observation look unattributed — which trains people to ignore the report.
    pub needed_approval: bool,
}

impl Act {
    pub fn new(event: &str) -> Self {
        Self {
            event: event.into(),
            source: None,
            acted_as: None,
            approval: None,
            needed_approval: false,
        }
    }

    pub fn reported_by(mut self, source: &str) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn acting_as(mut self, principal: &str) -> Self {
        self.acted_as = Some(principal.into());
        self
    }

    pub fn approved(mut self, approval_id: &str, by: &str) -> Self {
        self.approval = Some((approval_id.into(), by.into()));
        self.needed_approval = true;
        self
    }

    pub fn needing_approval(mut self) -> Self {
        self.needed_approval = true;
        self
    }
}

/// Which principal each registered device is bound to.
pub type DeviceBindings = BTreeMap<String, String>;

/// **Follow the thread, and say where it broke.**
///
/// ★★ Checked in order, device first, because the earliest break makes every
/// later link unattributed anyway — reporting the last one would send somebody
/// to fix a symptom.
pub fn trace(act: &Act, devices: &DeviceBindings) -> Provenance {
    let Some(source) = act.source.as_ref() else {
        return Provenance::Broken { at: Break::NoDevice { event: act.event.clone() } };
    };
    let Some(bound_to) = devices.get(source) else {
        // ★★ A source naming an unregistered device is a different finding from
        //    no source at all: one is a gap, the other points at something that
        //    does not exist, and only the second means go and look.
        return Provenance::Broken {
            at: Break::UnknownDevice { event: act.event.clone(), source: source.clone() },
        };
    };
    if bound_to.is_empty() {
        return Provenance::Broken {
            at: Break::DeviceWithoutAPrincipal { device: source.clone() },
        };
    }
    let Some(acted_as) = act.acted_as.as_ref() else {
        return Provenance::Broken { at: Break::NoPrincipal { event: act.event.clone() } };
    };
    if !act.needed_approval {
        return Provenance::Complete {
            device: source.clone(),
            principal: acted_as.clone(),
            approval: "none needed".into(),
        };
    }
    let Some((approval, approved_by)) = act.approval.as_ref() else {
        return Provenance::Broken { at: Break::NoApproval { event: act.event.clone() } };
    };
    if approved_by != acted_as {
        return Provenance::Broken {
            at: Break::ApprovedBySomebodyElse {
                event: act.event.clone(),
                acted_as: acted_as.clone(),
                approved_by: approved_by.clone(),
            },
        };
    }
    Provenance::Complete {
        device: source.clone(),
        principal: acted_as.clone(),
        approval: approval.clone(),
    }
}

/// Everything the audit cannot attribute, with where each thread broke.
///
/// ★★★ The report SEAM 5 exists to make possible. A list of acts nobody can
/// place is the honest form of "the audit is a list of assertions" — said out
/// loud, with the acts named, rather than discovered later.
pub fn unattributed(acts: &[Act], devices: &DeviceBindings) -> Vec<(String, Break)> {
    acts.iter()
        .filter_map(|a| match trace(a, devices) {
            Provenance::Broken { at } => Some((a.event.clone(), at)),
            Provenance::Complete { .. } => None,
        })
        .collect()
}

/// How much of the log is attributable, as a fraction.
///
/// ★★ One number for "is the thread holding". A slow drift from 0.99 to 0.6 is
/// the shape of a device quietly falling out of registration, and nothing else
/// in the system would notice it.
pub fn attributable_fraction(acts: &[Act], devices: &DeviceBindings) -> Option<f64> {
    (!acts.is_empty()).then(|| {
        acts.iter().filter(|a| trace(a, devices).is_complete()).count() as f64 / acts.len() as f64
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bindings() -> DeviceBindings {
        [("bonnie's phone".to_string(), "bonnie".to_string())].into_iter().collect()
    }

    fn full() -> Act {
        Act::new("e1").reported_by("bonnie's phone").acting_as("bonnie").approved("ap-1", "bonnie")
    }

    #[test]
    fn a_thread_that_holds_end_to_end_attributes_the_act() {
        let p = trace(&full(), &bindings());
        assert!(p.is_complete());
        assert_eq!(p.attributable_to(), Some("bonnie"));
        assert!(p.describe().contains("bonnie's phone"));
    }

    #[test]
    fn an_act_nobody_reported_is_unattributed_and_not_probably_the_phone() {
        // ★★★ A missing link is not a probable one, and everything downstream
        //     of it is unattributed with it.
        let orphan = Act::new("e2").acting_as("bonnie").approved("ap-2", "bonnie");
        let p = trace(&orphan, &bindings());
        assert_eq!(p.attributable_to(), None, "there is no half-attribution to read");
        assert!(matches!(p, Provenance::Broken { at: Break::NoDevice { .. } }));
    }

    #[test]
    fn a_source_naming_a_device_nobody_registered_is_its_own_finding() {
        // ★★ One is a gap in the record; the other is a record pointing at
        //    something that does not exist, and only the second means somebody
        //    should go and look.
        let ghost = full().reported_by("a phone nobody set up");
        match trace(&ghost, &bindings()) {
            Provenance::Broken { at: Break::UnknownDevice { source, .. } } => {
                assert_eq!(source, "a phone nobody set up");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_thread_running_through_two_different_people_is_the_break_that_matters_most() {
        // ★★★ The only break that is not an absence. It looks complete from
        //     either end: the device really did report it, and somebody really
        //     did approve it.
        let crossed = Act::new("e3")
            .reported_by("bonnie's phone")
            .acting_as("bonnie")
            .approved("ap-9", "somebody else");
        let p = trace(&crossed, &bindings());
        assert!(!p.is_complete());
        assert!(p.describe().contains("looks complete from either \nend")
            || p.describe().contains("looks complete from either end"));
    }

    #[test]
    fn a_device_registered_and_bound_to_nobody_breaks_at_the_device() {
        let unbound: DeviceBindings =
            [("a shared tablet".to_string(), String::new())].into_iter().collect();
        let act = full().reported_by("a shared tablet");
        assert!(matches!(
            trace(&act, &unbound),
            Provenance::Broken { at: Break::DeviceWithoutAPrincipal { .. } }
        ));
    }

    #[test]
    fn an_act_that_needed_approving_and_was_not_is_unattributed() {
        let unapproved =
            Act::new("e4").reported_by("bonnie's phone").acting_as("bonnie").needing_approval();
        assert!(matches!(
            trace(&unapproved, &bindings()),
            Provenance::Broken { at: Break::NoApproval { .. } }
        ));
    }

    #[test]
    fn an_ordinary_read_needs_no_approval_and_is_still_fully_attributed() {
        // ★★ Demanding approval for every observation would make ordinary reads
        //    look unattributed, which trains people to ignore the report.
        let read = Act::new("e5").reported_by("bonnie's phone").acting_as("bonnie");
        let p = trace(&read, &bindings());
        assert!(p.is_complete());
        assert!(p.describe().contains("none needed"));
    }

    #[test]
    fn the_earliest_break_is_the_one_reported() {
        // ★★ A later break is a symptom of the earlier one, and sending
        //    somebody to fix the symptom wastes the trip.
        let doubly_broken = Act::new("e6").needing_approval();
        assert!(matches!(
            trace(&doubly_broken, &bindings()),
            Provenance::Broken { at: Break::NoDevice { .. } }
        ));
    }

    #[test]
    fn what_the_audit_cannot_place_is_a_list_it_can_hand_over() {
        // ★★★ The honest form of "the audit is a list of assertions" — said out
        //     loud, with the acts named, rather than discovered later.
        let acts = vec![full(), Act::new("e7").acting_as("bonnie")];
        let missing = unattributed(&acts, &bindings());
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].0, "e7");
    }

    #[test]
    fn whether_the_thread_is_holding_is_one_number() {
        // ★★ A drift from 0.99 to 0.6 is a device quietly falling out of
        //    registration, and nothing else in the system would notice.
        let acts = vec![full(), full(), Act::new("e8")];
        let f = attributable_fraction(&acts, &bindings()).expect("computable");
        assert!((f - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(attributable_fraction(&[], &bindings()), None);
    }

    #[test]
    fn there_is_no_way_to_read_a_principal_out_of_a_broken_thread() {
        // ★★★ The whole discipline of the module in one assertion: a partial
        //     thread cannot be reported as somebody's act by a caller that
        //     forgot to check, because the answer is an Option.
        let broken = trace(&Act::new("e9"), &bindings());
        assert_eq!(broken.attributable_to(), None);
        assert!(broken.describe().starts_with("unattributed"));
    }
}
