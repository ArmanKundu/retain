//! Subject CRUD.
//!
//! Subjects are the spine of the app: sessions, cards and error-log entries all
//! hang off them, and `unit_level` / `subject_type` change what the rest of the
//! app offers rather than just how it looks.

use rusqlite::Connection;

use crate::models::{Subject, SubjectInput, SubjectType, UnitLevel};
use crate::util::rfc3339;

/// The brief caps this at six.
pub const MAX_SUBJECTS: i64 = 6;

fn row_to_subject(row: &rusqlite::Row<'_>) -> rusqlite::Result<Subject> {
    Ok(Subject {
        id: row.get(0)?,
        name: row.get(1)?,
        colour: row.get(2)?,
        unit_level: UnitLevel::from_str(&row.get::<_, String>(3)?),
        subject_type: SubjectType::from_str(&row.get::<_, String>(4)?),
        weekly_goal_minutes: row.get(5)?,
        sort_order: row.get(6)?,
        // SQLite has no bool: 0/1 comes back as an integer and we convert here.
        archived: row.get::<_, i64>(7)? == 1,
    })
}

const SELECT_COLUMNS: &str =
    "id, name, colour, unit_level, subject_type, weekly_goal_minutes, sort_order, archived";

pub fn list(conn: &Connection, include_archived: bool) -> anyhow::Result<Vec<Subject>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM subjects {} ORDER BY sort_order, id",
        if include_archived { "" } else { "WHERE archived = 0" }
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row_to_subject)?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn create(conn: &Connection, input: SubjectInput) -> anyhow::Result<Subject> {
    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM subjects WHERE archived = 0",
        [],
        |row| row.get(0),
    )?;
    if active >= MAX_SUBJECTS {
        anyhow::bail!("You can have up to {MAX_SUBJECTS} subjects at a time.");
    }

    let name = input.name.trim();
    if name.is_empty() {
        anyhow::bail!("A subject needs a name.");
    }

    // Append to the end of the list.
    let next_order: i64 = conn
        .query_row("SELECT COALESCE(MAX(sort_order), -1) + 1 FROM subjects", [], |row| {
            row.get(0)
        })
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO subjects
           (name, colour, unit_level, subject_type, weekly_goal_minutes, sort_order, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            name,
            input.colour,
            input.unit_level.as_str(),
            input.subject_type.as_str(),
            input.weekly_goal_minutes,
            next_order,
            rfc3339(chrono::Utc::now()),
        ],
    )?;

    let id = conn.last_insert_rowid();
    get(conn, id)
}

pub fn get(conn: &Connection, id: i64) -> anyhow::Result<Subject> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM subjects WHERE id = ?1");
    Ok(conn.query_row(&sql, [id], row_to_subject)?)
}

pub fn update(conn: &Connection, id: i64, input: SubjectInput) -> anyhow::Result<Subject> {
    let name = input.name.trim();
    if name.is_empty() {
        anyhow::bail!("A subject needs a name.");
    }

    conn.execute(
        "UPDATE subjects
            SET name = ?1, colour = ?2, unit_level = ?3,
                subject_type = ?4, weekly_goal_minutes = ?5
          WHERE id = ?6",
        rusqlite::params![
            name,
            input.colour,
            input.unit_level.as_str(),
            input.subject_type.as_str(),
            input.weekly_goal_minutes,
            id
        ],
    )?;

    get(conn, id)
}

/// Archive rather than delete.
///
/// Deleting would cascade to every session, card and error-log entry attached to
/// the subject. Dropping a subject in Settings should not silently erase a term
/// of study history, so archived subjects disappear from pickers and rings while
/// their data stays intact and their grid squares keep their colour.
pub fn archive(conn: &Connection, id: i64) -> anyhow::Result<()> {
    conn.execute("UPDATE subjects SET archived = 1 WHERE id = ?1", [id])?;
    Ok(())
}

pub fn unarchive(conn: &Connection, id: i64) -> anyhow::Result<()> {
    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM subjects WHERE archived = 0",
        [],
        |row| row.get(0),
    )?;
    if active >= MAX_SUBJECTS {
        anyhow::bail!("You already have {MAX_SUBJECTS} active subjects.");
    }
    conn.execute("UPDATE subjects SET archived = 0 WHERE id = ?1", [id])?;
    Ok(())
}

/// Persist a drag-reorder. `ordered_ids` is the full list in its new order.
pub fn reorder(conn: &Connection, ordered_ids: Vec<i64>) -> anyhow::Result<()> {
    for (position, id) in ordered_ids.iter().enumerate() {
        conn.execute(
            "UPDATE subjects SET sort_order = ?1 WHERE id = ?2",
            rusqlite::params![position as i64, id],
        )?;
    }
    Ok(())
}

pub fn set_weekly_goal(conn: &Connection, id: i64, minutes: Option<i64>) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE subjects SET weekly_goal_minutes = ?1 WHERE id = ?2",
        rusqlite::params![minutes, id],
    )?;
    Ok(())
}
