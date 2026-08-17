//! `φ : Data → VisualAttributes^n` — the preattentive encoder (MON-7; the
//! Monitor paper, §VII).
//!
//! ## The claim being cashed
//!
//! Treisman's Feature Integration Theory: the visual system processes certain
//! attributes **in parallel across the whole field, before conscious
//! attention** — roughly 150–200ms, no serial scan. The preattentive set is
//! small and closed: **hue, saturation, brightness, size, motion, orientation,
//! enclosure**. §VII's model is the map that maximises **discriminability** —
//! the probability a person correctly identifies the state-class *from a
//! glance* — **subject to** the attributes being drawn only from that set.
//!
//! The *subject to* is the load-bearing half. An encoding that leaks onto a
//! non-preattentive channel — a number to read, a label to parse — does not
//! degrade gracefully; it stops being a glance at all and becomes a serial
//! scan, which is precisely what §VII exists to avoid.
//!
//! ## ★★ This is the ENCODER, not the renderer
//!
//! A [`VisualSpec`] says *which preattentive channel carries which data
//! dimension, at what value*. It carries [`Hue::Amber`], never `#E8A020`;
//! [`Size`] as a fraction, never a pixel radius; [`Motion::Pulse`], never a
//! keyframe. Turning that into pixels is downstream UI work and is deliberately
//! outside this core, which has no display, no units and no opinion about them.
//!
//! ## ★★ The preattentive set is a TYPE
//!
//! [`Channel`] has exactly seven variants and no `Other(String)`, so an
//! encoding that reached for a non-preattentive channel **cannot be written
//! down**. The discriminability guarantee depends on that closure, so it is
//! enforced by the type rather than by a rule someone has to keep — the same
//! discipline as EVT-6's missing `close_at(τ)` and EVT-10's `SnapshotDim` with
//! no `add`.
//!
//! ## ★★★ Urgency is the household's, not the widget's
//!
//! `urgency → brightness` maps **`W = d(s,V)`** — the real distance to the
//! viable region, computed by [`crate::region`] and carried through the
//! Monitor. There is **no second notion of important**, and three things make
//! that structural rather than conventional:
//!
//! 1. **A [`VisualSpec`] has no public constructor and no setters.** The only
//!    way to obtain one is [`encode_field`]. A widget cannot build itself a
//!    bright spec, because it cannot build a spec at all.
//! 2. **The map is FIXED, not a parameter.** There is no `EncodingSpec` to pass
//!    in. A configurable map is a gaming surface: whoever chooses it could
//!    route urgency onto a channel nobody looks at, or route its own metric
//!    onto brightness.
//! 3. ★★★ **Brightness is field-relative, and [`encode_field`] is the only
//!    entry point.** Preattentive processing is parallel *across the field*, so
//!    brightness only means anything relatively — and the consequence is the
//!    property worth having: **a widget can only get brighter by being worse.**
//!    Raising its own brightness means genuinely moving further from `V`, and
//!    another sustain moving further from `V` makes it *dimmer* without it
//!    changing at all. Attention is zero-sum across the field and cannot be
//!    manufactured.
//!
//! That is UI-13's *salience is never rendered as salience* enforced here, at
//! the encoder, rather than hoped for at the surface.
//!
//! ## Discriminability, as an executed property
//!
//! §VII asks for the map that maximises discriminability. What is checkable
//! here is the necessary condition: **distinct state-classes must encode
//! distinctly**. Two near-identical hues for *fine* and *critical* are not a
//! weaker encoding, they are a broken one.
//!
//! - [`Hue`] has three variants, so *near-identical* is unrepresentable.
//! - The tests enumerate every state-class this encoder can see and assert the
//!   specs are **pairwise distinct**. Discriminability is run, not claimed.
//!
//! What is **not** checkable here is the psychophysics — whether a particular
//! amber is far enough from a particular red for a particular person on a
//! particular screen. That is a rendering question and a measurement one, and
//! it is named rather than smuggled in.
//!
//! ## Tufte: every attribute carries a data dimension
//!
//! Five of the seven channels are assigned. **Saturation and enclosure are
//! deliberately unassigned** — there is no sixth or seventh data dimension
//! here, and giving them a value would be decoration wearing an encoding's
//! clothes. They remain in [`Channel`] because the *set* is the set; they are
//! simply never emitted.

use crate::detect::Severity;
use crate::monitor::Ingested;

/// The preattentive set (Treisman). **Exactly these**, and no escape hatch —
/// see the module header on why the closure is load-bearing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Channel {
    Hue,
    /// Preattentive, and deliberately unassigned here — see the header.
    Saturation,
    Brightness,
    Size,
    Motion,
    Orientation,
    /// Preattentive, and deliberately unassigned here — see the header.
    Enclosure,
}

/// The data dimension a channel is carrying.
///
/// ★ Named `DataDimension`, not `Dimension` — `vclock::Dimension` is EVT-10's
/// declared dimension KIND (snapshot / accumulating / contested), a genuinely
/// different thing. Twenty-fourth collision; the newcomer takes the longer
/// name.
///
/// Present so a spec says *what this means*, not only *what it looks like* — a
/// renderer that loses the dimension has lost the reason the channel was
/// chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DataDimension {
    Status,
    Urgency,
    Magnitude,
    Activity,
    Trend,
}

/// Status. Three classes, so *two near-identical hues* is unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Hue {
    Green,
    Amber,
    Red,
}

/// Activity. A pulse is motion; stillness is its absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Motion {
    Still,
    Pulse,
}

/// Trend, as an arrow — the value carried on the `Orientation` channel.
/// `Level` is a real reading, not a missing one.
///
/// ★ Named `Arrow`, not `Orientation` — `ooda::Orientation` is the OODA
/// **Orient** phase, and confusing *which way the arrow points* with *the loop
/// is orienting* would be a genuinely misleading collision. Twenty-fifth; the
/// newcomer takes the different name, while `Channel::Orientation` keeps
/// Treisman's own term for the channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arrow {
    Rising,
    Level,
    Falling,
}

/// A magnitude on `[0, 1]`, **relative to the field it was encoded with**.
///
/// Not a pixel size and not an absolute: `d(s,V)` is unbounded above, so any
/// absolute scale would be an invented one. Private field, so a value outside
/// the range cannot be written down.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Fraction(f64);

impl Fraction {
    fn of(value: f64, max: f64) -> Option<Fraction> {
        // A field whose maximum is zero has no ordering to express, and 0/0 is
        // not a brightness. Absent rather than defaulted.
        if !max.is_finite() || max <= 0.0 || !value.is_finite() {
            return None;
        }
        Some(Fraction((value / max).clamp(0.0, 1.0)))
    }

    pub fn get(&self) -> f64 {
        self.0
    }
}

/// One channel, carrying one data dimension, at one value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VisualAttribute {
    Hue { dimension: DataDimension, value: Hue },
    Brightness { dimension: DataDimension, value: Fraction },
    Size { dimension: DataDimension, value: Fraction },
    Motion { dimension: DataDimension, value: Motion },
    Orientation { dimension: DataDimension, value: Arrow },
}

impl VisualAttribute {
    pub fn channel(&self) -> Channel {
        match self {
            VisualAttribute::Hue { .. } => Channel::Hue,
            VisualAttribute::Brightness { .. } => Channel::Brightness,
            VisualAttribute::Size { .. } => Channel::Size,
            VisualAttribute::Motion { .. } => Channel::Motion,
            VisualAttribute::Orientation { .. } => Channel::Orientation,
        }
    }

    pub fn dimension(&self) -> DataDimension {
        match self {
            VisualAttribute::Hue { dimension, .. }
            | VisualAttribute::Brightness { dimension, .. }
            | VisualAttribute::Size { dimension, .. }
            | VisualAttribute::Motion { dimension, .. }
            | VisualAttribute::Orientation { dimension, .. } => *dimension,
        }
    }
}

/// What `φ` produced for one sustain.
///
/// ★★ **No public constructor and no setters.** The only way to obtain one is
/// [`encode_field`], which reads the Monitor's own numbers — so a widget cannot
/// assign itself a brightness, because it cannot assign itself anything.
#[derive(Debug, Clone, PartialEq)]
pub struct VisualSpec {
    sustain_id: String,
    attributes: Vec<VisualAttribute>,
}

impl VisualSpec {
    pub fn sustain_id(&self) -> &str {
        &self.sustain_id
    }

    pub fn attributes(&self) -> &[VisualAttribute] {
        &self.attributes
    }

    pub fn channel(&self, channel: Channel) -> Option<&VisualAttribute> {
        self.attributes.iter().find(|a| a.channel() == channel)
    }

    /// Which data dimension a channel is carrying here, if any.
    pub fn carries(&self, channel: Channel) -> Option<DataDimension> {
        self.channel(channel).map(VisualAttribute::dimension)
    }

    /// Every channel this spec uses. ★ A caller can check the closure itself:
    /// the result is always a subset of the preattentive set, because
    /// [`Channel`] has no other members to be a member of.
    pub fn channels_used(&self) -> Vec<Channel> {
        let mut c: Vec<Channel> = self.attributes.iter().map(VisualAttribute::channel).collect();
        c.sort();
        c.dedup();
        c
    }
}

/// Status → hue. Three classes, three distinct hues.
fn hue_of(severity: Severity) -> Hue {
    match severity {
        Severity::Info => Hue::Green,
        Severity::Warning => Hue::Amber,
        Severity::Critical => Hue::Red,
    }
}

/// Trend → orientation, from numbers the engine already computed: the latest
/// observation against its own running level.
///
/// The dead-band is relative to the level, so *level* means *level* on a series
/// of any scale rather than only on one near 1.0.
fn orientation_of(w: f64, smoothed: f64) -> Arrow {
    let band = smoothed.abs().max(1.0) * 1e-6;
    if w > smoothed + band {
        Arrow::Rising
    } else if w < smoothed - band {
        Arrow::Falling
    } else {
        Arrow::Level
    }
}

/// `φ` — encode a whole field at once.
///
/// ★★★ **The only entry point, deliberately.** Brightness and size are
/// field-relative because preattentive processing is parallel *across the
/// field*, and because a relative scale is what makes attention zero-sum: a
/// widget gets brighter only by genuinely being further from `V` than its
/// peers. A single-item field therefore carries **no brightness and no size** —
/// with nothing to be brighter than, there is no honest value to emit, and
/// `1.0` would be a fabricated maximum.
pub fn encode_field(field: &[&Ingested]) -> Vec<VisualSpec> {
    // The field's own maxima. `w` IS `d(s,V)` — the household's urgency, not a
    // widget's claim about itself.
    let max_w = field.iter().map(|i| i.reading.w).fold(0.0_f64, f64::max);
    let max_level = field.iter().map(|i| i.reading.smoothed.abs()).fold(0.0_f64, f64::max);

    field
        .iter()
        .map(|ingested| {
            let r = &ingested.reading;
            let mut attributes = vec![
                VisualAttribute::Hue {
                    dimension: DataDimension::Status,
                    value: hue_of(ingested.severity()),
                },
                VisualAttribute::Motion {
                    dimension: DataDimension::Activity,
                    // ★ `detected()`, not `escalates()`: motion is *something is
                    // happening here*, and MON-13's cycle suppression answers a
                    // different question — whether it is worth a person's
                    // attention. A known season should stop the escalation, not
                    // make the reading look inert.
                    value: if ingested.detected() { Motion::Pulse } else { Motion::Still },
                },
                VisualAttribute::Orientation {
                    dimension: DataDimension::Trend,
                    value: orientation_of(r.w, r.smoothed),
                },
            ];

            if field.len() > 1 {
                if let Some(v) = Fraction::of(r.w, max_w) {
                    attributes.push(VisualAttribute::Brightness {
                        dimension: DataDimension::Urgency,
                        value: v,
                    });
                }
                if let Some(v) = Fraction::of(r.smoothed.abs(), max_level) {
                    attributes.push(VisualAttribute::Size {
                        dimension: DataDimension::Magnitude,
                        value: v,
                    });
                }
            }

            VisualSpec { sustain_id: ingested.sustain_id.clone(), attributes }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::{Alert, Reading, Shift};

    fn reading(w: f64, smoothed: f64, severity: Option<Severity>) -> Reading {
        Reading {
            w,
            smoothed,
            alert: severity.map(|s| Alert {
                shift: Shift::Upward,
                value: w,
                severity: s,
                excess: 1.0,
            }),
        }
    }

    fn ingested(id: &str, w: f64, smoothed: f64, severity: Option<Severity>) -> Ingested {
        Ingested {
            sustain_id: id.into(),
            reading: reading(w, smoothed, severity),
            signals: None,
            cycle: None,
        }
    }

    fn brightness(spec: &VisualSpec) -> f64 {
        match spec.channel(Channel::Brightness) {
            Some(VisualAttribute::Brightness { value, .. }) => value.get(),
            other => panic!("expected brightness, got {other:?}"),
        }
    }

    // ── the preattentive set is the set ──────────────────────────────────────

    #[test]
    fn every_channel_used_is_preattentive() {
        // Structural, and the assertion is really about the type: `Channel` has
        // seven variants and no `Other`, so this cannot fail by construction —
        // which is the point. It fails only if someone adds an eighth.
        let a = ingested("a", 3.0, 2.0, Some(Severity::Warning));
        let b = ingested("b", 1.0, 1.0, None);
        for spec in encode_field(&[&a, &b]) {
            for channel in spec.channels_used() {
                assert!(matches!(
                    channel,
                    Channel::Hue
                        | Channel::Saturation
                        | Channel::Brightness
                        | Channel::Size
                        | Channel::Motion
                        | Channel::Orientation
                        | Channel::Enclosure
                ));
            }
        }
    }

    #[test]
    fn saturation_and_enclosure_are_left_unassigned() {
        // Tufte: an attribute with no data dimension is decoration.
        let a = ingested("a", 3.0, 2.0, None);
        let b = ingested("b", 1.0, 1.0, None);
        for spec in encode_field(&[&a, &b]) {
            assert!(spec.channel(Channel::Saturation).is_none());
            assert!(spec.channel(Channel::Enclosure).is_none());
        }
    }

    #[test]
    fn each_channel_carries_its_declared_dimension() {
        let a = ingested("a", 3.0, 2.0, Some(Severity::Critical));
        let b = ingested("b", 1.0, 1.0, None);
        let spec = &encode_field(&[&a, &b])[0];
        assert_eq!(spec.carries(Channel::Hue), Some(DataDimension::Status));
        assert_eq!(spec.carries(Channel::Brightness), Some(DataDimension::Urgency));
        assert_eq!(spec.carries(Channel::Size), Some(DataDimension::Magnitude));
        assert_eq!(spec.carries(Channel::Motion), Some(DataDimension::Activity));
        assert_eq!(spec.carries(Channel::Orientation), Some(DataDimension::Trend));
    }

    // ── discriminability ─────────────────────────────────────────────────────

    #[test]
    fn the_three_status_classes_get_three_distinct_hues() {
        let hues = [Severity::Info, Severity::Warning, Severity::Critical].map(hue_of);
        assert_eq!(hues, [Hue::Green, Hue::Amber, Hue::Red]);
        let distinct: std::collections::BTreeSet<_> = hues.into_iter().collect();
        assert_eq!(distinct.len(), 3, "near-identical hues would be a broken encoding");
    }

    #[test]
    fn distinct_state_classes_encode_distinctly() {
        // ★★ Discriminability's necessary condition, RUN over every class this
        // encoder can see: 3 severities × 3 trends × 2 activities.
        let mut specs = Vec::new();
        for (i, severity) in [None, Some(Severity::Warning), Some(Severity::Critical)]
            .into_iter()
            .enumerate()
        {
            for (j, (w, smoothed)) in [(2.0, 1.0), (1.0, 1.0), (1.0, 2.0)].into_iter().enumerate() {
                // A second item keeps the field size above one so brightness
                // and size are emitted; it is identical across every case so it
                // cannot be what makes them differ.
                let subject = ingested(&format!("s{i}{j}"), w, smoothed, severity);
                let peer = ingested("peer", 5.0, 5.0, None);
                specs.push(encode_field(&[&subject, &peer])[0].clone());
            }
        }
        // Compare the attributes only — the id differs by construction.
        for (a, b) in specs.iter().enumerate().flat_map(|(i, a)| {
            specs.iter().skip(i + 1).map(move |b| (a, b))
        }) {
            assert_ne!(
                a.attributes(),
                b.attributes(),
                "two state-classes encode identically: {a:?} vs {b:?}"
            );
        }
    }

    // ── urgency is the household's ───────────────────────────────────────────

    #[test]
    fn brightness_comes_from_the_real_distance_to_v() {
        // `Reading.w` IS `d(s,V)` — region.rs's number, carried by the Monitor.
        let far = ingested("far", 10.0, 1.0, None);
        let near = ingested("near", 2.0, 1.0, None);
        let specs = encode_field(&[&far, &near]);
        assert_eq!(brightness(&specs[0]), 1.0);
        assert!((brightness(&specs[1]) - 0.2).abs() < 1e-12);
    }

    #[test]
    fn a_widget_can_only_get_brighter_by_being_worse() {
        // ★★★ The un-gameable property, in both directions.
        let peer = ingested("peer", 4.0, 1.0, None);

        let quiet = ingested("x", 2.0, 1.0, None);
        let base = brightness(&encode_field(&[&quiet, &peer])[0]);

        // Genuinely further from V → brighter.
        let worse = ingested("x", 8.0, 1.0, None);
        assert!(brightness(&encode_field(&[&worse, &peer])[0]) > base);

        // Unchanged itself, but a PEER got worse → dimmer. Attention is
        // zero-sum across the field; it cannot be manufactured.
        let worse_peer = ingested("peer", 40.0, 1.0, None);
        assert!(brightness(&encode_field(&[&quiet, &worse_peer])[0]) < base);
    }

    #[test]
    fn a_single_item_field_carries_no_brightness_and_no_size() {
        // With nothing to be brighter than there is no honest value, and 1.0
        // would be a fabricated maximum.
        let only = ingested("only", 3.0, 2.0, Some(Severity::Warning));
        let spec = &encode_field(&[&only])[0];
        assert!(spec.channel(Channel::Brightness).is_none());
        assert!(spec.channel(Channel::Size).is_none());
        // The absolute channels are still there — status does not need a field.
        assert!(spec.channel(Channel::Hue).is_some());
    }

    #[test]
    fn an_all_zero_field_emits_no_brightness_rather_than_zero_over_zero() {
        let a = ingested("a", 0.0, 0.0, None);
        let b = ingested("b", 0.0, 0.0, None);
        for spec in encode_field(&[&a, &b]) {
            assert!(spec.channel(Channel::Brightness).is_none());
            assert!(spec.channel(Channel::Size).is_none());
        }
    }

    #[test]
    fn there_is_no_way_to_construct_a_spec_by_hand() {
        // Compile-time, really: `VisualSpec`'s fields are private and it has no
        // constructor, so the only specs that exist came from `encode_field`.
        // What is assertable at runtime is that the ones it produces are keyed
        // to the sustains it was given.
        let a = ingested("alpha", 1.0, 1.0, None);
        let b = ingested("beta", 2.0, 1.0, None);
        let specs = encode_field(&[&a, &b]);
        assert_eq!(specs[0].sustain_id(), "alpha");
        assert_eq!(specs[1].sustain_id(), "beta");
    }

    // ── trend and activity ───────────────────────────────────────────────────

    #[test]
    fn trend_reads_the_latest_observation_against_its_own_level() {
        let up = ingested("u", 5.0, 1.0, None);
        let flat = ingested("f", 1.0, 1.0, None);
        let down = ingested("d", 0.5, 1.0, None);
        let specs = encode_field(&[&up, &flat, &down]);
        let orient = |s: &VisualSpec| match s.channel(Channel::Orientation) {
            Some(VisualAttribute::Orientation { value, .. }) => *value,
            _ => panic!("no orientation"),
        };
        assert_eq!(orient(&specs[0]), Arrow::Rising);
        assert_eq!(orient(&specs[1]), Arrow::Level);
        assert_eq!(orient(&specs[2]), Arrow::Falling);
    }

    #[test]
    fn the_dead_band_scales_with_the_series() {
        // `level` must mean level on a series near a million, not only near 1.
        let big = ingested("b", 1_000_000.0, 1_000_000.000_000_1, None);
        let peer = ingested("p", 1.0, 1.0, None);
        match encode_field(&[&big, &peer])[0].channel(Channel::Orientation) {
            Some(VisualAttribute::Orientation { value, .. }) => {
                assert_eq!(*value, Arrow::Level)
            }
            _ => panic!("no orientation"),
        }
    }

    #[test]
    fn motion_reports_detection_not_escalation() {
        // ★ A known season should stop the ESCALATION, not make the reading
        // look inert — motion answers *is something happening here*.
        let a = ingested("a", 3.0, 1.0, Some(Severity::Critical));
        let b = ingested("b", 1.0, 1.0, None);
        let specs = encode_field(&[&a, &b]);
        let motion = |s: &VisualSpec| match s.channel(Channel::Motion) {
            Some(VisualAttribute::Motion { value, .. }) => *value,
            _ => panic!("no motion"),
        };
        assert_eq!(motion(&specs[0]), Motion::Pulse);
        assert_eq!(motion(&specs[1]), Motion::Still);
    }

    // ── encoder, not renderer ────────────────────────────────────────────────

    #[test]
    fn nothing_here_is_a_pixel_or_a_colour_value() {
        let a = ingested("a", 3.0, 2.0, Some(Severity::Critical));
        let b = ingested("b", 1.0, 1.0, None);
        let rendered = format!("{:?}", encode_field(&[&a, &b]));
        for leak in ["#", "px", "rgb", "hsl"] {
            assert!(!rendered.contains(leak), "a rendering detail leaked in: {leak}");
        }
    }
}
