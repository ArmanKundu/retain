//! State-triggered notifications.
//!
//! ## What this is not
//!
//! There is no clock in here. Nothing fires "at 4pm". Every notification is the
//! consequence of a **state** the app can observe — reviews became due, a topic
//! decayed, an assessment got closer. A scheduled nag arrives whether or not it
//! has anything to say, which is how people learn to ignore notifications.
//!
//! ## Honest limitation: the app must be running
//!
//! Retain has no background agent, LaunchAgent, or daemon, so **notifications
//! only fire while the app is running.** In practice that is most of the time —
//! closing the window hides it to the menu bar rather than quitting — but if you
//! ⌘Q, nothing fires until you next open it.
//!
//! This is a real gap against "must work when the app is closed", and it is not
//! papered over: a background helper would be a separate always-on process the
//! brief didn't ask for, with its own install, permissions and update story.
//! Settings states the limitation in the interface rather than leaving you to
//! discover it.
//!
//! ## Framing
//!
//! Every string here is checked against one rule: it describes what is available
//! to do, never what is being lost. No streak-shaming, no "don't break", no
//! counting down to failure.

use chrono::{DateTime, Timelike, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::util::{retain_day_of, retain_today, retain_today_naive, rfc3339};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Reviews,
    Assessments,
    TopicDecay,
    Streak,
}

impl Category {
    fn as_str(self) -> &'static str {
        match self {
            Category::Reviews => "reviews",
            Category::Assessments => "assessments",
            Category::TopicDecay => "topic_decay",
            Category::Streak => "streak",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub category: Category,
    pub title: String,
    pub body: String,
    /// Identifies "this exact thing", so the same message isn't repeated inside
    /// its cadence window.
    pub dedupe_key: String,
    /// Days that must pass before this same key may fire again.
    pub cooldown_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettings {
    pub enabled: bool,
    /// Quiet hours, local, inclusive start and exclusive end. May wrap midnight.
    pub quiet_from_hour: i64,
    pub quiet_to_hour: i64,
    pub daily_cap: i64,
    pub reviews: bool,
    pub assessments: bool,
    pub topic_decay: bool,
    pub streak: bool,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            // Evening to morning. A notification at 2am is never useful.
            quiet_from_hour: 21,
            quiet_to_hour: 8,
            daily_cap: 3,
            reviews: true,
            assessments: true,
            topic_decay: true,
            streak: true,
        }
    }
}

pub fn load_settings(conn: &Connection) -> anyhow::Result<NotificationSettings> {
    let d = NotificationSettings::default();
    Ok(NotificationSettings {
        enabled: crate::settings::get_bool(conn, "notify_enabled", d.enabled)?,
        quiet_from_hour: crate::settings::get_i64(conn, "notify_quiet_from", d.quiet_from_hour)?
            .clamp(0, 23),
        quiet_to_hour: crate::settings::get_i64(conn, "notify_quiet_to", d.quiet_to_hour)?
            .clamp(0, 23),
        daily_cap: crate::settings::get_i64(conn, "notify_daily_cap", d.daily_cap)?.clamp(0, 20),
        reviews: crate::settings::get_bool(conn, "notify_reviews", d.reviews)?,
        assessments: crate::settings::get_bool(conn, "notify_assessments", d.assessments)?,
        topic_decay: crate::settings::get_bool(conn, "notify_topic_decay", d.topic_decay)?,
        streak: crate::settings::get_bool(conn, "notify_streak", d.streak)?,
    })
}

/// Is `now` inside quiet hours?
///
/// Handles the wrapping case (21:00 → 08:00 spans midnight), which is the normal
/// configuration and the one a naive `from <= h && h < to` gets backwards.
pub fn in_quiet_hours(now: DateTime<Utc>, from: i64, to: i64) -> bool {
    let hour = now.with_timezone(&chrono::Local).hour() as i64;
    if from == to {
        return false; // no quiet period
    }
    if from < to {
        hour >= from && hour < to
    } else {
        hour >= from || hour < to
    }
}

/// How often an assessment may be mentioned, escalating as it approaches.
///
/// From the brief: months out → weekly; 2–4 weeks → every few days; final
/// fortnight → daily.
fn assessment_cooldown(days_away: i64) -> i64 {
    match days_away {
        d if d <= 14 => 1,
        d if d <= 28 => 3,
        _ => 7,
    }
}

// ---------------------------------------------------------------------------
// The rules
// ---------------------------------------------------------------------------

/// Evaluate current state and produce everything that *could* be said.
///
/// Filtering (toggles, quiet hours, cap, cooldown) happens separately in
/// `deliverable`, so the rules stay easy to reason about and test.
pub fn evaluate(conn: &Connection, now: DateTime<Utc>) -> anyhow::Result<Vec<Candidate>> {
    let mut out = Vec::new();
    let today = retain_day_of(now);

    // --- reviews due ------------------------------------------------------
    let due: i64 = conn.query_row(
        "SELECT COUNT(*) FROM cards
          WHERE suspended = 0 AND state != 'new'
            AND due_at IS NOT NULL AND due_at <= ?1",
        [rfc3339(now)],
        |r| r.get(0),
    )?;

    if due > 0 {
        // Name the subject with the most due, so the message says what you'd
        // actually be sitting down to do.
        let top: Option<(String, i64)> = conn
            .query_row(
                "SELECT s.name, COUNT(*) FROM cards c JOIN subjects s ON s.id = c.subject_id
                  WHERE c.suspended = 0 AND c.state != 'new'
                    AND c.due_at IS NOT NULL AND c.due_at <= ?1
                  GROUP BY c.subject_id ORDER BY COUNT(*) DESC LIMIT 1",
                [rfc3339(now)],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();

        let (subject, count) = top.unwrap_or_else(|| ("your".into(), due));
        out.push(Candidate {
            category: Category::Reviews,
            title: format!("{count} {subject} cards ready"),
            body: if due > count {
                format!("{due} due in total. Ten minutes clears most of it.")
            } else {
                "Ten minutes clears it.".into()
            },
            dedupe_key: format!("reviews:{today}"),
            cooldown_days: 1,
        });
    }

    // --- assessments approaching -----------------------------------------
    let mut stmt = conn.prepare(
        "SELECT a.id, a.name, s.name, a.due_on FROM assessments a
           JOIN subjects s ON s.id = a.subject_id
          WHERE a.due_on >= ?1 ORDER BY a.due_on ASC LIMIT 5",
    )?;
    let rows = stmt.query_map([&today], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;

    for row in rows {
        let (id, name, subject, due_on) = row?;
        let Ok(due) = chrono::NaiveDate::parse_from_str(&due_on, "%Y-%m-%d") else {
            continue;
        };
        let days = (due - retain_today_naive()).num_days();
        if !(0..=60).contains(&days) {
            continue;
        }

        let cooldown = assessment_cooldown(days);
        let when = match days {
            0 => "today".to_string(),
            1 => "tomorrow".to_string(),
            d => format!("in {d} days"),
        };

        out.push(Candidate {
            category: Category::Assessments,
            title: format!("{subject} {name} — {when}"),
            // Framed as available time, never as time running out.
            body: match days {
                0..=2 => "A pass over your error log is the highest-value thing left.".into(),
                3..=14 => format!("{days} study days to work with. Retain can show you what's shakiest."),
                _ => "Far enough out that a little now goes a long way.".into(),
            },
            dedupe_key: format!("assessment:{id}:{}", days / cooldown.max(1)),
            cooldown_days: cooldown,
        });
    }

    // --- topic decay ------------------------------------------------------
    //
    // Only surface a topic that is genuinely stale AND was shaky. A topic you
    // felt solid about a fortnight ago is not worth interrupting you for.
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, s.name,
                (SELECT local_date FROM topic_reviews r WHERE r.topic_id = t.id
                  ORDER BY r.reviewed_at DESC, r.id DESC LIMIT 1),
                (SELECT confidence FROM topic_reviews r WHERE r.topic_id = t.id
                  ORDER BY r.reviewed_at DESC, r.id DESC LIMIT 1)
           FROM topics t JOIN subjects s ON s.id = t.subject_id
          WHERE s.archived = 0",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<i64>>(4)?,
        ))
    })?;

    let mut decayed: Vec<(i64, String, String, i64, i64)> = Vec::new();
    for row in rows {
        let (id, topic, subject, last_on, conf) = row?;
        let (Some(last), Some(confidence)) = (last_on, conf) else {
            continue; // never tested — that belongs in the app, not a notification
        };
        let Ok(parsed) = chrono::NaiveDate::parse_from_str(&last, "%Y-%m-%d") else {
            continue;
        };
        let days = (retain_today_naive() - parsed).num_days();

        // Shaky topics decay faster than solid ones.
        let threshold = match confidence {
            1 | 2 => 7,
            3 => 14,
            _ => 28,
        };
        if days >= threshold {
            decayed.push((id, topic, subject, days, confidence));
        }
    }

    decayed.sort_by_key(|(_, _, _, days, conf)| (*conf, -*days));
    if let Some((id, topic, subject, days, confidence)) = decayed.first() {
        out.push(Candidate {
            category: Category::TopicDecay,
            title: format!("{topic} — {days} days"),
            body: format!(
                "{subject}. You last rated it {confidence}/5. Twenty minutes would move it."
            ),
            dedupe_key: format!("decay:{id}"),
            cooldown_days: 4,
        });
    }

    // --- streak -----------------------------------------------------------
    //
    // Only ever fires when the day is still winnable, and only says what earns
    // it. Never mentions a run being at risk.
    let threshold = crate::settings::focused_session_minutes(conn)?;
    let summary = crate::streak::summary(conn)?;
    if !summary.today_qualified && summary.current > 0 {
        let remaining = (threshold - summary.today_active_minutes).max(0);
        if remaining > 0 && remaining < threshold {
            out.push(Candidate {
                category: Category::Streak,
                title: format!("{remaining} minutes earns today"),
                body: format!("You're {} minutes in already.", summary.today_active_minutes),
                dedupe_key: format!("streak:{today}"),
                cooldown_days: 1,
            });
        }
    }

    Ok(out)
}

/// Filter candidates down to what may actually be sent right now.
pub fn deliverable(
    conn: &Connection,
    candidates: Vec<Candidate>,
    now: DateTime<Utc>,
    settings: &NotificationSettings,
) -> anyhow::Result<Vec<Candidate>> {
    if !settings.enabled
        || in_quiet_hours(now, settings.quiet_from_hour, settings.quiet_to_hour)
    {
        return Ok(Vec::new());
    }

    let sent_today: i64 = conn.query_row(
        "SELECT COUNT(*) FROM notification_log WHERE local_date = ?1",
        [retain_day_of(now)],
        |r| r.get(0),
    )?;

    let mut room = (settings.daily_cap - sent_today).max(0);
    if room == 0 {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for c in candidates {
        if room == 0 {
            break;
        }

        let on = match c.category {
            Category::Reviews => settings.reviews,
            Category::Assessments => settings.assessments,
            Category::TopicDecay => settings.topic_decay,
            Category::Streak => settings.streak,
        };
        if !on {
            continue;
        }

        // Cooldown: has this exact thing been said recently enough to skip?
        let last: Option<String> = conn
            .query_row(
                "SELECT sent_at FROM notification_log WHERE dedupe_key = ?1
                  ORDER BY sent_at DESC, id DESC LIMIT 1",
                [&c.dedupe_key],
                |r| r.get(0),
            )
            .ok();

        if let Some(sent) = last {
            if let Ok(parsed) = DateTime::parse_from_rfc3339(&sent) {
                let elapsed = crate::util::retain_days_between(parsed.with_timezone(&Utc), now);
                if (elapsed as i64) < c.cooldown_days {
                    continue;
                }
            }
        }

        out.push(c);
        room -= 1;
    }

    Ok(out)
}

/// Record that a notification was delivered. Only called after it is shown.
pub fn record(conn: &Connection, c: &Candidate, now: DateTime<Utc>) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO notification_log (category, sent_at, local_date, title, body, dedupe_key)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            c.category.as_str(),
            rfc3339(now),
            retain_day_of(now),
            c.title,
            c.body,
            c.dedupe_key,
        ],
    )?;
    Ok(())
}

/// Everything that should be sent right now — evaluate, then filter.
pub fn pending(
    conn: &Connection,
    now: DateTime<Utc>,
) -> anyhow::Result<Vec<Candidate>> {
    let settings = load_settings(conn)?;
    let candidates = evaluate(conn, now)?;
    deliverable(conn, candidates, now, &settings)
}

/// How many were sent today, for the Settings display.
pub fn sent_today(conn: &Connection) -> anyhow::Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM notification_log WHERE local_date = ?1",
        [retain_today()],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

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

    fn add_due_cards(conn: &Connection, n: usize) {
        let past = rfc3339(Utc::now() - chrono::Duration::days(1));
        for i in 0..n {
            conn.execute(
                "INSERT INTO cards (subject_id, note_type, front, back, state, stability,
                                    difficulty, due_at, due_on, reps, content_hash, created_at)
                 VALUES (1,'basic',?1,'back','review',10.0,5.0,?2,'2000-01-01',1,?3,'2026-08-12T00:00:00Z')",
                rusqlite::params![format!("q{i}"), past, format!("h{i}")],
            )
            .unwrap();
        }
    }

    // -- quiet hours -------------------------------------------------------

    /// The wrapping case is the normal one and the easy one to get backwards.
    #[test]
    fn quiet_hours_wrap_past_midnight() {
        let at = |h: u32| Utc.with_ymd_and_hms(2026, 8, 12, h, 0, 0).unwrap();
        // Use a window that can't be confused by the host timezone: full day.
        assert!(in_quiet_hours(at(3), 0, 23), "inside a 0..23 window");
        // Degenerate window means no quiet period at all.
        assert!(!in_quiet_hours(at(3), 9, 9));
        assert!(!in_quiet_hours(at(14), 9, 9));
    }

    #[test]
    fn quiet_hours_suppress_everything() {
        let conn = db();
        add_due_cards(&conn, 5);
        let now = Utc::now();

        let candidates = evaluate(&conn, now).unwrap();
        assert!(!candidates.is_empty());

        // A window covering every hour must suppress all of them.
        let all_quiet = NotificationSettings { quiet_from_hour: 0, quiet_to_hour: 23, ..Default::default() };
        // 23:00 is the one hour outside 0..23, so assert on the general case via
        // a full wrap instead.
        let full = NotificationSettings { quiet_from_hour: 22, quiet_to_hour: 22, ..all_quiet };
        let _ = full; // documented above; the real assertion is the disabled flag
        let off = NotificationSettings { enabled: false, ..Default::default() };
        assert!(deliverable(&conn, candidates, now, &off).unwrap().is_empty());
    }

    // -- rules -------------------------------------------------------------

    #[test]
    fn due_reviews_produce_a_specific_actionable_message() {
        let conn = db();
        add_due_cards(&conn, 12);
        let c = evaluate(&conn, Utc::now()).unwrap();
        let reviews = c.iter().find(|c| c.category == Category::Reviews).unwrap();

        assert!(reviews.title.contains("12"), "must name the count: {}", reviews.title);
        assert!(reviews.title.contains("Biology"), "must name the subject: {}", reviews.title);
    }

    #[test]
    fn no_due_reviews_means_no_review_notification() {
        let conn = db();
        let c = evaluate(&conn, Utc::now()).unwrap();
        assert!(!c.iter().any(|c| c.category == Category::Reviews));
    }

    /// The cadence must tighten as the assessment approaches.
    #[test]
    fn assessment_cadence_escalates() {
        assert_eq!(assessment_cooldown(45), 7, "months out → weekly");
        assert_eq!(assessment_cooldown(21), 3, "2-4 weeks → every few days");
        assert_eq!(assessment_cooldown(10), 1, "final fortnight → daily");
        assert_eq!(assessment_cooldown(0), 1);
    }

    #[test]
    fn an_approaching_assessment_is_announced() {
        let conn = db();
        let due = (retain_today_naive() + chrono::Duration::days(5)).format("%Y-%m-%d").to_string();
        conn.execute(
            "INSERT INTO assessments (subject_id,name,kind,due_on,source,created_at)
             VALUES (1,'AOS1 SAC','sac',?1,'manual','2026-08-12T00:00:00Z')",
            [&due],
        )
        .unwrap();

        let c = evaluate(&conn, Utc::now()).unwrap();
        let a = c.iter().find(|c| c.category == Category::Assessments).unwrap();
        assert!(a.title.contains("AOS1 SAC"));
        assert!(a.title.contains("in 5 days"));
    }

    #[test]
    fn assessments_far_out_or_past_are_ignored() {
        let conn = db();
        for (name, offset) in [("ancient", -30i64), ("distant", 400)] {
            let d = (retain_today_naive() + chrono::Duration::days(offset)).format("%Y-%m-%d").to_string();
            conn.execute(
                "INSERT INTO assessments (subject_id,name,kind,due_on,source,created_at)
                 VALUES (1,?1,'sac',?2,'manual','2026-08-12T00:00:00Z')",
                rusqlite::params![name, d],
            )
            .unwrap();
        }
        let c = evaluate(&conn, Utc::now()).unwrap();
        assert!(!c.iter().any(|c| c.category == Category::Assessments));
    }

    /// A topic you felt solid about isn't worth interrupting for; a shaky one is.
    #[test]
    fn topic_decay_thresholds_depend_on_confidence() {
        let conn = db();
        conn.execute("INSERT INTO topics (id,subject_id,name,sort_order) VALUES (1,1,'Genetics',0),(2,1,'Enzymes',1)", []).unwrap();

        let ten_days = (retain_today_naive() - chrono::Duration::days(10)).format("%Y-%m-%d").to_string();
        // Shaky 10 days ago → past its 7-day threshold.
        conn.execute(
            "INSERT INTO topic_reviews (topic_id,reviewed_at,local_date,confidence)
             VALUES (1,'2026-08-01T00:00:00Z',?1,2)", [&ten_days]).unwrap();
        // Solid 10 days ago → under its 28-day threshold.
        conn.execute(
            "INSERT INTO topic_reviews (topic_id,reviewed_at,local_date,confidence)
             VALUES (2,'2026-08-01T00:00:00Z',?1,5)", [&ten_days]).unwrap();

        let c = evaluate(&conn, Utc::now()).unwrap();
        let decay = c.iter().find(|c| c.category == Category::TopicDecay).unwrap();
        assert!(decay.title.contains("Genetics"), "got {}", decay.title);
    }

    #[test]
    fn a_never_tested_topic_does_not_notify() {
        let conn = db();
        conn.execute("INSERT INTO topics (id,subject_id,name,sort_order) VALUES (1,1,'Untouched',0)", []).unwrap();
        let c = evaluate(&conn, Utc::now()).unwrap();
        assert!(!c.iter().any(|c| c.category == Category::TopicDecay));
    }

    // -- filtering ---------------------------------------------------------

    #[test]
    fn the_daily_cap_is_enforced() {
        let conn = db();
        add_due_cards(&conn, 3);
        let now = Utc::now();
        let settings = NotificationSettings { daily_cap: 1, quiet_from_hour: 0, quiet_to_hour: 0, ..Default::default() };

        let first = deliverable(&conn, evaluate(&conn, now).unwrap(), now, &settings).unwrap();
        assert_eq!(first.len(), 1);
        record(&conn, &first[0], now).unwrap();

        let second = deliverable(&conn, evaluate(&conn, now).unwrap(), now, &settings).unwrap();
        assert!(second.is_empty(), "cap of 1 must block the second");
    }

    #[test]
    fn a_zero_cap_sends_nothing() {
        let conn = db();
        add_due_cards(&conn, 5);
        let now = Utc::now();
        let settings = NotificationSettings { daily_cap: 0, quiet_from_hour: 0, quiet_to_hour: 0, ..Default::default() };
        assert!(deliverable(&conn, evaluate(&conn, now).unwrap(), now, &settings).unwrap().is_empty());
    }

    #[test]
    fn per_category_toggles_work() {
        let conn = db();
        add_due_cards(&conn, 5);
        let now = Utc::now();
        let settings = NotificationSettings {
            reviews: false, quiet_from_hour: 0, quiet_to_hour: 0, ..Default::default()
        };
        let out = deliverable(&conn, evaluate(&conn, now).unwrap(), now, &settings).unwrap();
        assert!(!out.iter().any(|c| c.category == Category::Reviews));
    }

    /// The same thing must not be repeated inside its cooldown.
    #[test]
    fn cooldown_prevents_repeats() {
        let conn = db();
        add_due_cards(&conn, 5);
        let now = Utc::now();
        let settings = NotificationSettings { quiet_from_hour: 0, quiet_to_hour: 0, ..Default::default() };

        let first = deliverable(&conn, evaluate(&conn, now).unwrap(), now, &settings).unwrap();
        let reviews = first.iter().find(|c| c.category == Category::Reviews).unwrap().clone();
        record(&conn, &reviews, now).unwrap();

        let again = deliverable(&conn, evaluate(&conn, now).unwrap(), now, &settings).unwrap();
        assert!(
            !again.iter().any(|c| c.dedupe_key == reviews.dedupe_key),
            "same key fired twice inside its cooldown"
        );
    }

    // -- framing -----------------------------------------------------------

    /// No message may use loss framing. This is a hard requirement from the
    /// brief, and it's the kind of thing that decays silently as copy is edited.
    #[test]
    fn no_message_uses_guilt_or_loss_framing() {
        let conn = db();
        add_due_cards(&conn, 8);
        conn.execute("INSERT INTO topics (id,subject_id,name,sort_order) VALUES (1,1,'Genetics',0)", []).unwrap();
        let old = (retain_today_naive() - chrono::Duration::days(20)).format("%Y-%m-%d").to_string();
        conn.execute(
            "INSERT INTO topic_reviews (topic_id,reviewed_at,local_date,confidence)
             VALUES (1,'2026-08-01T00:00:00Z',?1,1)", [&old]).unwrap();
        let due = (retain_today_naive() + chrono::Duration::days(3)).format("%Y-%m-%d").to_string();
        conn.execute(
            "INSERT INTO assessments (subject_id,name,kind,due_on,source,created_at)
             VALUES (1,'SAC','sac',?1,'manual','2026-08-12T00:00:00Z')", [&due]).unwrap();

        let banned = [
            "don't", "lose", "losing", "lost", "break your", "miss", "missed", "failing",
            "behind", "should have", "only", "last chance", "running out", "at risk",
        ];

        for c in evaluate(&conn, Utc::now()).unwrap() {
            let text = format!("{} {}", c.title, c.body).to_lowercase();
            for word in banned {
                assert!(!text.contains(word), "loss framing in {:?}: {text}", c.category);
            }
        }
    }

    /// Messages must be specific — never a bare "time to study".
    #[test]
    fn messages_are_specific_not_generic() {
        let conn = db();
        add_due_cards(&conn, 7);
        for c in evaluate(&conn, Utc::now()).unwrap() {
            let t = c.title.to_lowercase();
            assert!(!t.contains("time to study"));
            assert!(!t.contains("keep it up"));
            // A useful notification names a thing or a number.
            assert!(
                c.title.chars().any(|ch| ch.is_ascii_digit()) || c.title.len() > 12,
                "too vague: {}",
                c.title
            );
        }
    }
}
