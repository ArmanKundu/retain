//! The plan, and what happens to it when a day goes wrong.
//!
//! You meant to do chemistry on Tuesday. Tuesday you got home at nine. The
//! question this module answers is what Wednesday should now look like.
//!
//! The naive answer — dump everything onto tomorrow — is worse than doing
//! nothing. It produces a Wednesday with five hours of work on it, you look at
//! it once, and you stop opening the app. So rollover here is **capacity
//! aware**: it walks forward day by day, asks how much time each day actually
//! has after your classes and shifts and rest, and places the slipped work in
//! the first day it genuinely fits. Overflow keeps walking.
//!
//! Three rules make this trustworthy rather than merely clever:
//!
//!   1. **Nothing moves past its own deadline.** Revision for Thursday's SAC
//!      cannot be rescheduled to Friday. If it won't fit before the due date it
//!      is reported as stuck, and you decide — the one call the app must never
//!      make quietly on your behalf.
//!   2. **The original date is kept.** `first_planned_on` is never rewritten,
//!      so the UI can say "this has moved four times since the 3rd". A plan
//!      that silently launders its own history is how you end up three weeks
//!      behind without noticing.
//!   3. **It is deterministic and idempotent.** Same inputs, same result;
//!      running it twice on one day changes nothing the second time. The AI
//!      writes plan items, but it never decides where slipped work lands —
//!      a schedule that reshuffles differently each time you open the app is
//!      one you cannot plan around.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::{blocks, util};

/// The waking window rollover plans inside. Matches the week grid.
const DAY_START: i64 = 7 * 60;
const DAY_END: i64 = 22 * 60;

/// The most any single day will be given, however empty it looks.
///
/// A free Saturday has fourteen hours in it and you are not going to study for
/// fourteen hours. Planning as if you might is how a planner loses your trust
/// in one weekend.
const DAILY_CAP_MIN: i64 = 240;

/// How far ahead rollover will look before giving up on an item.
///
/// Past this, "it didn't fit" is the honest answer. Silently parking work in
/// three weeks' time is how a backlog becomes invisible.
const HORIZON_DAYS: i64 = 14;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub id: i64,
    pub subject_id: Option<i64>,
    pub subject_name: Option<String>,
    pub colour: Option<String>,
    pub title: String,
    pub detail: Option<String>,
    pub planned_on: String,
    pub first_planned_on: String,
    pub est_minutes: i64,
    pub due_on: Option<String>,
    pub status: String,
    /// Times rollover has moved this. Shown in the UI once it reaches two.
    pub moves: i64,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPlanItem {
    pub subject_id: Option<i64>,
    pub title: String,
    pub detail: Option<String>,
    pub planned_on: String,
    pub est_minutes: i64,
    pub due_on: Option<String>,
    pub source: Option<String>,
}

const SELECT: &str = "SELECT p.id, p.subject_id, s.name, s.colour, p.title, p.detail,
        p.planned_on, p.first_planned_on, p.est_minutes, p.due_on, p.status, p.moves, p.source
   FROM plan_items p LEFT JOIN subjects s ON s.id = p.subject_id";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PlanItem> {
    Ok(PlanItem {
        id: r.get(0)?,
        subject_id: r.get(1)?,
        subject_name: r.get(2)?,
        colour: r.get(3)?,
        title: r.get(4)?,
        detail: r.get(5)?,
        planned_on: r.get(6)?,
        first_planned_on: r.get(7)?,
        est_minutes: r.get(8)?,
        due_on: r.get(9)?,
        status: r.get(10)?,
        moves: r.get(11)?,
        source: r.get(12)?,
    })
}

fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| anyhow!("\"{s}\" isn't a date."))
}

pub fn create(conn: &Connection, input: &NewPlanItem, now: DateTime<Utc>) -> Result<i64> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(anyhow!("Give it a title so you know what it was."));
    }
    let planned = parse_date(&input.planned_on)?;

    // A deadline before the day it's planned for means the plan is already
    // wrong, and rollover would report it stuck on its very first pass.
    if let Some(due) = input.due_on.as_deref() {
        if parse_date(due)? < planned {
            return Err(anyhow!("That's due before the day you've planned it for."));
        }
    }

    let est = input.est_minutes.clamp(5, DAILY_CAP_MIN);
    let source = match input.source.as_deref() {
        Some("ai") => "ai",
        Some("assessment") => "assessment",
        _ => "manual",
    };

    conn.execute(
        "INSERT INTO plan_items
           (subject_id, title, detail, planned_on, first_planned_on, est_minutes, due_on,
            source, created_at)
         VALUES (?1,?2,?3,?4,?4,?5,?6,?7,?8)",
        params![
            input.subject_id,
            title,
            input.detail.as_deref().map(str::trim).filter(|d| !d.is_empty()),
            input.planned_on,
            est,
            input.due_on,
            source,
            util::rfc3339(now),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Everything on one day, in the order it was planned.
pub fn for_date(conn: &Connection, date: &str) -> Result<Vec<PlanItem>> {
    let sql = format!("{SELECT} WHERE p.planned_on = ?1 ORDER BY p.status = 'done', p.id");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([date], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Everything still outstanding between two dates, for the week view.
pub fn between(conn: &Connection, from: &str, to: &str) -> Result<Vec<PlanItem>> {
    let sql = format!(
        "{SELECT} WHERE p.planned_on BETWEEN ?1 AND ?2 ORDER BY p.planned_on, p.status = 'done', p.id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([from, to], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Mark done or skipped.
///
/// Skipped is a real outcome, not a failure state — deciding you don't need to
/// do something is a decision, and rollover must not drag it forward forever
/// after you've made it.
pub fn set_status(conn: &Connection, id: i64, status: &str, now: DateTime<Utc>) -> Result<()> {
    if !matches!(status, "planned" | "done" | "skipped") {
        return Err(anyhow!("Unknown status."));
    }
    conn.execute(
        "UPDATE plan_items SET status = ?2, done_at = ?3 WHERE id = ?1",
        params![
            id,
            status,
            (status != "planned").then(|| util::rfc3339(now)),
        ],
    )?;
    Ok(())
}

/// Move one item by hand. Counts as a move, same as rollover's.
pub fn move_to(conn: &Connection, id: i64, date: &str) -> Result<()> {
    parse_date(date)?;
    conn.execute(
        "UPDATE plan_items SET planned_on = ?2, moves = moves + 1 WHERE id = ?1",
        params![id, date],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM plan_items WHERE id = ?1", [id])?;
    Ok(())
}

// -- rollover ---------------------------------------------------------------

/// One item that moved, so the UI can say what happened rather than silently
/// presenting a different Wednesday.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Moved {
    pub id: i64,
    pub title: String,
    pub subject_name: Option<String>,
    pub from: String,
    pub to: String,
    pub moves: i64,
}

/// An item that could not be placed, and why.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stuck {
    pub id: i64,
    pub title: String,
    pub subject_name: Option<String>,
    pub from: String,
    /// Plain enough to show as-is.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Rollover {
    pub moved: Vec<Moved>,
    pub stuck: Vec<Stuck>,
}

/// How much study a date can take, after commitments and the daily cap.
fn capacity(conn: &Connection, date: NaiveDate) -> Result<i64> {
    let blocks = blocks::for_date(conn, date)?;
    Ok(blocks::free_minutes(&blocks, DAY_START, DAY_END).min(DAILY_CAP_MIN))
}

/// Move everything left behind onto days that can actually take it.
///
/// Runs on launch and at the day boundary. Returns what it did — the caller
/// shows it, because a plan that rearranges itself without telling you is
/// indistinguishable from a plan that lost your work.
pub fn rollover(conn: &Connection, today: NaiveDate) -> Result<Rollover> {
    let today_iso = today.format("%Y-%m-%d").to_string();

    // Oldest first, so the thing that has been waiting longest gets the first
    // gap. Ties break on id: two items planned the same day keep the order you
    // entered them, which makes the result reproducible.
    let sql = format!("{SELECT} WHERE p.status = 'planned' AND p.planned_on < ?1
                       ORDER BY p.planned_on, p.id");
    let mut stmt = conn.prepare(&sql)?;
    let overdue: Vec<PlanItem> = stmt
        .query_map([&today_iso], row)?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    if overdue.is_empty() {
        stamp(conn, &today_iso)?;
        return Ok(Rollover::default());
    }

    // Remaining capacity per day, seeded from what is *already* planned there.
    // Without this, work slipping onto a Wednesday that was already full would
    // overfill it — the exact failure this whole module exists to avoid.
    let mut remaining: Vec<(NaiveDate, i64)> = Vec::with_capacity(HORIZON_DAYS as usize);
    for offset in 0..HORIZON_DAYS {
        let date = today + Duration::days(offset);
        let iso = date.format("%Y-%m-%d").to_string();
        let planned: i64 = conn.query_row(
            "SELECT COALESCE(SUM(est_minutes), 0) FROM plan_items
              WHERE planned_on = ?1 AND status = 'planned'",
            [&iso],
            |r| r.get(0),
        )?;
        remaining.push((date, (capacity(conn, date)? - planned).max(0)));
    }

    let mut out = Rollover::default();

    for item in overdue {
        let due = item.due_on.as_deref().map(parse_date).transpose()?;

        // A deadline already in the past is not a scheduling problem. Moving it
        // anywhere is wrong, and dropping it silently is worse.
        if due.is_some_and(|d| d < today) {
            out.stuck.push(Stuck {
                id: item.id,
                title: item.title.clone(),
                subject_name: item.subject_name.clone(),
                from: item.planned_on.clone(),
                reason: "Its deadline has passed.".into(),
            });
            continue;
        }

        let slot = remaining
            .iter_mut()
            .filter(|(date, _)| due.is_none_or(|d| *date <= d))
            .find(|(_, free)| *free >= item.est_minutes);

        let Some((date, free)) = slot else {
            out.stuck.push(Stuck {
                id: item.id,
                title: item.title.clone(),
                subject_name: item.subject_name.clone(),
                from: item.planned_on.clone(),
                reason: match due {
                    Some(_) => "There's no free time left before it's due.".into(),
                    None => format!("Nothing in the next {HORIZON_DAYS} days has room for it."),
                },
            });
            continue;
        };

        *free -= item.est_minutes;
        let to = date.format("%Y-%m-%d").to_string();
        let moves = item.moves + 1;

        conn.execute(
            "UPDATE plan_items SET planned_on = ?2, moves = ?3 WHERE id = ?1",
            params![item.id, &to, moves],
        )?;

        out.moved.push(Moved {
            id: item.id,
            title: item.title,
            subject_name: item.subject_name,
            from: item.planned_on,
            to,
            moves,
        });
    }

    stamp(conn, &today_iso)?;
    Ok(out)
}

/// Records that today has been walked, so the caller can skip re-running on
/// every window focus.
fn stamp(conn: &Connection, today_iso: &str) -> Result<()> {
    crate::settings::set(conn, "plan_rolled_on", today_iso)
}

/// Whether rollover has already run today.
pub fn rolled_today(conn: &Connection, today: NaiveDate) -> Result<bool> {
    let stamp = crate::settings::get(conn, "plan_rolled_on")?;
    Ok(stamp.as_deref() == Some(today.format("%Y-%m-%d").to_string().as_str()))
}

/// A line for the assistant's context: what you're meant to be doing, and what
/// has been slipping.
pub fn summary(conn: &Connection, today: NaiveDate) -> Result<String> {
    let today_iso = today.format("%Y-%m-%d").to_string();
    let items = for_date(conn, &today_iso)?;
    if items.is_empty() {
        return Ok("Nothing planned for today.".into());
    }

    let mut out = String::from("Planned today:\n");
    for i in &items {
        let subject = i.subject_name.as_deref().unwrap_or("General");
        let done = if i.status == "done" { " (done)" } else { "" };
        // The move count is the useful signal here — it tells the assistant
        // which intentions keep getting deferred, which is usually the thing
        // worth talking about.
        let slipped = if i.moves >= 2 && i.status == "planned" {
            format!(" — moved {} times since {}", i.moves, i.first_planned_on)
        } else {
            String::new()
        };
        out.push_str(&format!(
            "- {subject}: {} ({} min){done}{slipped}\n",
            i.title, i.est_minutes
        ));
    }
    Ok(out)
}

#[cfg(test)]
#[path = "plan/tests.rs"]
mod tests;
