//! The study assistant.
//!
//! ## The one design decision that matters
//!
//! It defaults to **strict grounding**: it answers from material you supplied
//! and, when your material doesn't cover something, says so rather than filling
//! the gap from the model's own memory. You can turn that off per conversation,
//! and when you do, anything outside your material is labelled as such.
//!
//! That default is deliberate and it is the whole point. A model asked about
//! VCE Biology will answer confidently either way; the difference is whether
//! you can check it. An answer traceable to your study design is worth having.
//! An equally confident answer assembled from a half-remembered syllabus is
//! actively harmful, because you'd revise from it and never find out.
//!
//! ## What it can and can't do
//!
//! It sees your own material (retrieved per question), any files attached to
//! the message, and a compact summary of your Retain data — what's due, what's
//! coming up, this week's hours. So "what should I do tonight?" is answerable.
//!
//! It cannot *act*. It won't create a card or move an assessment, because a
//! model that silently writes to your deck is a model whose mistakes you
//! inherit. It points you at the screen instead.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::resources::Excerpt;
use crate::util;

/// Excerpts retrieved per question. Enough to cover a topic from several
/// angles, few enough to leave room for the conversation itself.
pub const RETRIEVE: i64 = 6;

/// How many previous turns are replayed. A study conversation rarely depends on
/// something twenty messages back, and every replayed turn costs tokens.
const HISTORY_TURNS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grounding {
    /// Only your material. Gaps are stated, not filled.
    Strict,
    /// Your material first, then general knowledge, labelled.
    Open,
}

impl Grounding {
    pub fn as_str(self) -> &'static str {
        match self {
            Grounding::Strict => "strict",
            Grounding::Open => "open",
        }
    }

    fn parse(s: &str) -> Self {
        if s == "open" {
            Grounding::Open
        } else {
            Grounding::Strict
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: i64,
    pub subject_id: Option<i64>,
    pub subject_name: Option<String>,
    pub colour: Option<String>,
    pub title: String,
    pub grounding: Grounding,
    pub message_count: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: i64,
    pub name: String,
    pub words: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: i64,
    pub role: String,
    pub body: String,
    /// Citations for an assistant message.
    pub sources: Vec<Excerpt>,
    pub model: Option<String>,
    pub attachments: Vec<Attachment>,
    pub created_at: String,
}

/// A file sent with a question.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAttachment {
    pub name: String,
    pub content: String,
}

// ---------------------------------------------------------------------------
// Conversations
// ---------------------------------------------------------------------------

pub fn create(
    conn: &Connection,
    subject_id: Option<i64>,
    grounding: Grounding,
    now: DateTime<Utc>,
) -> Result<i64> {
    let stamp = util::rfc3339(now);
    conn.execute(
        "INSERT INTO conversations (subject_id, title, grounding, created_at, updated_at)
         VALUES (?1, 'New conversation', ?2, ?3, ?3)",
        rusqlite::params![subject_id, grounding.as_str(), stamp],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list(conn: &Connection, limit: i64) -> Result<Vec<Conversation>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.subject_id, s.name, s.colour, c.title, c.grounding,
                (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id),
                c.updated_at
           FROM conversations c
           LEFT JOIN subjects s ON s.id = c.subject_id
          ORDER BY c.updated_at DESC, c.id DESC LIMIT ?1",
    )?;

    let rows = stmt
        .query_map([limit], |r| {
            Ok(Conversation {
                id: r.get(0)?,
                subject_id: r.get(1)?,
                subject_name: r.get(2)?,
                colour: r.get(3)?,
                title: r.get(4)?,
                grounding: Grounding::parse(&r.get::<_, String>(5)?),
                message_count: r.get(6)?,
                updated_at: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

pub fn set_grounding(conn: &Connection, id: i64, grounding: Grounding) -> Result<()> {
    conn.execute(
        "UPDATE conversations SET grounding = ?2 WHERE id = ?1",
        rusqlite::params![id, grounding.as_str()],
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    conn.execute("DELETE FROM conversations WHERE id = ?1", [id])?;
    Ok(())
}

pub fn messages(conn: &Connection, conversation_id: i64) -> Result<Vec<Message>> {
    let mut stmt = conn.prepare(
        "SELECT id, role, body, sources, model, created_at
           FROM messages WHERE conversation_id = ?1 ORDER BY id",
    )?;

    // A message row read positionally: id, role, body, sources, model, created.
    #[allow(clippy::type_complexity)]
    let rows: Vec<(i64, String, String, Option<String>, Option<String>, String)> = stmt
        .query_map([conversation_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })?
        .collect::<Result<_, _>>()?;

    let mut out = Vec::with_capacity(rows.len());
    for (id, role, body, sources, model, created_at) in rows {
        let mut att = conn.prepare(
            "SELECT id, name, words FROM message_attachments WHERE message_id = ?1 ORDER BY id",
        )?;
        let attachments = att
            .query_map([id], |r| {
                Ok(Attachment {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    words: r.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        out.push(Message {
            id,
            role,
            body,
            // A citation blob we can't parse costs the citations, not the
            // message.
            sources: sources
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            model,
            attachments,
            created_at,
        });
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Prompt assembly
// ---------------------------------------------------------------------------

pub const SYSTEM_STRICT: &str = "You are a study assistant for a VCE student in Victoria, \
Australia.\n\n\
ANSWER ONLY FROM THE MATERIAL PROVIDED BELOW. This is the rule that matters most.\n\
- If the material covers the question, answer from it and stay close to its wording.\n\
- If it covers the question only partly, answer that part and say plainly what is missing.\n\
- If it does not cover the question at all, say so directly: \"Your material doesn't cover \
this.\" Then say what would answer it — a specific document or dot point to add.\n\
- Never fill a gap from your own knowledge. A confident answer the student cannot trace back \
to their own notes is worse than no answer, because they will revise from it.\n\n\
You may also use the student's Retain data (what's due, what's coming up) when it is given, \
and you may reason about their study plan from it. Be concise and specific.";

pub const SYSTEM_OPEN: &str = "You are a study assistant for a VCE student in Victoria, \
Australia.\n\n\
Use the student's own material first and stay close to its wording where it applies. When you \
go beyond it, say so explicitly — for example \"this isn't in your notes, but generally…\" — so \
they always know which parts they can trace and which they can't.\n\n\
You may also use the student's Retain data (what's due, what's coming up) when it is given. \
Be concise and specific. Don't pad.";

/// A compact picture of the student's current state.
///
/// This is what lets "what should I work on tonight?" be answerable. Computed
/// in Rust from real rows — the model is never asked to count anything.
pub fn app_context(conn: &Connection) -> String {
    let mut lines: Vec<String> = Vec::new();

    if let Ok(counts) = crate::cards::counts(conn, None) {
        if counts.due_reviews > 0 || counts.new_remaining_total > 0 {
            lines.push(format!(
                "Flashcards due today: {} reviews, {} new available.",
                counts.due_reviews, counts.new_remaining_total
            ));
        }
    }

    if let Ok(items) = crate::assessments::list(conn, false) {
        let soon: Vec<String> = items
            .iter()
            .filter(|a| a.days_away >= 0 && a.days_away <= 21)
            .map(|a| format!("{} ({}) in {} days", a.name, a.subject_name, a.days_away))
            .take(5)
            .collect();
        if !soon.is_empty() {
            lines.push(format!("Coming up: {}.", soon.join("; ")));
        }
    }

    if let Ok(facts) = crate::ai::weekly_facts(conn) {
        if facts.sessions > 0 {
            lines.push(format!(
                "This week: {}h {}m across {} sessions.",
                facts.total_minutes / 60,
                facts.total_minutes % 60,
                facts.sessions
            ));
        }
        if !facts.untouched.is_empty() {
            lines.push(format!("Not studied this week: {}.", facts.untouched.join(", ")));
        }
    }

    // Committed time, so "what should I do tonight?" accounts for the fact
    // that tonight might already be spoken for.
    let commitments = crate::blocks::week_summary(conn, crate::util::retain_today_naive())
        .unwrap_or_default();

    // What they meant to do today, and what has been slipping. Without this the
    // assistant answers "what should I work on?" from scratch every time and
    // ignores the plan sitting on the screen next to it.
    let today_plan = crate::plan::summary(conn, chrono::Local::now().date_naive())
        .unwrap_or_default();

    if lines.is_empty() && commitments.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    if !lines.is_empty() {
        out.push_str(&format!("--- The student's Retain data ---\n{}\n", lines.join("\n")));
    }
    if !commitments.is_empty() {
        out.push('\n');
        out.push_str(&commitments);
    }
    if !today_plan.is_empty() {
        out.push('\n');
        out.push_str(&today_plan);
    }
    out
}

/// Build the user-side prompt for one turn.
///
/// Order is deliberate: retrieved material, then attachments, then app data,
/// then the history, then the question last. Models weight the end of a prompt
/// most heavily, and the question is the part that must not get lost.
pub fn build_prompt(
    excerpts: &[Excerpt],
    attachments: &[NewAttachment],
    app_data: &str,
    history: &[Message],
    question: &str,
    grounding: Grounding,
) -> String {
    let mut out = String::new();

    match crate::resources::context_block(excerpts) {
        Some(block) => out.push_str(&block),
        None if grounding == Grounding::Strict => {
            // Saying nothing was found is what produces "your material doesn't
            // cover this" rather than a silent fall back to general knowledge.
            out.push_str(
                "--- The student's material ---\n\
                 Nothing in the student's uploaded material matched this question.\n",
            );
        }
        None => {}
    }

    for a in attachments {
        out.push_str(&format!(
            "\n--- Attached to this message: {} ---\n{}\n",
            a.name,
            a.content.trim()
        ));
    }

    if !app_data.is_empty() {
        out.push('\n');
        out.push_str(app_data);
    }

    if !history.is_empty() {
        out.push_str("\n--- Earlier in this conversation ---\n");
        for m in history.iter().rev().take(HISTORY_TURNS).rev() {
            let who = if m.role == "user" { "Student" } else { "You" };
            // Long turns are trimmed; the gist is what matters for continuity.
            let body: String = m.body.chars().take(700).collect();
            out.push_str(&format!("{who}: {body}\n"));
        }
    }

    out.push_str(&format!("\n--- The student's question ---\n{}\n", question.trim()));
    out
}

// ---------------------------------------------------------------------------
// Persisting a turn
// ---------------------------------------------------------------------------

pub fn add_user_message(
    conn: &mut Connection,
    conversation_id: i64,
    body: &str,
    attachments: &[NewAttachment],
    now: DateTime<Utc>,
) -> Result<i64> {
    let tx = conn.transaction()?;

    tx.execute(
        "INSERT INTO messages (conversation_id, role, body, created_at)
         VALUES (?1, 'user', ?2, ?3)",
        rusqlite::params![conversation_id, body, util::rfc3339(now)],
    )?;
    let id = tx.last_insert_rowid();

    {
        let mut stmt = tx.prepare(
            "INSERT INTO message_attachments (message_id, name, content, words)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for a in attachments {
            stmt.execute(rusqlite::params![
                id,
                a.name,
                a.content,
                a.content.split_whitespace().count() as i64
            ])?;
        }
    }

    // The first question becomes the conversation's title — a list of twenty
    // items called "New conversation" is unusable.
    tx.execute(
        "UPDATE conversations
            SET updated_at = ?2,
                title = CASE WHEN title = 'New conversation' THEN ?3 ELSE title END
          WHERE id = ?1",
        rusqlite::params![conversation_id, util::rfc3339(now), title_from(body)],
    )?;

    tx.commit()?;
    Ok(id)
}

pub fn add_assistant_message(
    conn: &Connection,
    conversation_id: i64,
    body: &str,
    sources: &[Excerpt],
    model: Option<&str>,
    now: DateTime<Utc>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO messages (conversation_id, role, body, sources, model, created_at)
         VALUES (?1, 'assistant', ?2, ?3, ?4, ?5)",
        rusqlite::params![
            conversation_id,
            body,
            serde_json::to_string(sources).ok(),
            model,
            util::rfc3339(now),
        ],
    )?;
    let id = conn.last_insert_rowid();

    conn.execute(
        "UPDATE conversations SET updated_at = ?2 WHERE id = ?1",
        rusqlite::params![conversation_id, util::rfc3339(now)],
    )?;

    Ok(id)
}

fn title_from(question: &str) -> String {
    let one_line = question.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.is_empty() {
        return "New conversation".into();
    }
    if one_line.chars().count() <= 60 {
        return one_line;
    }
    format!("{}…", one_line.chars().take(60).collect::<String>().trim_end())
}

/// The whole conversation as Markdown, for export or printing.
pub fn to_markdown(conversation: &Conversation, messages: &[Message]) -> Result<String> {
    if messages.is_empty() {
        return Err(anyhow!("There's nothing in this conversation yet."));
    }

    let mut out = format!("# {}\n\n", conversation.title);
    if let Some(s) = &conversation.subject_name {
        out.push_str(&format!("**Subject:** {s}  \n"));
    }
    out.push_str(&format!(
        "**Grounding:** {}\n\n---\n",
        match conversation.grounding {
            Grounding::Strict => "only my own material",
            Grounding::Open => "my material, plus general knowledge",
        }
    ));

    for m in messages {
        out.push_str(if m.role == "user" { "\n## Question\n\n" } else { "\n## Answer\n\n" });
        out.push_str(m.body.trim());
        out.push('\n');

        if !m.attachments.is_empty() {
            let names: Vec<&str> = m.attachments.iter().map(|a| a.name.as_str()).collect();
            out.push_str(&format!("\n*Attached: {}*\n", names.join(", ")));
        }
        if !m.sources.is_empty() {
            out.push_str("\n*Sources: ");
            let names: Vec<String> = m
                .sources
                .iter()
                .map(|s| s.resource_title.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            out.push_str(&names.join(", "));
            out.push_str("*\n");
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests;
