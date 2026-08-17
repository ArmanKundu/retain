//! Card persistence and the review queue.
//!
//! `scheduler.rs` owns the FSRS mathematics and the state machine. This module's
//! only job is to load a card out of SQLite, hand it to the scheduler, and write
//! back exactly what came out — plus the queue rules that decide what you're
//! shown next.
//!
//! ## The two queue rules from the brief, and why they're asymmetric
//!
//! **New cards are capped; reviews never are.** Capping reviews would hide a
//! backlog rather than prevent one: the cards stay due, the count keeps growing,
//! and the app quietly lies about how much work is outstanding. Capping *new*
//! cards is the only lever that actually controls future load, because every new
//! card introduced today becomes a review obligation for months.
//!
//! **Due reviews are always offered before new cards.** Same reason: taking on
//! new material while owing reviews is how a deck becomes unmanageable.

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::anki_import::{self, NoteType, ParsedCard};
use crate::scheduler::{self, CardSnapshot, CardState, Rating, SchedulerConfig};
use crate::settings;
use crate::util::{retain_day_of, retain_day_start, retain_today, retain_today_naive, rfc3339};

/// Default new cards per day, per active subject. The brief's figure.
pub const DEFAULT_NEW_PER_DAY: i64 = 15;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

pub fn config(conn: &Connection) -> anyhow::Result<SchedulerConfig> {
    let retention = settings::get(conn, "fsrs_desired_retention")?
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.90)
        // FSRS is only meaningful in this band; outside it the intervals stop
        // making sense rather than merely being aggressive.
        .clamp(0.70, 0.99);

    Ok(SchedulerConfig {
        desired_retention: retention,
        ..SchedulerConfig::default()
    })
}

pub fn new_per_day(conn: &Connection) -> anyhow::Result<i64> {
    Ok(settings::get_i64(conn, "new_cards_per_day", DEFAULT_NEW_PER_DAY)?.clamp(0, 999))
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub added: usize,
    /// Cards already present (same subject, same content hash). Reported rather
    /// than silently skipped, so a re-paste tells you it changed nothing.
    pub duplicates: usize,
}

pub fn import(
    conn: &Connection,
    subject_id: i64,
    topic_id: Option<i64>,
    parsed: &[ParsedCard],
) -> anyhow::Result<ImportResult> {
    let now = rfc3339(Utc::now());
    let mut result = ImportResult::default();

    for card in parsed {
        let hash = anki_import::content_hash(&card.front, &card.back, card.cloze_index);
        let note_type = match card.note_type {
            NoteType::Basic => "basic",
            NoteType::Cloze => "cloze",
            NoteType::Quote => "quote",
        };

        // `INSERT OR IGNORE` against the (subject_id, content_hash) unique index
        // makes re-importing the same paste a no-op instead of a duplicate deck.
        let changed = conn.execute(
            "INSERT OR IGNORE INTO cards
               (subject_id, topic_id, note_type, front, back, extra, cloze_index,
                tags, state, content_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'new', ?9, ?10)",
            rusqlite::params![
                subject_id,
                topic_id,
                note_type,
                card.front,
                card.back,
                card.extra,
                card.cloze_index,
                card.tags.join(" "),
                hash,
                now,
            ],
        )?;

        if changed == 1 {
            result.added += 1;
        } else {
            result.duplicates += 1;
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub card_id: i64,
    pub subject_id: i64,
    pub subject_name: String,
    pub colour: String,
    pub note_type: String,
    pub front: String,
    pub back: String,
    pub extra: Option<String>,
    pub cloze_index: Option<i64>,
    pub state: CardState,
    /// True when this card has never been reviewed. Drives the "introducing a
    /// new card" affordance in the UI.
    pub is_new: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueCounts {
    /// Every card actually due. Never capped — see the module docs.
    pub due_reviews: i64,
    /// New cards that may still be introduced today, after the cap.
    pub new_available: i64,
    pub new_introduced_today: i64,
    pub new_per_day: i64,
    /// New cards sitting in the deck untouched, regardless of today's cap.
    pub new_remaining_total: i64,
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueueItem> {
    let state = CardState::from_str(&row.get::<_, String>(9)?);
    Ok(QueueItem {
        card_id: row.get(0)?,
        subject_id: row.get(1)?,
        subject_name: row.get(2)?,
        colour: row.get(3)?,
        note_type: row.get(4)?,
        front: row.get(5)?,
        back: row.get(6)?,
        extra: row.get(7)?,
        cloze_index: row.get(8)?,
        state,
        is_new: state == CardState::New,
    })
}

const ITEM_COLUMNS: &str = "c.id, c.subject_id, s.name, s.colour, c.note_type, \
                            c.front, c.back, c.extra, c.cloze_index, c.state";

/// How many new cards each subject may still introduce today.
fn new_allowance(conn: &Connection, subject_id: i64) -> anyhow::Result<i64> {
    let cap = new_per_day(conn)?;
    let today = retain_today();
    let used: i64 = conn.query_row(
        "SELECT COUNT(*) FROM cards WHERE subject_id = ?1 AND introduced_on = ?2",
        rusqlite::params![subject_id, today],
        |r| r.get(0),
    )?;
    Ok((cap - used).max(0))
}

/// The next cards to study.
///
/// Due reviews come first and in full; new cards fill whatever room is left,
/// bounded by each subject's remaining daily allowance.
pub fn queue(conn: &Connection, subject_id: Option<i64>, limit: i64) -> anyhow::Result<Vec<QueueItem>> {
    let now = rfc3339(Utc::now());
    let mut out = Vec::new();

    // --- 1. Due reviews, uncapped -----------------------------------------
    //
    // `due_at <= now` covers both cases uniformly: intraday learning steps are
    // stamped with a real time, and interday cards are stamped with the start of
    // their due Retain day (4am), so they become available the moment that study
    // day begins rather than at the hour the last review happened.
    //
    // Ordering is by due time, so the most overdue card is offered first, and
    // subjects interleave naturally rather than one subject monopolising the
    // session.
    let sql = format!(
        "SELECT {ITEM_COLUMNS}
           FROM cards c JOIN subjects s ON s.id = c.subject_id
          WHERE c.suspended = 0
            AND c.state != 'new'
            AND c.due_at IS NOT NULL
            AND c.due_at <= ?1
            AND (?2 IS NULL OR c.subject_id = ?2)
          ORDER BY c.due_at ASC, c.id ASC
          LIMIT ?3"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![now, subject_id, limit], row_to_item)?;
    for r in rows {
        out.push(r?);
    }

    if out.len() as i64 >= limit {
        return Ok(out);
    }

    // --- 2. New cards, capped per subject per day -------------------------
    let remaining = limit - out.len() as i64;
    let sql = format!(
        "SELECT {ITEM_COLUMNS}
           FROM cards c JOIN subjects s ON s.id = c.subject_id
          WHERE c.suspended = 0
            AND c.state = 'new'
            AND s.archived = 0
            AND (?1 IS NULL OR c.subject_id = ?1)
          ORDER BY c.subject_id ASC, c.id ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let candidates = stmt.query_map(rusqlite::params![subject_id], row_to_item)?;

    // Track the allowance as we go so a single call can't hand out more than the
    // cap for one subject.
    let mut allowance: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    let mut taken = 0i64;

    for candidate in candidates {
        let item = candidate?;
        if taken >= remaining {
            break;
        }
        let left = match allowance.get(&item.subject_id) {
            Some(v) => *v,
            None => {
                let v = new_allowance(conn, item.subject_id)?;
                allowance.insert(item.subject_id, v);
                v
            }
        };
        if left <= 0 {
            continue;
        }
        allowance.insert(item.subject_id, left - 1);
        out.push(item);
        taken += 1;
    }

    Ok(out)
}

pub fn counts(conn: &Connection, subject_id: Option<i64>) -> anyhow::Result<QueueCounts> {
    let now = rfc3339(Utc::now());
    let today = retain_today();
    let cap = new_per_day(conn)?;

    let due_reviews: i64 = conn.query_row(
        "SELECT COUNT(*) FROM cards
          WHERE suspended = 0 AND state != 'new'
            AND due_at IS NOT NULL AND due_at <= ?1
            AND (?2 IS NULL OR subject_id = ?2)",
        rusqlite::params![now, subject_id],
        |r| r.get(0),
    )?;

    let new_introduced_today: i64 = conn.query_row(
        "SELECT COUNT(*) FROM cards
          WHERE introduced_on = ?1 AND (?2 IS NULL OR subject_id = ?2)",
        rusqlite::params![today, subject_id],
        |r| r.get(0),
    )?;

    let new_remaining_total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM cards c JOIN subjects s ON s.id = c.subject_id
          WHERE c.state = 'new' AND c.suspended = 0 AND s.archived = 0
            AND (?1 IS NULL OR c.subject_id = ?1)",
        rusqlite::params![subject_id],
        |r| r.get(0),
    )?;

    // Sum each subject's own remaining allowance rather than applying the cap
    // once globally — the cap is per subject.
    let mut stmt = conn.prepare(
        "SELECT c.subject_id, COUNT(*) FROM cards c JOIN subjects s ON s.id = c.subject_id
          WHERE c.state = 'new' AND c.suspended = 0 AND s.archived = 0
            AND (?1 IS NULL OR c.subject_id = ?1)
          GROUP BY c.subject_id",
    )?;
    let per_subject = stmt.query_map(rusqlite::params![subject_id], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
    })?;

    let mut new_available = 0i64;
    for row in per_subject {
        let (sid, waiting) = row?;
        new_available += waiting.min(new_allowance(conn, sid)?);
    }

    Ok(QueueCounts {
        due_reviews,
        new_available,
        new_introduced_today,
        new_per_day: cap,
        new_remaining_total,
    })
}

// ---------------------------------------------------------------------------
// Answering
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerResult {
    pub card_id: i64,
    pub state: CardState,
    /// Days until the next review; `None` for an intraday learning step.
    pub interval_days: Option<i64>,
    pub due_at: String,
    pub stability: f32,
    pub difficulty: f32,
    pub reps: i64,
    pub lapses: i64,
}

fn load_snapshot(conn: &Connection, card_id: i64) -> anyhow::Result<CardSnapshot> {
    let (state, stability, difficulty, last_review, reps, lapses, step): (
        String,
        Option<f64>,
        Option<f64>,
        Option<String>,
        i64,
        i64,
        i64,
    ) = conn.query_row(
        "SELECT state, stability, difficulty, last_review_at, reps, lapses, learning_step
           FROM cards WHERE id = ?1",
        [card_id],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        },
    )?;

    Ok(CardSnapshot {
        state: CardState::from_str(&state),
        stability: stability.map(|v| v as f32),
        difficulty: difficulty.map(|v| v as f32),
        last_review_at: last_review
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|d| d.with_timezone(&Utc)),
        reps,
        lapses,
        learning_step: step,
    })
}

/// Apply a rating and persist the result.
///
/// `presented_at` is the moment the card was shown, carried from the UI so the
/// `review_log` row records genuine thinking time rather than a fabricated
/// zero — the streak's "reviews cleared" branch depends on that row being real.
pub fn answer(
    conn: &Connection,
    card_id: i64,
    rating: Rating,
    presented_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> anyhow::Result<AnswerResult> {
    let snapshot = load_snapshot(conn, card_id)?;
    let cfg = config(conn)?;
    let engine = scheduler::engine()?;

    // Capture the due date the card carried BEFORE rescheduling.
    //
    // This has to happen ahead of the UPDATE below. Reading it afterwards gets
    // the card's *new* due date, which would make `review_log.due_on` record
    // when the card is next due rather than when it was due — and that silently
    // breaks `streak::due_count_on`, whose whole job is to reconcile "was due on
    // day D" against "was reviewed on day D". A card answered on its due day
    // would stop matching itself.
    //
    // A brand-new card has no prior due date; it was introduced rather than due,
    // so it is recorded against today.
    let due_on_before: String = conn
        .query_row("SELECT due_on FROM cards WHERE id = ?1", [card_id], |r| {
            r.get::<_, Option<String>>(0)
        })?
        .unwrap_or_else(|| retain_day_of(now));

    let scheduled = scheduler::schedule(&engine, card_id, &snapshot, rating, now, &cfg)?;

    // Interday intervals are anchored to the START of the target study day, so a
    // card scheduled at 9pm becomes available at 4am on its due day rather than
    // at 9pm that evening. Intraday steps keep the scheduler's exact instant.
    let due_at = match scheduled.interval_days {
        Some(days) => retain_day_start(retain_today_naive() + chrono::Duration::days(days)),
        None => scheduled.due_at,
    };
    let due_on = retain_day_of(due_at);

    // The day the card stopped being new is written once and never revised, so
    // the daily cap can't be dodged by answering Again until it resets.
    let introduced_on: Option<String> = if snapshot.state == CardState::New {
        Some(retain_day_of(now))
    } else {
        None
    };

    conn.execute(
        "UPDATE cards
            SET state = ?1, stability = ?2, difficulty = ?3,
                due_at = ?4, due_on = ?5, last_review_at = ?6,
                reps = ?7, lapses = ?8, learning_step = ?9,
                introduced_on = COALESCE(introduced_on, ?10)
          WHERE id = ?11",
        rusqlite::params![
            scheduled.state.as_str(),
            scheduled.stability as f64,
            scheduled.difficulty as f64,
            rfc3339(due_at),
            due_on,
            rfc3339(now),
            scheduled.reps,
            scheduled.lapses,
            scheduled.learning_step,
            introduced_on,
            card_id,
        ],
    )?;

    // Feed the streak's "reviews cleared" branch. This row is the only evidence
    // that a review genuinely happened, so it carries both timestamps.
    conn.execute(
        "INSERT INTO review_log
           (item_type, item_id, subject_id, due_on, presented_at, rated_at,
            duration_ms, rating, local_date)
         VALUES ('card', ?1,
                 (SELECT subject_id FROM cards WHERE id = ?1),
                 ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            card_id,
            due_on_before,
            rfc3339(presented_at),
            rfc3339(now),
            (now - presented_at).num_milliseconds().max(0),
            rating as i64,
            retain_day_of(now),
        ],
    )?;

    Ok(AnswerResult {
        card_id,
        state: scheduled.state,
        interval_days: scheduled.interval_days,
        due_at: rfc3339(due_at),
        stability: scheduled.stability,
        difficulty: scheduled.difficulty,
        reps: scheduled.reps,
        lapses: scheduled.lapses,
    })
}

/// Projected review load per day — the debt you're taking on.
pub fn future_load(conn: &Connection, days: i64) -> anyhow::Result<Vec<(String, i64)>> {
    let today = retain_today_naive();
    let mut out = Vec::new();

    for offset in 0..days.clamp(1, 365) {
        let day = (today + chrono::Duration::days(offset))
            .format("%Y-%m-%d")
            .to_string();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM cards
              WHERE suspended = 0 AND state != 'new' AND due_on = ?1",
            [&day],
            |r| r.get(0),
        )?;
        out.push((day, count));
    }

    Ok(out)
}


// ---------------------------------------------------------------------------
// Managing a deck
// ---------------------------------------------------------------------------

/// One card as it appears in a browse list.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CardRow {
    pub id: i64,
    pub front: String,
    pub back: String,
    pub note_type: String,
    pub state: String,
    pub suspended: bool,
    pub lapses: i64,
    pub reps: i64,
    /// Memory strength in days. `None` before the first answer.
    pub stability: Option<f64>,
    pub due_on: Option<String>,
    pub topic_name: Option<String>,
}

/// Every card in a deck, worst first.
///
/// Worst-first because the reason to open a card list is almost always to fix
/// something: a card you keep failing is badly written more often than it is
/// badly learnt, and it should be the one you see. Alphabetical would bury it.
pub fn list(
    conn: &Connection,
    subject_id: i64,
    topic_id: Option<i64>,
    limit: i64,
) -> anyhow::Result<Vec<CardRow>> {
    // Placeholders are positional, so the LIMIT's number moves with the
    // presence of a topic filter. Hard-coding `?3` for both branches binds two
    // parameters against three placeholders and fails at runtime rather than
    // at compile time — which is exactly how it got through the first time.
    let (scope, limit_ph) = if topic_id.is_some() {
        ("c.subject_id = ?1 AND c.topic_id = ?2", "?3")
    } else {
        ("c.subject_id = ?1", "?2")
    };
    let sql = format!(
        "SELECT c.id, c.front, c.back, c.note_type, c.state, c.suspended,
                c.lapses, c.reps, c.stability, c.due_on, t.name
           FROM cards c LEFT JOIN topics t ON t.id = c.topic_id
          WHERE {scope}
          ORDER BY c.lapses DESC, COALESCE(c.stability, 0) ASC, c.id
          LIMIT {limit_ph}"
    );

    let read = |r: &rusqlite::Row<'_>| -> rusqlite::Result<CardRow> {
        Ok(CardRow {
            id: r.get(0)?,
            front: r.get(1)?,
            back: r.get(2)?,
            note_type: r.get(3)?,
            state: r.get(4)?,
            suspended: r.get::<_, i64>(5)? == 1,
            lapses: r.get(6)?,
            reps: r.get(7)?,
            stability: r.get(8)?,
            due_on: r.get(9)?,
            topic_name: r.get(10)?,
        })
    };

    let mut stmt = conn.prepare(&sql)?;
    let rows = match topic_id {
        Some(t) => stmt
            .query_map(rusqlite::params![subject_id, t, limit], read)?
            .collect::<Result<Vec<_>, _>>()?,
        None => stmt
            .query_map(rusqlite::params![subject_id, limit], read)?
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

/// Delete a card outright.
///
/// Its review history goes too. `review_log` rows are keyed on `item_id` with
/// no foreign key — they're an audit trail the streak reads, and leaving them
/// behind would let a deleted card keep propping up a streak day. They're
/// removed explicitly because SQLite can't cascade what isn't declared.
pub fn delete(conn: &Connection, card_id: i64) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM review_log WHERE item_type = 'card' AND item_id = ?1",
        [card_id],
    )?;
    conn.execute("DELETE FROM cards WHERE id = ?1", [card_id])?;
    Ok(())
}

/// Take a card out of rotation without losing it.
///
/// The right answer for a card you can't fix right now: deleting loses the
/// history and the wording, and leaving it in means meeting it every day.
pub fn set_suspended(conn: &Connection, card_id: i64, suspended: bool) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE cards SET suspended = ?2 WHERE id = ?1",
        rusqlite::params![card_id, suspended as i64],
    )?;
    Ok(())
}

/// Rewrite a card's text.
///
/// Scheduling state is untouched. A reworded card is the same card — you still
/// know roughly as much as you did — and resetting its interval would punish
/// you for improving it, which is exactly backwards for a leech.
pub fn edit(conn: &Connection, card_id: i64, front: &str, back: &str) -> anyhow::Result<()> {
    let front = front.trim();
    let back = back.trim();
    if front.is_empty() || back.is_empty() {
        return Err(anyhow::anyhow!("A card needs both sides."));
    }

    conn.execute(
        "UPDATE cards SET front = ?2, back = ?3, content_hash = ?4 WHERE id = ?1",
        rusqlite::params![
            card_id,
            front,
            back,
            // Kept in step so a later re-import doesn't see the original text
            // as a new card and add a duplicate beside the edited one.
            anki_import::content_hash(front, back, None),
        ],
    )?;
    Ok(())
}

/// Forget a card's history and start it over.
///
/// For a card that is genuinely lost — twelve lapses and no sign of sticking.
/// Its history stays in `review_log` because that record is about what you did,
/// which is still true; only the scheduling is reset.
pub fn reset(conn: &Connection, card_id: i64) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE cards SET state = 'new', stability = NULL, difficulty = NULL,
                due_at = NULL, due_on = NULL, last_review_at = NULL,
                reps = 0, lapses = 0, learning_step = 0, introduced_on = NULL
          WHERE id = ?1",
        [card_id],
    )?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::anki_import;

    pub(crate) fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        // The real chain, not a hand-picked subset — a fixture that drifts
        // from production tests the fixture.
        crate::db::run_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
             VALUES (1,'Biology','#4BA97B','3_4','science',0,'2026-08-12T00:00:00Z'),
                    (2,'Chemistry','#5B8DEF','1_2','science',1,'2026-08-12T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    pub(crate) fn add_cards(conn: &Connection, subject: i64, n: usize) {
        let text: String = (0..n)
            .map(|i| format!("front {subject}-{i}\tback {subject}-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let parsed = anki_import::parse(&text, None);
        assert_eq!(parsed.cards.len(), n);
        import(conn, subject, None, &parsed.cards).unwrap();
    }

    // A card's scheduling columns, read positionally in tests.
    #[allow(clippy::type_complexity)]
    fn card_row(conn: &Connection, id: i64) -> (String, Option<f64>, Option<f64>, i64, i64, Option<String>, Option<String>) {
        conn.query_row(
            "SELECT state, stability, difficulty, reps, lapses, due_at, due_on FROM cards WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .unwrap()
    }

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    // -- import ------------------------------------------------------------

    #[test]
    fn import_dedupes_on_reimport() {
        let conn = db();
        add_cards(&conn, 1, 3);
        let parsed = anki_import::parse("front 1-0\tback 1-0", None);
        let again = import(&conn, 1, None, &parsed.cards).unwrap();
        assert_eq!(again.added, 0);
        assert_eq!(again.duplicates, 1);
    }

    // -- state machine through persistence ---------------------------------

    /// New → Learning → Review, with everything persisted between steps.
    #[test]
    fn new_to_learning_to_review_persists() {
        let conn = db();
        add_cards(&conn, 1, 1);
        let id = 1;

        assert_eq!(card_row(&conn, id).0, "new");

        let t0 = now();
        let a = answer(&conn, id, Rating::Good, t0, t0).unwrap();
        assert_eq!(a.state, CardState::Learning);
        let (state, stab, diff, reps, lapses, due_at, _) = card_row(&conn, id);
        assert_eq!(state, "learning");
        assert!(stab.unwrap() > 0.0, "stability must persist");
        assert!(diff.unwrap() > 0.0, "difficulty must persist");
        assert_eq!(reps, 1);
        assert_eq!(lapses, 0);
        assert!(due_at.is_some());

        // Second Good graduates off the last learning step.
        let t1 = t0 + chrono::Duration::minutes(10);
        let b = answer(&conn, id, Rating::Good, t1, t1).unwrap();
        assert_eq!(b.state, CardState::Review);
        assert!(b.interval_days.unwrap() >= 1);
        assert_eq!(card_row(&conn, id).0, "review");
        assert_eq!(card_row(&conn, id).3, 2, "reps must accumulate");
    }

    #[test]
    fn all_four_ratings_persist_distinct_outcomes() {
        for (rating, expect_state) in [
            (Rating::Again, CardState::Learning),
            (Rating::Hard, CardState::Learning),
            (Rating::Good, CardState::Learning),
            (Rating::Easy, CardState::Review),
        ] {
            let conn = db();
            add_cards(&conn, 1, 1);
            let t = now();
            let a = answer(&conn, 1, rating, t, t).unwrap();
            assert_eq!(a.state, expect_state, "rating {rating:?}");
            assert_eq!(card_row(&conn, 1).3, 1);
        }
    }

    /// Review → Relearning on a lapse, with `lapses` incremented in the row.
    #[test]
    fn lapse_moves_to_relearning_and_persists_lapse_count() {
        let conn = db();
        add_cards(&conn, 1, 1);
        let t = now();
        answer(&conn, 1, Rating::Easy, t, t).unwrap(); // straight to Review
        assert_eq!(card_row(&conn, 1).0, "review");

        let later = t + chrono::Duration::days(3);
        let a = answer(&conn, 1, Rating::Again, later, later).unwrap();
        assert_eq!(a.state, CardState::Relearning);
        assert_eq!(a.lapses, 1);

        let (state, _, _, _, lapses, _, _) = card_row(&conn, 1);
        assert_eq!(state, "relearning");
        assert_eq!(lapses, 1, "lapse count must persist");
    }

    /// A second review in the same Retain day must reach FSRS as `days_elapsed = 0`,
    /// which routes it to the crate's short-term path rather than the interday one.
    ///
    /// The discriminator is a comparison, not an absolute value. For a same-day
    /// *Good*, FSRS-6 computes `sinc = exp(w17·(rating−3+w18))·s^(−w19)` ≈ 0.994
    /// and then floors it with `max(sinc, 1.0)` — so stability is deliberately
    /// left unchanged. Asserting that it *moves* would be asserting the algorithm
    /// is something other than what it is. What must hold is that a same-day
    /// review and a next-day review produce **different** outcomes; if elapsed
    /// days were being dropped or hard-coded, they'd be identical.
    #[test]
    fn same_day_review_takes_a_different_path_from_next_day() {
        let seed = |gap: chrono::Duration| {
            let conn = db();
            add_cards(&conn, 1, 1);
            let t = now();
            answer(&conn, 1, Rating::Good, t, t).unwrap();
            let before = card_row(&conn, 1).1.unwrap();
            answer(&conn, 1, Rating::Good, t + gap, t + gap).unwrap();
            (before, card_row(&conn, 1).1.unwrap())
        };

        let (before_same, same_day) = seed(chrono::Duration::minutes(3));
        let (_, next_day) = seed(chrono::Duration::days(1));

        assert_ne!(
            same_day, next_day,
            "same-day and next-day reviews must not schedule identically — \
             elapsed days are being dropped"
        );

        // And the same-day short-term path must never *reduce* stability on a
        // successful answer, which is what the `max(sinc, 1.0)` floor guarantees.
        assert!(
            same_day >= before_same,
            "a same-day success dropped stability: {before_same} → {same_day}"
        );
    }

    /// Difficulty updates on every review regardless of elapsed days, so it is
    /// the signal that a same-day review registered at all.
    #[test]
    fn same_day_review_still_updates_difficulty() {
        let conn = db();
        add_cards(&conn, 1, 1);
        let t = now();
        answer(&conn, 1, Rating::Good, t, t).unwrap();
        let d1 = card_row(&conn, 1).2.unwrap();

        let t2 = t + chrono::Duration::minutes(3);
        answer(&conn, 1, Rating::Again, t2, t2).unwrap();
        let d2 = card_row(&conn, 1).2.unwrap();

        assert!(d2 > d1, "failing a card must make it harder: {d1} → {d2}");
        assert_eq!(card_row(&conn, 1).3, 2, "both reviews counted");
    }

    /// Answering late must feed FSRS the ACTUAL elapsed days, not the interval
    /// that was scheduled. A card answered 30 days after a 1-day schedule must
    /// end up with a different stability than one answered on time.
    #[test]
    fn uses_actual_elapsed_days_not_scheduled_interval() {
        let t = now();

        let on_time = {
            let conn = db();
            add_cards(&conn, 1, 1);
            answer(&conn, 1, Rating::Easy, t, t).unwrap();
            let later = t + chrono::Duration::days(1);
            answer(&conn, 1, Rating::Good, later, later).unwrap().stability
        };

        let very_late = {
            let conn = db();
            add_cards(&conn, 1, 1);
            answer(&conn, 1, Rating::Easy, t, t).unwrap();
            let later = t + chrono::Duration::days(60);
            answer(&conn, 1, Rating::Good, later, later).unwrap().stability
        };

        assert_ne!(
            on_time, very_late,
            "elapsed time must affect the outcome; the scheduled interval was identical"
        );
    }

    // -- queue rules -------------------------------------------------------

    #[test]
    fn new_cards_are_capped_per_subject_per_day() {
        let conn = db();
        add_cards(&conn, 1, 40);
        settings::set(&conn, "new_cards_per_day", "15").unwrap();

        let q = queue(&conn, None, 100).unwrap();
        assert_eq!(q.len(), 15, "cap is 15/day/subject");

        // Introduce all 15, then the well is dry for today.
        let t = now();
        for item in &q {
            answer(&conn, item.card_id, Rating::Good, t, t).unwrap();
        }
        let c = counts(&conn, Some(1)).unwrap();
        assert_eq!(c.new_introduced_today, 15);
        assert_eq!(c.new_available, 0, "no allowance left today");
        assert_eq!(c.new_remaining_total, 25, "but the deck still has more");
    }

    /// The cap is PER SUBJECT: two subjects each get their own 15.
    #[test]
    fn the_cap_is_per_subject_not_global() {
        let conn = db();
        add_cards(&conn, 1, 30);
        add_cards(&conn, 2, 30);
        settings::set(&conn, "new_cards_per_day", "15").unwrap();

        let q = queue(&conn, None, 100).unwrap();
        assert_eq!(q.len(), 30, "15 from each of two subjects");
        assert_eq!(q.iter().filter(|i| i.subject_id == 1).count(), 15);
        assert_eq!(q.iter().filter(|i| i.subject_id == 2).count(), 15);
    }

    /// Answering Again must not refund the day's allowance.
    #[test]
    fn failing_a_new_card_still_consumes_its_allowance() {
        let conn = db();
        add_cards(&conn, 1, 5);
        settings::set(&conn, "new_cards_per_day", "2").unwrap();
        let t = now();

        let q = queue(&conn, None, 100).unwrap();
        assert_eq!(q.len(), 2);
        for item in &q {
            answer(&conn, item.card_id, Rating::Again, t, t).unwrap();
        }
        assert_eq!(counts(&conn, Some(1)).unwrap().new_available, 0);
    }

    /// Reviews are NEVER capped, even far beyond the new-card limit.
    #[test]
    fn reviews_are_never_capped() {
        let conn = db();
        add_cards(&conn, 1, 50);
        settings::set(&conn, "new_cards_per_day", "1").unwrap();

        // Force 50 cards into a due review state directly.
        let past = rfc3339(now() - chrono::Duration::days(1));
        conn.execute(
            "UPDATE cards SET state='review', stability=10.0, difficulty=5.0,
                    due_at=?1, due_on='2000-01-01', reps=1",
            [&past],
        )
        .unwrap();

        let c = counts(&conn, None).unwrap();
        assert_eq!(c.due_reviews, 50, "all due reviews must be reported");

        let q = queue(&conn, None, 500).unwrap();
        assert_eq!(q.len(), 50, "no cap applies to reviews");
    }

    /// Due reviews must be offered before any new card.
    #[test]
    fn due_reviews_come_before_new_cards() {
        let conn = db();
        add_cards(&conn, 1, 10);
        let past = rfc3339(now() - chrono::Duration::days(1));
        // Make cards 1-3 due reviews; 4-10 stay new.
        conn.execute(
            "UPDATE cards SET state='review', stability=10.0, difficulty=5.0,
                    due_at=?1, due_on='2000-01-01', reps=1 WHERE id <= 3",
            [&past],
        )
        .unwrap();

        let q = queue(&conn, None, 100).unwrap();
        assert!(q.len() > 3);
        for item in q.iter().take(3) {
            assert!(!item.is_new, "reviews must lead the queue");
        }
        assert!(q[3].is_new, "new cards follow");
    }

    /// A card scheduled into the future must not appear in the queue.
    #[test]
    fn future_cards_are_not_due() {
        let conn = db();
        add_cards(&conn, 1, 1);
        let t = now();
        answer(&conn, 1, Rating::Easy, t, t).unwrap();

        let q = queue(&conn, None, 100).unwrap();
        assert!(
            q.iter().all(|i| i.card_id != 1),
            "a card just scheduled days out must not be due now"
        );
    }

    /// Queue ordering across subjects: most overdue first, interleaved.
    #[test]
    fn queue_orders_by_due_time_across_subjects() {
        let conn = db();
        add_cards(&conn, 1, 2);
        add_cards(&conn, 2, 2);
        let base = now() - chrono::Duration::days(5);

        // Interleave due times so subject order alone can't produce the answer.
        for (id, offset_hours) in [(1i64, 0i64), (3, 1), (2, 2), (4, 3)] {
            conn.execute(
                "UPDATE cards SET state='review', stability=10.0, difficulty=5.0,
                        due_at=?1, due_on='2000-01-01', reps=1 WHERE id=?2",
                rusqlite::params![rfc3339(base + chrono::Duration::hours(offset_hours)), id],
            )
            .unwrap();
        }

        let q = queue(&conn, None, 100).unwrap();
        let ids: Vec<i64> = q.iter().map(|i| i.card_id).collect();
        assert_eq!(ids, vec![1, 3, 2, 4], "must be by due time, not by subject");
    }

    #[test]
    fn queue_can_be_filtered_to_one_subject() {
        let conn = db();
        add_cards(&conn, 1, 5);
        add_cards(&conn, 2, 5);
        let q = queue(&conn, Some(2), 100).unwrap();
        assert!(!q.is_empty());
        assert!(q.iter().all(|i| i.subject_id == 2));
    }

    // -- persistence / durability -----------------------------------------

    /// Due dates must round-trip through the database, and land on the start of
    /// the study day rather than the hour the review happened.
    #[test]
    fn due_dates_persist_and_anchor_to_the_study_day() {
        let conn = db();
        add_cards(&conn, 1, 1);
        let t = now();
        let a = answer(&conn, 1, Rating::Easy, t, t).unwrap();

        let (_, _, _, _, _, due_at, due_on) = card_row(&conn, 1);
        assert_eq!(due_at.unwrap(), a.due_at, "due_at must persist verbatim");

        let expected_day =
            (retain_today_naive() + chrono::Duration::days(a.interval_days.unwrap()))
                .format("%Y-%m-%d")
                .to_string();
        assert_eq!(due_on.unwrap(), expected_day);
    }

    /// "Restart the app mid-review": reopening the database must see exactly the
    /// committed state, with no partial writes and a clean integrity check.
    #[test]
    fn state_survives_a_simulated_restart() {
        let file = std::env::temp_dir().join(format!("retain-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&file);

        let expected = {
            let conn = Connection::open(&file).unwrap();
            conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
            conn.execute_batch(include_str!("db/migrations/001_init.sql")).unwrap();
            conn.execute_batch(include_str!("db/migrations/002_capture_cards_errors.sql")).unwrap();
            conn.execute(
                "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
                 VALUES (1,'Biology','#4BA97B','3_4','science',0,'2026-08-12T00:00:00Z')",
                [],
            )
            .unwrap();
            add_cards(&conn, 1, 5);

            let t = now();
            // Answer two of five, then "crash" by dropping the connection.
            answer(&conn, 1, Rating::Good, t, t).unwrap();
            answer(&conn, 2, Rating::Easy, t, t).unwrap();
            card_row(&conn, 1)
        };

        // Reopen — a fresh process would see exactly this.
        let conn = Connection::open(&file).unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0)).unwrap();
        assert_eq!(check, "ok", "database must not be corrupt after restart");

        assert_eq!(card_row(&conn, 1), expected, "state must survive verbatim");
        assert_eq!(card_row(&conn, 3).0, "new", "unanswered cards stay new");

        // And the partially-reviewed card is still answerable.
        let t = now();
        let resumed = answer(&conn, 1, Rating::Good, t, t).unwrap();
        assert_eq!(resumed.reps, 2);

        let _ = std::fs::remove_file(&file);
    }

    /// Every answer must leave a review_log row — the streak's "reviews cleared"
    /// branch is only allowed to trust genuine, timestamped review activity.
    #[test]
    fn answering_writes_an_auditable_review_log_row() {
        let conn = db();
        add_cards(&conn, 1, 1);
        let presented = now();
        let rated = presented + chrono::Duration::seconds(7);
        answer(&conn, 1, Rating::Good, presented, rated).unwrap();

        let (rating, duration, item): (i64, i64, String) = conn
            .query_row(
                "SELECT rating, duration_ms, item_type FROM review_log WHERE item_id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(rating, 3);
        assert_eq!(item, "card");
        assert_eq!(duration, 7000, "thinking time must be recorded, not zeroed");
    }

    // -- tuning knobs ------------------------------------------------------

    #[test]
    fn target_retention_is_read_from_settings_and_changes_intervals() {
        let relaxed = {
            let conn = db();
            add_cards(&conn, 1, 1);
            settings::set(&conn, "fsrs_desired_retention", "0.80").unwrap();
            let t = now();
            answer(&conn, 1, Rating::Easy, t, t).unwrap().interval_days.unwrap()
        };
        let strict = {
            let conn = db();
            add_cards(&conn, 1, 1);
            settings::set(&conn, "fsrs_desired_retention", "0.95").unwrap();
            let t = now();
            answer(&conn, 1, Rating::Easy, t, t).unwrap().interval_days.unwrap()
        };
        assert!(strict < relaxed, "0.95 gave {strict}d, 0.80 gave {relaxed}d");
    }

    #[test]
    fn retention_setting_is_clamped_to_a_sane_band() {
        let conn = db();
        settings::set(&conn, "fsrs_desired_retention", "9.9").unwrap();
        assert!(config(&conn).unwrap().desired_retention <= 0.99);
        settings::set(&conn, "fsrs_desired_retention", "0.01").unwrap();
        assert!(config(&conn).unwrap().desired_retention >= 0.70);
    }

    /// Fuzz must actually spread intervals: identical cards answered identically
    /// must not all land on the same day.
    #[test]
    fn intervals_are_fuzzed_across_cards() {
        let conn = db();
        add_cards(&conn, 1, 40);
        settings::set(&conn, "new_cards_per_day", "40").unwrap();
        let t = now();

        let mut intervals = std::collections::HashSet::new();
        for id in 1..=40i64 {
            // Two Easys to reach a long enough interval for fuzz to have room.
            answer(&conn, id, Rating::Easy, t, t).unwrap();
            let later = t + chrono::Duration::days(10);
            let a = answer(&conn, id, Rating::Easy, later, later).unwrap();
            intervals.insert(a.interval_days.unwrap());
        }
        assert!(
            intervals.len() > 1,
            "all 40 identical cards landed on the same interval — fuzz is not applied"
        );
    }

    /// `review_log.due_on` must record the day the card **was** due, not the day
    /// it is due next. Reading it after the reschedule (the original bug) made a
    /// card answered on its due day stop matching itself in
    /// `streak::due_count_on`, which reconciles "was due on D" against
    /// "was reviewed on D".
    #[test]
    fn review_log_records_the_original_due_date_not_the_new_one() {
        let conn = db();
        add_cards(&conn, 1, 1);

        // Put the card in a review state that was due yesterday.
        let yesterday = (retain_today_naive() - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        conn.execute(
            "UPDATE cards SET state='review', stability=10.0, difficulty=5.0,
                    due_at=?1, due_on=?2, reps=1 WHERE id=1",
            rusqlite::params![rfc3339(now() - chrono::Duration::days(1)), yesterday],
        )
        .unwrap();

        let t = now();
        let result = answer(&conn, 1, Rating::Good, t, t).unwrap();

        let logged: String = conn
            .query_row("SELECT due_on FROM review_log WHERE item_id = 1", [], |r| r.get(0))
            .unwrap();

        assert_eq!(logged, yesterday, "must log the date the card WAS due");
        assert_ne!(
            logged,
            retain_day_of(
                DateTime::parse_from_rfc3339(&result.due_at).unwrap().with_timezone(&Utc)
            ),
            "must not log the newly scheduled date"
        );
    }

    /// End to end: a card due today, answered today, must make that day count as
    /// "reviews cleared" for the streak.
    #[test]
    fn answering_a_due_card_clears_that_day_for_the_streak() {
        let conn = db();
        add_cards(&conn, 1, 1);

        let today = retain_today();
        conn.execute(
            "UPDATE cards SET state='review', stability=10.0, difficulty=5.0,
                    due_at=?1, due_on=?2, reps=1 WHERE id=1",
            rusqlite::params![rfc3339(now() - chrono::Duration::hours(1)), today],
        )
        .unwrap();

        let t = now();
        answer(&conn, 1, Rating::Good, t, t).unwrap();

        // The streak's branch B must now see the day as fully cleared.
        let qualifying = crate::streak::qualifying_days(&conn, 20).unwrap();
        assert!(
            qualifying.contains(&today),
            "clearing every due review must earn the day"
        );
    }

    #[test]
    fn future_load_reports_the_debt() {
        let conn = db();
        add_cards(&conn, 1, 3);
        let t = now();
        for id in 1..=3i64 {
            answer(&conn, id, Rating::Easy, t, t).unwrap();
        }
        let load = future_load(&conn, 30).unwrap();
        assert_eq!(load.len(), 30);
        assert!(load.iter().map(|(_, n)| n).sum::<i64>() >= 3);
    }

    // -- managing a deck ----------------------------------------------------

    fn log_count(conn: &Connection, card: i64) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM review_log WHERE item_type = 'card' AND item_id = ?1",
            [card],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// A card you keep failing is badly written more often than badly learnt,
    /// so it's the one you should see first when you open the list.
    #[test]
    fn the_card_list_puts_the_worst_first() {
        let conn = db();
        add_cards(&conn, 1, 3);
        conn.execute("UPDATE cards SET lapses = 9, stability = 1.0 WHERE id = 2", []).unwrap();
        conn.execute("UPDATE cards SET lapses = 0, stability = 40.0 WHERE id = 1", []).unwrap();

        let rows = list(&conn, 1, None, 50).unwrap();
        assert_eq!(rows[0].id, 2, "nine lapses leads");
        assert_eq!(rows.last().unwrap().id, 1, "the solid one is last");
    }

    /// The streak reads `review_log` by `item_id` with no foreign key. Leaving
    /// the rows behind would let a deleted card keep propping up a streak day.
    #[test]
    fn deleting_a_card_takes_its_review_history_with_it() {
        let conn = db();
        add_cards(&conn, 1, 1);
        answer(&conn, 1, Rating::Good, now(), now()).unwrap();

        assert_eq!(log_count(&conn, 1), 1);
        delete(&conn, 1).unwrap();

        let cards: i64 = conn.query_row("SELECT COUNT(*) FROM cards", [], |r| r.get(0)).unwrap();
        assert_eq!(cards, 0);
        assert_eq!(log_count(&conn, 1), 0, "an orphan would still count toward a streak");
    }

    #[test]
    fn a_suspended_card_stops_appearing_in_the_queue() {
        let conn = db();
        add_cards(&conn, 1, 2);

        set_suspended(&conn, 1, true).unwrap();
        let ids: Vec<i64> = queue(&conn, None, 50).unwrap().iter().map(|q| q.card_id).collect();
        assert!(!ids.contains(&1));
        assert!(ids.contains(&2));

        set_suspended(&conn, 1, false).unwrap();
        let back: Vec<i64> = queue(&conn, None, 50).unwrap().iter().map(|q| q.card_id).collect();
        assert!(back.contains(&1));
    }

    /// A reworded card is the same card. Resetting its interval would punish
    /// you for improving it, which is backwards for the leeches you'd edit.
    #[test]
    fn editing_a_card_keeps_its_schedule() {
        let conn = db();
        add_cards(&conn, 1, 1);
        answer(&conn, 1, Rating::Good, now(), now()).unwrap();

        let before: (Option<f64>, i64) = conn
            .query_row("SELECT stability, reps FROM cards WHERE id = 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();

        edit(&conn, 1, "  New front  ", "New back").unwrap();

        let after: (String, Option<f64>, i64) = conn
            .query_row("SELECT front, stability, reps FROM cards WHERE id = 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();

        assert_eq!(after.0, "New front", "trimmed");
        assert_eq!(after.1, before.0, "stability untouched");
        assert_eq!(after.2, before.1);
    }

    /// So a later re-import doesn't see the original wording as a new card and
    /// add a duplicate beside the edited one.
    #[test]
    fn editing_updates_the_hash_used_to_spot_duplicates() {
        let conn = db();
        add_cards(&conn, 1, 1);
        let before: String = conn
            .query_row("SELECT content_hash FROM cards WHERE id = 1", [], |r| r.get(0))
            .unwrap();

        edit(&conn, 1, "Changed", "Also changed").unwrap();

        let after: String = conn
            .query_row("SELECT content_hash FROM cards WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_ne!(before, after);
    }

    #[test]
    fn a_card_cannot_be_edited_into_having_no_sides() {
        let conn = db();
        add_cards(&conn, 1, 1);

        assert!(edit(&conn, 1, "", "back").is_err());
        assert!(edit(&conn, 1, "front", "   ").is_err());
    }

    /// Resetting is for a card that's genuinely lost. What you *did* is still
    /// true, so the log stays; only the scheduling goes.
    #[test]
    fn resetting_clears_the_schedule_but_not_the_record() {
        let conn = db();
        add_cards(&conn, 1, 1);
        answer(&conn, 1, Rating::Again, now(), now()).unwrap();
        answer(&conn, 1, Rating::Good, now(), now()).unwrap();

        reset(&conn, 1).unwrap();

        let row = &list(&conn, 1, None, 10).unwrap()[0];
        assert_eq!(row.state, "new");
        assert_eq!(row.reps, 0);
        assert_eq!(row.stability, None);
        assert_eq!(log_count(&conn, 1), 2, "what you did is still what you did");
    }

}

/// What each rating would do to this card, without recording anything.
///
/// Added for the Review screen, which shows the next interval under each button
/// so the scheduling is legible without exposing FSRS itself. It is a pure read:
/// it runs the same `scheduler::schedule` the real answer path runs, against the
/// same snapshot, and writes nothing. Duplicating the interval maths in
/// TypeScript instead would have created a second, drifting implementation of
/// the one thing in the app that must not drift.
pub fn preview(
    conn: &Connection,
    card_id: i64,
    now: DateTime<Utc>,
) -> anyhow::Result<Vec<(scheduler::Rating, Option<i64>)>> {
    let snapshot = load_snapshot(conn, card_id)?;
    let cfg = config(conn)?;
    let engine = scheduler::engine()?;

    [
        scheduler::Rating::Again,
        scheduler::Rating::Hard,
        scheduler::Rating::Good,
        scheduler::Rating::Easy,
    ]
    .into_iter()
    .map(|rating| {
        let s = scheduler::schedule(&engine, card_id, &snapshot, rating, now, &cfg)?;
        Ok((rating, s.interval_days))
    })
    .collect()
}

#[cfg(test)]
mod preview_tests {
    use super::*;

    /// The preview must agree with what answering actually does — it exists to
    /// make scheduling legible, so a preview that disagreed with reality would
    /// be worse than showing nothing.
    #[test]
    fn the_preview_matches_what_answering_actually_schedules() {
        let conn = tests::db();
        tests::add_cards(&conn, 1, 1);
        let id: i64 = conn
            .query_row("SELECT id FROM cards LIMIT 1", [], |r| r.get(0))
            .unwrap();

        let now = chrono::Utc::now();
        let previewed = preview(&conn, id, now).unwrap();
        assert_eq!(previewed.len(), 4, "one entry per rating");

        // Answering Good must land on exactly the interval the preview promised.
        let promised = previewed
            .iter()
            .find(|(r, _)| *r == scheduler::Rating::Good)
            .unwrap()
            .1;
        let actual = answer(&conn, id, scheduler::Rating::Good, now, now).unwrap();
        assert_eq!(promised, actual.interval_days);
    }

    /// Previewing must not touch the card. If it did, looking at the buttons
    /// would silently reschedule what you were about to review.
    #[test]
    fn previewing_records_nothing() {
        let conn = tests::db();
        tests::add_cards(&conn, 1, 1);
        let id: i64 = conn
            .query_row("SELECT id FROM cards LIMIT 1", [], |r| r.get(0))
            .unwrap();

        let before: (String, i64, Option<String>) = conn
            .query_row("SELECT state, reps, due_at FROM cards WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        let logs_before: i64 = conn
            .query_row("SELECT COUNT(*) FROM review_log", [], |r| r.get(0))
            .unwrap();

        preview(&conn, id, chrono::Utc::now()).unwrap();

        let after: (String, i64, Option<String>) = conn
            .query_row("SELECT state, reps, due_at FROM cards WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        let logs_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM review_log", [], |r| r.get(0))
            .unwrap();

        assert_eq!(before, after, "the card was modified by a preview");
        assert_eq!(logs_before, logs_after, "a preview wrote a review log entry");
    }

    /// Harder ratings must never schedule further out than easier ones.
    #[test]
    fn the_previewed_intervals_are_ordered() {
        let conn = tests::db();
        tests::add_cards(&conn, 1, 1);
        let id: i64 = conn
            .query_row("SELECT id FROM cards LIMIT 1", [], |r| r.get(0))
            .unwrap();

        // Take it out of the learning steps so all four are interday.
        let now = chrono::Utc::now();
        answer(&conn, id, scheduler::Rating::Easy, now, now).unwrap();

        let p = preview(&conn, id, now).unwrap();
        let days: Vec<i64> = p.iter().map(|(_, d)| d.unwrap_or(0)).collect();

        assert!(days[0] <= days[2], "Again must not exceed Good: {days:?}");
        assert!(days[1] <= days[2], "Hard must not exceed Good: {days:?}");
        assert!(days[2] <= days[3], "Good must not exceed Easy: {days:?}");
    }
}

/// Cards for practice: no due dates, no scheduling, nothing written back.
///
/// The review queue is the right way to learn and the wrong tool three days
/// before a SAC, when you want to go through Genetics *now* regardless of what
/// FSRS thinks. Doing that through the real queue would be actively harmful:
/// answering forty cards early tells the scheduler you needed them early, and
/// it shortens every one of their intervals in response. A week of cramming
/// would leave your schedule permanently worse.
///
/// So practice reads and never writes. Nothing here touches `cards`, and
/// nothing reaches `review_log` — which also means a practice run cannot
/// manufacture a streak day.
pub fn practice(
    conn: &Connection,
    subject_id: i64,
    topic_id: Option<i64>,
    limit: i64,
) -> anyhow::Result<Vec<QueueItem>> {
    // Weakest first: lowest stability, then least-reviewed. Practice exists to
    // find the gaps, and starting with the cards you already know is a
    // pleasant way to learn nothing.
    let sql = format!(
        "SELECT {ITEM_COLUMNS}
           FROM cards c JOIN subjects s ON s.id = c.subject_id
          WHERE c.subject_id = ?1 AND c.suspended = 0 {}
          ORDER BY COALESCE(c.stability, 0), c.reps, c.id
          LIMIT ?2",
        if topic_id.is_some() { "AND c.topic_id = ?3" } else { "" }
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = match topic_id {
        Some(t) => stmt
            .query_map(rusqlite::params![subject_id, limit, t], row_to_item)?
            .collect::<Result<Vec<_>, _>>()?,
        None => stmt
            .query_map(rusqlite::params![subject_id, limit], row_to_item)?
            .collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}
