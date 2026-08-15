//! Letting the assistant act, without letting it act unsupervised.
//!
//! Until now the assistant could only talk. Ask it to put revision on Thursday
//! and it wrote you a paragraph about putting revision on Thursday, and you did
//! the typing. This module is the other half: a fixed set of things it can do to
//! your data, proposed as buttons you press.
//!
//! # The safety design, and why it is shaped this way
//!
//! Retain ingests arbitrary PDFs into the library, and retrieved chunks of those
//! PDFs go into the prompt. A study design downloaded from anywhere is untrusted
//! text that reaches the model. So the threat isn't "the model has a bad day" —
//! it's that **anything in your library can try to issue instructions**. Three
//! properties follow from that, and none of them are negotiable:
//!
//!   1. **The set of actions is closed.** There is no "run this command", no
//!      shell, no AppleScript, no arbitrary HTTP. A proposal that doesn't parse
//!      into one of the variants below is discarded, not guessed at. This is why
//!      the module is an enum rather than a generic dispatcher.
//!   2. **Every action is a proposal until you press the button.** Nothing here
//!      writes as a side effect of the model replying.
//!   3. **The label on the button is generated from the parsed action**, in
//!      `summary()`, never taken from text the model wrote. Otherwise a
//!      proposal could describe itself as one thing and do another, and the
//!      confirmation step would be theatre.
//!
//! Subjects are resolved by name against your own subject list, and an action
//! naming a subject you don't have is rejected outright rather than silently
//! filed under nothing.

use anyhow::{anyhow, Result};
use chrono::{NaiveDate, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// The complete set of things the assistant may do. Adding a variant is a
/// deliberate act; there is no escape hatch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum Action {
    /// Put something on the plan.
    #[serde(rename = "plan.add")]
    PlanAdd {
        title: String,
        #[serde(default)]
        subject: Option<String>,
        /// `YYYY-MM-DD`.
        on: String,
        #[serde(default = "default_minutes")]
        minutes: i64,
        #[serde(default)]
        due: Option<String>,
    },

    /// Record a SAC or exam.
    #[serde(rename = "assessment.add")]
    AssessmentAdd {
        name: String,
        subject: String,
        /// `YYYY-MM-DD`.
        due: String,
        #[serde(default)]
        kind: Option<String>,
    },

    /// Claim time in the week, so the planner stops suggesting you study then.
    #[serde(rename = "block.add")]
    BlockAdd {
        title: String,
        /// 0 = Monday.
        weekday: i64,
        /// `HH:MM`, 24-hour.
        start: String,
        end: String,
        #[serde(default)]
        kind: Option<String>,
    },

    /// Make a flashcard.
    #[serde(rename = "card.add")]
    CardAdd {
        subject: String,
        front: String,
        back: String,
    },

    /// Open a link. The one action that reaches outside Retain, and the reason
    /// it is restricted to `https://` is that everything else — `file://`,
    /// `javascript:`, a custom scheme registered by some other app — is a way to
    /// make the OS do something on behalf of text that came out of a PDF.
    #[serde(rename = "open.url")]
    OpenUrl { url: String },
}

fn default_minutes() -> i64 {
    30
}

/// A parsed, validated action plus the sentence shown on its button.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub action: Action,
    /// Derived from `action`, never from model prose. See the module docs.
    pub summary: String,
    /// True for everything that leaves the app. Shown more prominently.
    pub external: bool,
}

/// The instructions handed to the model. Kept next to the enum so the two can't
/// drift — a documented action that doesn't exist produces silent no-ops.
pub const TOOL_PROMPT: &str = r#"
You can propose actions for the student to confirm. You never perform them
yourself; each one becomes a button they press.

To propose actions, end your reply with a fenced block tagged `retain-actions`
containing a JSON array. Propose only what was actually asked for — an unwanted
button is worse than no button. Most replies should have no block at all.

Available actions:
  {"action":"plan.add","title":"Redox worksheet","subject":"Chemistry","on":"2026-08-18","minutes":45}
  {"action":"assessment.add","name":"Unit 3 SAC","subject":"Biology","due":"2026-09-02","kind":"sac"}
  {"action":"block.add","title":"Shift","weekday":5,"start":"09:00","end":"17:00","kind":"work"}
  {"action":"card.add","subject":"Biology","front":"...","back":"..."}
  {"action":"open.url","url":"https://..."}

Rules:
  - Dates are YYYY-MM-DD. weekday is 0=Monday.
  - `subject` must exactly match one of the student's subjects.
  - block kinds: class, tuition, work, commute, exercise, family, rest, other.
  - assessment kinds: sac, exam, other.
  - Only https:// links.
"#;

/// Pull the actions out of a reply, and give back the reply without them.
///
/// Returns `(prose, proposals)`. Anything that fails to parse or validate is
/// dropped rather than repaired: a half-understood instruction to write to your
/// data is not something to guess at.
pub fn extract(conn: &Connection, reply: &str) -> (String, Vec<Proposal>) {
    let Some((prose, json)) = split_block(reply) else {
        return (reply.trim().to_string(), Vec::new());
    };

    // Deserialised one element at a time, not as `Vec<Action>` in one go.
    // Serde fails a whole sequence on the first element it can't read, so a
    // single unrecognised entry would silently discard every good action
    // alongside it — and an unrecognised entry is exactly what you get whenever
    // the model invents an action name.
    let raw: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap_or_default();
    let proposals = raw
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Action>(v).ok())
        .filter_map(|a| validate(conn, a).ok())
        .collect();

    (prose, proposals)
}

/// Find the ```retain-actions fence and split it off the prose.
fn split_block(reply: &str) -> Option<(String, String)> {
    let start = reply.find("```retain-actions")?;
    let after = &reply[start + "```retain-actions".len()..];
    let end = after.find("```")?;

    let prose = reply[..start].trim().to_string();
    Some((prose, after[..end].trim().to_string()))
}

fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| anyhow!("\"{s}\" isn't a date"))
}

/// `HH:MM` to minutes past midnight.
fn parse_clock(s: &str) -> Result<i64> {
    let (h, m) = s.split_once(':').ok_or_else(|| anyhow!("\"{s}\" isn't a time"))?;
    let h: i64 = h.trim().parse()?;
    let m: i64 = m.trim().parse()?;
    if !(0..24).contains(&h) || !(0..60).contains(&m) {
        return Err(anyhow!("\"{s}\" isn't a time"));
    }
    Ok(h * 60 + m)
}

/// Resolve a subject by name, case-insensitively, against the student's own
/// list. A name that isn't theirs is an error, never a new subject — the
/// assistant does not get to invent subjects.
fn subject_id(conn: &Connection, name: &str) -> Result<i64> {
    conn.query_row(
        "SELECT id FROM subjects WHERE lower(name) = lower(?1) AND archived = 0",
        [name.trim()],
        |r| r.get(0),
    )
    .map_err(|_| anyhow!("no subject called \"{name}\""))
}

/// Check an action against reality and build its label.
pub fn validate(conn: &Connection, action: Action) -> Result<Proposal> {
    let (summary, external) = match &action {
        Action::PlanAdd { title, subject, on, minutes, due } => {
            let date = parse_date(on)?;
            if let Some(s) = subject {
                subject_id(conn, s)?;
            }
            if let Some(d) = due {
                if parse_date(d)? < date {
                    return Err(anyhow!("due before the day it's planned for"));
                }
            }
            let who = subject.as_deref().map(|s| format!("{s}: ")).unwrap_or_default();
            (
                format!(
                    "Add to your plan for {} — {who}{title} ({minutes} min)",
                    friendly(date)
                ),
                false,
            )
        }

        Action::AssessmentAdd { name, subject, due, kind } => {
            let date = parse_date(due)?;
            subject_id(conn, subject)?;
            let kind = normalise_kind(kind.as_deref(), &["sac", "exam", "other"], "other");
            (
                format!("Record a {kind}: {subject} — {name}, {}", friendly(date)),
                false,
            )
        }

        Action::BlockAdd { title, weekday, start, end, kind } => {
            if !(0..=6).contains(weekday) {
                return Err(anyhow!("weekday out of range"));
            }
            let (s, e) = (parse_clock(start)?, parse_clock(end)?);
            if e <= s {
                return Err(anyhow!("a block has to end after it starts"));
            }
            let kind = normalise_kind(
                kind.as_deref(),
                &["class", "tuition", "work", "commute", "exercise", "family", "rest", "other"],
                "other",
            );
            (
                format!(
                    "Block out {} {start}–{end} every {} as {kind}",
                    title, WEEKDAYS[*weekday as usize]
                ),
                false,
            )
        }

        Action::CardAdd { subject, front, back } => {
            subject_id(conn, subject)?;
            if front.trim().is_empty() || back.trim().is_empty() {
                return Err(anyhow!("a card needs both sides"));
            }
            (format!("Make a {subject} card: {}", truncate(front, 60)), false)
        }

        Action::OpenUrl { url } => {
            // Not a formatting preference. `file://` reads your disk,
            // `javascript:` runs code, and a custom scheme hands control to
            // whichever app claimed it — all reachable from text that arrived
            // inside a PDF.
            if !url.starts_with("https://") {
                return Err(anyhow!("only https links"));
            }
            (format!("Open {}", truncate(url, 70)), true)
        }
    };

    Ok(Proposal { action, summary, external })
}

const WEEKDAYS: [&str; 7] = [
    "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
];

fn normalise_kind(given: Option<&str>, allowed: &[&str], fallback: &str) -> String {
    given
        .map(str::trim)
        .map(str::to_lowercase)
        .filter(|k| allowed.contains(&k.as_str()))
        .unwrap_or_else(|| fallback.to_string())
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max - 1).collect::<String>())
}

fn friendly(date: NaiveDate) -> String {
    date.format("%-d %b").to_string()
}

/// What happened, in a sentence, for the confirmation row.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub ok: bool,
    pub message: String,
    /// Set for `open.url` so the command layer knows to hand it to the opener.
    pub open: Option<String>,
}

/// Perform one action.
///
/// Re-validates first. The frontend sends back a proposal it was given, but a
/// command handler that trusts its input is one bug away from being the hole
/// this whole module exists to close.
pub fn apply(conn: &Connection, action: Action) -> Result<Applied> {
    let proposal = validate(conn, action)?;

    match proposal.action {
        Action::PlanAdd { title, subject, on, minutes, due } => {
            let subject_id = subject.as_deref().map(|s| subject_id(conn, s)).transpose()?;
            crate::plan::create(
                conn,
                &crate::plan::NewPlanItem {
                    subject_id,
                    title: title.clone(),
                    detail: None,
                    planned_on: on,
                    est_minutes: minutes,
                    due_on: due,
                    source: Some("ai".into()),
                },
                Utc::now(),
            )?;
            Ok(Applied { ok: true, message: format!("Added “{title}” to your plan."), open: None })
        }

        Action::AssessmentAdd { name, subject, due, kind } => {
            let sid = subject_id(conn, &subject)?;
            let kind = match normalise_kind(kind.as_deref(), &["sac", "exam", "other"], "other")
                .as_str()
            {
                "sac" => crate::assessments::AssessmentKind::Sac,
                "exam" => crate::assessments::AssessmentKind::Exam,
                _ => crate::assessments::AssessmentKind::Other,
            };
            crate::assessments::create(
                conn,
                crate::assessments::AssessmentInput {
                    subject_id: sid,
                    name: name.clone(),
                    kind,
                    due_on: due,
                    topic_ids: None,
                },
            )?;
            Ok(Applied { ok: true, message: format!("Recorded “{name}”."), open: None })
        }

        Action::BlockAdd { title, weekday, start, end, kind } => {
            crate::blocks::create(
                conn,
                &crate::blocks::NewBlock {
                    title: title.clone(),
                    kind: normalise_kind(
                        kind.as_deref(),
                        &[
                            "class", "tuition", "work", "commute", "exercise", "family", "rest",
                            "other",
                        ],
                        "other",
                    ),
                    weekday: Some(weekday),
                    on_date: None,
                    start_min: parse_clock(&start)?,
                    end_min: parse_clock(&end)?,
                    // Assistant-created blocks are commitments, never study
                    // time. Marking one available would quietly tell the planner
                    // your shift is free study.
                    available: false,
                    subject_id: None,
                    note: None,
                    link: None,
                },
                Utc::now(),
            )?;
            Ok(Applied { ok: true, message: format!("Blocked out “{title}”."), open: None })
        }

        Action::CardAdd { subject, front, back } => {
            let sid = subject_id(conn, &subject)?;
            crate::cards::import(
                conn,
                sid,
                None,
                &[crate::anki_import::ParsedCard {
                    note_type: crate::anki_import::NoteType::Basic,
                    front,
                    back,
                    extra: None,
                    cloze_index: None,
                    tags: vec!["assistant".into()],
                }],
            )?;
            Ok(Applied { ok: true, message: "Card added.".into(), open: None })
        }

        Action::OpenUrl { url } => Ok(Applied {
            ok: true,
            message: format!("Opened {}", truncate(&url, 60)),
            open: Some(url),
        }),
    }
}

#[cfg(test)]
#[path = "tools/tests.rs"]
mod tests;
