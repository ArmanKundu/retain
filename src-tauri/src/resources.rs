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
    /// A school's practice exam. Not VCAA — someone's best guess at what VCAA
    /// will ask — so it is weighted below the real thing and above notes.
    TrialTest,
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
            ResourceKind::TrialTest => "trial_test",
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
            ResourceKind::TrialTest => "Trial tests",
            ResourceKind::ExamSolution => "Solutions and reports",
            ResourceKind::SchoolNotes => "School notes",
            ResourceKind::PersonalNotes => "My notes",
            ResourceKind::Textbook => "Textbook",
            ResourceKind::Other => "Other",
        }
    }

    /// Every kind, in the order they should be offered and created.
    pub fn all() -> [ResourceKind; 8] {
        [
            ResourceKind::StudyDesign,
            ResourceKind::PastPaper,
            ResourceKind::ExamSolution,
            ResourceKind::TrialTest,
            ResourceKind::Textbook,
            ResourceKind::SchoolNotes,
            ResourceKind::PersonalNotes,
            ResourceKind::Other,
        ]
    }

    /// Whether this kind is filed per unit.
    ///
    /// The dimension does not apply evenly, and pretending it does is what
    /// makes a folder tree annoying rather than useful. A study design covers
    /// the whole 3&4 sequence; a VCAA exam examines both units in one paper; a
    /// textbook spans the year. Asking which unit those belong to has no
    /// answer. Your notes, your school's notes and your trial tests are the
    /// things you genuinely keep per unit, so those are the only ones that get
    /// a unit folder.
    pub fn per_unit(self) -> bool {
        matches!(
            self,
            ResourceKind::SchoolNotes | ResourceKind::PersonalNotes | ResourceKind::TrialTest
        )
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
            // A trial exam is a school's prediction of a VCAA paper. Useful, and
            // not evidence of what VCAA actually asks — so it sits below the
            // real paper and above anyone's notes about it.
            ResourceKind::TrialTest => 3,
            ResourceKind::Textbook => 4,
            ResourceKind::SchoolNotes => 5,
            ResourceKind::PersonalNotes => 6,
            ResourceKind::Other => 7,
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
            ResourceKind::TrialTest => "From a school trial exam (not a VCAA paper)",
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
            "trial_test" => ResourceKind::TrialTest,
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
    // The folder a file was dropped into is what says which unit it belongs to.
    let unit = origin_path.and_then(|p| crate::workspace::unit_from_path(std::path::Path::new(p)));
    let id = add(conn, subject_id, title, kind, unit, source, raw, now)?;
    if let Some(path) = origin_path {
        conn.execute(
            "UPDATE resources SET origin_path = ?2 WHERE id = ?1",
            rusqlite::params![id, path],
        )?;
    }
    Ok(id)
}

#[allow(clippy::too_many_arguments)]
pub fn add(
    conn: &mut Connection,
    subject_id: Option<i64>,
    title: &str,
    kind: ResourceKind,
    // 3, 4, or `None` for material that spans the sequence — a study design, a
    // VCAA exam, a textbook. `None` is a real answer here, not missing data.
    unit: Option<i64>,
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
        "INSERT INTO resources (subject_id, title, kind, unit, source, content, word_count, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            subject_id,
            title,
            kind.as_str(),
            // Only kinds that are genuinely filed per unit carry one; storing a
            // unit on a study design would make it invisible to a search for the
            // other unit's material.
            unit.filter(|_| kind.per_unit()),
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

// ---------------------------------------------------------------------------
// Topics, from the study design
// ---------------------------------------------------------------------------

/// The bullet a study design uses for its dot points.
///
/// `U+F0B7`, a private-use character from the Symbol font — which is what the
/// PDF actually contains, not a real bullet. Extraction preserves it, so it is
/// the most reliable marker in the document.
const SD_BULLET: char = '\u{F0B7}';

/// Headings that end a run of key knowledge.
const SECTION_ENDS: [&str; 6] = [
    // Any "Key …skills" heading. English and Accounting write "Key skills"
    // without "science", and it was coming through as a topic.
    "key skills",
    "unit ",
    "area of study",
    "outcome",
    "assessment",
    "detailed study",
];

/// Pull the topic names out of a study design.
///
/// Automatic tagging matches questions against the student's topic list, and
/// that list was empty — nobody types thirty topic names by hand, so nothing
/// was ever tagged. The names are sitting in the study design they already
/// uploaded: VCAA writes a heading above each run of dot points.
///
/// ```text
/// Key knowledge
///
/// Cellular structure and function      <- this
///
///  cells as the basic structural…    <- dot points
/// ```
///
/// Taken verbatim. These are VCAA's words out of the student's own file, which
/// is the difference between reading a curriculum and inventing one.
pub fn topics_from_study_design(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_key_knowledge = false;
    // Set by an `Area of Study N` line; the next real line is its title.
    let mut expect_aos_title = false;

    let lines: Vec<&str> = content.lines().collect();
    for raw in lines {
        let line = raw.trim();
        let lower = line.to_lowercase();

        if lower == "key knowledge" {
            in_key_knowledge = true;
            expect_aos_title = false;
            continue;
        }

        // Some subjects list dot points straight under Key knowledge with no
        // headings between — Maths and English do. Their Area of Study titles
        // are the only topic names the document carries, and they are still
        // VCAA's own words.
        if lower.starts_with("area of study") {
            in_key_knowledge = false;
            // A contents-page line is padded out with dots to a page number.
            expect_aos_title = !line.contains("....");
            continue;
        }
        if expect_aos_title {
            if line.is_empty() {
                continue;
            }
            expect_aos_title = false;
            let clean = line.trim_end_matches(['.', ':']).trim();
            let words = clean.split_whitespace().count();
            if (2..=12).contains(&words)
                && clean.chars().count() <= 90
                && clean.chars().next().is_some_and(|c| c.is_alphabetic())
                && !out.iter().any(|t| t.eq_ignore_ascii_case(clean))
            {
                out.push(clean.to_string());
            }
            continue;
        }

        if !in_key_knowledge {
            continue;
        }

        // A dot point is the content under a heading, not a heading. Some
        // subjects nest a second level with a minus or a dash, and those are
        // dot points too.
        if line.starts_with(SD_BULLET)
            || line.starts_with(['•', '\u{2212}', '\u{2013}', '\u{2014}', '-', '*'])
        {
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if SECTION_ENDS.iter().any(|s| lower.starts_with(s))
            || (lower.starts_with("key ") && lower.ends_with("skills"))
        {
            in_key_knowledge = false;
            continue;
        }
        // Page furniture, which sits between the headings.
        if lower.contains("study design") || lower.contains("vcaa") || lower.starts_with('©') {
            continue;
        }
        // A heading is a few words. Anything longer is the wrapped tail of a
        // dot point, whose bullet was on the previous line.
        let words = line.split_whitespace().count();
        if !(1..=9).contains(&words) || line.chars().count() > 80 {
            continue;
        }
        // A heading starts with a letter, in upper case. A wrapped tail starts
        // lower case, or with the bracket it was broken inside.
        if !line.chars().next().is_some_and(|c| c.is_alphabetic() && c.is_uppercase()) {
            continue;
        }

        let clean = line.trim_end_matches(['.', ':']).trim().to_string();
        if !clean.is_empty() && !out.iter().any(|t| t.eq_ignore_ascii_case(&clean)) {
            out.push(clean);
        }
    }

    out
}

/// A topic heading together with the words VCAA uses to describe it.
///
/// Matching a question against topic *names* barely works: a heading reads
/// "Cellular structure and function" and the question says "active transport
/// across the plasma membrane". Measured on the real library, whole-name
/// matching tagged 37 questions out of 6,529.
///
/// The vocabulary is in the dot points underneath each heading, and it is the
/// vocabulary the exam uses — "osmosis", "chloroplasts", "facilitated
/// diffusion". Still VCAA's own words out of the student's own file.
#[derive(Debug, Clone, PartialEq)]
pub struct TopicVocabulary {
    pub name: String,
    /// Distinctive words from the dot points. Lowercased, deduplicated.
    pub terms: Vec<String>,
}

/// Words too common to identify anything, even at five letters or more.
///
/// Deliberately short. A long stop list starts removing real terms, and the
/// length filter already excludes most function words.
const NOT_DISTINCTIVE: [&str; 24] = [
    "including", "between", "their", "which", "these", "those", "there", "where", "other",
    "different", "various", "example", "examples", "understanding", "identify", "describe",
    "explain", "explaining", "involving", "related", "relating", "including", "through",
    "within",
];

/// Headings that are about how the subject is assessed, not what it contains.
///
/// A study design carries a run of these near the end. "Satisfactory
/// completion" tagged 5,558 of 6,529 real questions on its first run — its
/// vocabulary is the generic language of assessment, which every question
/// shares.
const ADMINISTRATIVE: [&str; 8] = [
    "satisfactory completion",
    "levels of achievement",
    "school-based assessment",
    "external assessment",
    "scope of study",
    "entry",
    "duration",
    "changes to the study design",
];

/// Headings and their vocabulary, in document order.
///
/// Terms shared by most of a subject's topics are dropped afterwards: a word
/// that appears under every heading distinguishes none of them, and those are
/// exactly the words administrative sections are made of.
pub fn topic_vocabulary(content: &str) -> Vec<TopicVocabulary> {
    let mut out: Vec<TopicVocabulary> = Vec::new();
    let names = topics_from_study_design(content);
    if names.is_empty() {
        return out;
    }

    // Walk the document again, attributing each line to whichever heading it
    // last passed. A dot point belongs to the heading above it.
    let mut current: Option<String> = None;
    let mut terms: Vec<String> = Vec::new();

    let flush = |out: &mut Vec<TopicVocabulary>, name: Option<String>, terms: &mut Vec<String>| {
        if let Some(name) = name {
            terms.sort();
            terms.dedup();
            if !terms.is_empty() {
                out.push(TopicVocabulary { name, terms: std::mem::take(terms) });
            } else {
                terms.clear();
            }
        }
    };

    for raw in content.lines() {
        let line = raw.trim();
        let clean = line.trim_end_matches(['.', ':']).trim();

        if let Some(name) = names.iter().find(|n| n.as_str() == clean) {
            flush(&mut out, current.take(), &mut terms);
            current = Some(name.clone());
            continue;
        }
        if current.is_none() {
            continue;
        }

        for word in line.split(|c: char| !c.is_alphanumeric()) {
            let w = word.to_lowercase();
            if w.chars().count() >= 5
                && w.chars().next().is_some_and(|c| c.is_alphabetic())
                && !NOT_DISTINCTIVE.contains(&w.as_str())
            {
                terms.push(w);
            }
        }
    }
    flush(&mut out, current.take(), &mut terms);

    out.retain(|t| !ADMINISTRATIVE.iter().any(|a| t.name.to_lowercase().starts_with(a)));
    drop_shared_terms(&mut out);
    out.retain(|t| t.terms.len() >= 4);
    out
}

/// Remove terms that appear under a third or more of the headings.
///
/// This is the cheap form of inverse document frequency, and it is what stops
/// generic vocabulary from tagging everything: "students" and "analysis" sit
/// under most headings in a study design and identify none of them, while
/// "chloroplasts" sits under one.
fn drop_shared_terms(topics: &mut [TopicVocabulary]) {
    if topics.len() < 3 {
        return;
    }

    let mut seen: std::collections::HashMap<&str, usize> = Default::default();
    for t in topics.iter() {
        for term in &t.terms {
            *seen.entry(term.as_str()).or_insert(0) += 1;
        }
    }

    // Two headings is enough to disqualify a term. A word that describes two
    // different topics cannot tell you which one a question belongs to, and
    // being strict here is what stops the skills sections — whose vocabulary
    // is the generic language of scientific method — from tagging everything.
    let common: std::collections::HashSet<String> = seen
        .into_iter()
        .filter(|(_, n)| *n >= 2)
        .map(|(w, _)| w.to_string())
        .collect();

    for t in topics.iter_mut() {
        t.terms.retain(|term| !common.contains(term));
    }
}

/// Create topics for a subject from its study designs.
///
/// Idempotent — a topic that already exists is left alone, so re-running after
/// uploading a newer study design adds what's new rather than duplicating
/// everything.
pub fn import_topics(conn: &Connection, subject_id: i64) -> Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT content FROM resources WHERE subject_id = ?1 AND kind = 'study_design'",
    )?;
    let designs: Vec<String> = stmt
        .query_map([subject_id], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    let mut added = 0;
    for (order, name) in designs
        .iter()
        .flat_map(|d| topics_from_study_design(d))
        .enumerate()
    {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM topics WHERE subject_id = ?1 AND lower(name) = lower(?2)",
            rusqlite::params![subject_id, name],
            |r| r.get(0),
        )?;
        if exists > 0 {
            continue;
        }
        conn.execute(
            "INSERT INTO topics (subject_id, name, kind, sort_order) VALUES (?1, ?2, 'aos', ?3)",
            rusqlite::params![subject_id, name, order as i64],
        )?;
        added += 1;
    }

    Ok(added)
}

#[cfg(test)]
mod tests;
