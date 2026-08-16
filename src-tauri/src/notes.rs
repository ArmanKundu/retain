//! Writing things down.
//!
//! A note is an ordered list of blocks — a paragraph, a heading, a checkbox, a
//! screenshot. That shape is the whole reason this isn't a text column: a
//! checkbox has state, an image has bytes, and you have to be able to move a
//! block without rewriting the document around it.
//!
//! # Ordering, which is the part that goes wrong
//!
//! `position` is dense and contiguous — 0, 1, 2 — with a unique index on
//! `(note_id, position)`. Every structural change renumbers the whole note.
//!
//! The usual alternative is fractional indices: insert between 1 and 2 at 1.5,
//! never touch the neighbours. That's the right answer for a document with a
//! million rows and several writers. Here it buys nothing and costs precision:
//! repeatedly inserting between the same two blocks halves the gap each time,
//! and after about fifty insertions in one spot the floats stop being
//! distinguishable and the order silently scrambles. A note is dozens of rows
//! and one writer, so the renumber is cheaper than that class of bug.
//!
//! The unique index is what makes this safe rather than merely tidy: a
//! renumbering that leaves two blocks on the same position fails loudly instead
//! of producing a note whose order depends on which row SQLite reads first.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::util;

/// Every kind of block. Adding one means touching the CHECK constraint in
/// migration 012, so the two can't drift apart silently.
pub const KINDS: [&str; 11] = [
    "paragraph", "h1", "h2", "h3", "bullet", "numbered", "todo", "quote", "code", "divider",
    "image",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub id: i64,
    pub position: i64,
    pub kind: String,
    pub text: String,
    pub checked: bool,
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NoteSummary {
    pub id: i64,
    pub subject_id: Option<i64>,
    pub subject_name: Option<String>,
    pub colour: Option<String>,
    pub title: String,
    pub on_date: Option<String>,
    /// First few words of the body, for the list. A list of "Untitled" rows is
    /// unusable, and most notes never get a title typed into them.
    pub preview: String,
    pub block_count: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: i64,
    pub subject_id: Option<i64>,
    /// Joined for the printed header — a page with no subject on it is one you
    /// can't file.
    pub subject_name: Option<String>,
    pub topic_id: Option<i64>,
    pub title: String,
    pub on_date: Option<String>,
    pub blocks: Vec<Block>,
    pub updated_at: String,
}

fn valid_kind(kind: &str) -> Result<()> {
    if KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(anyhow!("\"{kind}\" isn't a block type."))
    }
}

/// Start a note. Always has one empty paragraph, so there is somewhere to type
/// the instant it opens — an editor that needs a click before it accepts a
/// keystroke is one you stop reaching for.
pub fn create(
    conn: &Connection,
    subject_id: Option<i64>,
    title: &str,
    on_date: Option<&str>,
    now: DateTime<Utc>,
) -> Result<i64> {
    let stamp = util::rfc3339(now);
    let title = match title.trim() {
        "" => "Untitled",
        t => t,
    };

    conn.execute(
        "INSERT INTO notes (subject_id, title, on_date, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        rusqlite::params![subject_id, title, on_date, stamp],
    )?;
    let id = conn.last_insert_rowid();

    conn.execute(
        "INSERT INTO note_blocks (note_id, position, kind, text) VALUES (?1, 0, 'paragraph', '')",
        [id],
    )?;
    Ok(id)
}

pub fn list(conn: &Connection, subject_id: Option<i64>, limit: i64) -> Result<Vec<NoteSummary>> {
    let sql = "SELECT n.id, n.subject_id, s.name, s.colour, n.title, n.on_date, n.updated_at,
                      (SELECT COUNT(*) FROM note_blocks b WHERE b.note_id = n.id),
                      -- The first block with anything in it, whatever kind it
                      -- is: a note that opens with a heading should preview the
                      -- heading rather than an empty string.
                      (SELECT b.text FROM note_blocks b
                        WHERE b.note_id = n.id AND TRIM(b.text) != ''
                        ORDER BY b.position LIMIT 1)
                 FROM notes n LEFT JOIN subjects s ON s.id = n.subject_id
                WHERE n.archived = 0 AND (?1 IS NULL OR n.subject_id = ?1)
                ORDER BY n.updated_at DESC, n.id DESC LIMIT ?2";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(rusqlite::params![subject_id, limit], |r| {
            let preview: Option<String> = r.get(9 - 1)?;
            Ok(NoteSummary {
                id: r.get(0)?,
                subject_id: r.get(1)?,
                subject_name: r.get(2)?,
                colour: r.get(3)?,
                title: r.get(4)?,
                on_date: r.get(5)?,
                updated_at: r.get(6)?,
                block_count: r.get(7)?,
                preview: preview.map(|p| truncate(&p, 90)).unwrap_or_default(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max - 1).collect::<String>())
}

pub fn get(conn: &Connection, id: i64) -> Result<Note> {
    let mut stmt = conn.prepare(
        "SELECT n.id, n.subject_id, n.topic_id, n.title, n.on_date, n.updated_at, s.name
           FROM notes n LEFT JOIN subjects s ON s.id = n.subject_id
          WHERE n.id = ?1",
    )?;
    let mut note = stmt
        .query_row([id], |r| {
            Ok(Note {
                id: r.get(0)?,
                subject_id: r.get(1)?,
                topic_id: r.get(2)?,
                title: r.get(3)?,
                on_date: r.get(4)?,
                updated_at: r.get(5)?,
                subject_name: r.get(6)?,
                blocks: Vec::new(),
            })
        })
        .optional()?
        .ok_or_else(|| anyhow!("That note no longer exists."))?;

    note.blocks = blocks(conn, id)?;
    Ok(note)
}

pub fn blocks(conn: &Connection, note_id: i64) -> Result<Vec<Block>> {
    let mut stmt = conn.prepare(
        "SELECT id, position, kind, text, checked, image
           FROM note_blocks WHERE note_id = ?1 ORDER BY position",
    )?;
    let rows = stmt
        .query_map([note_id], |r| {
            Ok(Block {
                id: r.get(0)?,
                position: r.get(1)?,
                kind: r.get(2)?,
                text: r.get(3)?,
                checked: r.get::<_, i64>(4)? == 1,
                image: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn touch(conn: &Connection, note_id: i64, now: DateTime<Utc>) -> Result<()> {
    conn.execute(
        "UPDATE notes SET updated_at = ?2 WHERE id = ?1",
        rusqlite::params![note_id, util::rfc3339(now)],
    )?;
    Ok(())
}

pub fn set_title(conn: &Connection, id: i64, title: &str, now: DateTime<Utc>) -> Result<()> {
    let title = match title.trim() {
        "" => "Untitled",
        t => t,
    };
    conn.execute(
        "UPDATE notes SET title = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id, title, util::rfc3339(now)],
    )?;
    Ok(())
}

pub fn set_subject(conn: &Connection, id: i64, subject_id: Option<i64>, now: DateTime<Utc>) -> Result<()> {
    conn.execute(
        "UPDATE notes SET subject_id = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id, subject_id, util::rfc3339(now)],
    )?;
    Ok(())
}

/// Edit one block in place. Never moves it.
pub fn update_block(
    conn: &Connection,
    block_id: i64,
    kind: &str,
    text: &str,
    checked: bool,
    image: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    valid_kind(kind)?;

    let note_id: i64 = conn
        .query_row("SELECT note_id FROM note_blocks WHERE id = ?1", [block_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| anyhow!("That block no longer exists."))?;

    conn.execute(
        "UPDATE note_blocks SET kind = ?2, text = ?3, checked = ?4, image = ?5 WHERE id = ?1",
        rusqlite::params![block_id, kind, text, checked as i64, image],
    )?;
    touch(conn, note_id, now)
}

/// Positions are parked here while a run of blocks is renumbered.
///
/// The unique index on `(note_id, position)` is checked per row, so shifting a
/// run upward one at a time collides the moment block 2 tries to become 3 while
/// 3 still exists. `UPDATE ... ORDER BY` would sequence it, but that needs
/// `SQLITE_ENABLE_UPDATE_DELETE_LIMIT`, which the bundled build does not set.
///
/// So the run is moved somewhere nothing else can be, then brought back. No
/// note has a million blocks, so the parking range is always empty.
const PARK: i64 = 1_000_000;

/// Add a block directly below `after`, or at the end when `after` is `None`.
pub fn insert_block(
    conn: &mut Connection,
    note_id: i64,
    after: Option<i64>,
    kind: &str,
    text: &str,
    now: DateTime<Utc>,
) -> Result<i64> {
    valid_kind(kind)?;
    let tx = conn.transaction()?;

    let at = match after {
        Some(block) => {
            let p: i64 = tx
                .query_row(
                    "SELECT position FROM note_blocks WHERE id = ?1 AND note_id = ?2",
                    rusqlite::params![block, note_id],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| anyhow!("That block isn't in this note."))?;
            p + 1
        }
        None => tx.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM note_blocks WHERE note_id = ?1",
            [note_id],
            |r| r.get(0),
        )?,
    };

    // Out of the way, then back one lower. See `PARK`.
    tx.execute(
        "UPDATE note_blocks SET position = position + ?3 WHERE note_id = ?1 AND position >= ?2",
        rusqlite::params![note_id, at, PARK],
    )?;
    tx.execute(
        "UPDATE note_blocks SET position = position - ?2 + 1 WHERE note_id = ?1 AND position >= ?2",
        rusqlite::params![note_id, PARK],
    )?;

    tx.execute(
        "INSERT INTO note_blocks (note_id, position, kind, text) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![note_id, at, kind, text],
    )?;
    let id = tx.last_insert_rowid();

    tx.execute(
        "UPDATE notes SET updated_at = ?2 WHERE id = ?1",
        rusqlite::params![note_id, util::rfc3339(now)],
    )?;
    tx.commit()?;
    Ok(id)
}

/// Remove a block and close the gap.
///
/// The last block is never deleted. An empty note with no blocks has nowhere to
/// put the cursor, so backspacing through everything would leave a document you
/// cannot type in — it is emptied instead.
pub fn delete_block(conn: &mut Connection, block_id: i64, now: DateTime<Utc>) -> Result<()> {
    let tx = conn.transaction()?;

    let (note_id, position): (i64, i64) = tx
        .query_row(
            "SELECT note_id, position FROM note_blocks WHERE id = ?1",
            [block_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow!("That block no longer exists."))?;

    let remaining: i64 =
        tx.query_row("SELECT COUNT(*) FROM note_blocks WHERE note_id = ?1", [note_id], |r| {
            r.get(0)
        })?;

    if remaining <= 1 {
        tx.execute(
            "UPDATE note_blocks SET kind = 'paragraph', text = '', checked = 0, image = NULL
              WHERE id = ?1",
            [block_id],
        )?;
    } else {
        tx.execute("DELETE FROM note_blocks WHERE id = ?1", [block_id])?;
        tx.execute(
            "UPDATE note_blocks SET position = position + ?3 WHERE note_id = ?1 AND position > ?2",
            rusqlite::params![note_id, position, PARK],
        )?;
        tx.execute(
            "UPDATE note_blocks SET position = position - ?2 - 1 WHERE note_id = ?1 AND position >= ?2",
            rusqlite::params![note_id, PARK],
        )?;
    }

    tx.execute(
        "UPDATE notes SET updated_at = ?2 WHERE id = ?1",
        rusqlite::params![note_id, util::rfc3339(now)],
    )?;
    tx.commit()?;
    Ok(())
}

/// Swap a block with its neighbour.
///
/// Done as a three-step shuffle through a position that cannot collide, because
/// the unique index rejects the obvious two-statement swap at the moment both
/// rows briefly hold the same number.
pub fn move_block(conn: &mut Connection, block_id: i64, delta: i64, now: DateTime<Utc>) -> Result<()> {
    let tx = conn.transaction()?;

    let (note_id, position): (i64, i64) = tx
        .query_row(
            "SELECT note_id, position FROM note_blocks WHERE id = ?1",
            [block_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| anyhow!("That block no longer exists."))?;

    let target = position + delta.signum();

    let neighbour: Option<i64> = tx
        .query_row(
            "SELECT id FROM note_blocks WHERE note_id = ?1 AND position = ?2",
            rusqlite::params![note_id, target],
            |r| r.get(0),
        )
        .optional()?;

    // Already at the top or the bottom. Not an error — you held the shortcut.
    let Some(neighbour) = neighbour else {
        return Ok(());
    };

    // -1 is outside the valid range, so it can never collide with a real block.
    tx.execute("UPDATE note_blocks SET position = -1 WHERE id = ?1", [block_id])?;
    tx.execute(
        "UPDATE note_blocks SET position = ?2 WHERE id = ?1",
        rusqlite::params![neighbour, position],
    )?;
    tx.execute(
        "UPDATE note_blocks SET position = ?2 WHERE id = ?1",
        rusqlite::params![block_id, target],
    )?;

    tx.execute(
        "UPDATE notes SET updated_at = ?2 WHERE id = ?1",
        rusqlite::params![note_id, util::rfc3339(now)],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    conn.execute("DELETE FROM notes WHERE id = ?1", [id])?;
    Ok(())
}

/// A note as Markdown, for export and printing.
///
/// The blocks are the storage and this is a projection of them — which is the
/// right way round. Storing Markdown and parsing it back would lose checkbox
/// state and images on every round trip.
pub fn to_markdown(note: &Note) -> String {
    let mut out = format!("# {}\n\n", note.title);

    let mut number = 0;
    for b in &note.blocks {
        if b.kind != "numbered" {
            number = 0;
        }
        match b.kind.as_str() {
            "h1" => out.push_str(&format!("## {}\n\n", b.text)),
            "h2" => out.push_str(&format!("### {}\n\n", b.text)),
            "h3" => out.push_str(&format!("#### {}\n\n", b.text)),
            "bullet" => out.push_str(&format!("- {}\n", b.text)),
            "numbered" => {
                number += 1;
                out.push_str(&format!("{number}. {}\n", b.text));
            }
            "todo" => out.push_str(&format!(
                "- [{}] {}\n",
                if b.checked { "x" } else { " " },
                b.text
            )),
            "quote" => out.push_str(&format!("> {}\n\n", b.text)),
            "code" => out.push_str(&format!("```\n{}\n```\n\n", b.text)),
            "divider" => out.push_str("---\n\n"),
            "image" => {
                // The data URL is inlined so the exported file stands alone. A
                // relative path would break the moment the file is moved.
                let alt = if b.text.is_empty() { "Screenshot" } else { &b.text };
                match &b.image {
                    Some(src) => out.push_str(&format!("![{alt}]({src})\n\n")),
                    None => out.push_str(&format!("*[{alt} — image missing]*\n\n")),
                }
            }
            _ => {
                if !b.text.trim().is_empty() {
                    out.push_str(&format!("{}\n\n", b.text));
                }
            }
        }
    }

    out
}

#[cfg(test)]
#[path = "notes/tests.rs"]
mod tests;
