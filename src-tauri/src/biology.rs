//! Biology Units 3 & 4.
//!
//! ## A deliberate omission, stated up front
//!
//! This module ships **no VCAA content**. There are no dot points, no Area of
//! Study titles and no key-knowledge text baked into this binary, because the
//! study design is a VCAA document that changes between accreditation periods
//! and I have no copy of it to work from. Inventing plausible-looking dot
//! points would be worse than shipping none: you'd revise against them,
//! they'd be subtly wrong, and nothing in the app would ever tell you.
//!
//! What's here instead is the **structure** — a real hierarchy of unit → area
//! of study → dot point — plus an importer that turns a pasted outline into
//! that hierarchy. The content comes from your own copy of the study design,
//! which is the only source that can be authoritative.
//!
//! The same rule governs the command words below: they are described in plain
//! English as study guidance, and are not presented as quotations from VCAA's
//! glossary.
//!
//! ## What is specified and therefore implemented exactly
//!
//! The exam simulation's timings — 15 minutes reading, 2 hours 30 minutes
//! writing — come from the project brief, as does the Section A / Section B
//! split already in the `practice_exams` schema. Those are implemented as
//! given.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::util;

/// The tag that marks a card as terminology, so the deck can be filtered out of
/// (or down to) vocabulary without a separate table.
pub const TERMINOLOGY_TAG: &str = "terminology";

pub const READING_SECONDS: i64 = 15 * 60;
pub const WRITING_SECONDS: i64 = 150 * 60;

// ---------------------------------------------------------------------------
// Command words
// ---------------------------------------------------------------------------


// ---------------------------------------------------------------------------
// Error categories
// ---------------------------------------------------------------------------

/// Categories offered when logging an error against Biology.
///
/// The generic Science list is about *how* a mark was lost; these add *where*
/// in the course it was lost, which is what makes the recurring-error analysis
/// actionable for one subject. Both halves are offered together.
///
/// This list only ever applies to a subject named Biology at 3/4 level — see
/// `applies_to`. Forcing genetics and immunity onto a Methods error log would
/// make the picker useless.
pub const BIOLOGY_CATEGORIES: &[&str] = &[
    "terminology",
    "process/mechanism",
    "experimental design",
    "data interpretation",
    "command word",
    "application to novel context",
    "genetics",
    "cell biology",
    "immunity",
    "evolution",
];

/// Whether the Biology categories should be offered for this subject.
///
/// Deliberately narrow: the name must actually be Biology, and it must be a 3/4
/// subject. A Year 11 Biology 1/2 error log gets the ordinary Science list,
/// because the extra granularity only pays off when you're sitting the exam.
pub fn applies_to(subject_name: &str, unit_level: &str) -> bool {
    subject_name.trim().eq_ignore_ascii_case("biology") && unit_level == "3_4"
}

// ---------------------------------------------------------------------------
// Topic tree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TopicNode {
    pub id: i64,
    pub name: String,
    /// "unit" | "aos" | "dot_point", or None for free-form.
    pub kind: Option<String>,
    pub children: Vec<TopicNode>,

    // Progress, rolled up from the topic itself (not its children) so a parent
    // doesn't look revised because one child was.
    pub confidence: Option<i64>,
    pub last_reviewed_on: Option<String>,
    pub card_count: i64,
    pub error_count: i64,
}

/// The whole tree for a subject, with progress attached.
pub fn tree(conn: &Connection, subject_id: i64) -> Result<Vec<TopicNode>> {
    #[allow(clippy::type_complexity)]
    // Positional row read; naming this tuple would add a type for a lint's sake.
    let rows: Vec<(i64, Option<i64>, String, Option<String>)> = conn
        .prepare(
            "SELECT id, parent_id, name, kind FROM topics
              WHERE subject_id = ?1 ORDER BY sort_order, id",
        )?
        .query_map([subject_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;

    let mut nodes: Vec<TopicNode> = Vec::with_capacity(rows.len());
    let mut parents: Vec<Option<i64>> = Vec::with_capacity(rows.len());

    for (id, parent_id, name, kind) in rows {
        // Most recent review wins; `id DESC` breaks a same-second tie, which is
        // reachable because timestamps are stored to whole seconds.
        let review: Option<(i64, String)> = conn
            .query_row(
                "SELECT confidence, reviewed_at FROM topic_reviews
                  WHERE topic_id = ?1 ORDER BY reviewed_at DESC, id DESC LIMIT 1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();

        let card_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM cards WHERE topic_id = ?1",
            [id],
            |r| r.get(0),
        )?;
        let error_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM error_entries WHERE topic_id = ?1",
            [id],
            |r| r.get(0),
        )?;

        nodes.push(TopicNode {
            id,
            name,
            kind,
            children: Vec::new(),
            confidence: review.as_ref().map(|(c, _)| *c),
            last_reviewed_on: review.map(|(_, at)| at[..10.min(at.len())].to_string()),
            card_count,
            error_count,
        });
        parents.push(parent_id);
    }

    Ok(nest(nodes, parents))
}

/// Fold a flat list into a tree.
///
/// Iterative rather than recursive, and it keeps any node whose parent is
/// missing at the top level instead of dropping it — an orphan should be
/// visible and fixable, not invisible.
fn nest(nodes: Vec<TopicNode>, parents: Vec<Option<i64>>) -> Vec<TopicNode> {
    use std::collections::HashMap;

    let ids: Vec<i64> = nodes.iter().map(|n| n.id).collect();
    let index: HashMap<i64, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();

    // Children first, so a node is complete before it's moved into its parent.
    let mut slots: Vec<Option<TopicNode>> = nodes.into_iter().map(Some).collect();
    let mut order: Vec<usize> = (0..slots.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(depth(i, &parents, &index)));

    for i in order {
        let Some(parent_id) = parents[i] else { continue };
        let Some(&p) = index.get(&parent_id) else { continue };
        if p == i {
            continue; // a topic that is its own parent would loop
        }
        let Some(node) = slots[i].take() else { continue };
        match slots[p].as_mut() {
            Some(parent) => parent.children.push(node),
            None => slots[i] = Some(node),
        }
    }

    let mut roots: Vec<TopicNode> = slots.into_iter().flatten().collect();
    sort_tree(&mut roots);
    roots
}

fn depth(i: usize, parents: &[Option<i64>], index: &std::collections::HashMap<i64, usize>) -> usize {
    let mut d = 0;
    let mut cur = i;
    // Bounded so a cycle in the data can't hang the app.
    while d < 64 {
        let Some(pid) = parents[cur] else { break };
        let Some(&p) = index.get(&pid) else { break };
        if p == cur {
            break;
        }
        cur = p;
        d += 1;
    }
    d
}

fn sort_tree(nodes: &mut [TopicNode]) {
    nodes.sort_by_key(|n| n.id);
    for n in nodes.iter_mut() {
        sort_tree(&mut n.children);
    }
}

// ---------------------------------------------------------------------------
// Outline import
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineRow {
    pub depth: usize,
    pub name: String,
    pub kind: String,
}

/// Turn a pasted, indented outline into topic rows.
///
/// This is how the tree gets its content: you paste the structure straight out
/// of your own study design and it becomes a hierarchy. Indentation sets the
/// level — tabs, two spaces or four all work, and common list bullets and
/// numbering are stripped.
///
/// Kind is inferred from depth (unit → aos → dot point) purely as a display
/// hint. Nothing behavioural depends on it.
pub fn parse_outline(text: &str) -> Vec<OutlineRow> {
    let mut rows: Vec<OutlineRow> = Vec::new();
    // Indent widths seen so far, so mixed indentation still nests consistently.
    let mut levels: Vec<usize> = Vec::new();

    for raw in text.lines() {
        if raw.trim().is_empty() {
            continue;
        }

        let indent = raw
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            // A tab counts as four columns, matching how it renders.
            .map(|c| if c == '\t' { 4 } else { 1 })
            .sum::<usize>();

        let name = strip_bullet(raw.trim());
        if name.is_empty() {
            continue;
        }

        // Find this indent's level, adding one if it's deeper than anything yet.
        while levels.last().is_some_and(|&l| l > indent) {
            levels.pop();
        }
        if levels.last() != Some(&indent) {
            levels.push(indent);
        }
        let depth = levels.len().saturating_sub(1);

        rows.push(OutlineRow {
            kind: match depth {
                0 => "unit",
                1 => "aos",
                _ => "dot_point",
            }
            .to_string(),
            depth,
            name,
        });
    }

    rows
}

/// Remove `-`, `*`, `•`, `1.`, `1)`, `a.`, `(i)` and similar leading markers.
fn strip_bullet(s: &str) -> String {
    let t = s.trim_start_matches(['-', '*', '•', '·', '–', '—']).trim_start();

    let mut chars = t.char_indices().peekable();
    let mut end = 0;
    let mut saw_alnum = false;

    while let Some((i, c)) = chars.next() {
        if c.is_alphanumeric() && !saw_alnum {
            saw_alnum = true;
            end = i;
            continue;
        }
        if saw_alnum && (c == '.' || c == ')') {
            // `1.` or `iv)` — a marker only if it's short and followed by space.
            let label = &t[end..i];
            if label.len() <= 4 && chars.peek().map(|(_, n)| n.is_whitespace()) == Some(true) {
                return t[i + c.len_utf8()..].trim_start().to_string();
            }
            break;
        }
        if saw_alnum && !c.is_alphanumeric() {
            break;
        }
    }

    t.to_string()
}

/// Write an outline into the topics table, replacing what's there.
///
/// Replacing rather than merging is a real trade-off, so it is surfaced in the
/// UI rather than hidden: re-importing an outline drops the existing topic rows
/// for that subject, and `ON DELETE SET NULL` means cards and error entries keep
/// existing but lose their topic link. The alternative — matching topics by
/// name — silently mis-links a renamed dot point, which is harder to notice.
pub fn import_outline(conn: &mut Connection, subject_id: i64, rows: &[OutlineRow]) -> Result<usize> {
    if rows.is_empty() {
        return Err(anyhow!("Nothing to import."));
    }

    let tx = conn.transaction()?;
    tx.execute("DELETE FROM topics WHERE subject_id = ?1", [subject_id])?;

    // The most recent id seen at each depth, so a row attaches to the row above.
    let mut ancestry: Vec<i64> = Vec::new();
    let mut written = 0usize;

    for (order, row) in rows.iter().enumerate() {
        ancestry.truncate(row.depth);
        let parent = ancestry.last().copied();

        tx.execute(
            "INSERT INTO topics (subject_id, parent_id, name, kind, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![subject_id, parent, row.name, row.kind, order as i64],
        )?;

        ancestry.push(tx.last_insert_rowid());
        written += 1;
    }

    tx.commit()?;
    Ok(written)
}

// ---------------------------------------------------------------------------
// Exam simulation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Reading,
    Writing,
    Finished,
}

/// The stored state of an exam run.
///
/// Persisted as JSON in `app_settings` rather than its own table: there is only
/// ever one run, and everything about it is derived from a start instant, so a
/// table would be a row that is always either absent or singular.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExamRun {
    pub subject_id: i64,
    pub name: String,
    pub started_at: String,
    /// Set while paused.
    pub paused_at: Option<String>,
    pub paused_seconds: i64,
}

/// What the UI renders. Everything is computed from the run, so closing the
/// window or quitting mid-exam loses nothing.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExamState {
    pub run: ExamRun,
    pub phase: Phase,
    /// Seconds elapsed, excluding paused time.
    pub elapsed_seconds: i64,
    /// Seconds left in the current phase; zero once finished.
    pub remaining_seconds: i64,
    pub paused: bool,
    pub total_seconds: i64,
}

pub fn phase_for(elapsed: i64) -> Phase {
    if elapsed < READING_SECONDS {
        Phase::Reading
    } else if elapsed < READING_SECONDS + WRITING_SECONDS {
        Phase::Writing
    } else {
        Phase::Finished
    }
}

/// Derive the full state of a run at an instant.
pub fn state_at(run: &ExamRun, now: DateTime<Utc>) -> Result<ExamState> {
    let started: DateTime<Utc> = run.started_at.parse()?;

    let paused_now = match run.paused_at.as_deref() {
        Some(p) => {
            let at: DateTime<Utc> = p.parse()?;
            (now - at).num_seconds().max(0)
        }
        None => 0,
    };

    // Clamped at zero: a clock that moved backwards must not produce negative
    // elapsed time and flip the phase back to reading.
    let elapsed = ((now - started).num_seconds() - run.paused_seconds - paused_now).max(0);
    let phase = phase_for(elapsed);

    let remaining = match phase {
        Phase::Reading => READING_SECONDS - elapsed,
        Phase::Writing => READING_SECONDS + WRITING_SECONDS - elapsed,
        Phase::Finished => 0,
    };

    Ok(ExamState {
        run: run.clone(),
        phase,
        elapsed_seconds: elapsed,
        remaining_seconds: remaining.max(0),
        paused: run.paused_at.is_some(),
        total_seconds: READING_SECONDS + WRITING_SECONDS,
    })
}

const RUN_KEY: &str = "exam_sim_run";

pub fn load(conn: &Connection) -> Result<Option<ExamRun>> {
    let Some(raw) = crate::settings::get(conn, RUN_KEY)? else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    // A state blob we can't read is cleared rather than fatal — the worst case
    // is losing one exam timer, and refusing to start a new one would be worse.
    Ok(serde_json::from_str(&raw).ok())
}

fn save(conn: &Connection, run: Option<&ExamRun>) -> Result<()> {
    let value = match run {
        Some(r) => serde_json::to_string(r)?,
        None => String::new(),
    };
    crate::settings::set(conn, RUN_KEY, &value)?;
    Ok(())
}

pub fn start(conn: &Connection, subject_id: i64, name: &str, now: DateTime<Utc>) -> Result<ExamState> {
    if load(conn)?.is_some() {
        return Err(anyhow!("An exam is already running."));
    }

    let run = ExamRun {
        subject_id,
        name: if name.trim().is_empty() {
            "Practice exam".to_string()
        } else {
            name.trim().to_string()
        },
        started_at: util::rfc3339(now),
        paused_at: None,
        paused_seconds: 0,
    };

    save(conn, Some(&run))?;
    state_at(&run, now)
}

pub fn set_paused(conn: &Connection, paused: bool, now: DateTime<Utc>) -> Result<ExamState> {
    let mut run = load(conn)?.ok_or_else(|| anyhow!("No exam is running."))?;

    match (paused, run.paused_at.clone()) {
        // Resuming: bank the time spent paused so it never counts as exam time.
        (false, Some(at)) => {
            let at: DateTime<Utc> = at.parse()?;
            run.paused_seconds += (now - at).num_seconds().max(0);
            run.paused_at = None;
        }
        (true, None) => run.paused_at = Some(util::rfc3339(now)),
        // Already in the requested state.
        _ => {}
    }

    save(conn, Some(&run))?;
    state_at(&run, now)
}

/// Finish a run and log it, returning the new `practice_exams` row id.
///
/// Logging happens whether the run went the distance or was stopped early, and
/// the recorded seconds are the real ones. An exam log that quietly rounds a
/// 40-minute attempt up to the full paper would make the history worthless.
pub fn finish(conn: &Connection, now: DateTime<Utc>) -> Result<i64> {
    let run = load(conn)?.ok_or_else(|| anyhow!("No exam is running."))?;
    let state = state_at(&run, now)?;

    let reading = state.elapsed_seconds.min(READING_SECONDS);
    let writing = (state.elapsed_seconds - reading).max(0);

    conn.execute(
        "INSERT INTO practice_exams
           (subject_id, name, taken_on, reading_seconds, writing_seconds, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            run.subject_id,
            run.name,
            util::retain_today(),
            reading,
            writing,
            util::rfc3339(now),
        ],
    )?;

    let id = conn.last_insert_rowid();
    save(conn, None)?;
    Ok(id)
}

/// Abandon a run without logging it.
pub fn cancel(conn: &Connection) -> Result<()> {
    save(conn, None)
}

pub fn score(
    conn: &Connection,
    exam_id: i64,
    section_a: Option<i64>,
    section_b: Option<i64>,
) -> Result<()> {
    conn.execute(
        "UPDATE practice_exams SET section_a_score = ?2, section_b_score = ?3 WHERE id = ?1",
        rusqlite::params![exam_id, section_a, section_b],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PracticeExam {
    pub id: i64,
    pub name: String,
    pub taken_on: String,
    pub section_a_score: Option<i64>,
    pub section_a_max: i64,
    pub section_b_score: Option<i64>,
    pub section_b_max: i64,
    pub reading_seconds: Option<i64>,
    pub writing_seconds: Option<i64>,
}

pub fn history(conn: &Connection, subject_id: i64, limit: i64) -> Result<Vec<PracticeExam>> {
    let rows = conn
        .prepare(
            "SELECT id, name, taken_on, section_a_score, section_a_max,
                    section_b_score, section_b_max, reading_seconds, writing_seconds
               FROM practice_exams WHERE subject_id = ?1
              ORDER BY taken_on DESC, id DESC LIMIT ?2",
        )?
        .query_map(rusqlite::params![subject_id, limit], |r| {
            Ok(PracticeExam {
                id: r.get(0)?,
                name: r.get(1)?,
                taken_on: r.get(2)?,
                section_a_score: r.get(3)?,
                section_a_max: r.get(4)?,
                section_b_score: r.get(5)?,
                section_b_max: r.get(6)?,
                reading_seconds: r.get(7)?,
                writing_seconds: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

/// How many terminology cards exist for a subject, and how many are due.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeckSummary {
    pub total: i64,
    pub due: i64,
    pub new: i64,
}

pub fn terminology_summary(conn: &Connection, subject_id: i64) -> Result<DeckSummary> {
    let today = util::retain_today();
    let like = format!("%{TERMINOLOGY_TAG}%");

    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM cards WHERE subject_id = ?1 AND tags LIKE ?2",
        rusqlite::params![subject_id, &like],
        |r| r.get(0),
    )?;

    let due: i64 = conn.query_row(
        "SELECT COUNT(*) FROM cards
          WHERE subject_id = ?1 AND tags LIKE ?2
            AND state != 'new' AND due_on IS NOT NULL AND due_on <= ?3",
        rusqlite::params![subject_id, &like, today],
        |r| r.get(0),
    )?;

    let new: i64 = conn.query_row(
        "SELECT COUNT(*) FROM cards
          WHERE subject_id = ?1 AND tags LIKE ?2 AND state = 'new'",
        rusqlite::params![subject_id, &like],
        |r| r.get(0),
    )?;

    Ok(DeckSummary { total, due, new })
}

#[cfg(test)]
mod tests;
