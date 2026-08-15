//! When you can't study.
//!
//! Retain knew what you had to do and never knew when you could do it. A week
//! with tuition on Tuesday and a shift on Saturday is a different week from an
//! empty one, and advice that ignores that is advice you'll ignore back.
//!
//! A block is a claim on your time. Most are unavailable — class, work, family,
//! rest — but `available` is a flag rather than a consequence of the kind,
//! because only you know whether a particular free period is one you can
//! actually revise in.
//!
//! **Rest is a kind, not an absence.** A week with nothing in it doesn't mean a
//! week of free study time, and a planner that assumes so produces a plan
//! nobody follows. Marking rest explicitly is what lets the assistant say
//! "you've got four hours this week" and be right.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimeBlock {
    pub id: i64,
    pub title: String,
    pub kind: String,
    /// 0 = Monday. Set for a weekly commitment.
    pub weekday: Option<i64>,
    /// Set for a one-off.
    pub on_date: Option<String>,
    pub start_min: i64,
    pub end_min: i64,
    pub available: bool,
    pub subject_id: Option<i64>,
    pub subject_name: Option<String>,
    pub colour: Option<String>,
    pub note: Option<String>,
    /// A meeting URL, opened in the browser from the week grid.
    pub link: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewBlock {
    pub title: String,
    pub kind: String,
    pub weekday: Option<i64>,
    pub on_date: Option<String>,
    pub start_min: i64,
    pub end_min: i64,
    pub available: bool,
    pub subject_id: Option<i64>,
    pub note: Option<String>,
    pub link: Option<String>,
}

/// Validate before touching the database.
///
/// The schema has CHECK constraints for all of this, but a constraint violation
/// surfaces as "CHECK constraint failed: time_blocks", which tells the person
/// dragging a block nothing about what they did wrong.
pub fn validate(b: &NewBlock) -> Result<()> {
    if b.title.trim().is_empty() {
        return Err(anyhow!("Give it a name."));
    }
    if b.weekday.is_some() == b.on_date.is_some() {
        return Err(anyhow!("A block repeats weekly or happens once, not both."));
    }
    if let Some(w) = b.weekday {
        if !(0..=6).contains(&w) {
            return Err(anyhow!("That isn't a day of the week."));
        }
    }
    if b.end_min <= b.start_min {
        return Err(anyhow!("It has to end after it starts."));
    }
    if b.start_min < 0 || b.end_min > 1440 {
        return Err(anyhow!("A block has to fit inside one day."));
    }
    // Anything the app will later hand to the OS opener has to be checked here,
    // where the error can still be shown next to the field.
    if let Some(link) = b.link.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
        if !link.starts_with("https://") && !link.starts_with("http://") {
            return Err(anyhow!("A meeting link should start with https://"));
        }
    }
    Ok(())
}

pub fn create(conn: &Connection, b: &NewBlock, now: DateTime<Utc>) -> Result<i64> {
    validate(b)?;
    conn.execute(
        "INSERT INTO time_blocks
           (title, kind, weekday, on_date, start_min, end_min, available, subject_id, note,
            link, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        rusqlite::params![
            b.title.trim(),
            b.kind,
            b.weekday,
            b.on_date,
            b.start_min,
            b.end_min,
            b.available as i64,
            b.subject_id,
            b.note,
            b.link.as_deref().map(str::trim).filter(|l| !l.is_empty()),
            util::rfc3339(now),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update(conn: &Connection, id: i64, b: &NewBlock) -> Result<()> {
    validate(b)?;
    conn.execute(
        "UPDATE time_blocks
            SET title = ?2, kind = ?3, weekday = ?4, on_date = ?5, start_min = ?6,
                end_min = ?7, available = ?8, subject_id = ?9, note = ?10, link = ?11
          WHERE id = ?1",
        rusqlite::params![
            id,
            b.title.trim(),
            b.kind,
            b.weekday,
            b.on_date,
            b.start_min,
            b.end_min,
            b.available as i64,
            b.subject_id,
            b.note,
            b.link.as_deref().map(str::trim).filter(|l| !l.is_empty()),
        ],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM time_blocks WHERE id = ?1", [id])?;
    Ok(())
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TimeBlock> {
    Ok(TimeBlock {
        id: r.get(0)?,
        title: r.get(1)?,
        kind: r.get(2)?,
        weekday: r.get(3)?,
        on_date: r.get(4)?,
        start_min: r.get(5)?,
        end_min: r.get(6)?,
        available: r.get::<_, i64>(7)? == 1,
        subject_id: r.get(8)?,
        subject_name: r.get(9)?,
        colour: r.get(10)?,
        note: r.get(11)?,
        link: r.get(12)?,
    })
}

const SELECT: &str = "SELECT b.id, b.title, b.kind, b.weekday, b.on_date, b.start_min,
                             b.end_min, b.available, b.subject_id, s.name, s.colour, b.note, b.link
                        FROM time_blocks b LEFT JOIN subjects s ON s.id = b.subject_id";

/// Every block, for the editor.
pub fn all(conn: &Connection) -> Result<Vec<TimeBlock>> {
    let sql = format!("{SELECT} ORDER BY b.weekday, b.on_date, b.start_min");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// What applies on one date: its weekly blocks plus anything dated to it.
///
/// A dated block on the same day as a weekly one does not replace it — they
/// both appear, because "I have tuition every Tuesday and a dentist appointment
/// this Tuesday" is two commitments, not one.
pub fn for_date(conn: &Connection, date: NaiveDate) -> Result<Vec<TimeBlock>> {
    let weekday = date.weekday().num_days_from_monday() as i64;
    let iso = date.format("%Y-%m-%d").to_string();

    let sql = format!("{SELECT} WHERE b.weekday = ?1 OR b.on_date = ?2 ORDER BY b.start_min");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![weekday, iso], row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Minutes genuinely free on a date, given the blocks and a waking window.
///
/// Overlapping blocks are merged rather than summed — two commitments from
/// 4–5pm and 4:30–6pm consume ninety minutes, not the hundred and twenty that
/// naive subtraction would report. Getting that wrong would systematically
/// understate how much time you have, which is the failure mode that makes a
/// planner feel punishing.
pub fn free_minutes(blocks: &[TimeBlock], day_start: i64, day_end: i64) -> i64 {
    let mut busy: Vec<(i64, i64)> = blocks
        .iter()
        .filter(|b| !b.available)
        .map(|b| (b.start_min.max(day_start), b.end_min.min(day_end)))
        .filter(|(s, e)| e > s)
        .collect();

    busy.sort();

    let mut merged: Vec<(i64, i64)> = Vec::with_capacity(busy.len());
    for (start, end) in busy {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }

    let total = (day_end - day_start).max(0);
    total - merged.iter().map(|(s, e)| e - s).sum::<i64>()
}

/// A plain-English summary of the week, for the assistant.
///
/// This is what turns "what should I do tonight?" from a guess into an answer.
/// Computed here from real rows — the model is never asked to add up hours.
pub fn week_summary(conn: &Connection, from: NaiveDate) -> Result<String> {
    const DAY_START: i64 = 7 * 60;
    const DAY_END: i64 = 22 * 60;
    const NAMES: [&str; 7] = [
        "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
    ];

    let mut lines = Vec::new();

    for offset in 0..7 {
        let date = from + chrono::Duration::days(offset);
        let blocks = for_date(conn, date)?;
        if blocks.is_empty() {
            continue;
        }

        let free = free_minutes(&blocks, DAY_START, DAY_END);
        let committed: Vec<String> = blocks
            .iter()
            .filter(|b| !b.available)
            .map(|b| format!("{} {}–{}", b.title, clock(b.start_min), clock(b.end_min)))
            .collect();

        if committed.is_empty() {
            continue;
        }

        lines.push(format!(
            "{}: {} — about {}h {}m free.",
            NAMES[date.weekday().num_days_from_monday() as usize],
            committed.join(", "),
            free / 60,
            free % 60
        ));
    }

    if lines.is_empty() {
        return Ok(String::new());
    }

    Ok(format!(
        "--- The student's committed time this week (they cannot study during these) ---\n{}\n",
        lines.join("\n")
    ))
}

/// Minutes from midnight as a clock time.
pub fn clock(minutes: i64) -> String {
    let h = minutes / 60;
    let m = minutes % 60;
    let suffix = if h < 12 { "am" } else { "pm" };
    let display = match h % 12 {
        0 => 12,
        other => other,
    };
    if m == 0 {
        format!("{display}{suffix}")
    } else {
        format!("{display}:{m:02}{suffix}")
    }
}

#[cfg(test)]
mod tests;
