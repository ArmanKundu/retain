//! Streak, contribution grid and weekly goal rings.
//!
//! The rule this implements is written up in full in `docs/streak-rule.md`. The
//! short version:
//!
//!   A day qualifies if EITHER a completed session on that day accumulated at
//!   least `focused_session_minutes` of ACTIVE time, OR every review that was due
//!   that day was actually presented and rated that day.
//!
//! Freezes and rest days do not affect whether a day qualified. They only affect
//! whether a gap ends the run.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::Connection;

use crate::models::{DaySubjectSlice, GridDay, StreakSummary, WeeklyGoalRing};
use crate::settings;

/// Most freezes that can be held at once.
const MAX_FREEZES: i64 = 2;
/// Qualifying days needed to earn one back.
const QUALIFYING_DAYS_PER_FREEZE: i64 = 7;

// ---------------------------------------------------------------------------
// Branch A — focused sessions
// ---------------------------------------------------------------------------

/// Every local date with at least one COMPLETED session whose ACTIVE time met
/// the bar.
///
/// Three things this query is careful about:
///   * `ended_at IS NOT NULL` — a running timer earns nothing.
///   * `active_seconds`, never `elapsed_seconds` — pauses, idle and breaks are
///     already excluded from that column by the timer.
///   * `MAX(...) >= threshold` — the bar is one session reaching it, not the
///     day's total. Six scattered five-minute sessions are not a focused session.
fn days_with_focused_session(
    conn: &Connection,
    threshold_minutes: i64,
) -> anyhow::Result<HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT local_date
           FROM sessions
          WHERE ended_at IS NOT NULL
          GROUP BY local_date
         HAVING MAX(active_seconds) >= ?1",
    )?;

    let rows = stmt.query_map([threshold_minutes * 60], |row| row.get::<_, String>(0))?;

    let mut out = HashSet::new();
    for date in rows {
        out.insert(date?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Branch B — reviews genuinely cleared
// ---------------------------------------------------------------------------

/// How many distinct items were due on a given Retain day.
///
/// This is the union of two sets, because answering a card *moves* its due date:
///
///   * cards still carrying `due_on = D` (due that day, not yet answered), and
///   * anything in `review_log` recorded as having been due on D (answered, so
///     its `due_on` has since moved forward).
///
/// `UNION` over a composite key deduplicates the overlap — a learning card whose
/// intraday step lands it back on the same day appears in both sets but is one
/// item. Counting only current `due_on` would undercount every day you actually
/// did your reviews, which is precisely backwards.
fn due_count_on(conn: &Connection, date: &str) -> anyhow::Result<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM (
             SELECT 'card:' || id AS k
               FROM cards
              WHERE suspended = 0 AND state != 'new' AND due_on = ?1
             UNION
             SELECT item_type || ':' || item_id
               FROM review_log
              WHERE due_on = ?1
         )",
        [date],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Items due on `date` that were actually presented and rated on `date`.
fn reviewed_count_on(conn: &Connection, date: &str) -> anyhow::Result<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT item_type || ':' || item_id)
           FROM review_log
          WHERE due_on = ?1 AND local_date = ?1",
        [date],
        |row| row.get(0),
    )?;
    Ok(count)
}

fn reviews_cleared_on(conn: &Connection, date: &str) -> anyhow::Result<bool> {
    let due = due_count_on(conn, date)?;
    if due == 0 {
        return Ok(false);
    }
    Ok(reviewed_count_on(conn, date)? >= due)
}

// ---------------------------------------------------------------------------
// Qualifying days
// ---------------------------------------------------------------------------

/// The set of local dates that earned themselves, by either branch.
pub fn qualifying_days(conn: &Connection, threshold_minutes: i64) -> anyhow::Result<HashSet<String>> {
    let mut days = days_with_focused_session(conn, threshold_minutes)?;

    // Branch B is checked only on days that have review activity at all, so this
    // stays cheap rather than sweeping every date since install.
    let mut stmt = conn.prepare("SELECT DISTINCT local_date FROM review_log")?;
    let dates = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for date in dates {
        let date = date?;
        if !days.contains(&date) && reviews_cleared_on(conn, &date)? {
            days.insert(date);
        }
    }

    Ok(days)
}

// ---------------------------------------------------------------------------
// Freezes
// ---------------------------------------------------------------------------

pub fn available_freezes(conn: &Connection) -> anyhow::Result<i64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM streak_freezes WHERE consumed_on IS NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(n)
}

fn rest_weekdays(conn: &Connection) -> anyhow::Result<HashSet<i64>> {
    let mut stmt = conn.prepare("SELECT weekday FROM rest_days")?;
    let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
    let mut out = HashSet::new();
    for w in rows {
        out.insert(w?);
    }
    Ok(out)
}

fn is_rest_day(date: NaiveDate, rest: &HashSet<i64>) -> bool {
    rest.contains(&(date.weekday().num_days_from_monday() as i64))
}

/// Walk forward through any days that have passed since we last looked, granting
/// and consuming freezes as the rules say.
///
/// Doing this forward, once, and *recording the result* is what keeps freezes
/// honest. If they were recomputed from scratch on every read, changing the
/// threshold in Settings would retroactively rewrite which days were saved — the
/// past would keep shifting under the user.
///
/// Today is deliberately excluded: it is still in progress, and a day you haven't
/// finished yet has not been missed.
pub fn reconcile(conn: &Connection, threshold_minutes: i64) -> anyhow::Result<()> {
    let today = crate::util::retain_today_naive();
    let qualifying = qualifying_days(conn, threshold_minutes)?;
    let rest = rest_weekdays(conn)?;

    // Where we got up to last time. First run starts from the earliest day we
    // have any record of, or today if the database is empty.
    let last_done: Option<String> = settings::get(conn, "freezes_reconciled_through")?;
    let start = match last_done {
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")?
            .checked_add_signed(Duration::days(1))
            .unwrap_or(today),
        None => earliest_recorded_day(conn)?.unwrap_or(today),
    };

    let mut streak_progress: i64 = settings::get(conn, "qualifying_days_toward_freeze")?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let mut cursor = start;
    while cursor < today {
        let key = cursor.format("%Y-%m-%d").to_string();

        if qualifying.contains(&key) {
            // Earn progress toward replacing a spent freeze.
            streak_progress += 1;
            if streak_progress >= QUALIFYING_DAYS_PER_FREEZE {
                streak_progress = 0;
                if available_freezes(conn)? < MAX_FREEZES {
                    conn.execute(
                        "INSERT INTO streak_freezes (granted_on) VALUES (?1)",
                        [&key],
                    )?;
                }
            }
        } else if !is_rest_day(cursor, &rest) {
            // A genuine miss. Spend a freeze if there is one; otherwise the run
            // ends here and `current_streak` will see the gap.
            let spendable: Option<i64> = conn
                .query_row(
                    "SELECT id FROM streak_freezes WHERE consumed_on IS NULL
                      ORDER BY id LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .ok();

            if let Some(id) = spendable {
                conn.execute(
                    "UPDATE streak_freezes SET consumed_on = ?1 WHERE id = ?2",
                    rusqlite::params![key, id],
                )?;
            }
        }

        cursor += Duration::days(1);
    }

    settings::set(
        conn,
        "freezes_reconciled_through",
        &(today - Duration::days(1)).format("%Y-%m-%d").to_string(),
    )?;
    settings::set(
        conn,
        "qualifying_days_toward_freeze",
        &streak_progress.to_string(),
    )?;

    Ok(())
}

fn earliest_recorded_day(conn: &Connection) -> anyhow::Result<Option<NaiveDate>> {
    let raw: Option<String> = conn
        .query_row("SELECT MIN(local_date) FROM sessions", [], |row| row.get(0))
        .ok()
        .flatten();

    Ok(match raw {
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok(),
        None => None,
    })
}

/// Dates on which a freeze was spent — these are gaps the run survives.
fn frozen_days(conn: &Connection) -> anyhow::Result<HashSet<String>> {
    let mut stmt =
        conn.prepare("SELECT consumed_on FROM streak_freezes WHERE consumed_on IS NOT NULL")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut out = HashSet::new();
    for d in rows {
        out.insert(d?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// The run itself
// ---------------------------------------------------------------------------

/// Walk backwards from today counting qualifying days.
///
/// Today is a special case: if you haven't studied yet at 9am, the run is not
/// broken — it just hasn't been extended. Only *past* days can break it.
fn current_streak(
    qualifying: &HashSet<String>,
    frozen: &HashSet<String>,
    rest: &HashSet<i64>,
    today: NaiveDate,
    floor: NaiveDate,
) -> i64 {
    let mut count = 0;
    let mut cursor = today;

    // Today counts if earned, but a blank today doesn't stop the walk.
    if qualifying.contains(&today.format("%Y-%m-%d").to_string()) {
        count += 1;
    }
    cursor -= Duration::days(1);

    // `floor` is the earliest day we have any record of. It is what terminates
    // this loop, and it has to: a rest day neither counts nor breaks, so if every
    // weekday were nominated as a rest day, a loop that only stopped on a break
    // would walk backwards forever and hang the app. Bounding by the data means
    // the walk always terminates regardless of the rest-day configuration.
    while cursor >= floor {
        let key = cursor.format("%Y-%m-%d").to_string();

        if qualifying.contains(&key) {
            count += 1;
        } else if frozen.contains(&key) || is_rest_day(cursor, rest) {
            // Survived, but doesn't add to the count — a rest day is rest, not work.
        } else {
            break;
        }

        cursor -= Duration::days(1);
    }

    count
}

/// Longest run ever achieved, using the same rules.
fn longest_streak(
    qualifying: &HashSet<String>,
    frozen: &HashSet<String>,
    rest: &HashSet<i64>,
    from: NaiveDate,
    today: NaiveDate,
) -> i64 {
    let mut best = 0;
    let mut run = 0;
    let mut cursor = from;

    while cursor <= today {
        let key = cursor.format("%Y-%m-%d").to_string();
        if qualifying.contains(&key) {
            run += 1;
            best = best.max(run);
        } else if frozen.contains(&key) || is_rest_day(cursor, rest) {
            // Run continues across a covered gap.
        } else {
            run = 0;
        }
        cursor += Duration::days(1);
    }

    best
}

pub fn summary(conn: &Connection) -> anyhow::Result<StreakSummary> {
    let threshold = settings::focused_session_minutes(conn)?;

    reconcile(conn, threshold)?;

    let today = crate::util::retain_today_naive();
    let qualifying = qualifying_days(conn, threshold)?;
    let frozen = frozen_days(conn)?;
    let rest = rest_weekdays(conn)?;
    let from = earliest_recorded_day(conn)?.unwrap_or(today);

    let today_key = today.format("%Y-%m-%d").to_string();

    // Best active stretch today, so the UI can say "13 of 20 minutes" rather than
    // just "not yet".
    let today_active_seconds: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(active_seconds), 0) FROM sessions
              WHERE local_date = ?1 AND ended_at IS NOT NULL",
            [&today_key],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(StreakSummary {
        current: current_streak(&qualifying, &frozen, &rest, today, from),
        longest: longest_streak(&qualifying, &frozen, &rest, from, today),
        freezes_available: available_freezes(conn)?,
        rest_days: {
            let mut v: Vec<i64> = rest.into_iter().collect();
            v.sort();
            v
        },
        today_qualified: qualifying.contains(&today_key),
        today_active_minutes: today_active_seconds / 60,
        threshold_minutes: threshold,
    })
}

// ---------------------------------------------------------------------------
// Contribution grid
// ---------------------------------------------------------------------------

/// Per-day totals with a subject breakdown, for the year grid and its hover card.
pub fn grid(conn: &Connection, from: &str, to: &str) -> anyhow::Result<Vec<GridDay>> {
    let threshold = settings::focused_session_minutes(conn)?;
    let qualifying = qualifying_days(conn, threshold)?;

    let mut stmt = conn.prepare(
        "SELECT s.local_date, s.subject_id, subj.name, subj.colour, SUM(s.active_seconds)
           FROM sessions s
           JOIN subjects subj ON subj.id = s.subject_id
          WHERE s.ended_at IS NOT NULL
            AND s.local_date >= ?1 AND s.local_date <= ?2
          GROUP BY s.local_date, s.subject_id
          ORDER BY s.local_date",
    )?;

    let rows = stmt.query_map([from, to], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;

    // Group the flat rows into one entry per day.
    let mut by_day: HashMap<String, Vec<DaySubjectSlice>> = HashMap::new();
    for row in rows {
        let (date, subject_id, subject_name, colour, seconds) = row?;
        by_day.entry(date).or_default().push(DaySubjectSlice {
            subject_id,
            subject_name,
            colour,
            minutes: seconds / 60,
        });
    }

    let mut out: Vec<GridDay> = by_day
        .into_iter()
        .map(|(date, mut slices)| {
            slices.sort_by(|a, b| b.minutes.cmp(&a.minutes));
            GridDay {
                minutes: slices.iter().map(|s| s.minutes).sum(),
                qualified: qualifying.contains(&date),
                by_subject: slices,
                date,
            }
        })
        .collect();

    out.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(out)
}

// ---------------------------------------------------------------------------
// Weekly goal rings
// ---------------------------------------------------------------------------

/// Monday of the current local week, which is where a VCE week sensibly starts.
fn week_start() -> NaiveDate {
    let today = crate::util::retain_today_naive();
    today - Duration::days(today.weekday().num_days_from_monday() as i64)
}

pub fn weekly_rings(conn: &Connection) -> anyhow::Result<Vec<WeeklyGoalRing>> {
    let start = week_start().format("%Y-%m-%d").to_string();

    let mut stmt = conn.prepare(
        "SELECT subj.id, subj.name, subj.colour, subj.weekly_goal_minutes,
                COALESCE(SUM(s.active_seconds), 0)
           FROM subjects subj
           LEFT JOIN sessions s
             ON s.subject_id = subj.id
            AND s.ended_at IS NOT NULL
            AND s.local_date >= ?1
          WHERE subj.archived = 0
            AND subj.weekly_goal_minutes IS NOT NULL
            AND subj.weekly_goal_minutes > 0
          GROUP BY subj.id
          ORDER BY subj.sort_order, subj.id",
    )?;

    let rows = stmt.query_map([&start], |row| {
        Ok(WeeklyGoalRing {
            subject_id: row.get(0)?,
            subject_name: row.get(1)?,
            colour: row.get(2)?,
            goal_minutes: row.get(3)?,
            done_minutes: row.get::<_, i64>(4)? / 60,
        })
    })?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}
