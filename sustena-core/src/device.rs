//! **A device is a Sustain** (Ingest · §IX).
//!
//! ```text
//!   Σ_dev = ⟨ connectivity, battery, queue_depth, t_last_ack ⟩
//! ```
//!
//! ★★★ **Not a table with a `last_seen` column.** A capture device has state, it
//! has a region it is supposed to stay inside, and it goes out of that region in
//! ways that matter — so it is the same kind of object as a household, and
//! modelling it as one means staleness is an **ordinary invariant** rather than
//! a special-cased alarm somebody wrote by hand next to the ingest code. The
//! machinery that already watches a household watches the phone.
//!
//! ## The liveness invariant is evaluated by the RECEIVER
//!
//! ★★★ **A dead device cannot report that it is dead**, which is the whole
//! reason this is subtle. Every other dimension here is *self*-reported — the
//! phone says what its battery is — and self-reporting is exactly what stops
//! when the thing fails. So `t_last_ack` is written by the **receiving** Sustain
//! from its own clock, and the staleness reading is taken there. A device that
//! has been off for a week reports nothing at all, and nothing is precisely what
//! a healthy quiet day looks like from the same seat.
//!
//! ★★★ [`Reading::from_the_device`] therefore **refuses** to answer about
//! liveness. It is a type-level statement that a self-report cannot establish
//! its own aliveness, rather than a rule somebody has to remember.
//!
//! ## Nothing is inferred
//!
//! ★★ A device that never reported its battery has an **unknown** battery, not a
//! full one and not an empty one. The dimensions are `Option`, and a missing
//! reading stays missing all the way to the region check.
//!
//! ★★★ **Which makes the region UNMEASURABLE, and that is the honest answer
//! rather than an error to swallow.** A first draft of this module claimed a
//! missing dimension "simply is not bounded" and a test caught it: `Region`
//! refuses to measure a distance it has no number for, correctly. A device that
//! has stopped reporting its battery is not a healthy device — it is a device
//! nobody can currently judge, which is a third reading and worth its own name.
//! [`Health::Unmeasurable`] carries which dimensions went missing, so the answer
//! is *go and look at the battery*, not *fine* and not *broken*.

use serde_json::{json, Value};

use crate::liveness::{liveness, Heartbeat, Liveness};
use crate::outbox::{queue_depth, Pending};
use crate::region::{Interval, Region};

/// What the device itself said about itself.
///
/// ★★ Every field is optional. A device that reported only its queue has told
/// the truth about its queue and nothing about its battery, and filling the gap
/// with a plausible number is inventing a measurement.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelfReport {
    /// 0.0–1.0, as the device measured it.
    pub battery: Option<f64>,
    /// Whether it believed it had a connection when it last spoke.
    pub connected: Option<bool>,
}

/// `Σ_dev` — one capture device, as a Sustain.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub id: String,
    pub reported: SelfReport,
    /// ★★★ Written by the RECEIVER from its own clock, never by the device.
    pub t_last_ack: Option<i64>,
    /// What it declared about how often it would speak, if anything.
    pub heartbeat: Option<Heartbeat>,
    /// Gaps the receiver has observed between contacts.
    pub observed_gaps_ms: Vec<i64>,
    /// What is still waiting to be delivered from this device.
    pub pending: Vec<Pending>,
}

impl Device {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.into(),
            reported: SelfReport::default(),
            t_last_ack: None,
            heartbeat: None,
            observed_gaps_ms: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// **The device's state, in the ordinary shape.**
    ///
    /// ★★ A plain `Value` with declared dimensions, so `Region`, the predicate
    /// evaluator and everything else that reads a Sustain's state read this one
    /// without knowing it is a phone.
    pub fn state(&self) -> Value {
        let mut s = json!({ "queue_depth": queue_depth(&self.pending) as f64 });
        // Absent dimensions are absent, not defaulted — see the module docs.
        if let Some(b) = self.reported.battery {
            s["battery"] = json!(b);
        }
        if let Some(c) = self.reported.connected {
            s["connected"] = json!(c);
        }
        if let Some(t) = self.t_last_ack {
            s["t_last_ack"] = json!(t);
        }
        s
    }

    /// How long since the receiver last heard anything.
    pub fn gap_ms(&self, now: i64) -> Option<i64> {
        self.t_last_ack.map(|t| now - t)
    }
}

/// **A device's viable region, as ordinary intervals.**
///
/// ★★★ `queue_depth ≤ max` is a plain invariant, not alarm code. The leading
/// health metric (ING-10) becomes a bound like any other, which means the
/// existing distance-to-region, Lyapunov and monitoring machinery all apply to a
/// phone without a line being written for phones.
///
/// ★★ Battery is bounded **below** only. There is no such thing as too much
/// charge, and a two-sided interval would report a fully charged phone as
/// drifting toward an edge.
pub fn region(max_queue: f64, min_battery: f64) -> Region {
    Region::new()
        .bounding(Interval::at_most("queue_depth", max_queue))
        .bounding(Interval::at_least("battery", min_battery))
        .weighing("queue_depth", 1.0)
        .weighing("battery", 1.0)
}

/// How the device sits against its region.
#[derive(Debug, Clone, PartialEq)]
pub enum Health {
    /// Inside its region.
    Fine,
    /// Outside, by this much.
    Drifting { distance: f64 },
    /// ★★★ **A dimension the region bounds was never reported.**
    ///
    /// Neither fine nor drifting. A device that stopped reporting its battery is
    /// one nobody can currently judge, and calling that healthy is how a silent
    /// failure reads as a quiet week.
    Unmeasurable { missing: String },
}

impl Health {
    pub fn describe(&self, id: &str) -> String {
        match self {
            Self::Fine => format!("{id} is within bounds"),
            Self::Drifting { distance } => format!("{id} is outside its bounds by {distance:.2}"),
            Self::Unmeasurable { missing } => {
                format!("{id} has not reported {missing}, so it cannot be judged either way")
            }
        }
    }
}

/// **Measure a device against its region.**
///
/// ★★ Errors that are not about a missing reading are still errors — a bad
/// interval is a mistake in the region, not a fact about the device, and
/// dressing it up as `Unmeasurable` would hide a declaration bug behind a phone.
pub fn health(device: &Device, region: &Region) -> Result<Health, crate::region::RegionError> {
    match region.distance(&device.state()) {
        Ok(d) if d.is_zero() => Ok(Health::Fine),
        Ok(d) => Ok(Health::Drifting { distance: d.weighted }),
        Err(crate::region::RegionError::NotANumber { dim }) => {
            Ok(Health::Unmeasurable { missing: dim })
        }
        Err(e) => Err(e),
    }
}

/// Who is asking, and therefore what they are entitled to conclude.
#[derive(Debug, Clone, PartialEq)]
pub enum Reading {
    /// ★★★ The device speaking about itself. **Cannot establish liveness.**
    ///
    /// A type-level statement, not a rule to remember: there is no liveness
    /// field on this variant to read.
    FromTheDevice { reported: SelfReport },
    /// The receiving Sustain, which has its own clock and its own silence.
    FromTheReceiver { liveness: Liveness, queue_depth: usize },
}

impl Reading {
    /// What the device can say about itself — which is everything except
    /// whether it is still there.
    pub fn from_the_device(device: &Device) -> Reading {
        Reading::FromTheDevice { reported: device.reported.clone() }
    }

    /// **What the receiver can say**, which is the only seat liveness can be
    /// read from.
    ///
    /// ★★ A device that has never been heard from is `Unknown`, not stale: never
    /// having spoken and having stopped speaking are different, and only one of
    /// them is a fault.
    pub fn from_the_receiver(device: &Device, now: i64, percentile: f64) -> Reading {
        let live = match device.gap_ms(now) {
            None => Liveness::Unknown {
                why: format!("{} has never reported in", device.id),
            },
            Some(gap) => {
                liveness(gap, device.heartbeat, &device.observed_gaps_ms, percentile)
            }
        };
        Reading::FromTheReceiver { liveness: live, queue_depth: queue_depth(&device.pending) }
    }

    /// Is this device stale? `None` when the asker is not in a position to know.
    ///
    /// ★★★ Returns `Option`, and the device's own reading returns `None` — a
    /// caller cannot accidentally read a self-report as a liveness answer.
    pub fn is_stale(&self) -> Option<bool> {
        match self {
            Self::FromTheDevice { .. } => None,
            Self::FromTheReceiver { liveness, .. } => Some(liveness.is_stale()),
        }
    }

    pub fn describe(&self, id: &str) -> String {
        match self {
            Self::FromTheDevice { reported } => {
                let battery = reported
                    .battery
                    .map(|b| format!("{:.0}% battery", b * 100.0))
                    .unwrap_or_else(|| "battery unknown".into());
                format!("{id} says: {battery} — it cannot tell you whether it is still there")
            }
            Self::FromTheReceiver { liveness, queue_depth } => {
                format!("{} · {queue_depth} waiting to be delivered", liveness.describe(id))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;

    fn gaps(hours: &[i64]) -> Vec<i64> {
        hours.iter().map(|h| h * HOUR).collect()
    }

    fn phone() -> Device {
        let mut d = Device::new("bonnie's phone");
        d.reported = SelfReport { battery: Some(0.42), connected: Some(true) };
        d.t_last_ack = Some(100 * HOUR);
        d.observed_gaps_ms = gaps(&[1, 1, 2, 1, 2, 1]);
        d
    }

    #[test]
    fn a_device_is_read_by_the_same_machinery_that_reads_a_household() {
        // ★★★ Not a table with a last_seen column. Its state is an ordinary
        //     state and its region is ordinary intervals, so distance-to-region
        //     works on a phone without a line written for phones.
        let r = region(10.0, 0.15);
        let d = r.distance(&phone().state()).expect("measurable");
        assert!(d.is_zero(), "a healthy phone is inside its region");
    }

    #[test]
    fn a_full_queue_puts_the_device_outside_its_region_like_any_other_breach() {
        // ★★ queue_depth is the leading health metric, and here it is a bound
        //    like any other rather than alarm code beside the ingest path.
        let mut d = phone();
        d.pending = (0..25).map(|i| Pending::new(&format!("m{i}"))).collect();
        let dist = region(10.0, 0.15).distance(&d.state()).expect("measurable");
        assert!(!dist.is_zero());
    }

    #[test]
    fn the_device_cannot_tell_you_whether_it_is_still_there() {
        // ★★★ A type-level statement rather than a rule to remember: there is
        //     no liveness on this variant to read, and `is_stale` says `None`.
        let r = Reading::from_the_device(&phone());
        assert_eq!(r.is_stale(), None);
        assert!(r.describe("bonnie's phone").contains("cannot tell you whether it is still there"));
    }

    #[test]
    fn the_receiver_can_because_it_has_its_own_clock_and_its_own_silence() {
        // ★★★ Self-reporting is exactly what stops when the thing fails. The
        //     seat that can see the absence is the one that must judge it.
        let r = Reading::from_the_receiver(&phone(), 148 * HOUR, 0.9);
        assert_eq!(r.is_stale(), Some(true));
        assert!(r.describe("bonnie's phone").contains("longer than it usually goes"));
    }

    #[test]
    fn a_phone_that_reported_recently_is_not_stale() {
        let r = Reading::from_the_receiver(&phone(), 101 * HOUR, 0.9);
        assert_eq!(r.is_stale(), Some(false));
    }

    #[test]
    fn never_having_spoken_is_not_the_same_as_having_stopped() {
        // ★★ Only one of them is a fault, and a device that has just been
        //    registered should not raise one.
        let fresh = Device::new("a phone nobody has set up yet");
        let r = Reading::from_the_receiver(&fresh, 500 * HOUR, 0.9);
        assert_eq!(r.is_stale(), Some(false), "unknown is not stale");
        assert!(r.describe("it").contains("never reported in"));
    }

    #[test]
    fn a_battery_nobody_reported_is_unknown_and_not_empty() {
        // ★★ Filling the gap with a plausible number is inventing a
        //    measurement — an invented empty battery is a false alarm, and an
        //    invented full one hides a real problem.
        let bare = Device::new("a quiet phone");
        assert!(bare.state().get("battery").is_none());
    }

    #[test]
    fn a_device_that_stopped_reporting_is_unjudgeable_rather_than_healthy() {
        // ★★★ Found by a test, not by reading: a first draft claimed a missing
        //     dimension "simply is not bounded". `Region` refuses to measure a
        //     distance it has no number for, correctly — and calling that
        //     healthy is exactly how a silent failure reads as a quiet week.
        let bare = Device::new("a quiet phone");
        match health(&bare, &region(10.0, 0.15)).expect("a reading") {
            Health::Unmeasurable { missing } => assert_eq!(missing, "battery"),
            other => panic!("must not read as fine: {other:?}"),
        }
    }

    #[test]
    fn a_healthy_phone_reads_as_fine_and_a_full_queue_as_drifting() {
        assert_eq!(health(&phone(), &region(10.0, 0.15)).expect("read"), Health::Fine);
        let mut d = phone();
        d.pending = (0..25).map(|i| Pending::new(&format!("m{i}"))).collect();
        assert!(matches!(
            health(&d, &region(10.0, 0.15)).expect("read"),
            Health::Drifting { .. }
        ));
    }

    #[test]
    fn a_mistake_in_the_region_is_still_a_mistake_and_not_a_fact_about_the_phone() {
        // ★★ Dressing a bad declaration up as `Unmeasurable` would hide a
        //    region bug behind a device. Only a MISSING reading is a fact about
        //    the phone; an unparseable relation is a mistake in the region.
        let broken = Region::new().relating("this is not a predicate at all");
        assert!(health(&phone(), &broken).is_err());
    }

    #[test]
    fn a_declared_heartbeat_governs_over_the_observed_gaps() {
        // ★★ A promise beats an estimate — the same rule `liveness.rs` sets,
        //    reached through the device rather than restated.
        let mut d = phone();
        d.heartbeat = Some(Heartbeat::new(24 * HOUR, HOUR));
        let r = Reading::from_the_receiver(&d, 110 * HOUR, 0.9);
        assert_eq!(r.is_stale(), Some(false), "ten hours is inside a daily promise");
    }

    #[test]
    fn the_queue_travels_with_the_liveness_reading() {
        // ★★ "It has not reported for two days and has nine things waiting" is
        //    a different problem from "it has not reported and had nothing to
        //    say", and one line should be able to tell them apart.
        let mut d = phone();
        d.pending = (0..9).map(|i| Pending::new(&format!("m{i}"))).collect();
        match Reading::from_the_receiver(&d, 148 * HOUR, 0.9) {
            Reading::FromTheReceiver { queue_depth, .. } => assert_eq!(queue_depth, 9),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn battery_is_bounded_below_only() {
        // ★★ There is no such thing as too much charge, and a two-sided
        //    interval would report a fully charged phone as drifting.
        let mut d = phone();
        d.reported.battery = Some(1.0);
        assert!(region(10.0, 0.15).distance(&d.state()).expect("measurable").is_zero());
    }
}
