//! **The message's own clock** — `t_event` at the boundary (Ingest · §VIII).
//!
//! ```text
//!   "…on 20/7/26 at 4:30 PM…"   →  t_event
//!   the moment the phone read it →  t_ingest
//!   skew = t_ingest − t_event
//! ```
//!
//! ★★★ **Without `t_event` there is only one clock, and skew is not merely
//! unknown — it is unmeasurable.** Every captured message carried the moment
//! the phone happened to read it, which for a backfill of two thousand texts is
//! the same afternoon for all of them. A month of spending collapses into one
//! timestamp, every window closes on the wrong side, and nothing in the system
//! can tell because there is no second reading to disagree with the first.
//!
//! ## Three ways to be wrong by a lot
//!
//! ★★★ **`12 AM` is midnight and `12 PM` is noon**, and the arithmetic that
//! looks obvious gets both backwards. A twelve-hour error puts an evening
//! payment on the wrong day, which is the wrong month-end for one transaction
//! in thirty.
//!
//! ★★★ **`20/7/26` is day-first.** Safaricom and KCB both write DD/MM/YY, and
//! the real samples settle it — `20/7` cannot be a month. Reading it the other
//! way moves an event by up to eleven months, silently, and only for the days
//! that happen to be ambiguous. That is worse than being always wrong: the
//! failure is invisible for two thirds of every month.
//!
//! ★★★ **A date that cannot be read yields `None`, never *now*.** Falling back
//! to the current moment would make skew exactly zero — the one value that
//! looks healthy — for precisely the messages whose time nobody could read.
//! A missing clock has to be missing.
//!
//! ## What this does not do
//!
//! ★★ **No time zone.** The core carries no locale data and does not guess one;
//! a caller supplies the offset its source is in, the same way `period.rs` has
//! zone data injected rather than bundled. A Kenyan sender is UTC+3, but that is
//! the host's fact about its own messages, not something this module should
//! assume on everyone's behalf.

use std::collections::BTreeMap;

use serde_json::Value;

/// A wall-clock reading with no zone attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Local {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

/// Two-digit years below this belong to the current century.
///
/// ★★ `26` is 2026, not 1926. The window is stated rather than assumed because
/// it is a real choice with a real expiry, and somebody reading this in 2070
/// should find the decision rather than infer it.
const CENTURY_PIVOT: i64 = 70;

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days from 1970-01-01 to this date. Proleptic Gregorian, no leap seconds.
fn days_from_epoch(year: i64, month: u32, day: u32) -> i64 {
    // Howard Hinnant's civil-from-days, inverted. Chosen because it is exact
    // for every date rather than only for the ones a test happens to try.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

impl Local {
    /// Milliseconds since the epoch, given the offset its source is in.
    ///
    /// ★★ The offset is the CALLER's declaration. A message says half past four;
    /// it does not say half past four where.
    pub fn to_epoch_ms(self, utc_offset_minutes: i64) -> i64 {
        let days = days_from_epoch(self.year, self.month, self.day);
        let minutes = days * 1_440 + self.hour as i64 * 60 + self.minute as i64;
        (minutes - utc_offset_minutes) * 60_000
    }

    fn valid(self) -> bool {
        (1..=12).contains(&self.month)
            && self.day >= 1
            && self.day <= days_in_month(self.year, self.month)
            && self.hour < 24
            && self.minute < 60
    }
}

/// `20/7/26` — **day first**.
pub fn parse_date(raw: &str) -> Option<(i64, u32, u32)> {
    let parts: Vec<&str> = raw.trim().split(['/', '-', '.']).collect();
    if parts.len() != 3 {
        return None;
    }
    let day: u32 = parts[0].trim().parse().ok()?;
    let month: u32 = parts[1].trim().parse().ok()?;
    let raw_year: i64 = parts[2].trim().parse().ok()?;
    let year = match parts[2].trim().len() {
        1 | 2 => {
            if raw_year < CENTURY_PIVOT {
                2000 + raw_year
            } else {
                1900 + raw_year
            }
        }
        4 => raw_year,
        _ => return None,
    };
    Some((year, month, day))
}

/// `4:30 PM`, `12:25pm`, `09:14 A.M.`, or a bare `16:30`.
pub fn parse_time(raw: &str) -> Option<(u32, u32)> {
    let cleaned = raw.trim().to_uppercase().replace('.', "").replace(' ', "");
    let (digits, meridiem) = if let Some(rest) = cleaned.strip_suffix("AM") {
        (rest, Some(false))
    } else if let Some(rest) = cleaned.strip_suffix("PM") {
        (rest, Some(true))
    } else {
        (cleaned.as_str(), None)
    };

    let (h, m) = digits.split_once(':')?;
    let hour: u32 = h.parse().ok()?;
    // Seconds, where a source prints them, are dropped rather than refused.
    let minute: u32 = m.split(':').next()?.parse().ok()?;

    let hour = match meridiem {
        // ★★★ The two the obvious arithmetic gets backwards. 12 AM is midnight,
        //     12 PM is noon, and a twelve-hour error puts an evening payment on
        //     the wrong day.
        Some(true) if hour == 12 => 12,
        Some(true) if hour < 12 => hour + 12,
        Some(false) if hour == 12 => 0,
        Some(false) if hour < 12 => hour,
        None if hour < 24 => hour,
        _ => return None,
    };
    Some((hour, minute))
}

/// **The moment the message says it happened.**
///
/// ★★★ `None` when it cannot be read — never *now*. Falling back to the current
/// moment makes skew exactly zero, the one value that looks healthy, for
/// precisely the messages nobody could read a time from.
pub fn event_time(fields: &BTreeMap<String, Value>) -> Option<Local> {
    let text = |k: &str| fields.get(k).and_then(Value::as_str);

    // A rule may capture the two halves separately or as one `datetime`.
    let (date_raw, time_raw) = match (text("date"), text("time")) {
        (Some(d), Some(t)) => (d.to_string(), t.to_string()),
        _ => {
            let joined = text("datetime")?;
            let (d, t) = joined.split_once(" at ").or_else(|| joined.split_once(' '))?;
            (d.to_string(), t.to_string())
        }
    };

    let (year, month, day) = parse_date(&date_raw)?;
    let (hour, minute) = parse_time(&time_raw)?;
    let local = Local { year, month, day, hour, minute };
    local.valid().then_some(local)
}

/// `skew = t_ingest − t_event`, in milliseconds.
///
/// ★★★ A **negative** skew is returned as it is. It means the message claims a
/// moment after the phone read it — a real condition, usually a clock set
/// wrong — and clamping it to zero would erase the one reading that says so.
pub fn skew_ms(t_ingest_ms: i64, t_event_ms: i64) -> i64 {
    t_ingest_ms - t_event_ms
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, Value> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), json!(v))).collect()
    }

    #[test]
    fn a_real_mpesa_stamp_reads_day_first() {
        // ★★★ `20/7` cannot be a month, so the real samples settle it. Reading
        //     it the other way moves an event by up to eleven months — and only
        //     on the days that happen to be ambiguous, so the failure is
        //     invisible for two thirds of every month.
        assert_eq!(parse_date("20/7/26"), Some((2026, 7, 20)));
        assert_eq!(parse_date("01/08/2026"), Some((2026, 8, 1)));
        assert_eq!(parse_date("2/8/26"), Some((2026, 8, 2)), "day first even when ambiguous");
    }

    #[test]
    fn midnight_and_noon_are_the_two_that_go_wrong() {
        // ★★★ A twelve-hour error puts an evening payment on the wrong day.
        assert_eq!(parse_time("12:00 AM"), Some((0, 0)), "12 AM is midnight");
        assert_eq!(parse_time("12:00 PM"), Some((12, 0)), "12 PM is noon");
        assert_eq!(parse_time("12:25pm"), Some((12, 25)));
        assert_eq!(parse_time("11:15 PM"), Some((23, 15)));
        assert_eq!(parse_time("9:14 AM"), Some((9, 14)));
    }

    #[test]
    fn the_shapes_the_real_senders_actually_print_all_read() {
        for (raw, want) in [
            ("4:30 PM", (16, 30)),
            ("12:21 PM", (12, 21)),
            ("09:14 A.M.", (9, 14)),
            ("16:30", (16, 30)),
            ("16:30:45", (16, 30)),
        ] {
            assert_eq!(parse_time(raw), Some(want), "{raw}");
        }
    }

    #[test]
    fn a_two_digit_year_belongs_to_this_century() {
        // ★★ Stated rather than assumed: it is a real choice with a real
        //    expiry, and somebody reading this later should find the decision.
        assert_eq!(parse_date("1/1/26").unwrap().0, 2026);
        assert_eq!(parse_date("1/1/99").unwrap().0, 1999);
    }

    #[test]
    fn a_date_that_does_not_exist_is_refused() {
        // ★★ 31 February is not a date, and accepting it would put a
        //    transaction on a day that never happened.
        assert!(event_time(&fields(&[("date", "31/2/26"), ("time", "9:00 AM")])).is_none());
        assert!(event_time(&fields(&[("date", "1/13/26"), ("time", "9:00 AM")])).is_none());
        assert!(event_time(&fields(&[("date", "29/2/26"), ("time", "9:00 AM")])).is_none());
        assert!(event_time(&fields(&[("date", "29/2/24"), ("time", "9:00 AM")])).is_some());
    }

    #[test]
    fn a_message_whose_time_cannot_be_read_has_no_event_time_rather_than_now() {
        // ★★★ Falling back to the current moment makes skew exactly ZERO — the
        //     one value that looks healthy — for precisely the messages nobody
        //     could read a time from.
        assert!(event_time(&fields(&[("amount", "500")])).is_none());
        assert!(event_time(&fields(&[("date", "nonsense"), ("time", "9:00 AM")])).is_none());
        assert!(event_time(&fields(&[("date", "1/1/26"), ("time", "nonsense")])).is_none());
    }

    #[test]
    fn a_rule_that_captured_one_joined_stamp_reads_too() {
        let t = event_time(&fields(&[("datetime", "20/7/26 at 4:30 PM")])).expect("reads");
        assert_eq!(t, Local { year: 2026, month: 7, day: 20, hour: 16, minute: 30 });
    }

    #[test]
    fn the_epoch_conversion_is_exact_at_a_known_point() {
        // ★★ Checked against a fixed, verifiable instant rather than against
        //    itself: 1970-01-01T00:00 UTC is zero.
        let epoch = Local { year: 1970, month: 1, day: 1, hour: 0, minute: 0 };
        assert_eq!(epoch.to_epoch_ms(0), 0);
        // And a day later is exactly a day.
        let next = Local { year: 1970, month: 1, day: 2, hour: 0, minute: 0 };
        assert_eq!(next.to_epoch_ms(0), 86_400_000);
    }

    #[test]
    fn the_offset_is_the_callers_declaration_not_this_modules_guess() {
        // ★★★ A message says half past four; it does not say half past four
        //     WHERE. The core carries no locale data and does not invent one.
            let t = Local { year: 2026, month: 7, day: 20, hour: 16, minute: 30 };
        let utc = t.to_epoch_ms(0);
        let nairobi = t.to_epoch_ms(180);
        assert_eq!(utc - nairobi, 3 * 3_600_000, "three hours earlier in UTC terms");
    }

    #[test]
    fn a_backfill_of_a_months_texts_no_longer_collapses_into_one_afternoon() {
        // ★★★ The failure this exists to end. Read on one afternoon, every
        //     message carried that afternoon's timestamp; a month of spending
        //     became one instant and every window closed on the wrong side.
        let july = event_time(&fields(&[("date", "3/7/26"), ("time", "9:00 AM")])).expect("reads");
        let august =
            event_time(&fields(&[("date", "3/8/26"), ("time", "9:00 AM")])).expect("reads");
        assert!(august.to_epoch_ms(180) - july.to_epoch_ms(180) > 30 * 86_400_000);
    }

    #[test]
    fn a_clock_set_wrong_shows_as_negative_skew_rather_than_zero() {
        // ★★★ It means the message claims a moment AFTER the phone read it — a
        //     real condition, and clamping it to zero would erase the one
        //     reading that says so.
        assert_eq!(skew_ms(1_000, 5_000), -4_000);
        assert_eq!(skew_ms(5_000, 1_000), 4_000);
    }
}
