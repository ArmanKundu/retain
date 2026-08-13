//! Assessments, countdowns, and retrospective revision.
//!
//! ## Retrospective, never prospective
//!
//! The brief rules out a timetable that pre-assigns topics to future dates,
//! because those break the first time life intervenes and then you abandon the
//! whole system. So there is deliberately **no table of future topic
//! assignments** anywhere in this module.
//!
//! What exists instead:
//!
//!   * `topic_reviews` — a log of what you actually revised, when, and how
//!     confident you felt afterwards. Purely a record of the past.
//!   * `surface()` — ranks topics by how long ago you last touched them and how
//!     shaky you felt. It answers "what should I look at now", computed fresh
//!     each time, and is never persisted as a plan.
//!   * `review_points()` — dates counting backwards from an assessment. These
//!     are *when to revise*, not *what to revise*; the "what" still comes from
//!     `surface()` on the day. Missing one costs nothing because nothing was
//!     assigned to it.

use chrono::NaiveDate;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::util::{retain_today, retain_today_naive, rfc3339};

/// Expanding intervals before an assessment, from the brief.
pub const REVIEW_OFFSETS: [i64; 7] = [1, 2, 3, 5, 7, 14, 30];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentKind {
    Sac,
    Exam,
    Other,
}

impl AssessmentKind {
    fn as_str(self) -> &'static str {
        match self {
            AssessmentKind::Sac => "sac",
            AssessmentKind::Exam => "exam",
            AssessmentKind::Other => "other",
        }
    }
    fn from_str(s: &str) -> Self {
        match s {
            "sac" => AssessmentKind::Sac,
            "exam" => AssessmentKind::Exam,
            _ => AssessmentKind::Other,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentInput {
    pub subject_id: i64,
    pub name: String,
    pub kind: AssessmentKind,
    pub due_on: String,
    pub topic_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    pub id: i64,
    pub subject_id: i64,
    pub subject_name: String,
    pub colour: String,
    pub name: String,
    pub kind: AssessmentKind,
    pub due_on: String,
    /// Days from today. Negative once it's passed.
    pub days_away: i64,
    pub source: String,
    pub topic_ids: Vec<i64>,
    /// Dates to revise, counting backwards. Past ones are dropped.
    pub upcoming_review_points: Vec<String>,
}

/// The dates to revise before an assessment, nearest first.
///
/// Only future points are returned — a review point that has already passed is
/// noise, and showing it as "missed" would be exactly the guilt framing the
/// brief rules out.
pub fn review_points(due: NaiveDate, today: NaiveDate) -> Vec<String> {
    let mut points: Vec<NaiveDate> = REVIEW_OFFSETS
        .iter()
        .map(|d| due - chrono::Duration::days(*d))
        .filter(|d| *d >= today && *d <= due)
        .collect();
    points.sort();
    points.iter().map(|d| d.format("%Y-%m-%d").to_string()).collect()
}

pub fn create(conn: &Connection, input: AssessmentInput) -> anyhow::Result<i64> {
    let name = input.name.trim();
    if name.is_empty() {
        anyhow::bail!("An assessment needs a name.");
    }
    NaiveDate::parse_from_str(&input.due_on, "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("'{}' isn't a valid date.", input.due_on))?;

    conn.execute(
        "INSERT INTO assessments (subject_id, name, kind, due_on, source, created_at)
         VALUES (?1, ?2, ?3, ?4, 'manual', ?5)",
        rusqlite::params![
            input.subject_id,
            name,
            input.kind.as_str(),
            input.due_on,
            rfc3339(chrono::Utc::now()),
        ],
    )?;
    let id = conn.last_insert_rowid();

    for topic_id in input.topic_ids.unwrap_or_default() {
        conn.execute(
            "INSERT OR IGNORE INTO assessment_topics (assessment_id, topic_id) VALUES (?1, ?2)",
            rusqlite::params![id, topic_id],
        )?;
    }

    Ok(id)
}

pub fn list(conn: &Connection, include_past: bool) -> anyhow::Result<Vec<Assessment>> {
    let today = retain_today();
    let today_naive = retain_today_naive();

    let sql = format!(
        "SELECT a.id, a.subject_id, s.name, s.colour, a.name, a.kind, a.due_on, a.source
           FROM assessments a JOIN subjects s ON s.id = a.subject_id
          {}
          ORDER BY a.due_on ASC",
        if include_past { "" } else { "WHERE a.due_on >= ?1" }
    );

    let mut stmt = conn.prepare(&sql)?;
    // A row's columns, read positionally. Naming this tuple would add a
    // type that exists only to satisfy a lint.
    #[allow(clippy::type_complexity)]
    let map = |r: &rusqlite::Row<'_>| -> rusqlite::Result<(i64, i64, String, String, String, String, String, String)> {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
    };
    let rows: Vec<_> = if include_past {
        stmt.query_map([], map)?.collect::<Result<_, _>>()?
    } else {
        stmt.query_map([&today], map)?.collect::<Result<_, _>>()?
    };

    let mut out = Vec::new();
    for (id, subject_id, subject_name, colour, name, kind, due_on, source) in rows {
        let due = NaiveDate::parse_from_str(&due_on, "%Y-%m-%d")
            .unwrap_or(today_naive);

        let mut topics = conn.prepare(
            "SELECT topic_id FROM assessment_topics WHERE assessment_id = ?1",
        )?;
        let topic_ids: Vec<i64> = topics
            .query_map([id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;

        out.push(Assessment {
            id,
            subject_id,
            subject_name,
            colour,
            name,
            kind: AssessmentKind::from_str(&kind),
            due_on: due_on.clone(),
            days_away: (due - today_naive).num_days(),
            source,
            topic_ids,
            upcoming_review_points: review_points(due, today_naive),
        });
    }

    Ok(out)
}

pub fn delete(conn: &Connection, id: i64) -> anyhow::Result<()> {
    conn.execute("DELETE FROM assessments WHERE id = ?1", [id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Retrospective revision
// ---------------------------------------------------------------------------

/// Record that you tested yourself on a topic, and how it felt.
pub fn log_topic_review(
    conn: &Connection,
    topic_id: i64,
    confidence: i64,
    note: Option<&str>,
) -> anyhow::Result<()> {
    if !(1..=5).contains(&confidence) {
        anyhow::bail!("Confidence runs 1 to 5.");
    }
    let now = chrono::Utc::now();
    conn.execute(
        "INSERT INTO topic_reviews (topic_id, reviewed_at, local_date, confidence, note)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            topic_id,
            rfc3339(now),
            crate::util::retain_day_of(now),
            confidence,
            note,
        ],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicStatus {
    pub topic_id: i64,
    pub topic_name: String,
    pub subject_id: i64,
    pub subject_name: String,
    pub colour: String,
    pub last_reviewed_on: Option<String>,
    pub days_since: Option<i64>,
    pub last_confidence: Option<i64>,
    pub review_count: i64,
    /// Higher means more worth looking at now. Ranking only — not a score shown
    /// as a judgement.
    pub priority: f64,
}

/// Rank topics by what deserves attention now.
///
/// Two signals, combined: how long since you last looked, and how shaky you felt
/// when you did. A topic never reviewed outranks everything — you cannot be
/// confident about something you have not tested.
///
/// Computed fresh on every call and never stored. That's what keeps it
/// retrospective: there is no plan to fall behind.
pub fn surface(
    conn: &Connection,
    subject_id: Option<i64>,
    limit: i64,
) -> anyhow::Result<Vec<TopicStatus>> {
    let today = retain_today_naive();

    let mut stmt = conn.prepare(
        // `id DESC` is a required tiebreak, not decoration. Timestamps are
        // stored to whole-second precision, so two reviews of the same topic
        // inside one second are indistinguishable by `reviewed_at` alone and
        // SQLite would pick either — making "most recent confidence"
        // nondeterministic exactly when someone corrects a rating they just
        // entered. `id` is monotonic, so it resolves the tie correctly.
        "SELECT t.id, t.name, t.subject_id, s.name, s.colour,
                (SELECT local_date FROM topic_reviews r WHERE r.topic_id = t.id
                  ORDER BY r.reviewed_at DESC, r.id DESC LIMIT 1),
                (SELECT confidence FROM topic_reviews r WHERE r.topic_id = t.id
                  ORDER BY r.reviewed_at DESC, r.id DESC LIMIT 1),
                (SELECT COUNT(*) FROM topic_reviews r WHERE r.topic_id = t.id)
           FROM topics t JOIN subjects s ON s.id = t.subject_id
          WHERE s.archived = 0 AND (?1 IS NULL OR t.subject_id = ?1)",
    )?;

    let rows = stmt.query_map(rusqlite::params![subject_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<i64>>(6)?,
            r.get::<_, i64>(7)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (topic_id, topic_name, subject_id, subject_name, colour, last_on, last_conf, count) =
            row?;

        let days_since = last_on.as_ref().and_then(|d| {
            NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .ok()
                .map(|parsed| (today - parsed).num_days())
        });

        // Never reviewed sorts above everything. Otherwise: days since, plus a
        // weighting for low confidence — a 1/5 from a week ago should outrank a
        // 5/5 from three weeks ago.
        let priority = match (days_since, last_conf) {
            (None, _) => f64::MAX / 2.0,
            (Some(days), Some(conf)) => days as f64 + (5 - conf) as f64 * 4.0,
            (Some(days), None) => days as f64,
        };

        out.push(TopicStatus {
            topic_id,
            topic_name,
            subject_id,
            subject_name,
            colour,
            last_reviewed_on: last_on,
            days_since,
            last_confidence: last_conf,
            review_count: count,
            priority,
        });
    }

    out.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limit.clamp(1, 500) as usize);
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
        conn.execute(
            "INSERT INTO topics (id, subject_id, name, sort_order)
             VALUES (1,1,'Immunity',0),(2,1,'Photosynthesis',1),(3,1,'Evolution',2)",
            [],
        )
        .unwrap();
        conn
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    // -- review points -----------------------------------------------------

    #[test]
    fn review_points_count_backwards_from_the_assessment() {
        let pts = review_points(day("2026-10-01"), day("2026-08-01"));
        assert_eq!(
            pts,
            vec![
                "2026-09-01", // 30 days out
                "2026-09-17", // 14
                "2026-09-24", // 7
                "2026-09-26", // 5
                "2026-09-28", // 3
                "2026-09-29", // 2
                "2026-09-30", // 1
            ]
        );
    }

    /// Points already passed are dropped — surfacing a missed one would be
    /// loss framing, which the brief rules out.
    #[test]
    fn past_review_points_are_dropped() {
        let pts = review_points(day("2026-10-01"), day("2026-09-27"));
        assert_eq!(pts, vec!["2026-09-28", "2026-09-29", "2026-09-30"]);
    }

    #[test]
    fn an_assessment_in_the_past_has_no_review_points() {
        assert!(review_points(day("2026-01-01"), day("2026-08-01")).is_empty());
    }

    // -- assessments -------------------------------------------------------

    #[test]
    fn creating_and_listing_with_a_countdown() {
        let conn = db();
        let future = (retain_today_naive() + chrono::Duration::days(10))
            .format("%Y-%m-%d")
            .to_string();
        create(
            &conn,
            AssessmentInput {
                subject_id: 1,
                name: "Unit 3 AOS1 SAC".into(),
                kind: AssessmentKind::Sac,
                due_on: future.clone(),
                topic_ids: Some(vec![1, 2]),
            },
        )
        .unwrap();

        let list = list(&conn, false).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].days_away, 10);
        assert_eq!(list[0].kind, AssessmentKind::Sac);
        assert_eq!(list[0].topic_ids.len(), 2);
        assert!(!list[0].upcoming_review_points.is_empty());
    }

    #[test]
    fn a_bad_date_is_refused() {
        let conn = db();
        let bad = create(
            &conn,
            AssessmentInput {
                subject_id: 1,
                name: "x".into(),
                kind: AssessmentKind::Exam,
                due_on: "not-a-date".into(),
                topic_ids: None,
            },
        );
        assert!(bad.is_err());
        assert!(list(&conn, true).unwrap().is_empty());
    }

    #[test]
    fn a_name_is_required() {
        let conn = db();
        assert!(create(
            &conn,
            AssessmentInput {
                subject_id: 1,
                name: "   ".into(),
                kind: AssessmentKind::Exam,
                due_on: "2026-12-01".into(),
                topic_ids: None,
            },
        )
        .is_err());
    }

    #[test]
    fn past_assessments_are_hidden_unless_asked_for() {
        let conn = db();
        create(&conn, AssessmentInput {
            subject_id: 1, name: "Old".into(), kind: AssessmentKind::Sac,
            due_on: "2020-01-01".into(), topic_ids: None,
        }).unwrap();

        assert!(list(&conn, false).unwrap().is_empty());
        assert_eq!(list(&conn, true).unwrap().len(), 1);
        assert!(list(&conn, true).unwrap()[0].days_away < 0);
    }

    #[test]
    fn deleting_an_assessment_removes_its_topic_links() {
        let conn = db();
        let id = create(&conn, AssessmentInput {
            subject_id: 1, name: "SAC".into(), kind: AssessmentKind::Sac,
            due_on: "2026-12-01".into(), topic_ids: Some(vec![1, 2]),
        }).unwrap();

        delete(&conn, id).unwrap();
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM assessment_topics", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }

    // -- retrospective surfacing -------------------------------------------

    #[test]
    fn confidence_must_be_one_to_five() {
        let conn = db();
        assert!(log_topic_review(&conn, 1, 0, None).is_err());
        assert!(log_topic_review(&conn, 1, 6, None).is_err());
        assert!(log_topic_review(&conn, 1, 3, None).is_ok());
    }

    /// A topic never reviewed outranks everything — you can't be confident about
    /// something you haven't tested.
    #[test]
    fn never_reviewed_topics_come_first() {
        let conn = db();
        log_topic_review(&conn, 1, 1, None).unwrap();
        log_topic_review(&conn, 2, 5, None).unwrap();

        let ranked = surface(&conn, None, 10).unwrap();
        assert_eq!(ranked[0].topic_id, 3, "the untouched topic should lead");
        assert_eq!(ranked[0].review_count, 0);
        assert!(ranked[0].days_since.is_none());
    }

    /// Low confidence outranks a longer gap at high confidence.
    #[test]
    fn low_confidence_outranks_a_stale_but_solid_topic() {
        let conn = db();
        // Topic 1: shaky, 7 days ago. Topic 2: solid, 20 days ago.
        log_topic_review(&conn, 1, 1, None).unwrap();
        log_topic_review(&conn, 2, 5, None).unwrap();
        log_topic_review(&conn, 3, 5, None).unwrap();

        let seven = (retain_today_naive() - chrono::Duration::days(7)).format("%Y-%m-%d").to_string();
        let twenty = (retain_today_naive() - chrono::Duration::days(20)).format("%Y-%m-%d").to_string();
        conn.execute("UPDATE topic_reviews SET local_date=?1 WHERE topic_id=1", [&seven]).unwrap();
        conn.execute("UPDATE topic_reviews SET local_date=?1 WHERE topic_id=2", [&twenty]).unwrap();
        conn.execute("UPDATE topic_reviews SET local_date=?1 WHERE topic_id=3", [&twenty]).unwrap();

        let ranked = surface(&conn, None, 10).unwrap();
        // 1 → 7 + (5-1)*4 = 23;  2 and 3 → 20 + 0 = 20.
        assert_eq!(ranked[0].topic_id, 1, "shaky-and-recent must beat solid-and-stale");
        assert_eq!(ranked[0].last_confidence, Some(1));
    }

    /// Two reviews inside the same second must still order correctly.
    /// Timestamps are whole-second, so this only works because of the `id DESC`
    /// tiebreak in `surface` — without it, correcting a rating you just entered
    /// gives a nondeterministic answer.
    #[test]
    fn the_most_recent_confidence_is_the_one_used() {
        let conn = db();
        log_topic_review(&conn, 1, 1, None).unwrap();
        log_topic_review(&conn, 1, 5, None).unwrap();

        let ranked = surface(&conn, Some(1), 10).unwrap();
        let t1 = ranked.iter().find(|t| t.topic_id == 1).unwrap();
        assert_eq!(t1.last_confidence, Some(5), "latest review wins");
        assert_eq!(t1.review_count, 2, "but the history is kept");
    }

    #[test]
    fn surfacing_can_be_scoped_to_one_subject() {
        let conn = db();
        conn.execute(
            "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
             VALUES (2,'Chemistry','#5B8DEF','1_2','science',1,'2026-08-12T00:00:00Z')",
            [],
        ).unwrap();
        conn.execute("INSERT INTO topics (id,subject_id,name,sort_order) VALUES (9,2,'Acids',0)", []).unwrap();

        let only_chem = surface(&conn, Some(2), 10).unwrap();
        assert_eq!(only_chem.len(), 1);
        assert_eq!(only_chem[0].topic_id, 9);
    }

    /// Nothing about surfacing is persisted — it is recomputed every call, which
    /// is what makes it retrospective rather than a plan you can fall behind.
    #[test]
    fn surfacing_writes_nothing() {
        let conn = db();
        log_topic_review(&conn, 1, 3, None).unwrap();
        let before: i64 = conn.query_row("SELECT COUNT(*) FROM topic_reviews", [], |r| r.get(0)).unwrap();

        surface(&conn, None, 10).unwrap();
        surface(&conn, None, 10).unwrap();

        let after: i64 = conn.query_row("SELECT COUNT(*) FROM topic_reviews", [], |r| r.get(0)).unwrap();
        assert_eq!(before, after, "surfacing must be read-only");
    }

    /// There is no table anywhere that assigns a topic to a future date.
    #[test]
    fn no_prospective_assignment_table_exists() {
        let conn = db();
        let planning: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                  WHERE type='table' AND (name LIKE '%timetable%' OR name LIKE '%schedule%'
                        OR name LIKE '%planned%')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(planning, 0, "a prospective timetable is an explicit non-goal");
    }
}
