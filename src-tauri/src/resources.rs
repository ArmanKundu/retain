//! Study material you supply, and finding the relevant bits of it.
//!
//! ## Why this exists
//!
//! Without it, an AI feature answering a Biology question is working from
//! whatever the model happens to remember about VCE Biology — which is exactly
//! the failure mode the rest of this app is built to avoid. With your own study
//! design and past papers loaded, a generated practice question can be grounded
//! in the actual document rather than a plausible reconstruction of it.
//!
//! ## How retrieval works, and what it isn't
//!
//! Chunks are indexed with SQLite's FTS5 and retrieved by keyword relevance
//! (BM25). This is **not** semantic search — there are no embeddings and no
//! vector database, because both would mean either a network round trip per
//! query or a model file several times the size of the app. For a personal
//! corpus of a study design and a dozen past papers, keyword search over
//! stemmed terms finds the right pages, and it works with the wifi off.
//!
//! The honest limitation: ask about "protein synthesis" and it finds pages
//! containing those words. Ask about "how cells make things" and it may not.
//! The UI shows which excerpts were used, so a bad retrieval is visible rather
//! than silently shaping an answer.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::util;

/// Target chunk size in characters.
///
/// Roughly 250 words — big enough to hold a complete key-knowledge point or an
/// exam question with its stem, small enough that several fit in a prompt
/// alongside the actual instruction.
const CHUNK_CHARS: usize = 1400;

/// How much a chunk may overlap the previous one, so a point split across a
/// boundary still appears whole in at least one chunk.
const CHUNK_OVERLAP: usize = 180;

/// Nothing longer than this is accepted from one file. A whole textbook would
/// be indexed happily and then never retrieved usefully.
const MAX_CHARS: usize = 4_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    StudyDesign,
    PastPaper,
    ExamSolution,
    SchoolNotes,
    PersonalNotes,
    Textbook,
    Other,
}

impl ResourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourceKind::StudyDesign => "study_design",
            ResourceKind::PastPaper => "past_paper",
            ResourceKind::ExamSolution => "exam_solution",
            ResourceKind::SchoolNotes => "school_notes",
            ResourceKind::PersonalNotes => "personal_notes",
            ResourceKind::Textbook => "textbook",
            ResourceKind::Other => "other",
        }
    }

    /// The subfolder this kind lives in.
    ///
    /// This is the filing system: `Retain/Biology/Past papers/`. Dropping a PDF
    /// into the right folder is the only tagging you do, and it carries both
    /// the subject and the kind — which is why there's no per-file form.
    pub fn folder(self) -> &'static str {
        match self {
            ResourceKind::StudyDesign => "Study design",
            ResourceKind::PastPaper => "Past papers",
            ResourceKind::ExamSolution => "Solutions and reports",
            ResourceKind::SchoolNotes => "School notes",
            ResourceKind::PersonalNotes => "My notes",
            ResourceKind::Textbook => "Textbook",
            ResourceKind::Other => "Other",
        }
    }

    /// Every kind, in the order they should be offered and created.
    pub fn all() -> [ResourceKind; 7] {
        [
            ResourceKind::StudyDesign,
            ResourceKind::PastPaper,
            ResourceKind::ExamSolution,
            ResourceKind::SchoolNotes,
            ResourceKind::PersonalNotes,
            ResourceKind::Textbook,
            ResourceKind::Other,
        ]
    }

    /// How much weight an answer should give this.
    ///
    /// Lower sorts first. The study design outranks everything because it
    /// defines what's examinable; a marking scheme outranks a past paper
    /// because it says what actually earned marks; your own notes come last
    /// because they record what you understood at the time, which is exactly
    /// what you're trying to correct.
    pub fn authority(self) -> u8 {
        match self {
            ResourceKind::StudyDesign => 0,
            ResourceKind::ExamSolution => 1,
            ResourceKind::PastPaper => 2,
            ResourceKind::Textbook => 3,
            ResourceKind::SchoolNotes => 4,
            ResourceKind::PersonalNotes => 5,
            ResourceKind::Other => 6,
        }
    }

    /// How a retrieved excerpt should be introduced to the model.
    ///
    /// The distinction matters: a study design says what is examinable, a past
    /// paper shows how it gets asked. Labelling them identically invites the
    /// model to treat an old exam question as a syllabus requirement.
    pub fn context_label(self) -> &'static str {
        match self {
            ResourceKind::StudyDesign => "From the study design (authoritative on what is examinable)",
            ResourceKind::PastPaper => "From a past exam paper",
            ResourceKind::ExamSolution => "From a marking scheme or examiner's report",
            ResourceKind::SchoolNotes => "From school notes",
            ResourceKind::PersonalNotes => "From the student's own notes",
            ResourceKind::Textbook => "From a textbook",
            ResourceKind::Other => "From the student's material",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "study_design" => ResourceKind::StudyDesign,
            "past_paper" => ResourceKind::PastPaper,
            "exam_solution" => ResourceKind::ExamSolution,
            // Pre-migration rows said only "notes" without saying whose.
            "school_notes" | "notes" => ResourceKind::SchoolNotes,
            "personal_notes" => ResourceKind::PersonalNotes,
            "textbook" => ResourceKind::Textbook,
            _ => ResourceKind::Other,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub id: i64,
    pub subject_id: Option<i64>,
    pub subject_name: Option<String>,
    pub title: String,
    pub kind: ResourceKind,
    pub source: Option<String>,
    pub word_count: i64,
    pub chunk_count: i64,
    pub added_at: String,
}

/// An excerpt retrieved for a question, with enough provenance to check it.
///
/// `Deserialize` because citations are stored as JSON alongside the answer they
/// grounded, and read back when the conversation is reopened.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Excerpt {
    pub resource_id: i64,
    pub resource_title: String,
    pub kind: ResourceKind,
    pub ordinal: i64,
    pub content: String,
}

// ---------------------------------------------------------------------------
// Text preparation
// ---------------------------------------------------------------------------

/// Tidy extracted text without destroying its structure.
///
/// PDF extraction in particular produces hard-wrapped lines, page furniture and
/// runs of blank lines. Collapsing all whitespace would merge separate dot
/// points into one paragraph and make chunk boundaries meaningless, so blank
/// lines — which mark real breaks — are preserved.
pub fn normalise(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut blank_run = 0usize;

    for line in raw.replace('\r', "\n").lines() {
        // Collapse runs of spaces and strip trailing whitespace.
        let cleaned = line.split_whitespace().collect::<Vec<_>>().join(" ");

        if cleaned.is_empty() {
            blank_run += 1;
            // At most one blank line survives; PDFs often have a dozen.
            if blank_run == 1 {
                out.push('\n');
            }
            continue;
        }

        blank_run = 0;
        out.push_str(&cleaned);
        out.push('\n');
    }

    out.trim().to_string()
}

/// Split text into overlapping chunks, preferring paragraph then sentence
/// boundaries so an excerpt rarely begins or ends mid-thought.
pub fn chunk(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    if chars.len() <= CHUNK_CHARS {
        return vec![text.to_string()];
    }

    let mut out = Vec::new();
    let mut start = 0usize;

    while start < chars.len() {
        let hard_end = (start + CHUNK_CHARS).min(chars.len());

        // Look backwards from the target end for a natural break, but never
        // give up more than a third of the chunk to find one.
        let floor = start + (CHUNK_CHARS * 2 / 3);
        let mut end = hard_end;

        if hard_end < chars.len() {
            let mut found = None;
            for i in (floor.min(hard_end)..hard_end).rev() {
                if chars[i] == '\n' {
                    found = Some(i + 1);
                    break;
                }
                if found.is_none() && (chars[i] == '.' || chars[i] == '?' || chars[i] == '!') {
                    found = Some(i + 1);
                }
            }
            if let Some(f) = found {
                end = f;
            }
        }

        let piece: String = chars[start..end].iter().collect();
        let piece = piece.trim().to_string();
        if !piece.is_empty() {
            out.push(piece);
        }

        if end >= chars.len() {
            break;
        }
        // Step forward, minus the overlap. `max(1)` guarantees progress even if
        // a pathological boundary search returned something tiny.
        start = end.saturating_sub(CHUNK_OVERLAP).max(start + 1);
    }

    out
}

pub fn word_count(text: &str) -> i64 {
    text.split_whitespace().count() as i64
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/// Add a resource that came from a file on disk.
///
/// `origin_path` is what makes a folder re-syncable: the next Sync sees the
/// path is already indexed and skips it rather than storing a second copy.
#[allow(clippy::too_many_arguments)]
pub fn add_from_file(
    conn: &mut Connection,
    subject_id: Option<i64>,
    title: &str,
    kind: ResourceKind,
    source: Option<&str>,
    raw: &str,
    origin_path: Option<&str>,
    now: DateTime<Utc>,
) -> Result<i64> {
    let id = add(conn, subject_id, title, kind, source, raw, now)?;
    if let Some(path) = origin_path {
        conn.execute(
            "UPDATE resources SET origin_path = ?2 WHERE id = ?1",
            rusqlite::params![id, path],
        )?;
    }
    Ok(id)
}

pub fn add(
    conn: &mut Connection,
    subject_id: Option<i64>,
    title: &str,
    kind: ResourceKind,
    source: Option<&str>,
    raw: &str,
    now: DateTime<Utc>,
) -> Result<i64> {
    let content = normalise(raw);

    if content.trim().is_empty() {
        return Err(anyhow!(
            "That file has no readable text in it. If it's a scanned PDF, the pages are images \
             rather than text and there's nothing to extract."
        ));
    }
    if content.chars().count() > MAX_CHARS {
        return Err(anyhow!("That's far larger than a study design or exam; not importing it."));
    }

    let title = title.trim();
    let title = if title.is_empty() { "Untitled" } else { title };

    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO resources (subject_id, title, kind, source, content, word_count, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            subject_id,
            title,
            kind.as_str(),
            source,
            content,
            word_count(&content),
            util::rfc3339(now),
        ],
    )?;
    let id = tx.last_insert_rowid();

    {
        let mut stmt =
            tx.prepare("INSERT INTO resource_chunks (resource_id, ordinal, content) VALUES (?1,?2,?3)")?;
        for (i, piece) in chunk(&content).into_iter().enumerate() {
            stmt.execute(rusqlite::params![id, i as i64, piece])?;
        }
    }

    tx.commit()?;
    Ok(id)
}

/// Re-chunk anything whose text is present but whose chunks are gone.
///
/// Migration 005 rebuilt the `resources` table. With `foreign_keys = ON` — which
/// is how the app opens the database — SQLite's `DROP TABLE` runs an implicit
/// `DELETE FROM` first, and that fired `resource_chunks`' `ON DELETE CASCADE`.
/// Every chunk of every uploaded resource was deleted, and the `'rebuild'` of
/// the FTS index that followed dutifully rebuilt it from nothing. The library
/// still listed six study designs; searching them returned nothing.
///
/// The text itself was never lost — `resources.content` came through the rebuild
/// intact — so this re-derives the chunks from it with the ordinary chunker.
///
/// Runs at startup. Cheap when there's nothing to do: one indexed count per
/// resource, and no writes.
pub fn reindex_missing(conn: &mut Connection) -> Result<usize> {
    let stale: Vec<(i64, String)> = {
        let mut stmt = conn.prepare(
            "SELECT r.id, r.content FROM resources r
              WHERE length(r.content) > 0
                AND NOT EXISTS (SELECT 1 FROM resource_chunks c WHERE c.resource_id = r.id)",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    if stale.is_empty() {
        return Ok(0);
    }

    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO resource_chunks (resource_id, ordinal, content) VALUES (?1,?2,?3)",
        )?;
        for (id, content) in &stale {
            for (i, piece) in chunk(content).into_iter().enumerate() {
                stmt.execute(rusqlite::params![id, i as i64, piece])?;
            }
        }
    }
    tx.commit()?;

    Ok(stale.len())
}

pub fn list(conn: &Connection, subject_id: Option<i64>) -> Result<Vec<Resource>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.subject_id, s.name, r.title, r.kind, r.source, r.word_count,
                (SELECT COUNT(*) FROM resource_chunks c WHERE c.resource_id = r.id),
                r.added_at
           FROM resources r
           LEFT JOIN subjects s ON s.id = r.subject_id
          WHERE (?1 IS NULL OR r.subject_id = ?1)
          ORDER BY r.added_at DESC, r.id DESC",
    )?;

    let rows = stmt
        .query_map([subject_id], |r| {
            Ok(Resource {
                id: r.get(0)?,
                subject_id: r.get(1)?,
                subject_name: r.get(2)?,
                title: r.get(3)?,
                kind: ResourceKind::parse(&r.get::<_, String>(4)?),
                source: r.get(5)?,
                word_count: r.get(6)?,
                chunk_count: r.get(7)?,
                added_at: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    // `ON DELETE CASCADE` removes the chunks, and the FTS delete trigger fires
    // for each — which is what keeps removed material from still being
    // retrieved as context.
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    conn.execute("DELETE FROM resources WHERE id = ?1", [id])?;
    Ok(())
}

/// Turn a question into an FTS5 MATCH expression.
///
/// Built by hand rather than passed through, because FTS5 query syntax treats
/// `"`, `*`, `:`, `-` and `^` as operators — a student's question containing an
/// apostrophe or a hyphen would otherwise be a syntax error rather than a
/// search. Every term is quoted, which makes them literal.
pub fn to_match_query(question: &str) -> Option<String> {
    // Very common words match nearly every chunk and drown out the real terms.
    const STOP: &[&str] = &[
        "the", "a", "an", "and", "or", "of", "in", "to", "for", "is", "are", "was", "were", "be",
        "how", "what", "why", "which", "does", "do", "did", "that", "this", "it", "as", "on",
        "with", "at", "by", "from", "can", "you", "your", "me", "my", "i",
    ];

    let terms: Vec<String> = question
        .split(|c: char| !c.is_alphanumeric())
        .map(|t| t.trim().to_lowercase())
        .filter(|t| t.len() >= 3 && !STOP.contains(&t.as_str()))
        .take(12)
        .map(|t| format!("\"{t}\""))
        .collect();

    if terms.is_empty() {
        return None;
    }
    // OR rather than AND: a question rarely shares every term with the page
    // that answers it, and BM25 ranking already pushes the best matches up.
    Some(terms.join(" OR "))
}

/// The most relevant excerpts for a question.
pub fn search(
    conn: &Connection,
    question: &str,
    subject_id: Option<i64>,
    limit: i64,
) -> Result<Vec<Excerpt>> {
    let Some(query) = to_match_query(question) else {
        return Ok(Vec::new());
    };

    let mut stmt = conn.prepare(
        "SELECT r.id, r.title, r.kind, c.ordinal, c.content
           FROM resource_chunks_fts f
           JOIN resource_chunks c ON c.id = f.rowid
           JOIN resources r       ON r.id = c.resource_id
          WHERE resource_chunks_fts MATCH ?1
            AND (?2 IS NULL OR r.subject_id = ?2)
          ORDER BY bm25(resource_chunks_fts) LIMIT ?3",
    )?;

    let rows: Vec<Excerpt> = stmt
        .query_map(rusqlite::params![query, subject_id, limit], |r| {
            Ok(Excerpt {
                resource_id: r.get(0)?,
                resource_title: r.get(1)?,
                kind: ResourceKind::parse(&r.get::<_, String>(2)?),
                ordinal: r.get(3)?,
                content: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

/// Order excerpts so the most authoritative sources are read first.
///
/// BM25 ranks by textual relevance, which is the right first pass but the wrong
/// last word: a lucky keyword match in your own notes should not outrank the
/// study design paragraph that defines the term. Models weight earlier context
/// less than later, so this puts the authoritative material where it will be
/// read as the frame rather than as an afterthought.
pub fn by_authority(mut excerpts: Vec<Excerpt>) -> Vec<Excerpt> {
    excerpts.sort_by_key(|e| e.kind.authority());
    excerpts
}

/// Render excerpts as a context block for a prompt.
///
/// Returns `None` when nothing was retrieved, so callers can fall back to an
/// ungrounded prompt rather than sending an empty "here is your material"
/// preamble that invites the model to invent some.
pub fn context_block(excerpts: &[Excerpt]) -> Option<String> {
    if excerpts.is_empty() {
        return None;
    }

    let mut out = String::from(
        "Use the student's own material below where it is relevant. It is authoritative — \
         prefer it over your own recollection, and say so if it doesn't cover the question.\n",
    );

    for e in excerpts {
        out.push_str(&format!(
            "\n--- {} ({}) ---\n{}\n",
            e.kind.context_label(),
            e.resource_title,
            e.content.trim()
        ));
    }

    Some(out)
}

#[cfg(test)]
mod tests;
