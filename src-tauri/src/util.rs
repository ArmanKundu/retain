//! Time helpers, and — more importantly — Retain's single day-boundary policy.
//!
//! # The day-boundary policy
//!
//! **A Retain day runs from 4am local time to 4am local time the next morning.**
//!
//! This is one deliberate choice applied to *everything* that buckets by day:
//! the streak, the contribution grid, new-card daily caps, FSRS elapsed-day
//! calculations, the review queue, and notification rate limits. Mixing
//! policies across those — UTC in one place, local midnight in another, a 4am
//! cutoff in a third — produces bugs that only appear late at night and are
//! nearly impossible to reproduce in daylight.
//!
//! ## Why 4am rather than local midnight
//!
//! A student finishing a session at 1am does not think of that as tomorrow's
//! work. Under a midnight boundary it would:
//!
//!   * land on tomorrow's square in the contribution grid,
//!   * count toward tomorrow's streak while leaving today's blank — so a night
//!     of study could *break* the streak it should have earned,
//!   * consume tomorrow's new-card allowance before tomorrow starts,
//!   * and count as a full day elapsed for FSRS after a few hours.
//!
//! A 4am boundary makes all four behave the way the person means. It is also
//! what Anki uses by default, so the FSRS elapsed-day counts Retain feeds the
//! scheduler match the ecosystem the algorithm was tuned in.
//!
//! ## Why it is a constant and not a setting
//!
//! Because it must be consistent. A user who changed it would silently rewrite
//! which day past sessions belonged to, moving grid squares and potentially
//! breaking a streak retroactively. The value is documented here and in the
//! README instead.

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};

/// The hour (local) at which one Retain day becomes the next.
pub const DAY_ROLLOVER_HOUR: i64 = 4;

/// Format an instant the way every timestamp column in the schema expects.
pub fn rfc3339(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Which Retain day an instant belongs to, as a `NaiveDate`.
///
/// The implementation is the whole policy: shift local time back by the rollover
/// hour, then take the calendar date. 1am becomes 9pm the previous day; 5am
/// stays put.
pub fn retain_day_naive(dt: DateTime<Utc>) -> NaiveDate {
    (dt.with_timezone(&Local) - Duration::hours(DAY_ROLLOVER_HOUR)).date_naive()
}

/// Which Retain day an instant belongs to, as 'YYYY-MM-DD'.
///
/// This is the value stored in every `local_date` / `due_on` / `logged_on`
/// column. Nothing in the app may compute a day bucket any other way.
pub fn retain_day_of(dt: DateTime<Utc>) -> String {
    retain_day_naive(dt).format("%Y-%m-%d").to_string()
}

/// Today's Retain day.
pub fn retain_today() -> String {
    retain_day_of(Utc::now())
}

/// Today's Retain day as a `NaiveDate`.
pub fn retain_today_naive() -> NaiveDate {
    retain_day_naive(Utc::now())
}

/// The instant a given Retain day begins — 4am local on that date.
///
/// Interday card intervals are anchored here rather than to the moment of
/// answering, so a card scheduled "5 days out" becomes due at the start of that
/// study day rather than at whatever time of day the previous review happened.
/// Without this, a card reviewed at 9pm would not appear until 9pm five days
/// later, and the queue would trickle in through the day instead of being ready
/// when you sit down.
pub fn retain_day_start(day: NaiveDate) -> DateTime<Utc> {
    use chrono::TimeZone;
    let naive = day
        .and_hms_opt(DAY_ROLLOVER_HOUR as u32, 0, 0)
        .expect("valid rollover hour");
    // A local time can be ambiguous or absent across a DST transition.
    // `.earliest()` picks the first valid instant; falling back to UTC keeps this
    // total rather than panicking on one day a year.
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&naive))
}

/// Whole Retain days between two instants, never negative.
///
/// This is what FSRS receives as `days_elapsed`. Because both ends go through
/// the same policy, a review at 1am the "next" morning is correctly zero days
/// after an 11pm review — they are the same Retain day.
pub fn retain_days_between(from: DateTime<Utc>, to: DateTime<Utc>) -> u32 {
    (retain_day_naive(to) - retain_day_naive(from))
        .num_days()
        .max(0) as u32
}

/// Render a duration for the menu bar: `M:SS` under an hour, `H:MM:SS` beyond it.
///
/// Kept narrow on purpose — the menu bar has very little room, and a jittering
/// width is distracting in a status item you glance at all day.
pub fn format_clock(total_seconds: i64) -> String {
    let s = total_seconds.max(0);
    let hours = s / 3600;
    let minutes = (s % 3600) / 60;
    let seconds = s % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Build an instant from a LOCAL wall-clock time, so these tests assert the
    /// policy rather than the host machine's offset.
    fn local(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Local
            .with_ymd_and_hms(y, m, d, h, min, 0)
            .single()
            .expect("unambiguous local time")
            .with_timezone(&Utc)
    }

    #[test]
    fn late_night_belongs_to_the_previous_day() {
        // 1am on the 13th is still the 12th's study day.
        assert_eq!(retain_day_of(local(2026, 8, 13, 1, 0)), "2026-08-12");
        // 11pm on the 12th, obviously also the 12th.
        assert_eq!(retain_day_of(local(2026, 8, 12, 23, 0)), "2026-08-12");
        // 3:59am is the last minute of the 12th.
        assert_eq!(retain_day_of(local(2026, 8, 13, 3, 59)), "2026-08-12");
    }

    #[test]
    fn the_boundary_is_four_am() {
        assert_eq!(retain_day_of(local(2026, 8, 13, 4, 0)), "2026-08-13");
        assert_eq!(retain_day_of(local(2026, 8, 13, 4, 1)), "2026-08-13");
    }

    #[test]
    fn ordinary_daytime_is_unsurprising() {
        assert_eq!(retain_day_of(local(2026, 8, 12, 9, 0)), "2026-08-12");
        assert_eq!(retain_day_of(local(2026, 8, 12, 15, 30)), "2026-08-12");
    }

    /// An 11pm review and a 1am review are the SAME Retain day, so FSRS must be
    /// told zero days elapsed — not one. This is the case a midnight boundary
    /// gets wrong, and it directly affects scheduling.
    #[test]
    fn eleven_pm_to_one_am_is_zero_days_elapsed() {
        let evening = local(2026, 8, 12, 23, 0);
        let after_midnight = local(2026, 8, 13, 1, 0);
        assert_eq!(retain_days_between(evening, after_midnight), 0);
    }

    #[test]
    fn crossing_the_rollover_counts_one_day() {
        let before = local(2026, 8, 12, 23, 0); // day 12
        let after = local(2026, 8, 13, 10, 0); // day 13
        assert_eq!(retain_days_between(before, after), 1);
    }

    #[test]
    fn days_between_never_goes_negative() {
        let later = local(2026, 8, 20, 12, 0);
        let earlier = local(2026, 8, 12, 12, 0);
        assert_eq!(retain_days_between(later, earlier), 0);
        assert_eq!(retain_days_between(earlier, later), 8);
    }

    #[test]
    fn day_string_and_naive_agree() {
        let t = local(2026, 8, 13, 2, 0);
        assert_eq!(retain_day_of(t), retain_day_naive(t).format("%Y-%m-%d").to_string());
    }

    #[test]
    fn clock_formatting() {
        assert_eq!(format_clock(0), "0:00");
        assert_eq!(format_clock(65), "1:05");
        assert_eq!(format_clock(3661), "1:01:01");
        assert_eq!(format_clock(-5), "0:00");
    }
}
