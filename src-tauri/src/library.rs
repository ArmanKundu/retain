//! Everything the AI has produced for you, kept.
//!
//! Before this, an AI answer lived until you navigated away. Notes you asked
//! for, practice questions, a weekly review — all gone, with no way to find
//! them again or print them. Every generation now lands here automatically:
//! nothing is saved by pressing a save button, because a save button you have
//! to remember is a feature that mostly doesn't happen.
//!
//! Items are plain text (Markdown, in practice), so exporting is a file write
//! rather than a rendering problem, and printing is the browser's job.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::util;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Notes,
    PracticeQuestion,
    WeeklyReview,
    Answer,
    Cards,
}

impl ItemKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemKind::Notes => "notes",
            ItemKind::PracticeQuestion => "practice_question",
            ItemKind::WeeklyReview => "weekly_review",
            ItemKind::Answer => "answer",
            ItemKind::Cards => "cards",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "notes" => ItemKind::Notes,
            "practice_question" => ItemKind::PracticeQuestion,
            "weekly_review" => ItemKind::WeeklyReview,
            "cards" => ItemKind::Cards,
            _ => ItemKind::Answer,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: i64,
    pub subject_id: Option<i64>,
    pub subject_name: Option<String>,
    pub colour: Option<String>,
    pub kind: ItemKind,
    pub title: String,
    pub prompt: Option<String>,
    pub body: String,
    pub model: Option<String>,
    pub pinned: bool,
    pub created_at: String,
}

/// Save a generation.
///
/// Deliberately infallible from the caller's point of view: it returns a
/// `Result`, but every call site ignores a failure. Losing the archive copy of
/// an answer must never cost you the answer itself, which is already on screen.
/// Eight parameters, all required and all independent — a struct would move
/// the same values behind a name the caller still has to fill in completely.
#[allow(clippy::too_many_arguments)]
pub fn save(
    conn: &Connection,
    subject_id: Option<i64>,
    kind: ItemKind,
    title: &str,
    prompt: Option<&str>,
    body: &str,
    model: Option<&str>,
    now: DateTime<Utc>,
) -> Result<i64> {
    let trimmed = title.trim();

    conn.execute(
        "INSERT INTO library_items
           (subject_id, kind, title, prompt, body, model, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            subject_id,
            kind.as_str(),
            if trimmed.is_empty() { fallback_title(kind, body) } else { trimmed.to_string() },
            prompt,
            body,
            model,
            util::rfc3339(now),
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

/// A title derived from the content, for generations that don't have one.
///
/// The first meaningful line, trimmed of Markdown heading marks. An item called
/// "Notes" among forty other items called "Notes" is unfindable.
fn fallback_title(kind: ItemKind, body: &str) -> String {
    let first = body
        .lines()
        .map(|l| l.trim_start_matches(['#', '*', '-', ' ']).trim())
        .find(|l| l.len() > 3)
        .unwrap_or("");

    if first.is_empty() {
        return match kind {
            ItemKind::Notes => "Notes",
            ItemKind::PracticeQuestion => "Practice question",
            ItemKind::WeeklyReview => "Weekly review",
            ItemKind::Cards => "Generated cards",
            ItemKind::Answer => "Answer",
        }
        .to_string();
    }

    let mut title: String = first.chars().take(80).collect();
    if first.chars().count() > 80 {
        title.push('…');
    }
    title
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub subject_id: Option<i64>,
    pub kind: Option<String>,
    pub search: Option<String>,
    pub only_pinned: Option<bool>,
}

pub fn list(conn: &Connection, filter: &Filter, limit: i64) -> Result<Vec<Item>> {
    let search = filter
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));

    let mut stmt = conn.prepare(
        "SELECT l.id, l.subject_id, s.name, s.colour, l.kind, l.title, l.prompt, l.body,
                l.model, l.pinned, l.created_at
           FROM library_items l
           LEFT JOIN subjects s ON s.id = l.subject_id
          WHERE (?1 IS NULL OR l.subject_id = ?1)
            AND (?2 IS NULL OR l.kind = ?2)
            AND (?3 IS NULL OR l.title LIKE ?3 OR l.body LIKE ?3)
            AND (?4 = 0 OR l.pinned = 1)
          ORDER BY l.pinned DESC, l.created_at DESC, l.id DESC
          LIMIT ?5",
    )?;

    let rows = stmt
        .query_map(
            rusqlite::params![
                filter.subject_id,
                filter.kind,
                search,
                filter.only_pinned.unwrap_or(false) as i64,
                limit
            ],
            |r| {
                Ok(Item {
                    id: r.get(0)?,
                    subject_id: r.get(1)?,
                    subject_name: r.get(2)?,
                    colour: r.get(3)?,
                    kind: ItemKind::parse(&r.get::<_, String>(4)?),
                    title: r.get(5)?,
                    prompt: r.get(6)?,
                    body: r.get(7)?,
                    model: r.get(8)?,
                    pinned: r.get::<_, i64>(9)? == 1,
                    created_at: r.get(10)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

pub fn set_pinned(conn: &Connection, id: i64, pinned: bool) -> Result<()> {
    conn.execute(
        "UPDATE library_items SET pinned = ?2 WHERE id = ?1",
        rusqlite::params![id, pinned as i64],
    )?;
    Ok(())
}

pub fn rename(conn: &Connection, id: i64, title: &str) -> Result<()> {
    let title = title.trim();
    if title.is_empty() {
        return Ok(());
    }
    conn.execute(
        "UPDATE library_items SET title = ?2 WHERE id = ?1",
        rusqlite::params![id, title],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM library_items WHERE id = ?1", [id])?;
    Ok(())
}

/// One item as a Markdown document, for saving to disk or printing.
///
/// Front matter carries the provenance — when, which subject, which model, and
/// what was asked. A note you find in six months with none of that attached is
/// hard to trust and impossible to reproduce.
pub fn to_markdown(item: &Item) -> String {
    let mut out = format!("# {}\n\n", item.title);

    let mut meta: Vec<String> = Vec::new();
    if let Some(s) = &item.subject_name {
        meta.push(format!("**Subject:** {s}"));
    }
    meta.push(format!("**Created:** {}", &item.created_at[..10.min(item.created_at.len())]));
    if let Some(m) = &item.model {
        meta.push(format!("**Generated by:** {m}"));
    }
    out.push_str(&meta.join("  \n"));
    out.push_str("\n\n");

    if let Some(p) = &item.prompt {
        if !p.trim().is_empty() {
            out.push_str(&format!("> **Asked:** {}\n\n", p.trim()));
        }
    }

    out.push_str("---\n\n");
    out.push_str(item.body.trim());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests;
