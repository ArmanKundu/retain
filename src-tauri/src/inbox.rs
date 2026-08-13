//! The capture inbox and its triage.
//!
//! Capture is deliberately dumb: it stores the raw line and moves on. Parsing
//! results are kept as *suggestions* alongside the text, never applied
//! automatically — a capture that silently filed itself under the wrong subject
//! with the wrong date is worse than one that waited, because you stop trusting
//! the inbox and then stop using it.
//!
//! Triage is where a capture becomes a task, a card, or an error-log entry.

use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;

use crate::capture::{self, SubjectHint};
use crate::util::{retain_day_of, rfc3339};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capture {
    pub id: i64,
    pub raw_text: String,
    pub created_at: String,
    pub suggested_subject_id: Option<i64>,
    pub suggested_subject_name: Option<String>,
    pub suggested_due_on: Option<String>,
    pub suggested_title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub subject_id: Option<i64>,
    pub subject_name: Option<String>,
    pub colour: Option<String>,
    pub due_on: Option<String>,
    pub done_at: Option<String>,
}

fn subject_hints(conn: &Connection) -> anyhow::Result<Vec<SubjectHint>> {
    let mut stmt = conn.prepare("SELECT id, name FROM subjects WHERE archived = 0")?;
    let rows = stmt.query_map([], |r| {
        Ok(SubjectHint {
            id: r.get(0)?,
            name: r.get(1)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Store a captured line. Returns what the parser made of it.
pub fn save(conn: &Connection, raw_text: &str) -> anyhow::Result<capture::ParsedCapture> {
    let text = raw_text.trim();
    if text.is_empty() {
        anyhow::bail!("Nothing to capture.");
    }

    let hints = subject_hints(conn)?;
    let parsed = capture::parse_now(text, &hints);
    let now = Utc::now();

    conn.execute(
        "INSERT INTO captures
           (raw_text, created_at, local_date, suggested_subject_id, suggested_due_on, suggested_title)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            text,
            rfc3339(now),
            retain_day_of(now),
            parsed.subject_id,
            parsed.due_on,
            if parsed.title.is_empty() { None } else { Some(parsed.title.clone()) },
        ],
    )?;

    Ok(parsed)
}

pub fn list_untriaged(conn: &Connection) -> anyhow::Result<Vec<Capture>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.raw_text, c.created_at, c.suggested_subject_id, s.name,
                c.suggested_due_on, c.suggested_title
           FROM captures c
           LEFT JOIN subjects s ON s.id = c.suggested_subject_id
          WHERE c.triaged_at IS NULL
          ORDER BY c.created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Capture {
            id: r.get(0)?,
            raw_text: r.get(1)?,
            created_at: r.get(2)?,
            suggested_subject_id: r.get(3)?,
            suggested_subject_name: r.get(4)?,
            suggested_due_on: r.get(5)?,
            suggested_title: r.get(6)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn untriaged_count(conn: &Connection) -> anyhow::Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM captures WHERE triaged_at IS NULL",
        [],
        |r| r.get(0),
    )?)
}

fn mark_triaged(conn: &Connection, capture_id: i64, to: &str) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE captures SET triaged_at = ?1, triaged_to = ?2 WHERE id = ?3",
        rusqlite::params![rfc3339(Utc::now()), to, capture_id],
    )?;
    Ok(())
}

/// Turn a capture into a task. The caller passes the final values, which may
/// differ from the suggestions — that's the point of triage.
pub fn triage_to_task(
    conn: &Connection,
    capture_id: i64,
    title: &str,
    subject_id: Option<i64>,
    due_on: Option<&str>,
) -> anyhow::Result<i64> {
    let title = title.trim();
    if title.is_empty() {
        anyhow::bail!("A task needs a title.");
    }

    conn.execute(
        "INSERT INTO tasks (title, subject_id, due_on, created_at, source_capture_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![title, subject_id, due_on, rfc3339(Utc::now()), capture_id],
    )?;
    let task_id = conn.last_insert_rowid();
    mark_triaged(conn, capture_id, "task")?;
    Ok(task_id)
}

/// Record that a capture became a card or an error entry. The card/entry itself
/// is created by its own module; this only closes the inbox item.
pub fn triage_to(conn: &Connection, capture_id: i64, destination: &str) -> anyhow::Result<()> {
    if !matches!(destination, "card" | "error_entry" | "discarded") {
        anyhow::bail!("Unknown triage destination: {destination}");
    }
    mark_triaged(conn, capture_id, destination)
}

pub fn list_tasks(conn: &Connection, include_done: bool) -> anyhow::Result<Vec<Task>> {
    let sql = format!(
        "SELECT t.id, t.title, t.subject_id, s.name, s.colour, t.due_on, t.done_at
           FROM tasks t LEFT JOIN subjects s ON s.id = t.subject_id
          {}
          ORDER BY (t.due_on IS NULL), t.due_on ASC, t.id DESC",
        if include_done { "" } else { "WHERE t.done_at IS NULL" }
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| {
        Ok(Task {
            id: r.get(0)?,
            title: r.get(1)?,
            subject_id: r.get(2)?,
            subject_name: r.get(3)?,
            colour: r.get(4)?,
            due_on: r.get(5)?,
            done_at: r.get(6)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn set_task_done(conn: &Connection, id: i64, done: bool) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE tasks SET done_at = ?1 WHERE id = ?2",
        rusqlite::params![if done { Some(rfc3339(Utc::now())) } else { None }, id],
    )?;
    Ok(())
}

pub fn delete_task(conn: &Connection, id: i64) -> anyhow::Result<()> {
    conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!("db/migrations/001_init.sql")).unwrap();
        conn.execute_batch(include_str!("db/migrations/002_capture_cards_errors.sql")).unwrap();
        conn.execute(
            "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
             VALUES (1,'Chemistry','#5B8DEF','1_2','science',0,'2026-08-12T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn a_capture_stores_the_raw_text_and_its_suggestions() {
        let conn = db();
        let parsed = save(&conn, "chem prac report fri").unwrap();
        assert_eq!(parsed.subject_id, Some(1));
        assert!(parsed.due_on.is_some());

        let inbox = list_untriaged(&conn).unwrap();
        assert_eq!(inbox.len(), 1);
        // The RAW line is preserved verbatim — the parse is a suggestion beside
        // it, not a replacement for it.
        assert_eq!(inbox[0].raw_text, "chem prac report fri");
        assert_eq!(inbox[0].suggested_title.as_deref(), Some("prac report"));
        assert_eq!(inbox[0].suggested_subject_name.as_deref(), Some("Chemistry"));
    }

    #[test]
    fn empty_capture_is_refused() {
        let conn = db();
        assert!(save(&conn, "   ").is_err());
        assert_eq!(untriaged_count(&conn).unwrap(), 0);
    }

    /// Suggestions are not applied — triage passes its own values, and those win.
    #[test]
    fn triage_uses_the_values_given_not_the_suggestions() {
        let conn = db();
        save(&conn, "chem prac report fri").unwrap();
        let item = list_untriaged(&conn).unwrap().remove(0);

        triage_to_task(&conn, item.id, "Write up titration", None, Some("2026-09-01")).unwrap();

        let tasks = list_tasks(&conn, false).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Write up titration");
        assert_eq!(tasks[0].subject_id, None, "the suggestion was overridden");
        assert_eq!(tasks[0].due_on.as_deref(), Some("2026-09-01"));
    }

    #[test]
    fn triaging_removes_it_from_the_inbox() {
        let conn = db();
        save(&conn, "something").unwrap();
        let item = list_untriaged(&conn).unwrap().remove(0);
        assert_eq!(untriaged_count(&conn).unwrap(), 1);

        triage_to_task(&conn, item.id, "Something", None, None).unwrap();
        assert_eq!(untriaged_count(&conn).unwrap(), 0);
    }

    #[test]
    fn discarding_also_clears_the_inbox_without_creating_a_task() {
        let conn = db();
        save(&conn, "never mind").unwrap();
        let item = list_untriaged(&conn).unwrap().remove(0);

        triage_to(&conn, item.id, "discarded").unwrap();
        assert_eq!(untriaged_count(&conn).unwrap(), 0);
        assert!(list_tasks(&conn, false).unwrap().is_empty());
    }

    #[test]
    fn an_unknown_triage_destination_is_refused() {
        let conn = db();
        save(&conn, "x").unwrap();
        let item = list_untriaged(&conn).unwrap().remove(0);
        assert!(triage_to(&conn, item.id, "somewhere_else").is_err());
        assert_eq!(untriaged_count(&conn).unwrap(), 1, "it stays in the inbox");
    }

    #[test]
    fn a_task_needs_a_title() {
        let conn = db();
        save(&conn, "x").unwrap();
        let item = list_untriaged(&conn).unwrap().remove(0);
        assert!(triage_to_task(&conn, item.id, "  ", None, None).is_err());
        assert_eq!(untriaged_count(&conn).unwrap(), 1, "failed triage must not consume it");
    }

    #[test]
    fn tasks_sort_undated_last() {
        let conn = db();
        for (t, d) in [("later", Some("2026-12-01")), ("undated", None), ("soon", Some("2026-08-13"))] {
            conn.execute(
                "INSERT INTO tasks (title, due_on, created_at) VALUES (?1, ?2, '2026-08-12T00:00:00Z')",
                rusqlite::params![t, d],
            )
            .unwrap();
        }
        let titles: Vec<String> = list_tasks(&conn, false).unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, vec!["soon", "later", "undated"]);
    }

    #[test]
    fn completing_and_reopening_a_task() {
        let conn = db();
        save(&conn, "x").unwrap();
        let item = list_untriaged(&conn).unwrap().remove(0);
        let id = triage_to_task(&conn, item.id, "Do it", None, None).unwrap();

        set_task_done(&conn, id, true).unwrap();
        assert!(list_tasks(&conn, false).unwrap().is_empty());
        assert_eq!(list_tasks(&conn, true).unwrap().len(), 1);

        set_task_done(&conn, id, false).unwrap();
        assert_eq!(list_tasks(&conn, false).unwrap().len(), 1);
    }

    /// Deleting the capture must not cascade into a task already made from it.
    #[test]
    fn deleting_a_capture_leaves_its_task_intact() {
        let conn = db();
        save(&conn, "x").unwrap();
        let item = list_untriaged(&conn).unwrap().remove(0);
        triage_to_task(&conn, item.id, "Survives", None, None).unwrap();

        conn.execute("DELETE FROM captures WHERE id = ?1", [item.id]).unwrap();

        let tasks = list_tasks(&conn, false).unwrap();
        assert_eq!(tasks.len(), 1, "ON DELETE SET NULL, not CASCADE");
        assert_eq!(tasks[0].title, "Survives");
    }
}
