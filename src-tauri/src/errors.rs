//! The error log, and the blind re-attempt workflow.
//!
//! ## Why this is built the way it is
//!
//! Re-reading a correct answer produces an illusion of competence: it feels like
//! learning and isn't. That is the standard way error logs fail, and the brief
//! is explicit about it. So the whole subsystem is arranged around one
//! invariant:
//!
//! > **The correct answer cannot reach the user until their own answer is
//! > committed.**
//!
//! It is enforced in three places, deliberately redundant:
//!
//! 1. **The API shape.** `start_reattempt` returns a `BlindPrompt`, a struct
//!    that has no field for the correct answer. It is not merely omitted at
//!    render time — there is nowhere in the type to put it, so no frontend bug
//!    can leak it.
//! 2. **The state machine.** `reveal` and `assess` both refuse to run unless
//!    `committed_at` is already set.
//! 3. **The database.** Two CHECK constraints on `error_reattempts` reject the
//!    same thing at the storage layer (see migration 002).
//!
//! And "fixed" is only ever set by a re-attempt that was committed blind and
//! then self-marked correct.

use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::models::SubjectType;
use crate::util::{retain_day_of, retain_today, rfc3339};

/// Minimum gap before a logged error is offered for a blind re-attempt.
///
/// The brief specifies at least a week. Sooner and you are testing short-term
/// recall of the correction rather than whether the underlying gap closed.
pub const MIN_REVISIT_DAYS: i64 = 7;

/// Error categories, per subject type, exactly as the brief lists them.
pub fn categories_for(subject_type: SubjectType) -> &'static [&'static str] {
    match subject_type {
        SubjectType::Science => &[
            "careless slip",
            "misread question",
            "misread command word",
            "didn't answer to mark allocation",
            "conceptual gap",
            "imprecise terminology",
            "restated the stem",
            "pre-planned answer",
            "data/graph misread",
            "ran out of time",
        ],
        SubjectType::Maths => &[
            "sign error",
            "wrong formula",
            "algebra slip",
            "didn't check domain/restrictions",
            "CAS misuse",
            "misread question",
            "conceptual gap",
            "didn't show working",
            "ran out of time",
        ],
        SubjectType::English => &[
            "weak thesis",
            "unsupported claim",
            "quote misused",
            "didn't address prompt",
            "structure",
            "expression",
            "ran out of time",
        ],
        SubjectType::Humanities => &[
            "conceptual gap",
            "misread command word",
            "insufficient evidence",
            "didn't answer to mark allocation",
            "ran out of time",
        ],
    }
}

/// VCAA's glossary of command terms, shown inline while tagging an entry.
///
/// Flagged for verification against the current VCAA glossary — these are
/// working definitions, not a quotation of the official document.
pub const COMMAND_WORDS: &[(&str, &str)] = &[
    ("identify", "Name it. No explanation is being asked for."),
    ("describe", "Say what happens or what something is like, in order and in detail."),
    ("explain", "Say how or why it happens — a description alone won't score."),
    ("compare", "Give similarities AND differences. Similarities are the half people skip."),
    ("contrast", "Give only the differences."),
    ("distinguish", "State the clear difference between two things, both sides named."),
    ("outline", "The main points only, briefly."),
    ("analyse", "Break it into parts and say how they relate."),
    ("evaluate", "Judge how good or effective it is, with evidence both ways."),
    ("discuss", "Present more than one side, then reach a position."),
    ("justify", "Give the reasons your answer or choice is the right one."),
    ("assess", "Weigh factors up and reach a judgement about importance."),
    ("predict", "Say what would happen, based on the biology given."),
    ("suggest", "Offer a plausible answer where more than one is defensible."),
    ("calculate", "Work out a number, showing the steps."),
    ("deduce", "Reach a conclusion that follows from the information given."),
];

// ---------------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEntryInput {
    pub subject_id: i64,
    pub topic_id: Option<i64>,
    pub source: Option<String>,
    pub command_word: Option<String>,
    pub question_text: Option<String>,
    /// Pasted screenshot, base64 data URL. Optional.
    pub question_image: Option<String>,
    pub my_answer: Option<String>,
    pub correct_answer: Option<String>,
    pub category: String,
    pub fix: Option<String>,
    pub marks_lost: Option<i64>,
    pub marks_available: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEntry {
    pub id: i64,
    pub subject_id: i64,
    pub subject_name: String,
    pub colour: String,
    pub topic_id: Option<i64>,
    pub topic_name: Option<String>,
    pub logged_on: String,
    pub source: Option<String>,
    pub command_word: Option<String>,
    pub question_text: Option<String>,
    pub has_image: bool,
    pub my_answer: Option<String>,
    /// Present when listing for editing. The blind re-attempt flow never uses
    /// this struct — see `BlindPrompt`.
    pub correct_answer: Option<String>,
    pub category: String,
    pub fix: Option<String>,
    pub marks_lost: Option<i64>,
    pub marks_available: Option<i64>,
    pub revisit_on: Option<String>,
    pub fixed_at: Option<String>,
    pub reattempt_count: i64,
}

pub fn create(conn: &Connection, input: ErrorEntryInput) -> anyhow::Result<i64> {
    if input.category.trim().is_empty() {
        anyhow::bail!("An error needs a category — that's what makes the log analysable.");
    }
    let now = Utc::now();
    let today = retain_day_of(now);

    // First re-attempt is scheduled a week out, per the brief's minimum.
    let revisit = (crate::util::retain_day_naive(now) + chrono::Duration::days(MIN_REVISIT_DAYS))
        .format("%Y-%m-%d")
        .to_string();

    conn.execute(
        "INSERT INTO error_entries
           (subject_id, topic_id, logged_on, source, command_word, question_text,
            question_image, my_answer, correct_answer, category, fix,
            marks_lost, marks_available, revisit_on, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        rusqlite::params![
            input.subject_id,
            input.topic_id,
            today,
            input.source,
            input.command_word,
            input.question_text,
            input.question_image,
            input.my_answer,
            input.correct_answer,
            input.category.trim(),
            input.fix,
            input.marks_lost,
            input.marks_available,
            revisit,
            rfc3339(now),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryFilter {
    pub subject_id: Option<i64>,
    pub category: Option<String>,
    pub topic_id: Option<i64>,
    pub search: Option<String>,
    pub only_unfixed: Option<bool>,
}

pub fn list(conn: &Connection, filter: &EntryFilter) -> anyhow::Result<Vec<ErrorEntry>> {
    // `LIKE ?` with a NULL pattern is NULL (not false), so each optional filter
    // is guarded by an explicit IS NULL check on the parameter instead.
    let search = filter.search.as_ref().map(|s| format!("%{}%", s.trim()));

    let mut stmt = conn.prepare(
        "SELECT e.id, e.subject_id, s.name, s.colour, e.topic_id, t.name, e.logged_on,
                e.source, e.command_word, e.question_text,
                e.question_image IS NOT NULL, e.my_answer, e.correct_answer,
                e.category, e.fix, e.marks_lost, e.marks_available,
                e.revisit_on, e.fixed_at,
                (SELECT COUNT(*) FROM error_reattempts r WHERE r.error_entry_id = e.id)
           FROM error_entries e
           JOIN subjects s ON s.id = e.subject_id
           LEFT JOIN topics t ON t.id = e.topic_id
          WHERE (?1 IS NULL OR e.subject_id = ?1)
            AND (?2 IS NULL OR e.category = ?2)
            AND (?3 IS NULL OR e.topic_id = ?3)
            AND (?4 IS NULL OR e.question_text LIKE ?4 OR e.my_answer LIKE ?4
                 OR e.fix LIKE ?4 OR e.source LIKE ?4)
            AND (?5 = 0 OR e.fixed_at IS NULL)
          ORDER BY e.logged_on DESC, e.id DESC",
    )?;

    let rows = stmt.query_map(
        rusqlite::params![
            filter.subject_id,
            filter.category,
            filter.topic_id,
            search,
            filter.only_unfixed.unwrap_or(false) as i64,
        ],
        |r| {
            Ok(ErrorEntry {
                id: r.get(0)?,
                subject_id: r.get(1)?,
                subject_name: r.get(2)?,
                colour: r.get(3)?,
                topic_id: r.get(4)?,
                topic_name: r.get(5)?,
                logged_on: r.get(6)?,
                source: r.get(7)?,
                command_word: r.get(8)?,
                question_text: r.get(9)?,
                has_image: r.get::<_, i64>(10)? == 1,
                my_answer: r.get(11)?,
                correct_answer: r.get(12)?,
                category: r.get(13)?,
                fix: r.get(14)?,
                marks_lost: r.get(15)?,
                marks_available: r.get(16)?,
                revisit_on: r.get(17)?,
                fixed_at: r.get(18)?,
                reattempt_count: r.get(19)?,
            })
        },
    )?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn delete(conn: &Connection, id: i64) -> anyhow::Result<()> {
    conn.execute("DELETE FROM error_entries WHERE id = ?1", [id])?;
    Ok(())
}

pub fn image(conn: &Connection, id: i64) -> anyhow::Result<Option<String>> {
    Ok(conn.query_row(
        "SELECT question_image FROM error_entries WHERE id = ?1",
        [id],
        |r| r.get(0),
    )?)
}

// ---------------------------------------------------------------------------
// Blind re-attempts
// ---------------------------------------------------------------------------

/// What the user is shown when re-attempting.
///
/// **There is deliberately no `correct_answer` field here.** The question, the
/// source and the marks are enough to have another go; the answer is fetched
/// only by `reveal`, and only after a commit. A frontend cannot leak what it was
/// never sent.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlindPrompt {
    pub reattempt_id: i64,
    pub entry_id: i64,
    pub subject_name: String,
    pub colour: String,
    pub topic_name: Option<String>,
    pub source: Option<String>,
    pub command_word: Option<String>,
    pub question_text: Option<String>,
    pub has_image: bool,
    pub marks_available: Option<i64>,
    pub presented_at: String,
}

/// Entries whose blind re-attempt is due.
pub fn due_reattempts(conn: &Connection, subject_id: Option<i64>) -> anyhow::Result<Vec<i64>> {
    let today = retain_today();
    let mut stmt = conn.prepare(
        "SELECT id FROM error_entries
          WHERE fixed_at IS NULL
            AND revisit_on IS NOT NULL AND revisit_on <= ?1
            AND (?2 IS NULL OR subject_id = ?2)
          ORDER BY revisit_on ASC, id ASC",
    )?;
    let rows = stmt.query_map(rusqlite::params![today, subject_id], |r| r.get::<_, i64>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Begin a blind re-attempt. Returns the question WITHOUT the answer.
pub fn start_reattempt(conn: &Connection, entry_id: i64) -> anyhow::Result<BlindPrompt> {
    let now = Utc::now();

    // A row's columns, read positionally. Naming this tuple would add a
    // type that exists only to satisfy a lint.
    #[allow(clippy::type_complexity)]
    let (subject_name, colour, topic_name, source, command_word, question_text, has_image, marks): (
        String, String, Option<String>, Option<String>, Option<String>, Option<String>, bool, Option<i64>,
    ) = conn.query_row(
        "SELECT s.name, s.colour, t.name, e.source, e.command_word, e.question_text,
                e.question_image IS NOT NULL, e.marks_available
           FROM error_entries e
           JOIN subjects s ON s.id = e.subject_id
           LEFT JOIN topics t ON t.id = e.topic_id
          WHERE e.id = ?1",
        [entry_id],
        |r| {
            Ok((
                r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?,
                r.get::<_, i64>(6)? == 1, r.get(7)?,
            ))
        },
    )?;

    conn.execute(
        "INSERT INTO error_reattempts (error_entry_id, presented_at, local_date)
         VALUES (?1, ?2, ?3)",
        rusqlite::params![entry_id, rfc3339(now), retain_day_of(now)],
    )?;

    Ok(BlindPrompt {
        reattempt_id: conn.last_insert_rowid(),
        entry_id,
        subject_name,
        colour,
        topic_name,
        source,
        command_word,
        question_text,
        has_image,
        marks_available: marks,
        presented_at: rfc3339(now),
    })
}

/// Lock in the blind answer. Must happen before anything can be revealed.
pub fn commit_reattempt(
    conn: &Connection,
    reattempt_id: i64,
    blind_answer: &str,
) -> anyhow::Result<()> {
    let already: Option<String> = conn.query_row(
        "SELECT committed_at FROM error_reattempts WHERE id = ?1",
        [reattempt_id],
        |r| r.get(0),
    )?;
    if already.is_some() {
        anyhow::bail!("This attempt has already been committed.");
    }

    conn.execute(
        "UPDATE error_reattempts SET committed_at = ?1, blind_answer = ?2 WHERE id = ?3",
        rusqlite::params![rfc3339(Utc::now()), blind_answer.trim(), reattempt_id],
    )?;
    Ok(())
}

fn require_committed(conn: &Connection, reattempt_id: i64) -> anyhow::Result<()> {
    let committed: Option<String> = conn.query_row(
        "SELECT committed_at FROM error_reattempts WHERE id = ?1",
        [reattempt_id],
        |r| r.get(0),
    )?;
    if committed.is_none() {
        anyhow::bail!(
            "Write down your answer first. Seeing the mark scheme before you've committed \
             turns this into re-reading, which is exactly what the error log exists to avoid."
        );
    }
    Ok(())
}

/// Reveal the mark scheme. Refuses until the blind answer is committed.
pub fn reveal_reattempt(conn: &Connection, reattempt_id: i64) -> anyhow::Result<Option<String>> {
    require_committed(conn, reattempt_id)?;

    conn.execute(
        "UPDATE error_reattempts SET revealed_at = COALESCE(revealed_at, ?1) WHERE id = ?2",
        rusqlite::params![rfc3339(Utc::now()), reattempt_id],
    )?;

    Ok(conn.query_row(
        "SELECT e.correct_answer FROM error_entries e
           JOIN error_reattempts r ON r.error_entry_id = e.id
          WHERE r.id = ?1",
        [reattempt_id],
        |r| r.get(0),
    )?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelfAssessment {
    Correct,
    Partial,
    Incorrect,
}

impl SelfAssessment {
    fn as_str(self) -> &'static str {
        match self {
            SelfAssessment::Correct => "correct",
            SelfAssessment::Partial => "partial",
            SelfAssessment::Incorrect => "incorrect",
        }
    }
}

/// Record the self-mark, and mark the entry fixed only on a correct blind attempt.
///
/// Returns whether the parent entry is now fixed.
pub fn assess_reattempt(
    conn: &Connection,
    reattempt_id: i64,
    assessment: SelfAssessment,
    marks_awarded: Option<i64>,
) -> anyhow::Result<bool> {
    require_committed(conn, reattempt_id)?;

    conn.execute(
        "UPDATE error_reattempts SET self_assessment = ?1, marks_awarded = ?2 WHERE id = ?3",
        rusqlite::params![assessment.as_str(), marks_awarded, reattempt_id],
    )?;

    let entry_id: i64 = conn.query_row(
        "SELECT error_entry_id FROM error_reattempts WHERE id = ?1",
        [reattempt_id],
        |r| r.get(0),
    )?;

    if assessment == SelfAssessment::Correct {
        conn.execute(
            "UPDATE error_entries SET fixed_at = ?1 WHERE id = ?2",
            rusqlite::params![rfc3339(Utc::now()), entry_id],
        )?;
        return Ok(true);
    }

    // Not fixed. Schedule another blind attempt a week out rather than leaving
    // it to resurface immediately — the gap is the point.
    let next = (crate::util::retain_today_naive() + chrono::Duration::days(MIN_REVISIT_DAYS))
        .format("%Y-%m-%d")
        .to_string();
    conn.execute(
        "UPDATE error_entries SET revisit_on = ?1 WHERE id = ?2",
        rusqlite::params![next, entry_id],
    )?;

    Ok(false)
}

// ---------------------------------------------------------------------------
// Analytics
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryCount {
    pub category: String,
    pub count: i64,
    pub marks_lost: i64,
}

/// Recurring error categories over the last `days`, most frequent first.
///
/// The brief calls this probably the most useful screen in the app: the point is
/// not any single mistake but the one you keep making.
pub fn recurring(
    conn: &Connection,
    subject_id: Option<i64>,
    days: i64,
) -> anyhow::Result<Vec<CategoryCount>> {
    let since = (crate::util::retain_today_naive() - chrono::Duration::days(days.clamp(1, 3650)))
        .format("%Y-%m-%d")
        .to_string();

    let mut stmt = conn.prepare(
        "SELECT category, COUNT(*), COALESCE(SUM(marks_lost), 0)
           FROM error_entries
          WHERE logged_on >= ?1 AND (?2 IS NULL OR subject_id = ?2)
          GROUP BY category
          ORDER BY COUNT(*) DESC, SUM(marks_lost) DESC",
    )?;

    let rows = stmt.query_map(rusqlite::params![since, subject_id], |r| {
        Ok(CategoryCount {
            category: r.get(0)?,
            count: r.get(1)?,
            marks_lost: r.get(2)?,
        })
    })?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
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
             VALUES (1,'Biology','#4BA97B','3_4','science',0,'2026-08-12T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    fn entry(conn: &Connection, category: &str) -> i64 {
        create(
            conn,
            ErrorEntryInput {
                subject_id: 1,
                topic_id: None,
                source: Some("2024 VCAA Q7b".into()),
                command_word: Some("explain".into()),
                question_text: Some("Explain why the enzyme denatures.".into()),
                question_image: None,
                my_answer: Some("it breaks".into()),
                correct_answer: Some("Heat disrupts hydrogen bonds, altering the active site.".into()),
                category: category.into(),
                fix: Some("Name the bond type.".into()),
                marks_lost: Some(2),
                marks_available: Some(3),
            },
        )
        .unwrap()
    }

    #[test]
    fn categories_match_the_brief_per_subject_type() {
        assert!(categories_for(SubjectType::Maths).contains(&"CAS misuse"));
        assert!(categories_for(SubjectType::English).contains(&"weak thesis"));
        assert!(categories_for(SubjectType::Science).contains(&"data/graph misread"));
        assert!(categories_for(SubjectType::Humanities).contains(&"insufficient evidence"));
        // Every list ends with the shared one.
        for t in [SubjectType::Science, SubjectType::Maths, SubjectType::English, SubjectType::Humanities] {
            assert!(categories_for(t).contains(&"ran out of time"));
        }
    }

    #[test]
    fn a_new_entry_is_scheduled_at_least_a_week_out() {
        let conn = db();
        let id = entry(&conn, "conceptual gap");
        let (logged, revisit): (String, String) = conn
            .query_row("SELECT logged_on, revisit_on FROM error_entries WHERE id=?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        let a = chrono::NaiveDate::parse_from_str(&logged, "%Y-%m-%d").unwrap();
        let b = chrono::NaiveDate::parse_from_str(&revisit, "%Y-%m-%d").unwrap();
        assert!((b - a).num_days() >= MIN_REVISIT_DAYS);
    }

    #[test]
    fn an_entry_needs_a_category() {
        let conn = db();
        let bad = create(
            &conn,
            ErrorEntryInput {
                subject_id: 1, topic_id: None, source: None, command_word: None,
                question_text: None, question_image: None, my_answer: None,
                correct_answer: None, category: "   ".into(), fix: None,
                marks_lost: None, marks_available: None,
            },
        );
        assert!(bad.is_err());
    }

    // -- the blind invariant ----------------------------------------------

    /// The prompt struct has nowhere to put the answer. This is the primary
    /// defence: a frontend cannot leak what it never received.
    #[test]
    fn the_blind_prompt_cannot_carry_the_answer() {
        let conn = db();
        let id = entry(&conn, "conceptual gap");
        let prompt = start_reattempt(&conn, id).unwrap();

        let json = serde_json::to_string(&prompt).unwrap();
        assert!(json.contains("Explain why the enzyme denatures"));
        assert!(
            !json.contains("hydrogen bonds"),
            "the mark scheme leaked into the blind prompt: {json}"
        );
        assert!(!json.to_lowercase().contains("correctanswer"));
    }

    #[test]
    fn revealing_before_committing_is_refused() {
        let conn = db();
        let id = entry(&conn, "conceptual gap");
        let prompt = start_reattempt(&conn, id).unwrap();

        let refused = reveal_reattempt(&conn, prompt.reattempt_id);
        assert!(refused.is_err(), "reveal must refuse before a commit");

        // And nothing was recorded as revealed.
        let revealed: Option<String> = conn
            .query_row("SELECT revealed_at FROM error_reattempts WHERE id=?1", [prompt.reattempt_id], |r| r.get(0))
            .unwrap();
        assert!(revealed.is_none());
    }

    #[test]
    fn self_assessing_before_committing_is_refused() {
        let conn = db();
        let id = entry(&conn, "conceptual gap");
        let prompt = start_reattempt(&conn, id).unwrap();

        assert!(assess_reattempt(&conn, prompt.reattempt_id, SelfAssessment::Correct, Some(3)).is_err());

        // Critically, the entry must NOT have been marked fixed.
        let fixed: Option<String> = conn
            .query_row("SELECT fixed_at FROM error_entries WHERE id=?1", [id], |r| r.get(0))
            .unwrap();
        assert!(fixed.is_none(), "an uncommitted attempt marked the entry fixed");
    }

    #[test]
    fn revealing_after_committing_returns_the_mark_scheme() {
        let conn = db();
        let id = entry(&conn, "conceptual gap");
        let prompt = start_reattempt(&conn, id).unwrap();

        commit_reattempt(&conn, prompt.reattempt_id, "Heat breaks H-bonds").unwrap();
        let answer = reveal_reattempt(&conn, prompt.reattempt_id).unwrap();
        assert!(answer.unwrap().contains("hydrogen bonds"));
    }

    #[test]
    fn a_committed_answer_cannot_be_rewritten_after_seeing_the_scheme() {
        let conn = db();
        let id = entry(&conn, "conceptual gap");
        let prompt = start_reattempt(&conn, id).unwrap();

        commit_reattempt(&conn, prompt.reattempt_id, "my first answer").unwrap();
        reveal_reattempt(&conn, prompt.reattempt_id).unwrap();

        let second = commit_reattempt(&conn, prompt.reattempt_id, "the correct answer, copied");
        assert!(second.is_err(), "committing twice would let the answer be edited after reveal");

        let stored: String = conn
            .query_row("SELECT blind_answer FROM error_reattempts WHERE id=?1", [prompt.reattempt_id], |r| r.get(0))
            .unwrap();
        assert_eq!(stored, "my first answer");
    }

    /// "Fixed" only ever comes from a committed, correct blind attempt.
    #[test]
    fn only_a_correct_blind_attempt_marks_an_entry_fixed() {
        let conn = db();
        let id = entry(&conn, "conceptual gap");

        // Partial → not fixed, and rescheduled.
        let p1 = start_reattempt(&conn, id).unwrap();
        commit_reattempt(&conn, p1.reattempt_id, "half right").unwrap();
        assert!(!assess_reattempt(&conn, p1.reattempt_id, SelfAssessment::Partial, Some(1)).unwrap());
        assert!(due_reattempts(&conn, None).unwrap().is_empty(), "must be pushed a week out");

        // Correct → fixed.
        let p2 = start_reattempt(&conn, id).unwrap();
        commit_reattempt(&conn, p2.reattempt_id, "full answer").unwrap();
        assert!(assess_reattempt(&conn, p2.reattempt_id, SelfAssessment::Correct, Some(3)).unwrap());

        let fixed: Option<String> = conn
            .query_row("SELECT fixed_at FROM error_entries WHERE id=?1", [id], |r| r.get(0))
            .unwrap();
        assert!(fixed.is_some());
    }

    /// The database rejects the same thing independently of the Rust layer.
    #[test]
    fn the_database_also_refuses_a_premature_reveal() {
        let conn = db();
        let id = entry(&conn, "conceptual gap");
        let direct = conn.execute(
            "INSERT INTO error_reattempts (error_entry_id, presented_at, revealed_at, local_date)
             VALUES (?1, '2026-08-12T01:00:00Z', '2026-08-12T01:00:00Z', '2026-08-12')",
            [id],
        );
        assert!(direct.is_err(), "CHECK constraint must reject reveal-without-commit");
    }

    #[test]
    fn fixed_entries_stop_resurfacing() {
        let conn = db();
        let id = entry(&conn, "conceptual gap");
        conn.execute("UPDATE error_entries SET revisit_on = '2000-01-01' WHERE id=?1", [id]).unwrap();
        assert_eq!(due_reattempts(&conn, None).unwrap(), vec![id]);

        let p = start_reattempt(&conn, id).unwrap();
        commit_reattempt(&conn, p.reattempt_id, "x").unwrap();
        assess_reattempt(&conn, p.reattempt_id, SelfAssessment::Correct, None).unwrap();

        assert!(due_reattempts(&conn, None).unwrap().is_empty());
    }

    // -- filtering and analytics ------------------------------------------

    #[test]
    fn filters_narrow_the_list() {
        let conn = db();
        entry(&conn, "conceptual gap");
        entry(&conn, "careless slip");
        entry(&conn, "conceptual gap");

        assert_eq!(list(&conn, &EntryFilter::default()).unwrap().len(), 3);
        assert_eq!(
            list(&conn, &EntryFilter { category: Some("careless slip".into()), ..Default::default() })
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            list(&conn, &EntryFilter { search: Some("denatures".into()), ..Default::default() })
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            list(&conn, &EntryFilter { search: Some("nonexistent".into()), ..Default::default() })
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            list(&conn, &EntryFilter { subject_id: Some(99), ..Default::default() }).unwrap().len(),
            0
        );
    }

    #[test]
    fn recurring_ranks_by_frequency_then_marks() {
        let conn = db();
        entry(&conn, "conceptual gap");
        entry(&conn, "conceptual gap");
        entry(&conn, "conceptual gap");
        entry(&conn, "careless slip");

        let top = recurring(&conn, None, 30).unwrap();
        assert_eq!(top[0].category, "conceptual gap");
        assert_eq!(top[0].count, 3);
        assert_eq!(top[0].marks_lost, 6);
        assert_eq!(top[1].category, "careless slip");
    }

    #[test]
    fn recurring_respects_the_window() {
        let conn = db();
        let id = entry(&conn, "conceptual gap");
        conn.execute("UPDATE error_entries SET logged_on='2020-01-01' WHERE id=?1", [id]).unwrap();
        assert!(recurring(&conn, None, 30).unwrap().is_empty());
        assert_eq!(recurring(&conn, None, 3650).unwrap().len(), 1);
    }

    #[test]
    fn deleting_an_entry_removes_its_reattempts() {
        let conn = db();
        let id = entry(&conn, "conceptual gap");
        let p = start_reattempt(&conn, id).unwrap();
        commit_reattempt(&conn, p.reattempt_id, "x").unwrap();

        delete(&conn, id).unwrap();
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM error_reattempts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "ON DELETE CASCADE must clean up");
    }
}

/// Categories for a specific subject, rather than just its type.
///
/// Biology 3/4 gets the generic Science list plus course-specific ones — see
/// `biology::BIOLOGY_CATEGORIES`. Every other subject gets exactly what it got
/// before, which is the point: the extra granularity is only useful where you
/// actually sit the exam, and offering "immunity" in a Methods error log would
/// make the picker worse for every other subject.
pub fn categories_for_subject(
    conn: &rusqlite::Connection,
    subject_id: i64,
) -> anyhow::Result<Vec<String>> {
    let (name, unit_level, subject_type): (String, String, String) = conn.query_row(
        "SELECT name, unit_level, subject_type FROM subjects WHERE id = ?1",
        [subject_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    let base = categories_for(match subject_type.as_str() {
        "maths" => SubjectType::Maths,
        "english" => SubjectType::English,
        "humanities" => SubjectType::Humanities,
        _ => SubjectType::Science,
    });

    let mut out: Vec<String> = base.iter().map(|s| s.to_string()).collect();

    if crate::biology::applies_to(&name, &unit_level) {
        for c in crate::biology::BIOLOGY_CATEGORIES {
            let c = c.to_string();
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod subject_category_tests {
    use super::*;

    fn db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("db/migrations/001_init.sql")).unwrap();
        conn.execute_batch(include_str!("db/migrations/002_capture_cards_errors.sql")).unwrap();
        conn.execute(
            "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
             VALUES (1,'Biology','#1','3_4','science',0,'2026-08-01T00:00:00Z'),
                    (3,'Chemistry','#3','3_4','science',2,'2026-08-01T00:00:00Z'),
                    (4,'Maths Methods','#4','3_4','maths',3,'2026-08-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn biology_three_four_gets_the_science_list_plus_course_categories() {
        let conn = db();
        let c = categories_for_subject(&conn, 1).unwrap();

        assert!(c.contains(&"careless slip".to_string()), "generic list should remain");
        assert!(c.contains(&"immunity".to_string()));
        assert!(c.contains(&"experimental design".to_string()));
    }

    /// The constraint that matters: these must not appear anywhere else.
    /// Biology at 1/2 gets the plain Science list. `subjects.name` is UNIQUE,
    /// so this is the same subject moved down a level rather than a second row.
    #[test]
    fn biology_one_two_gets_the_plain_science_list() {
        let conn = db();
        conn.execute("UPDATE subjects SET unit_level = '1_2' WHERE id = 1", []).unwrap();

        let c = categories_for_subject(&conn, 1).unwrap();
        assert!(c.contains(&"careless slip".to_string()));
        assert!(!c.contains(&"immunity".to_string()));
    }

    #[test]
    fn no_other_subject_is_given_biology_categories() {
        let conn = db();
        for id in [3, 4] {
            let c = categories_for_subject(&conn, id).unwrap();
            assert!(!c.contains(&"immunity".to_string()), "subject {id} got biology categories");
            assert!(!c.contains(&"genetics".to_string()), "subject {id} got biology categories");
        }
    }

    #[test]
    fn maths_still_gets_its_own_list() {
        let conn = db();
        let c = categories_for_subject(&conn, 4).unwrap();
        assert!(c.contains(&"CAS misuse".to_string()));
        assert!(!c.contains(&"data/graph misread".to_string()));
    }

    /// Duplicates would show the same option twice in the picker.
    #[test]
    fn the_combined_list_has_no_duplicates() {
        let conn = db();
        let mut c = categories_for_subject(&conn, 1).unwrap();
        let before = c.len();
        c.sort();
        c.dedup();
        assert_eq!(c.len(), before);
    }
}
