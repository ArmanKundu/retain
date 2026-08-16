//! How well you actually know a deck.
//!
//! Review could only answer "what's due now". That's the right question at 8pm
//! on a Tuesday and the wrong one in the week before a SAC, when what you need
//! is to look at Genetics specifically and find out whether you know it.
//!
//! # Retention strength, and why it isn't "percent correct"
//!
//! The obvious metric is right-answers over total-answers. It's also close to
//! useless: a card you've seen twice today and got right twice scores 100% and
//! you'll have forgotten it by Thursday. Percent-correct measures how easy your
//! recent queue was, not what you know.
//!
//! So a card's strength comes from its **stability** — FSRS's estimate of how
//! many days until you'd have a 90% chance of recalling it. That number is
//! about the memory, not about the last quiz:
//!
//!   * **New** — never answered. No evidence either way.
//!   * **Learning** — answered, but stability under a fortnight. You can
//!     produce it today; you can't produce it in three weeks.
//!   * **Mastered** — stability of a fortnight or more, and out of the learning
//!     steps. This is the one that means something, and it's why the number
//!     moves slowly: it's supposed to.
//!
//! Percent-correct is still reported, as **recent accuracy**, because it
//! answers a different and real question — "am I getting these right at the
//! moment" — and because a deck where accuracy is high and strength is low is a
//! deck you're cramming rather than learning. Seeing both is the point.

use anyhow::Result;
use chrono::NaiveDate;
use rusqlite::Connection;
use serde::Serialize;

/// Days of stability at which a card counts as mastered.
///
/// A fortnight. Long enough that you did not simply see it this week, short
/// enough to be reachable inside a term — a threshold nobody reaches is a
/// progress bar that never moves, which is worse than no progress bar.
const MASTERED_DAYS: f64 = 14.0;

/// Lapses before a card is called a leech.
///
/// Anki's default, and it holds up: a card you have forgotten eight times is
/// not a card you are failing to revise. It is a card that is badly written, or
/// that you never understood in the first place, and more repetitions will not
/// fix either. Surfacing it is a prompt to rewrite it.
const LEECH_LAPSES: i64 = 8;

/// Counts that make up a deck's state, at any level of the hierarchy.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Strength {
    pub total: i64,
    pub new: i64,
    pub learning: i64,
    pub mastered: i64,
    /// Cards forgotten enough times to be worth rewriting rather than redoing.
    pub leeches: i64,
    pub suspended: i64,
    pub due_today: i64,
    /// Mastered as a share of everything. 0 when there are no cards — not 100,
    /// which is what a naive division gives and is the opposite of the truth.
    pub mastery: f64,
    /// The next day anything here comes up, so a deck can say "nothing until
    /// Thursday" instead of just "0 due".
    pub next_due_on: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubjectMastery {
    pub subject_id: i64,
    pub name: String,
    pub colour: String,
    #[serde(flatten)]
    pub strength: Strength,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TopicMastery {
    /// `None` is the real bucket for cards filed under no topic, which is most
    /// of them until you organise a deck. Hiding it would hide most of your
    /// cards.
    pub topic_id: Option<i64>,
    pub name: String,
    #[serde(flatten)]
    pub strength: Strength,
}

/// A day's worth of answering, for the heatmap.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DayAccuracy {
    pub date: String,
    pub reviews: i64,
    /// Share rated Hard or better. `Again` is the only rating that means you
    /// didn't know it — Hard means you did, slowly.
    pub accuracy: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeckStats {
    #[serde(flatten)]
    pub strength: Strength,
    /// Reviews in the window behind `recent`.
    pub recent_reviews: i64,
    /// Share of those rated Hard or better, or `None` when there aren't any —
    /// which is different from 0% and must not be drawn as a bar at zero.
    pub recent_accuracy: Option<f64>,
    pub recent: Vec<DayAccuracy>,
    /// Mean stability across answered cards, in days. The number that actually
    /// moves as you learn something.
    pub average_stability: Option<f64>,
}

/// The select list that classifies a deck, shared so the definitions can't
/// drift between the subject query, the topic query and the deck query.
///
/// The thresholds are formatted in rather than bound: they are constants in
/// this file, never user input, and binding them would mean every caller
/// carrying three parameters that are always the same. `?1` is today's date and
/// is the only thing that varies.
fn classify() -> String {
    format!(
        "COUNT(c.id)                                                    AS total,
         SUM(c.reps = 0)                                                AS new,
         SUM(c.reps > 0 AND (c.state IN ('learning','relearning')
                             OR COALESCE(c.stability, 0) < {MASTERED_DAYS})) AS learning,
         SUM(c.reps > 0 AND c.state = 'review'
                        AND COALESCE(c.stability, 0) >= {MASTERED_DAYS})     AS mastered,
         SUM(c.lapses >= {LEECH_LAPSES})                                AS leeches,
         SUM(c.suspended = 1)                                           AS suspended,
         SUM(c.suspended = 0 AND c.due_on IS NOT NULL AND c.due_on <= ?1) AS due_today,
         AVG(CASE WHEN c.reps > 0 THEN c.stability END)                 AS avg_stability,
         MIN(CASE WHEN c.suspended = 0 THEN c.due_on END)               AS next_due"
    )
}

fn strength_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<(Strength, Option<f64>)> {
    let total: i64 = row.get("total")?;
    let mastered: i64 = row.get::<_, Option<i64>>("mastered")?.unwrap_or(0);

    Ok((
        Strength {
            total,
            new: row.get::<_, Option<i64>>("new")?.unwrap_or(0),
            learning: row.get::<_, Option<i64>>("learning")?.unwrap_or(0),
            mastered,
            leeches: row.get::<_, Option<i64>>("leeches")?.unwrap_or(0),
            suspended: row.get::<_, Option<i64>>("suspended")?.unwrap_or(0),
            due_today: row.get::<_, Option<i64>>("due_today")?.unwrap_or(0),
            // An empty deck is 0% mastered, not 100%. Dividing by zero and
            // calling the result complete is how a progress bar lies.
            mastery: if total > 0 {
                mastered as f64 / total as f64
            } else {
                0.0
            },
            next_due_on: row.get("next_due")?,
        },
        row.get::<_, Option<f64>>("avg_stability")?,
    ))
}

/// Every subject, with how much of it you actually hold.
pub fn by_subject(conn: &Connection, today: NaiveDate) -> Result<Vec<SubjectMastery>> {
    let sql = format!(
        "SELECT s.id, s.name, s.colour, {}
           FROM subjects s LEFT JOIN cards c ON c.subject_id = s.id
          WHERE s.archived = 0
          GROUP BY s.id ORDER BY s.sort_order, s.id",
        classify()
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([iso(today)], |r| {
            let (strength, _) = strength_from(r)?;
            Ok(SubjectMastery {
                subject_id: r.get(0)?,
                name: r.get(1)?,
                colour: r.get(2)?,
                strength,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The topics inside one subject, plus the cards filed under none.
pub fn by_topic(conn: &Connection, subject_id: i64, today: NaiveDate) -> Result<Vec<TopicMastery>> {
    let sql = format!(
        "SELECT t.id, COALESCE(t.name, 'Unfiled'), {}
           FROM cards c LEFT JOIN topics t ON t.id = c.topic_id
          WHERE c.subject_id = ?2
          GROUP BY c.topic_id
          ORDER BY t.id IS NULL, t.sort_order, t.name",
        classify()
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![iso(today), subject_id], |r| {
            let (strength, _) = strength_from(r)?;
            Ok(TopicMastery {
                topic_id: r.get(0)?,
                name: r.get(1)?,
                strength,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Everything about one deck: a subject, optionally narrowed to a topic.
pub fn deck(
    conn: &Connection,
    subject_id: i64,
    topic_id: Option<i64>,
    today: NaiveDate,
    window_days: i64,
) -> Result<DeckStats> {
    let scope = match topic_id {
        Some(_) => "c.subject_id = ?2 AND c.topic_id = ?3",
        None => "c.subject_id = ?2",
    };
    let sql = format!("SELECT {} FROM cards c WHERE {scope}", classify());

    let (strength, average_stability) = match topic_id {
        Some(t) => conn.query_row(&sql, rusqlite::params![iso(today), subject_id, t], strength_from)?,
        None => conn.query_row(&sql, rusqlite::params![iso(today), subject_id], strength_from)?,
    };

    let recent = recent_accuracy(conn, subject_id, topic_id, today, window_days)?;
    let recent_reviews: i64 = recent.iter().map(|d| d.reviews).sum();
    let hits: f64 = recent.iter().map(|d| d.accuracy * d.reviews as f64).sum();

    Ok(DeckStats {
        strength,
        recent_reviews,
        // No reviews is not 0% — a bar drawn at zero would read as "you got
        // everything wrong" when the truth is "you haven't started".
        recent_accuracy: (recent_reviews > 0).then(|| hits / recent_reviews as f64),
        recent,
        average_stability,
    })
}

/// Per-day accuracy over the last `window_days`, oldest first.
///
/// Days with no reviews are included with zero counts, so the heatmap has one
/// cell per day and gaps read as gaps rather than compressing the timeline.
fn recent_accuracy(
    conn: &Connection,
    subject_id: i64,
    topic_id: Option<i64>,
    today: NaiveDate,
    window_days: i64,
) -> Result<Vec<DayAccuracy>> {
    let from = (today - chrono::Duration::days(window_days - 1))
        .format("%Y-%m-%d")
        .to_string();
    let to = today.format("%Y-%m-%d").to_string();

    // Joined back to `cards` so a topic filter is possible at all: the log
    // records the subject but not the topic, and a card can be refiled.
    let topic_clause = match topic_id {
        Some(_) => "AND c.topic_id = ?4",
        None => "",
    };
    let sql = format!(
        "SELECT r.local_date,
                COUNT(*),
                -- Again is the only rating meaning you didn't know it. Hard
                -- means you did, slowly, and scoring it as a miss would make
                -- honest self-rating look like failure.
                SUM(r.rating > 1)
           FROM review_log r
           JOIN cards c ON c.id = r.item_id AND r.item_type = 'card'
          WHERE r.local_date BETWEEN ?1 AND ?2 AND c.subject_id = ?3 {topic_clause}
          GROUP BY r.local_date"
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut counted: std::collections::HashMap<String, (i64, i64)> = Default::default();

    let read = |r: &rusqlite::Row<'_>| -> rusqlite::Result<(String, i64, i64)> {
        Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<i64>>(2)?.unwrap_or(0)))
    };

    let rows: Vec<(String, i64, i64)> = match topic_id {
        Some(t) => stmt
            .query_map(rusqlite::params![from, to, subject_id, t], read)?
            .collect::<Result<_, _>>()?,
        None => stmt
            .query_map(rusqlite::params![from, to, subject_id], read)?
            .collect::<Result<_, _>>()?,
    };
    for (date, n, hit) in rows {
        counted.insert(date, (n, hit));
    }

    Ok((0..window_days)
        .map(|i| {
            let date = (today - chrono::Duration::days(window_days - 1 - i))
                .format("%Y-%m-%d")
                .to_string();
            let (reviews, hits) = counted.get(&date).copied().unwrap_or((0, 0));
            DayAccuracy {
                accuracy: if reviews > 0 { hits as f64 / reviews as f64 } else { 0.0 },
                date,
                reviews,
            }
        })
        .collect())
}

fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

#[cfg(test)]
#[path = "mastery/tests.rs"]
mod tests;
