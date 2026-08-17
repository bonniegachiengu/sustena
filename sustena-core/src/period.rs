//! Periods: half-open windows from an `RRULE` anchored to a zone id
//! (RECORD §VIII).
//!
//! ```text
//!   DTSTART;TZID=Africa/Nairobi:20260801T000000
//!   RRULE:FREQ=MONTHLY;BYMONTHDAY=1
//!     → [2026-08-01T00:00+03:00, 2026-09-01T00:00+03:00), …
//! ```
//!
//! §VIII: `[a,b)` says *which events belong*, `W ≥ b` says *when we may
//! answer*. EVT-6 built the second and the bare interval; this builds the
//! first, and the two halves meet in [`Period`], which wraps EVT-6's
//! [`Window`] rather than inventing a second interval type.
//!
//! ## ★★ Store the rule; recompute the expansion
//!
//! *"Recurrence is a rule, and the expansion of that rule into instants is a
//! derivation — so the rule is what should be stored, and the expansion
//! recomputed."*
//!
//! [`Recurrence`] is the stored object and it holds **no instants**: there is
//! no field of expanded periods and no constructor that accepts a list of
//! them. [`Recurrence::expand`] is a pure function of *(rule, tz data)*, and
//! the reason that matters is not tidiness — **the tz database is versioned**.
//! The same rule expanded under two tzdata releases can produce different
//! instants, and a stored expansion would silently keep the older answer while
//! everything around it moved. A test runs exactly that: one rule, two
//! providers, two answers.
//!
//! ## ★★ Anchor to a zone id, never a bare offset
//!
//! §VIII's reason is *not* daylight saving. `Africa/Nairobi` is EAT, UTC+03:00,
//! and observes none — the local DST case is vacuous. The reason to insist on
//! the zone id anyway is that **the tz database is itself a versioned,
//! revisable record**, so a zone id is a reference *into* that record while an
//! offset is a frozen copy of one reading from it.
//!
//! So the two are not the same kind of thing here, and are not typed as one.
//! [`Anchor::Zone`] resolves through the host's tz data and every period it
//! produces is stamped with the [`tzdata_version`](TzProvider::tzdata_version)
//! that derived it. [`Anchor::FixedOffset`] is kept — data really does arrive
//! carrying only `+03:00`, and refusing it outright would be dishonest about
//! that — but as **a distinct lesser thing**: it never consults the provider,
//! and its periods carry `tzdata_version: None`, so a period whose derivation
//! cannot be traced to a tz release is always distinguishable from one that
//! can. The DST machinery is still exercised, by a counterparty in a
//! DST-observing zone, since that is the only way to test the machinery Nairobi
//! makes vacuous.
//!
//! ## ★ Where the tz data comes from, and why it is injected
//!
//! ADR-0001 keeps clocks and I/O out of the core; identity and time come from
//! the host. Tz data is supplied the same way, through [`TzProvider`] — and
//! **§VIII's own argument is what makes that the right design rather than
//! merely the compliant one**. If the core bundled a tz database it would pin
//! one release invisibly, which is precisely the versioning the section warns
//! about. Supplied, the release is nameable, and every derived period records
//! which one produced it.
//!
//! The line is drawn where the versioning is:
//!
//! - **Civil-calendar arithmetic is a fixed algorithm**, not data. Leap years
//!   and month lengths do not get revised, so the Gregorian conversion here is
//!   computed rather than fetched.
//! - **Timezone offsets are versioned data.** Those are never computed here;
//!   they are asked for.
//!
//! ## ★ Half-open, and the partition is structural
//!
//! Each period's `end` **is** the next period's `start` — the expansion emits
//! `[occ[i], occ[i+1])`, so a gap or an overlap is not something the code
//! avoids, it is something the construction cannot express. What that buys is
//! Dijkstra's point (EWD831) at the place it actually matters: a year is the
//! sum of its twelve months, **not the months plus twelve boundary instants**.
//! An event landing exactly on a month boundary is contained in exactly one
//! period, which is asserted over every boundary rather than argued.
//!
//! ## One primitive, many consumers
//!
//! Budgets, meal plans, routines and Monitor's windows read this, rather than
//! each hard-coding its own calendar — the same build-once-share-once argument
//! §4E made for aggregates. Concretely: a [`Period`] hands out an EVT-6
//! [`Window`], so a consumer that already knows how to close a window on a
//! watermark needs nothing new.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::watermark::{Lateness, Window};

// ── civil calendar arithmetic (a fixed algorithm, not versioned data) ───────

/// A local date and time with **no zone attached** — what `DTSTART` carries
/// before `TZID` is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CivilDateTime {
    pub year: i64,
    /// 1..=12
    pub month: u32,
    /// 1..=31
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl CivilDateTime {
    pub fn new(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> Self {
        Self { year, month, day, hour, minute, second }
    }

    /// Midnight on a date.
    pub fn date(year: i64, month: u32, day: u32) -> Self {
        Self::new(year, month, day, 0, 0, 0)
    }

    fn is_valid(&self) -> bool {
        self.month >= 1
            && self.month <= 12
            && self.day >= 1
            && self.day <= days_in_month(self.year, self.month)
            && self.hour < 24
            && self.minute < 60
            && self.second < 60
    }

    /// Milliseconds from the civil epoch, **as if UTC** — the offset is applied
    /// separately, because that is the part that is versioned data.
    fn as_naive_millis(&self) -> i64 {
        let days = days_from_civil(self.year, self.month, self.day);
        let secs = self.hour as i64 * 3600 + self.minute as i64 * 60 + self.second as i64;
        (days * 86_400 + secs) * 1000
    }

    fn weekday(&self) -> Weekday {
        Weekday::from_days(days_from_civil(self.year, self.month, self.day))
    }

    fn plus_days(&self, n: i64) -> Self {
        let (year, month, day) =
            civil_from_days(days_from_civil(self.year, self.month, self.day) + n);
        Self { year, month, day, ..*self }
    }

    /// Add months, keeping the day-of-month. Returns `None` when the target
    /// month has no such day — RFC-5545 skips invalid dates rather than
    /// clamping them, and clamping 31 January to 28 February would silently
    /// invent an occurrence.
    fn plus_months_same_day(&self, n: i64, day: u32) -> Option<Self> {
        let total = self.year * 12 + (self.month as i64 - 1) + n;
        let year = total.div_euclid(12);
        let month = (total.rem_euclid(12) + 1) as u32;
        if day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Self { year, month, day, ..*self })
    }
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The inverse of [`days_from_civil`].
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// RFC-5545's `BYDAY` weekdays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Weekday {
    Mo,
    Tu,
    We,
    Th,
    Fr,
    Sa,
    Su,
}

impl Weekday {
    /// 1970-01-01 was a Thursday.
    fn from_days(z: i64) -> Self {
        match (z + 3).rem_euclid(7) {
            0 => Weekday::Mo,
            1 => Weekday::Tu,
            2 => Weekday::We,
            3 => Weekday::Th,
            4 => Weekday::Fr,
            5 => Weekday::Sa,
            _ => Weekday::Su,
        }
    }

    fn index(self) -> i64 {
        match self {
            Weekday::Mo => 0,
            Weekday::Tu => 1,
            Weekday::We => 2,
            Weekday::Th => 3,
            Weekday::Fr => 4,
            Weekday::Sa => 5,
            Weekday::Su => 6,
        }
    }
}

// ── tz data, supplied by the host ──────────────────────────────────────────

/// What a local wall-clock time means in a zone.
///
/// Three answers, because a local time genuinely has three cases and treating
/// the awkward two as the first is how DST bugs are made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalResolution {
    /// Exactly one instant. Offset in milliseconds east of UTC.
    Unambiguous { offset_ms: i64 },
    /// ★ Spring forward: this local time **does not exist**. `gap_ms` is how
    /// much was skipped; the offsets on either side are given so a policy can
    /// choose without guessing.
    Nonexistent { gap_ms: i64, before_ms: i64, after_ms: i64 },
    /// ★ Fall back: this local time happens **twice**.
    Ambiguous { earlier_ms: i64, later_ms: i64 },
}

/// Why a zone could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TzError {
    #[error("the supplied tz data ({version}) does not know the zone '{tzid}'")]
    UnknownZone { tzid: String, version: String },
}

/// The host's tz database.
///
/// ★ Injected rather than bundled, for §VIII's own reason: a bundled database
/// pins a release invisibly, and the whole point of anchoring to a zone id is
/// that the release is a thing that can change. Supplied, it can be named — and
/// every period this module derives records which version derived it.
pub trait TzProvider {
    /// Which tz database release this is. Recorded on every derived period.
    fn tzdata_version(&self) -> String;

    /// What does this local wall-clock time mean in this zone?
    fn offset_at(&self, tzid: &str, local: CivilDateTime) -> Result<LocalResolution, TzError>;
}

/// How to resolve a local time that does not exist or happens twice.
///
/// **Declared, never defaulted** — the type implements no `Default`, the same
/// discipline EVT-6 applies to [`Lateness`]: an awkward case that has not been
/// decided is not the same as one that has been decided quietly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DstPolicy {
    /// Nonexistent → push forward past the gap. Ambiguous → the earlier.
    /// The common civil reading: an 02:30 alarm on a spring-forward morning
    /// rings at 03:30.
    ShiftForward,
    /// Ambiguous → the earlier instant. Nonexistent → refuse.
    UseEarlier,
    /// Ambiguous → the later instant. Nonexistent → refuse.
    UseLater,
    /// ★ Refuse both. For a period whose boundary must be exact, saying *this
    /// local time is not a real instant* beats picking one.
    Refuse,
}

// ── the rule ───────────────────────────────────────────────────────────────

/// `FREQ`. The three shipped here; the rest of the grammar is a named slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
}

/// What `DTSTART` is anchored to.
///
/// ★ Two genuinely different things, kept apart deliberately — see the module
/// docs. A zone id is a reference into a versioned record; an offset is a
/// frozen copy of one reading from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anchor {
    /// `DTSTART;TZID=Africa/Nairobi`. Resolved through the host's tz data, and
    /// the resulting periods carry the version that resolved them.
    Zone { tzid: String },
    /// ★ A bare offset, in minutes east of UTC. **The lesser thing**: it never
    /// consults tz data, so its periods carry no `tzdata_version` and can never
    /// be mistaken for zone-anchored ones.
    FixedOffset { minutes: i32 },
}

/// One expanded period: an EVT-6 window, plus how it was derived.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Period {
    /// ★ EVT-6's window — not a second interval type. A consumer that can
    /// close a window on a watermark needs nothing new.
    pub window: Window,
    pub start_local: CivilDateTime,
    pub end_local: CivilDateTime,
    /// ★ `Some` for a zone-anchored period, `None` for a fixed-offset one, so
    /// a period whose derivation is traceable to a tz release is always
    /// distinguishable from one that is not.
    pub tzdata_version: Option<String>,
}

/// What can go wrong turning a rule into periods.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PeriodError {
    #[error("DTSTART {0:?} is not a real civil date-time")]
    BadDtStart(CivilDateTime),
    #[error("INTERVAL must be at least 1; 0 would repeat the same instant forever")]
    ZeroInterval,
    #[error("BYMONTHDAY={0} is not a day of any month")]
    BadMonthDay(u32),
    #[error(transparent)]
    Tz(#[from] TzError),
    #[error(
        "local time {local:?} does not exist in '{tzid}' (a {gap_ms}ms spring-forward gap) and \
         the declared policy is {policy:?} — refusing rather than choosing an instant for you"
    )]
    NonexistentLocalTime { local: CivilDateTime, tzid: String, gap_ms: i64, policy: DstPolicy },
    #[error(
        "local time {local:?} happens twice in '{tzid}' (a fall-back overlap) and the declared \
         policy is {policy:?} — refusing rather than choosing an instant for you"
    )]
    AmbiguousLocalTime { local: CivilDateTime, tzid: String, policy: DstPolicy },
    #[error(
        "could not find {wanted} occurrence(s) of the rule within {scanned} candidate steps — \
         the rule may select a date that occurs too rarely (e.g. BYMONTHDAY=31 at a long interval)"
    )]
    Exhausted { wanted: usize, scanned: usize },
    #[error(
        "the supplied tz data turned two increasing local times into instants {start} and {end} — \
         a period cannot start after it ends, so the provider is reported rather than worked around"
    )]
    NonMonotonicInstants { start: i64, end: i64 },
}

/// A stored recurrence rule.
///
/// ★★ It holds **no instants**. There is no field of expanded periods and no
/// constructor that takes a list of them, so the article's *store the rule,
/// recompute the expansion* is a property of the type rather than a convention
/// someone has to remember.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recurrence {
    /// `DTSTART`, as a local wall-clock time.
    pub dtstart: CivilDateTime,
    pub anchor: Anchor,
    pub freq: Freq,
    /// `INTERVAL`. 1 = every period.
    pub interval: u32,
    /// `BYMONTHDAY` — `MONTHLY` only. Defaults to `DTSTART`'s day.
    pub by_month_day: Option<u32>,
    /// `BYDAY` — `WEEKLY` only. Defaults to `DTSTART`'s weekday.
    pub by_day: Option<Weekday>,
    /// ★ Declared, because EVT-6 made it mandatory for a window and a period
    /// is a window. Every period this rule expands to carries it.
    pub lateness: Lateness,
    /// ★ Declared, for the same reason.
    pub dst: DstPolicy,
}

impl Recurrence {
    /// The common case: a monthly period starting on a given day.
    pub fn monthly(
        dtstart: CivilDateTime,
        tzid: impl Into<String>,
        by_month_day: u32,
        lateness: Lateness,
        dst: DstPolicy,
    ) -> Self {
        Self {
            dtstart,
            anchor: Anchor::Zone { tzid: tzid.into() },
            freq: Freq::Monthly,
            interval: 1,
            by_month_day: Some(by_month_day),
            by_day: None,
            lateness,
            dst,
        }
    }

    /// The zone id this is anchored to, or `None` for a fixed-offset anchor.
    pub fn tzid(&self) -> Option<&str> {
        match &self.anchor {
            Anchor::Zone { tzid } => Some(tzid),
            Anchor::FixedOffset { .. } => None,
        }
    }

    /// ★★ Derive `count` half-open periods. A pure function of *(rule, tz
    /// data)* — nothing is stored, and calling it twice with the same provider
    /// gives the same answer while calling it with a different tz release may
    /// not. That difference is the reason the expansion is not persisted.
    pub fn expand(
        &self,
        tz: &dyn TzProvider,
        count: usize,
    ) -> Result<Vec<Period>, PeriodError> {
        if !self.dtstart.is_valid() {
            return Err(PeriodError::BadDtStart(self.dtstart));
        }
        if self.interval == 0 {
            return Err(PeriodError::ZeroInterval);
        }
        if let Some(d) = self.by_month_day {
            if d == 0 || d > 31 {
                return Err(PeriodError::BadMonthDay(d));
            }
        }
        if count == 0 {
            return Ok(Vec::new());
        }

        // n periods need n+1 boundaries: each period's end IS the next one's
        // start, which is what makes the partition structural rather than
        // checked.
        let locals = self.occurrences(count + 1)?;
        let version = match &self.anchor {
            Anchor::Zone { .. } => Some(tz.tzdata_version()),
            Anchor::FixedOffset { .. } => None,
        };

        let mut instants = Vec::with_capacity(locals.len());
        for local in &locals {
            instants.push(self.to_instant(*local, tz)?);
        }

        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            // The occurrences are strictly increasing by construction, so this
            // can only fire if the supplied tz data moved an offset enough to
            // invert two of them -- a fact about the provider, reported rather
            // than panicked on.
            let window = Window::new(instants[i], instants[i + 1], self.lateness).map_err(|_| {
                PeriodError::NonMonotonicInstants { start: instants[i], end: instants[i + 1] }
            })?;
            out.push(Period {
                window,
                start_local: locals[i],
                end_local: locals[i + 1],
                tzdata_version: version.clone(),
            });
        }
        Ok(out)
    }

    /// The local wall-clock starts the rule selects, in order.
    fn occurrences(&self, wanted: usize) -> Result<Vec<CivilDateTime>, PeriodError> {
        // Generous, and bounded: a rule that selects a date occurring only
        // rarely (BYMONTHDAY=31 at a long interval) must run out honestly
        // rather than spin.
        let limit = wanted.saturating_mul(64).max(512);
        let mut out = Vec::with_capacity(wanted);

        match self.freq {
            Freq::Daily => {
                let step = self.interval as i64;
                let mut cur = self.dtstart;
                for _ in 0..limit {
                    out.push(cur);
                    if out.len() == wanted {
                        return Ok(out);
                    }
                    cur = cur.plus_days(step);
                }
            }
            Freq::Weekly => {
                let target = self.by_day.unwrap_or_else(|| self.dtstart.weekday());
                let ahead = (target.index() - self.dtstart.weekday().index()).rem_euclid(7);
                let mut cur = self.dtstart.plus_days(ahead);
                let step = 7 * self.interval as i64;
                for _ in 0..limit {
                    out.push(cur);
                    if out.len() == wanted {
                        return Ok(out);
                    }
                    cur = cur.plus_days(step);
                }
            }
            Freq::Monthly => {
                let day = self.by_month_day.unwrap_or(self.dtstart.day);
                let step = self.interval as i64;
                let mut n: i64 = 0;
                // If DTSTART's own month has no such day, or its day is before
                // the selected one, the first occurrence is later — both are
                // handled by scanning forward and skipping.
                for _ in 0..limit {
                    if let Some(c) = self.dtstart.plus_months_same_day(n, day) {
                        if c >= self.dtstart {
                            out.push(c);
                            if out.len() == wanted {
                                return Ok(out);
                            }
                        }
                    }
                    n += step;
                }
            }
        }
        Err(PeriodError::Exhausted { wanted, scanned: limit })
    }

    /// Local wall clock → instant, applying the declared DST policy.
    fn to_instant(&self, local: CivilDateTime, tz: &dyn TzProvider) -> Result<i64, PeriodError> {
        let naive = local.as_naive_millis();
        match &self.anchor {
            // ★ The lesser thing: no tz data is consulted at all.
            Anchor::FixedOffset { minutes } => Ok(naive - (*minutes as i64) * 60_000),
            Anchor::Zone { tzid } => match tz.offset_at(tzid, local)? {
                LocalResolution::Unambiguous { offset_ms } => Ok(naive - offset_ms),
                LocalResolution::Nonexistent { gap_ms, before_ms, .. } => match self.dst {
                    // Read the local time with the PRE-transition offset, which
                    // lands it `gap_ms` past the transition -- the common civil
                    // reading, where an 02:30 alarm on a spring-forward morning
                    // rings at 03:30.
                    DstPolicy::ShiftForward => Ok(naive - before_ms),
                    _ => Err(PeriodError::NonexistentLocalTime {
                        local,
                        tzid: tzid.clone(),
                        gap_ms,
                        policy: self.dst,
                    }),
                },
                LocalResolution::Ambiguous { earlier_ms, later_ms } => match self.dst {
                    DstPolicy::ShiftForward | DstPolicy::UseEarlier => Ok(naive - earlier_ms),
                    DstPolicy::UseLater => Ok(naive - later_ms),
                    DstPolicy::Refuse => Err(PeriodError::AmbiguousLocalTime {
                        local,
                        tzid: tzid.clone(),
                        policy: self.dst,
                    }),
                },
            },
        }
    }
}

/// Do these periods partition the span they cover — no gap, no overlap?
///
/// ★ True by construction for anything [`Recurrence::expand`] returns, since
/// each period's end is the next one's start. Offered because a *consumer*
/// assembling periods from elsewhere has no such guarantee, and because a
/// property worth relying on is worth being able to check.
pub fn partitions(periods: &[Period]) -> bool {
    periods.windows(2).all(|p| p[0].window.end() == p[1].window.start())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR_MS: i64 = 3_600_000;

    // ── the fixture providers ──────────────────────────────────────────────

    /// Africa/Nairobi: EAT, +03:00, **no DST** — §VIII's vacuous local case.
    struct Nairobi(&'static str);

    impl TzProvider for Nairobi {
        fn tzdata_version(&self) -> String {
            self.0.into()
        }
        fn offset_at(&self, tzid: &str, _l: CivilDateTime) -> Result<LocalResolution, TzError> {
            match tzid {
                "Africa/Nairobi" => Ok(LocalResolution::Unambiguous { offset_ms: 3 * HOUR_MS }),
                other => Err(TzError::UnknownZone {
                    tzid: other.into(),
                    version: self.tzdata_version(),
                }),
            }
        }
    }

    /// ★ A DST-observing counterparty, so the machinery Nairobi makes vacuous
    /// is still exercised. Europe/Berlin 2026: spring forward 29 Mar 02:00→03:00,
    /// fall back 25 Oct 03:00→02:00.
    struct Berlin;

    impl TzProvider for Berlin {
        fn tzdata_version(&self) -> String {
            "2026a".into()
        }
        fn offset_at(&self, tzid: &str, l: CivilDateTime) -> Result<LocalResolution, TzError> {
            if tzid != "Europe/Berlin" {
                return Err(TzError::UnknownZone {
                    tzid: tzid.into(),
                    version: self.tzdata_version(),
                });
            }
            // The spring-forward gap: 02:00–02:59 on 29 March does not exist.
            if l.year == 2026 && l.month == 3 && l.day == 29 && l.hour == 2 {
                return Ok(LocalResolution::Nonexistent {
                    gap_ms: HOUR_MS,
                    before_ms: HOUR_MS,
                    after_ms: 2 * HOUR_MS,
                });
            }
            // The fall-back overlap: 02:00–02:59 on 25 October happens twice.
            if l.year == 2026 && l.month == 10 && l.day == 25 && l.hour == 2 {
                return Ok(LocalResolution::Ambiguous {
                    earlier_ms: 2 * HOUR_MS,
                    later_ms: HOUR_MS,
                });
            }
            // Summer runs from the 29 March transition (03:00 local) to the
            // 25 October one (03:00 local). Compared at the HOUR, because a
            // day-granularity predicate would put 29 March 00:00 -- which is
            // still winter -- on the wrong side.
            let at = (l.month, l.day, l.hour);
            let summer = ((3, 29, 3)..(10, 25, 3)).contains(&at);
            Ok(LocalResolution::Unambiguous {
                offset_ms: if summer { 2 * HOUR_MS } else { HOUR_MS },
            })
        }
    }

    /// ★★ A LATER tzdata release in which the zone's offset changed — a real
    /// thing that happens, and the reason an expansion must not be stored.
    struct NairobiRevised;

    impl TzProvider for NairobiRevised {
        fn tzdata_version(&self) -> String {
            "2027b".into()
        }
        fn offset_at(&self, tzid: &str, _l: CivilDateTime) -> Result<LocalResolution, TzError> {
            match tzid {
                "Africa/Nairobi" => Ok(LocalResolution::Unambiguous { offset_ms: 4 * HOUR_MS }),
                other => Err(TzError::UnknownZone {
                    tzid: other.into(),
                    version: self.tzdata_version(),
                }),
            }
        }
    }

    fn monthly_first() -> Recurrence {
        Recurrence::monthly(
            CivilDateTime::date(2026, 8, 1),
            "Africa/Nairobi",
            1,
            Lateness::Drop,
            DstPolicy::ShiftForward,
        )
    }

    // ── civil arithmetic ───────────────────────────────────────────────────

    #[test]
    fn the_civil_conversion_round_trips() {
        for (y, m, d) in [(1970, 1, 1), (2000, 2, 29), (2026, 8, 1), (1899, 12, 31), (2100, 3, 1)] {
            let z = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(z), (y, m, d), "{y}-{m}-{d}");
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn weekdays_are_right_at_a_known_anchor() {
        // 1970-01-01 was a Thursday; 2026-08-01 is a Saturday.
        assert_eq!(Weekday::from_days(0), Weekday::Th);
        assert_eq!(CivilDateTime::date(2026, 8, 1).weekday(), Weekday::Sa);
    }

    #[test]
    fn leap_years_are_the_gregorian_rule_not_a_divide_by_four() {
        assert!(is_leap(2000) && !is_leap(1900) && is_leap(2024) && !is_leap(2026));
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
    }

    // ── ★ the partition ────────────────────────────────────────────────────

    #[test]
    fn a_year_is_the_sum_of_its_twelve_months_and_not_one_instant_more() {
        // ★★ Dijkstra (EWD831) at the place it matters. Twelve half-open
        // months must cover the year exactly: no gap, no overlap, and no
        // boundary instant counted twice.
        let periods = monthly_first().expand(&Nairobi("2026a"), 12).unwrap();
        assert_eq!(periods.len(), 12);

        assert!(partitions(&periods), "each end IS the next start");
        let span = periods[11].window.end() - periods[0].window.start();
        let sum: i64 = periods.iter().map(|p| p.window.end() - p.window.start()).sum();
        assert_eq!(sum, span, "the parts sum to the whole with nothing left over");

        // 2026-08-01 → 2027-08-01 in a non-leap-February year.
        assert_eq!(span, 365 * 24 * HOUR_MS);
        assert_eq!(periods[0].start_local, CivilDateTime::date(2026, 8, 1));
        assert_eq!(periods[11].end_local, CivilDateTime::date(2027, 8, 1));
    }

    #[test]
    fn a_boundary_instant_belongs_to_exactly_one_month() {
        // The concrete form of "not the months plus twelve boundary instants":
        // every boundary is in one period, never two and never none.
        use crate::event::{CausalStamp, Provenance};
        use crate::Event;

        let periods = monthly_first().expand(&Nairobi("2026a"), 12).unwrap();
        for p in &periods {
            for t in [p.window.start(), p.window.end()] {
                let at = Event {
                    id: "b".into(),
                    name: "event.test.boundary".into(),
                    t_event: t,
                    t_ingest: None,
                    provenance: Provenance::Observed,
                    source: None,
                    stamp: CausalStamp::new("n"),
                    causes: vec![],
                    mutations: vec![],
                };
                let hits = periods.iter().filter(|q| q.window.contains(&at)).count();
                // The final end is outside the covered span, which is correct
                // for a half-open partition — it belongs to the 13th month.
                assert!(hits <= 1, "an instant landed in two months at t={t}");
                if t < periods[11].window.end() {
                    assert_eq!(hits, 1, "an instant landed in no month at t={t}");
                }
            }
        }
    }

    // ── ★★ store the rule, recompute the expansion ─────────────────────────

    #[test]
    fn the_same_rule_expands_differently_under_a_different_tzdata_release() {
        // ★★ THE REASON THE EXPANSION IS NOT STORED. One rule, two releases,
        // two answers — a persisted expansion would have kept the older one
        // while everything around it moved.
        let rule = monthly_first();
        let old = rule.expand(&Nairobi("2026a"), 1).unwrap();
        let new = rule.expand(&NairobiRevised, 1).unwrap();

        assert_ne!(old[0].window.start(), new[0].window.start());
        assert_eq!(new[0].window.start(), old[0].window.start() - HOUR_MS);
        assert_eq!(old[0].start_local, new[0].start_local, "the LOCAL time is the same");
        assert_eq!(old[0].tzdata_version.as_deref(), Some("2026a"));
        assert_eq!(new[0].tzdata_version.as_deref(), Some("2027b"));
    }

    #[test]
    fn expansion_is_a_pure_function_of_the_rule_and_the_tz_data() {
        let rule = monthly_first();
        assert_eq!(
            rule.expand(&Nairobi("2026a"), 6).unwrap(),
            rule.expand(&Nairobi("2026a"), 6).unwrap()
        );
    }

    #[test]
    fn the_stored_rule_survives_serialisation_and_holds_no_instants() {
        // ★ The rule is the stored object. Round-tripping it carries the whole
        // recurrence, and the JSON has no period in it — there is no field for
        // one and no constructor that would accept one.
        let rule = monthly_first();
        let json = serde_json::to_string(&rule).unwrap();
        assert!(!json.contains("window"), "no expanded instants ride along");
        let back: Recurrence = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rule);
        assert_eq!(back.expand(&Nairobi("2026a"), 3).unwrap(), rule.expand(&Nairobi("2026a"), 3).unwrap());
    }

    // ── ★★ zone id, not a bare offset ──────────────────────────────────────

    #[test]
    fn a_zone_anchored_period_records_the_tzdata_that_derived_it() {
        let p = monthly_first().expand(&Nairobi("2026a"), 1).unwrap();
        assert_eq!(p[0].tzdata_version.as_deref(), Some("2026a"));
        assert_eq!(monthly_first().tzid(), Some("Africa/Nairobi"));
    }

    #[test]
    fn a_fixed_offset_anchor_is_a_distinct_lesser_thing() {
        // ★ Kept, because data really does arrive with only an offset — but
        // never mistakable for the real thing: it consults no tz data, and its
        // periods carry no version to trace.
        let mut rule = monthly_first();
        rule.anchor = Anchor::FixedOffset { minutes: 180 };

        let p = rule.expand(&Nairobi("2026a"), 1).unwrap();
        assert_eq!(p[0].tzdata_version, None, "untraceable, and it says so");
        assert_eq!(rule.tzid(), None);

        // It agrees with the zone TODAY — and that is exactly the trap: the
        // zone-anchored one would move under a revised release and this would
        // not.
        let zoned = monthly_first().expand(&Nairobi("2026a"), 1).unwrap();
        assert_eq!(p[0].window.start(), zoned[0].window.start());
        let revised = monthly_first().expand(&NairobiRevised, 1).unwrap();
        assert_ne!(p[0].window.start(), revised[0].window.start());
    }

    #[test]
    fn an_unknown_zone_is_refused_by_the_supplied_tz_data() {
        let mut rule = monthly_first();
        rule.anchor = Anchor::Zone { tzid: "Mars/Olympus".into() };
        assert!(matches!(
            rule.expand(&Nairobi("2026a"), 1),
            Err(PeriodError::Tz(TzError::UnknownZone { .. }))
        ));
    }

    // ── ★ the DST machinery, exercised by a DST-observing counterparty ─────

    #[test]
    fn a_dst_zone_period_spans_the_spring_forward_and_is_an_hour_short() {
        // ★ Nairobi's DST case is vacuous, so the machinery is exercised where
        // it is not: the March period in Berlin is 23 hours, not 24.
        let rule = Recurrence {
            dtstart: CivilDateTime::date(2026, 3, 28),
            anchor: Anchor::Zone { tzid: "Europe/Berlin".into() },
            freq: Freq::Daily,
            interval: 1,
            by_month_day: None,
            by_day: None,
            lateness: Lateness::Drop,
            dst: DstPolicy::ShiftForward,
        };
        let days = rule.expand(&Berlin, 3).unwrap();
        assert_eq!(days[0].window.end() - days[0].window.start(), 24 * HOUR_MS, "28-29 March");
        assert_eq!(
            days[1].window.end() - days[1].window.start(),
            23 * HOUR_MS,
            "29-30 March: the day the clocks went forward"
        );
        assert!(partitions(&days), "and it still partitions, short hour and all");
    }

    #[test]
    fn a_dst_zone_period_spans_the_fall_back_and_is_an_hour_long() {
        let rule = Recurrence {
            dtstart: CivilDateTime::date(2026, 10, 24),
            anchor: Anchor::Zone { tzid: "Europe/Berlin".into() },
            freq: Freq::Daily,
            interval: 1,
            by_month_day: None,
            by_day: None,
            lateness: Lateness::Drop,
            dst: DstPolicy::ShiftForward,
        };
        let days = rule.expand(&Berlin, 2).unwrap();
        assert_eq!(days[0].window.end() - days[0].window.start(), 24 * HOUR_MS, "24-25 October");
        assert_eq!(
            days[1].window.end() - days[1].window.start(),
            25 * HOUR_MS,
            "25-26 October: the day the clocks went back"
        );
        assert!(partitions(&days));
    }

    #[test]
    fn a_nonexistent_local_start_is_refused_under_a_policy_that_says_so() {
        let rule = Recurrence {
            dtstart: CivilDateTime::new(2026, 3, 29, 2, 30, 0),
            anchor: Anchor::Zone { tzid: "Europe/Berlin".into() },
            freq: Freq::Daily,
            interval: 1,
            by_month_day: None,
            by_day: None,
            lateness: Lateness::Drop,
            dst: DstPolicy::Refuse,
        };
        assert!(matches!(
            rule.expand(&Berlin, 1),
            Err(PeriodError::NonexistentLocalTime { .. })
        ));
    }

    #[test]
    fn an_ambiguous_local_start_resolves_by_the_declared_policy_not_a_default() {
        // ★ The same local time, three declared policies, three real answers —
        // and `Refuse` is one of them, because for a period boundary that must
        // be exact, saying "this is not one instant" beats picking.
        let base = Recurrence {
            dtstart: CivilDateTime::new(2026, 10, 25, 2, 30, 0),
            anchor: Anchor::Zone { tzid: "Europe/Berlin".into() },
            freq: Freq::Daily,
            interval: 1,
            by_month_day: None,
            by_day: None,
            lateness: Lateness::Drop,
            dst: DstPolicy::UseEarlier,
        };
        let earlier = base.expand(&Berlin, 1).unwrap()[0].window.start();

        let mut later_rule = base.clone();
        later_rule.dst = DstPolicy::UseLater;
        let later = later_rule.expand(&Berlin, 1).unwrap()[0].window.start();
        assert_eq!(later - earlier, HOUR_MS, "genuinely two different instants");

        let mut refusing = base.clone();
        refusing.dst = DstPolicy::Refuse;
        assert!(matches!(
            refusing.expand(&Berlin, 1),
            Err(PeriodError::AmbiguousLocalTime { .. })
        ));
    }

    // ── the rule grammar that IS shipped ───────────────────────────────────

    #[test]
    fn monthly_skips_a_month_that_has_no_such_day_rather_than_clamping() {
        // RFC-5545 skips invalid dates. Clamping 31 January to 28 February
        // would silently invent an occurrence that the rule never selected.
        let rule = Recurrence::monthly(
            CivilDateTime::date(2026, 1, 31),
            "Africa/Nairobi",
            31,
            Lateness::Drop,
            DstPolicy::ShiftForward,
        );
        let p = rule.expand(&Nairobi("2026a"), 3).unwrap();
        assert_eq!(p[0].start_local, CivilDateTime::date(2026, 1, 31));
        assert_eq!(p[1].start_local, CivilDateTime::date(2026, 3, 31), "February is skipped");
        assert_eq!(p[2].start_local, CivilDateTime::date(2026, 5, 31), "and April");
    }

    #[test]
    fn weekly_byday_lands_on_the_declared_weekday() {
        let rule = Recurrence {
            dtstart: CivilDateTime::date(2026, 8, 1), // a Saturday
            anchor: Anchor::Zone { tzid: "Africa/Nairobi".into() },
            freq: Freq::Weekly,
            interval: 1,
            by_month_day: None,
            by_day: Some(Weekday::Mo),
            lateness: Lateness::Drop,
            dst: DstPolicy::ShiftForward,
        };
        let p = rule.expand(&Nairobi("2026a"), 3).unwrap();
        assert_eq!(p[0].start_local, CivilDateTime::date(2026, 8, 3), "the first Monday after");
        for q in &p {
            assert_eq!(q.start_local.weekday(), Weekday::Mo);
            assert_eq!(q.window.end() - q.window.start(), 7 * 24 * HOUR_MS);
        }
    }

    #[test]
    fn interval_spaces_the_periods_out() {
        let mut rule = monthly_first();
        rule.interval = 3;
        let p = rule.expand(&Nairobi("2026a"), 2).unwrap();
        assert_eq!(p[0].start_local, CivilDateTime::date(2026, 8, 1));
        assert_eq!(p[0].end_local, CivilDateTime::date(2026, 11, 1));
        assert!(partitions(&p));
    }

    #[test]
    fn a_zero_interval_is_refused_rather_than_looping() {
        let mut rule = monthly_first();
        rule.interval = 0;
        assert_eq!(rule.expand(&Nairobi("2026a"), 1), Err(PeriodError::ZeroInterval));
    }

    #[test]
    fn a_nonsense_rule_is_refused_at_expansion() {
        let mut rule = monthly_first();
        rule.by_month_day = Some(32);
        assert_eq!(rule.expand(&Nairobi("2026a"), 1), Err(PeriodError::BadMonthDay(32)));

        let mut bad = monthly_first();
        bad.dtstart = CivilDateTime::date(2026, 2, 30);
        assert!(matches!(bad.expand(&Nairobi("2026a"), 1), Err(PeriodError::BadDtStart(_))));
    }

    #[test]
    fn zero_periods_is_an_empty_answer_not_an_error() {
        assert!(monthly_first().expand(&Nairobi("2026a"), 0).unwrap().is_empty());
    }

    // ── one primitive, many consumers ──────────────────────────────────────

    #[test]
    fn a_period_hands_out_an_evt6_window_carrying_the_declared_lateness() {
        // ★ Not a second interval type: a consumer that can close a window on
        // a watermark needs nothing new, and the lateness EVT-6 made mandatory
        // rides through from the rule.
        let mut rule = monthly_first();
        rule.lateness = Lateness::AccumulateAndRetract;
        let p = rule.expand(&Nairobi("2026a"), 1).unwrap();
        assert_eq!(p[0].window.lateness(), Lateness::AccumulateAndRetract);
    }

    #[test]
    fn a_period_closes_only_on_a_watermark_reaching_its_end() {
        use crate::watermark::{SourceGuarantee, Watermark};
        let p = monthly_first().expand(&Nairobi("2026a"), 1).unwrap();
        let w = &p[0].window;

        let short = Watermark::perfect(w.end(), SourceGuarantee::new("mpesa", HOUR_MS));
        assert!(!w.is_closed_by(&short), "the clock reaching the end is not enough");
        let reached = Watermark::perfect(w.end(), SourceGuarantee::new("mpesa", 0));
        assert!(w.is_closed_by(&reached));
    }
}
