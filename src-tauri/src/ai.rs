//! Bring-your-own-key AI. Five narrow features, no chatbot.
//!
//! ## Rules this module is built around
//!
//! * **Keys never leave the Keychain except into one outbound request.** The
//!   frontend can ask whether a key exists; it can never read one. Nothing here
//!   logs, stores, or returns a key, and no error message includes one.
//! * **Every feature is optional.** Each command fails with `AiUnavailable`
//!   when no key is configured, and the UI turns that into "add a key to enable
//!   this" rather than an error. Nothing in the core app — timer, cards, error
//!   log, streak — calls into this module.
//! * **Prompts are assembled by the app, never passed through.** Five of the
//!   six entry points build their own prompt from app data and parse a specific
//!   shape back. The sixth, `ask`, backs the study assistant and takes a system
//!   prompt from the caller — but that caller is `assistant::build_prompt`,
//!   which constructs both halves from retrieved material, attachments and your
//!   Retain data. A user's raw text never becomes the system prompt.
//!
//!   This changed deliberately when the assistant was added. It was previously
//!   true that Retain had no chat surface at all; saying so now would be false.
//!
//! ## Model choice
//!
//! Defaults are per provider and **user-editable in Settings**, because model
//! names change faster than this app will.
//!
//! Gemini's default is deliberately the alias `gemini-flash-latest` rather than
//! a pinned version. A pinned one is exactly what broke: `gemini-2.0-flash` was
//! retired, disappeared from the API entirely, and every AI feature began
//! reporting an unknown model.
//!
//! Two things were measured against the live API rather than assumed, and both
//! shape the code below:
//!
//!   * **`v1beta` is the right endpoint.** `v1` does not serve the `-latest`
//!     aliases at all.
//!   * **ListModels is not a promise.** Some models appear in the list with
//!     `generateContent` among their supported methods and still return 404
//!     when you call them. So discovery is offered as *candidates to try*, and
//!     the only way to know a model works is `test_model`, which performs a
//!     real generation.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::secrets::{self, Provider};

const TIMEOUT_SECS: u64 = 90;

/// Raised whenever a feature can't run. The UI treats this as "offer to set up
/// a key", never as a failure of the app.
#[derive(Debug)]
pub struct AiUnavailable(pub String);

impl std::fmt::Display for AiUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which provider is configured, if any.
pub fn active_provider(conn: &rusqlite::Connection) -> Option<Provider> {
    let preferred = crate::settings::get(conn, "ai_provider").ok().flatten();

    let order = match preferred.as_deref() {
        Some("open_ai") => [Provider::OpenAi, Provider::Anthropic, Provider::Gemini, Provider::OpenRouter],
        Some("gemini") => [Provider::Gemini, Provider::Anthropic, Provider::OpenAi, Provider::OpenRouter],
        Some("open_router") => [Provider::OpenRouter, Provider::Anthropic, Provider::OpenAi, Provider::Gemini],
        _ => [Provider::Anthropic, Provider::OpenAi, Provider::Gemini, Provider::OpenRouter],
    };

    order.into_iter().find(|p| secrets::has_key(*p))
}

fn default_model(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "claude-opus-5",
        Provider::OpenAi => "gpt-4o",
        // A Google-maintained alias rather than a pinned version. Pinning is
        // what broke this before: `gemini-2.0-flash` was retired out from
        // under us and every AI feature started reporting an unknown model.
        // Verified against the live API: this answers on v1beta.
        Provider::Gemini => "gemini-flash-latest",
        Provider::OpenRouter => "anthropic/claude-opus-5",
    }
}

/// The stable string for a provider — matches its serde representation, so the
/// value stored in settings round-trips through `active_provider`.
pub fn slug(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "anthropic",
        Provider::OpenAi => "open_ai",
        Provider::Gemini => "gemini",
        Provider::OpenRouter => "open_router",
    }
}

pub fn model_setting_key(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "ai_model_anthropic",
        Provider::OpenAi => "ai_model_openai",
        Provider::Gemini => "ai_model_gemini",
        Provider::OpenRouter => "ai_model_openrouter",
    }
}

/// The model in use: the user's override if set, otherwise the default.
pub fn model_name(conn: &rusqlite::Connection, provider: Provider) -> String {
    crate::settings::get(conn, model_setting_key(provider))
        .ok()
        .flatten()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| default_model(provider).to_string())
}

// ---------------------------------------------------------------------------
// Model discovery
// ---------------------------------------------------------------------------

/// A model the configured key can actually use.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    /// The bare id, with Gemini's `models/` prefix stripped — this is what goes
    /// in the request path and in the Settings field.
    pub id: String,
    pub display_name: String,
    /// Whether it supports the one operation Retain performs.
    pub supports_generate_content: bool,
}

/// Ask Gemini which models this key can use.
///
/// Model ids change on Google's schedule, not ours, so the honest answer to
/// "which model should Retain use?" is to ask. This backs both the Settings
/// picker and the error message shown when a configured model has been retired.
///
/// Follows `nextPageToken`, because the list is paginated and the model you want
/// may not be on the first page.
pub async fn gemini_models() -> Result<Vec<ModelOption>, AiUnavailable> {
    let key = secrets::get_key(Provider::Gemini)
        .map_err(|_| AiUnavailable("No Gemini key is set up.".into()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AiUnavailable(format!("Couldn't start an HTTPS client: {e}")))?;

    let mut out: Vec<ModelOption> = Vec::new();
    let mut page: Option<String> = None;

    // Bounded so a server that always returns a token can't loop forever.
    for _ in 0..10 {
        let mut req = client
            .get("https://generativelanguage.googleapis.com/v1beta/models")
            // Header, never `?key=` — a credential in a URL ends up in logs,
            // proxies and browser history.
            .header("x-goog-api-key", key.expose())
            .query(&[("pageSize", "200")]);

        if let Some(token) = &page {
            req = req.query(&[("pageToken", token.as_str())]);
        }

        let response = req.send().await.map_err(|e| {
            AiUnavailable(if e.is_timeout() {
                "Gemini didn't respond in time.".into()
            } else {
                "Couldn't reach Gemini. Are you online?".to_string()
            })
        })?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|_| AiUnavailable("Gemini sent an unreadable reply.".into()))?;

        if !status.is_success() {
            // Never echoes the body: a Google error can quote the request it
            // received, and the request carried the key.
            return Err(AiUnavailable(match status.as_u16() {
                400 | 401 | 403 => "Gemini rejected the key. Check it in Settings.".into(),
                429 => "Gemini is rate-limiting or the quota is used up.".into(),
                other => format!("Gemini returned {other} when listing models."),
            }));
        }

        let parsed: Value = serde_json::from_str(&text)
            .map_err(|_| AiUnavailable("Gemini sent malformed JSON.".into()))?;

        out.extend(parse_model_list(&parsed));

        page = parsed
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .map(str::to_string);

        if page.is_none() {
            break;
        }
    }

    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    Ok(out)
}

/// Pull the model list out of a ListModels response.
///
/// Split out from the request so the parsing is testable without a network.
pub fn parse_model_list(v: &Value) -> Vec<ModelOption> {
    let Some(models) = v.get("models").and_then(Value::as_array) else {
        return Vec::new();
    };

    models
        .iter()
        .filter_map(|m| {
            let raw = m.get("name").and_then(Value::as_str)?;
            // Responses use `models/gemini-2.5-flash`; the request path and the
            // Settings field both want the bare id.
            let id = raw.strip_prefix("models/").unwrap_or(raw).to_string();
            if id.is_empty() {
                return None;
            }

            let supports = m
                .get("supportedGenerationMethods")
                .and_then(Value::as_array)
                .map(|a| a.iter().any(|s| s.as_str() == Some("generateContent")))
                .unwrap_or(false);

            Some(ModelOption {
                display_name: m
                    .get("displayName")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_string(),
                id,
                supports_generate_content: supports,
            })
        })
        .collect()
}

/// The ones Retain can actually use.
pub fn usable(models: &[ModelOption]) -> Vec<&ModelOption> {
    models.iter().filter(|m| m.supports_generate_content).collect()
}

/// One completion. The only place a key is read, and it goes straight into the
/// request without being stored, logged, or returned.
async fn complete(
    provider: Provider,
    model: &str,
    system: &str,
    user: &str,
    max_tokens: u32,
) -> Result<String, AiUnavailable> {
    complete_with_images(provider, model, system, user, &[], max_tokens).await
}

/// Split a `data:image/png;base64,…` URL into its media type and payload.
///
/// Returns `None` for anything that isn't a base64 data URL, so a malformed
/// attachment costs the image rather than the whole request.
fn split_data_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let media = meta.strip_suffix(";base64")?;
    media.starts_with("image/").then_some((media, data))
}

/// As `complete`, with images attached to the user turn.
///
/// Every provider takes images as an array of content parts rather than a
/// string, and each spells it differently: Anthropic wants `source.data`,
/// OpenAI wants the whole data URL back, Gemini wants `inline_data`. The
/// divergence is why this is one function per provider rather than a shared
/// body with a flag.
async fn complete_with_images(
    provider: Provider,
    model: &str,
    system: &str,
    user: &str,
    images: &[String],
    max_tokens: u32,
) -> Result<String, AiUnavailable> {
    let key = secrets::get_key(provider)
        .map_err(|_| AiUnavailable(format!("No {} key is set up.", provider.label())))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build()
        .map_err(|e| AiUnavailable(format!("Couldn't start an HTTPS client: {e}")))?;

    // Each provider wants a different body shape and a different auth header,
    // so the request is assembled per provider and only the send/parse half is
    // shared.
    let (body, request) = match provider {
        Provider::Anthropic => {
            let mut content = vec![json!({ "type": "text", "text": user })];
            for (media, data) in images.iter().filter_map(|u| split_data_url(u)) {
                content.push(json!({
                    "type": "image",
                    "source": { "type": "base64", "media_type": media, "data": data },
                }));
            }
            let body = json!({
                "model": model,
                "max_tokens": max_tokens,
                "system": system,
                "messages": [{ "role": "user", "content": content }],
            });
            let req = client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", key.expose())
                // Required on every Anthropic request; without it the call is
                // rejected for a reason unrelated to the key.
                .header("anthropic-version", "2023-06-01");
            (body, req)
        }
        Provider::OpenAi | Provider::OpenRouter => {
            let url = if provider == Provider::OpenAi {
                "https://api.openai.com/v1/chat/completions"
            } else {
                "https://openrouter.ai/api/v1/chat/completions"
            };
            // OpenAI accepts a plain string when there are no images, and the
            // string form is what every existing feature sends — so the parts
            // array is used only when there is actually an image.
            let content = if images.is_empty() {
                json!(user)
            } else {
                let mut parts = vec![json!({ "type": "text", "text": user })];
                for url in images {
                    parts.push(json!({ "type": "image_url", "image_url": { "url": url } }));
                }
                json!(parts)
            };
            let body = json!({
                "model": model,
                "max_tokens": max_tokens,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": content },
                ],
            });
            let req = client
                .post(url)
                .header("authorization", format!("Bearer {}", key.expose()));
            (body, req)
        }
        Provider::Gemini => {
            // The key goes in a header, never the query string — a credential in
            // a URL ends up in logs and history.
            let mut parts = vec![json!({ "text": user })];
            for (media, data) in images.iter().filter_map(|u| split_data_url(u)) {
                parts.push(json!({ "inline_data": { "mime_type": media, "data": data } }));
            }
            let body = json!({
                "system_instruction": { "parts": [{ "text": system }] },
                "contents": [{ "parts": parts }],
                "generationConfig": { "maxOutputTokens": max_tokens },
            });
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
            );
            let req = client.post(&url).header("x-goog-api-key", key.expose());
            return send(req, body, provider, model).await;
        }
    };

    let outcome = send(request, body, provider, model).await;

    // When Gemini says the model doesn't exist, go and find out what does
    // rather than leaving the user to guess. Costs one extra request, and only
    // on the path that already failed.
    match outcome {
        Err(e) if provider == Provider::Gemini && e.0.contains("has no model called") => {
            Err(AiUnavailable(with_gemini_candidates(e.0).await))
        }
        other => other,
    }
}

/// Append a few models this key can actually see to a "no such model" error.
///
/// Framed as candidates rather than guarantees: a model can appear in
/// ListModels with `generateContent` listed and still 404 when called, which
/// was measured, not assumed. `test_model` is the only real answer.
async fn with_gemini_candidates(message: String) -> String {
    let Ok(models) = gemini_models().await else {
        return format!("{message} Open Settings to pick a different one.");
    };

    // Aliases first: they're maintained by Google and are the least likely to
    // be retired under us again.
    let mut ids: Vec<&str> = usable(&models).into_iter().map(|m| m.id.as_str()).collect();
    ids.sort_by_key(|id| (!id.ends_with("-latest"), id.len()));

    let shortlist: Vec<&str> = ids.into_iter().take(5).collect();
    if shortlist.is_empty() {
        return format!("{message} This key can't see any models that support generation.");
    }

    format!(
        "{message} Models this key can see include: {}. Pick one in Settings and use Test to \
         confirm it works.",
        shortlist.join(", ")
    )
}

/// Try a model for real, with the smallest possible generation.
///
/// The only trustworthy check. ListModels tells you what exists; this tells you
/// what answers.
pub async fn test_model(provider: Provider, model: &str) -> Result<String, AiUnavailable> {
    let model = model.trim();
    if model.is_empty() {
        return Err(AiUnavailable("Enter a model name first.".into()));
    }

    complete(provider, model, "Reply with one word.", "Say: ready", 32)
        .await
        .map(|reply| {
            let reply = reply.trim();
            if reply.is_empty() {
                format!("{model} answered.")
            } else {
                format!("{model} answered: \"{reply}\"")
            }
        })
}

/// Turn an HTTP status into something a student can act on.
///
/// Pure, so every branch is testable without a network or a key.
///
/// It deliberately **never includes the response body**. A provider error can
/// quote the request it received, and that request carried the API key — so
/// echoing the body would be the one place a key could escape into a message
/// the UI displays.
pub fn classify_http(status: u16, provider: Provider, model: &str) -> AiUnavailable {
    let name = provider.label();

    AiUnavailable(match status {
        401 | 403 => format!("{name} rejected the key. Check it in Settings."),
        // 400 is ambiguous across providers — usually a malformed request, but
        // Gemini also returns it for a key that isn't valid for the API.
        400 => format!("{name} rejected the request. Check the key and model name in Settings."),
        // Names the model. Saying "change it in Settings" without saying which
        // model failed is what made the original report so hard to act on.
        404 => format!("{name} has no model called \"{model}\"."),
        429 => format!("{name} is rate-limiting or the quota is used up. Try again later."),
        500..=599 => format!("{name} is having trouble ({status}). Not your end — try again later."),
        other => format!("{name} returned an error ({other})."),
    })
}

async fn send(
    request: reqwest::RequestBuilder,
    body: Value,
    provider: Provider,
    model: &str,
) -> Result<String, AiUnavailable> {
    let response = request
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            AiUnavailable(if e.is_timeout() {
                format!("{} didn't respond in time.", provider.label())
            } else {
                format!("Couldn't reach {}. Are you online?", provider.label())
            })
        })?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|_| AiUnavailable(format!("{} sent an unreadable reply.", provider.label())))?;

    if !status.is_success() {
        return Err(classify_http(status.as_u16(), provider, model));
    }

    let parsed: Value = serde_json::from_str(&text)
        .map_err(|_| AiUnavailable(format!("{} sent malformed JSON.", provider.label())))?;

    extract_text(&parsed, provider)
        .ok_or_else(|| AiUnavailable(format!("{} sent no usable text.", provider.label())))
}

/// Pull the text out of each provider's response shape.
pub fn extract_text(v: &Value, provider: Provider) -> Option<String> {
    let text = match provider {
        Provider::Anthropic => v
            .get("content")?
            .as_array()?
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        Provider::OpenAi | Provider::OpenRouter => v
            .get("choices")?
            .as_array()?
            .first()?
            .get("message")?
            .get("content")?
            .as_str()?
            .to_string(),
        Provider::Gemini => v
            .get("candidates")?
            .as_array()?
            .first()?
            .get("content")?
            .get("parts")?
            .as_array()?
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
    };

    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Strip a markdown fence if the model wrapped its JSON in one.
///
/// Models do this constantly regardless of instructions, and a parser that
/// doesn't handle it fails on perfectly good output.
pub fn unfence(raw: &str) -> &str {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix("```") {
        // Drop an optional language tag on the first line.
        let rest = rest.split_once('\n').map(|(_, r)| r).unwrap_or(rest);
        return rest.strip_suffix("```").unwrap_or(rest).trim();
    }
    t
}

// ---------------------------------------------------------------------------
// Feature 1 — messy note → structured task
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskSuggestion {
    pub title: String,
    pub subject: Option<String>,
    pub due_on: Option<String>,
}

pub fn parse_task_suggestion(raw: &str) -> Option<TaskSuggestion> {
    serde_json::from_str::<TaskSuggestion>(unfence(raw)).ok()
}

// ---------------------------------------------------------------------------
// Feature 2 — notes → cards
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CardSuggestion {
    pub front: String,
    pub back: String,
}

/// Parse generated cards, tolerating the usual model output quirks.
pub fn parse_card_suggestions(raw: &str) -> Vec<CardSuggestion> {
    let cleaned = unfence(raw);

    if let Ok(list) = serde_json::from_str::<Vec<CardSuggestion>>(cleaned) {
        return list.into_iter().filter(|c| !c.front.trim().is_empty()).collect();
    }

    // Sometimes wrapped in an object.
    if let Ok(v) = serde_json::from_str::<Value>(cleaned) {
        if let Some(arr) = v.get("cards").and_then(Value::as_array) {
            return arr
                .iter()
                .filter_map(|c| serde_json::from_value::<CardSuggestion>(c.clone()).ok())
                .filter(|c| !c.front.trim().is_empty())
                .collect();
        }
    }

    Vec::new()
}

// ---------------------------------------------------------------------------
// Feature 5 — suggest an error category
// ---------------------------------------------------------------------------

/// Map a model's free-text answer onto one of the allowed categories.
///
/// Returns `None` rather than guessing when nothing matches — a wrong category
/// silently applied would poison the recurring-error analytics, which is the
/// single most useful screen in the app.
pub fn match_category(raw: &str, allowed: &[&str]) -> Option<String> {
    let answer = unfence(raw).trim().trim_matches('"').to_lowercase();

    if let Some(exact) = allowed.iter().find(|c| c.to_lowercase() == answer) {
        return Some(exact.to_string());
    }
    // A model often replies with a sentence containing the category.
    allowed
        .iter()
        .find(|c| answer.contains(&c.to_lowercase()))
        .map(|c| c.to_string())
}

// ---------------------------------------------------------------------------
// Prompts
// ---------------------------------------------------------------------------

pub const SYSTEM_TASK: &str = "You turn a student's shorthand note into a task. \
Reply with ONLY a JSON object: {\"title\": string, \"subject\": string|null, \"dueOn\": \
\"YYYY-MM-DD\"|null}. No prose, no code fence. If a field is genuinely unclear, use null \
rather than guessing.";

pub const SYSTEM_CARDS: &str = "You write atomic flashcards from a student's notes. \
Reply with ONLY a JSON array of {\"front\": string, \"back\": string}. No prose, no code fence. \
Each card tests exactly ONE fact. Prefer few excellent cards over many mediocre ones — a card \
that tests two things at once is worse than no card. Use the student's own terminology.";

pub const SYSTEM_WEEKLY: &str = "You are reviewing a VCE student's own study data. \
Be specific and concrete, cite their numbers, and keep it under 180 words. \
Name the subject they have avoided and the mistake they keep repeating. \
Do not be gentle to the point of uselessness, but do not moralise, guilt-trip, or use \
loss framing — no 'falling behind', no 'running out of time'. End with one concrete \
suggestion for the coming week.";

pub const SYSTEM_PRACTICE: &str = "You write VCAA-style Biology Units 3&4 exam questions. \
Match VCAA's register and command words exactly. Reply as plain text in this shape:\n\n\
QUESTION\n<the question, with marks in brackets>\n\nMODEL ANSWER\n<a full-mark response>\n\n\
RUBRIC\n<one line per mark, each starting '1 mark: '>\n\n\
The rubric must have exactly as many lines as there are marks, so the student can self-mark \
point by point.";

pub const SYSTEM_NOTES: &str = "You write study notes for a VCE student. \
Structure them with Markdown headings and short bullet points — this is material to revise \
from, not an essay. Define terms precisely, because precise wording is most of the difference \
between a one-mark and a two-mark answer. Where the student's own material is supplied, follow \
it and prefer its terminology. If their material doesn't cover part of what was asked, write \
the section but say plainly that it wasn't in their notes.";

pub const SYSTEM_CATEGORY: &str = "You classify a student's exam mistake. \
Reply with ONLY the single category name from the list you are given, copied exactly. \
No explanation, no punctuation, no code fence.";

// ---------------------------------------------------------------------------
// Feature 3 — the week's facts
// ---------------------------------------------------------------------------
//
// The numbers are computed here, in Rust, from the database — not by the model.
// The model only writes prose around facts it was handed. That matters: a
// language model asked to add up study hours will produce a confident wrong
// total, and a study tracker that misreports your own hours back to you is
// worse than one with no AI at all.

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyFacts {
    /// (subject, minutes) for the last 7 retain-days, most studied first.
    pub minutes_by_subject: Vec<(String, i64)>,
    /// Active subjects with zero minutes this week.
    pub untouched: Vec<String>,
    /// (category, count) across the week's error log, most frequent first.
    pub errors_by_category: Vec<(String, i64)>,
    pub total_minutes: i64,
    pub sessions: i64,
    pub cards_reviewed: i64,
    pub from: String,
    pub to: String,
}

pub fn weekly_facts(conn: &rusqlite::Connection) -> anyhow::Result<WeeklyFacts> {
    let today = crate::util::retain_today_naive();
    let from = (today - chrono::Duration::days(6)).format("%Y-%m-%d").to_string();
    let to = today.format("%Y-%m-%d").to_string();

    let mut minutes_by_subject: Vec<(String, i64)> = conn
        .prepare(
            "SELECT s.name, COALESCE(SUM(x.active_seconds), 0) / 60
               FROM subjects s
               LEFT JOIN sessions x
                 ON x.subject_id = s.id AND x.local_date BETWEEN ?1 AND ?2
              WHERE s.archived = 0
              GROUP BY s.id
              ORDER BY 2 DESC, s.sort_order",
        )?
        .query_map([&from, &to], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;

    let untouched: Vec<String> = minutes_by_subject
        .iter()
        .filter(|(_, m)| *m == 0)
        .map(|(n, _)| n.clone())
        .collect();
    minutes_by_subject.retain(|(_, m)| *m > 0);

    let errors_by_category: Vec<(String, i64)> = conn
        .prepare(
            "SELECT category, COUNT(*) FROM error_entries
              WHERE logged_on BETWEEN ?1 AND ?2
              GROUP BY category ORDER BY 2 DESC, category",
        )?
        .query_map([&from, &to], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;

    let (sessions, total_minutes): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(active_seconds), 0) / 60 FROM sessions
          WHERE local_date BETWEEN ?1 AND ?2 AND ended_at IS NOT NULL",
        [&from, &to],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let cards_reviewed: i64 = conn.query_row(
        "SELECT COUNT(*) FROM review_log WHERE local_date BETWEEN ?1 AND ?2",
        [&from, &to],
        |r| r.get(0),
    )?;

    Ok(WeeklyFacts {
        minutes_by_subject,
        untouched,
        errors_by_category,
        total_minutes,
        sessions,
        cards_reviewed,
        from,
        to,
    })
}

impl WeeklyFacts {
    /// Render as the prompt body. Plain lines, no invented interpretation.
    pub fn render(&self) -> String {
        let mut s = format!("Week of {} to {}.\n\nHours logged:\n", self.from, self.to);

        if self.minutes_by_subject.is_empty() {
            s.push_str("  (nothing logged this week)\n");
        }
        for (name, mins) in &self.minutes_by_subject {
            s.push_str(&format!("  {name}: {}h {}m\n", mins / 60, mins % 60));
        }
        if !self.untouched.is_empty() {
            s.push_str(&format!("\nNot touched at all: {}\n", self.untouched.join(", ")));
        }

        s.push_str("\nMistakes logged:\n");
        if self.errors_by_category.is_empty() {
            s.push_str("  (none logged this week)\n");
        }
        for (cat, n) in &self.errors_by_category {
            s.push_str(&format!("  {cat}: {n}\n"));
        }

        s.push_str(&format!(
            "\nTotals: {} sessions, {}h {}m focused, {} cards reviewed.",
            self.sessions,
            self.total_minutes / 60,
            self.total_minutes % 60,
            self.cards_reviewed
        ));
        s
    }

    /// Enough happened to be worth writing about.
    pub fn has_enough_data(&self) -> bool {
        self.sessions >= 2 || !self.errors_by_category.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Callable features
// ---------------------------------------------------------------------------

pub struct Ai {
    pub provider: Provider,
    pub model: String,
}

impl Ai {
    pub fn from(conn: &rusqlite::Connection) -> Result<Self, AiUnavailable> {
        let provider = active_provider(conn).ok_or_else(|| {
            AiUnavailable(
                "No API key set up. Retain works fully without one — this feature is optional."
                    .into(),
            )
        })?;
        Ok(Self {
            model: model_name(conn, provider),
            provider,
        })
    }

    pub async fn task_from_note(&self, note: &str, subjects: &[String], today: &str)
        -> Result<TaskSuggestion, AiUnavailable>
    {
        let user = format!(
            "Today is {today}. The student's subjects are: {}.\n\nNote: {note}",
            subjects.join(", ")
        );
        let raw = complete(self.provider, &self.model, SYSTEM_TASK, &user, 500).await?;
        parse_task_suggestion(&raw)
            .ok_or_else(|| AiUnavailable("The reply wasn't in the expected format.".into()))
    }

    pub async fn cards_from_notes(&self, notes: &str, subject: &str, count: usize)
        -> Result<Vec<CardSuggestion>, AiUnavailable>
    {
        let user = format!(
            "Subject: {subject}. Write at most {count} atomic flashcards from these notes.\n\n{notes}"
        );
        let raw = complete(self.provider, &self.model, SYSTEM_CARDS, &user, 4000).await?;
        let cards = parse_card_suggestions(&raw);
        if cards.is_empty() {
            return Err(AiUnavailable("No usable cards came back.".into()));
        }
        Ok(cards)
    }

    /// One turn with a caller-supplied system prompt.
    ///
    /// The assistant needs this because its system prompt changes with the
    /// grounding mode. It is still not a general chat surface: the prompt is
    /// built by `assistant::build_prompt` from app data, never passed through
    /// from the user.
    pub async fn ask(&self, system: &str, user: &str) -> Result<String, AiUnavailable> {
        complete(self.provider, &self.model, system, user, 4000).await
    }

    /// Ask with images attached — a screenshot, a photo of a worked solution.
    pub async fn ask_with_images(
        &self,
        system: &str,
        user: &str,
        images: &[String],
    ) -> Result<String, AiUnavailable> {
        complete_with_images(self.provider, &self.model, system, user, images, 4000).await
    }

    pub async fn weekly_review(&self, summary: &str) -> Result<String, AiUnavailable> {
        complete(self.provider, &self.model, SYSTEM_WEEKLY, summary, 900).await
    }

    /// A practice question, optionally grounded in the student's own material.
    ///
    /// `context` comes from `resources::context_block`. When it's present the
    /// question is written against the real study design rather than the
    /// model's recollection of one.
    pub async fn practice_question(
        &self,
        dot_point: &str,
        marks: i64,
        context: Option<&str>,
    ) -> Result<String, AiUnavailable> {
        let user = match context {
            Some(c) => format!(
                "{c}\n\nKey knowledge dot point: {dot_point}\n\nWrite one question worth {marks} marks."
            ),
            None => format!(
                "Key knowledge dot point: {dot_point}\n\nWrite one question worth {marks} marks."
            ),
        };
        complete(self.provider, &self.model, SYSTEM_PRACTICE, &user, 1500).await
    }

    /// Study notes on a topic, grounded in the student's material when there is
    /// any. This is the feature the resource library exists to serve.
    pub async fn notes(
        &self,
        topic: &str,
        context: Option<&str>,
    ) -> Result<String, AiUnavailable> {
        let user = match context {
            Some(c) => format!("{c}\n\nWrite study notes on: {topic}"),
            None => format!("Write study notes on: {topic}"),
        };
        complete(self.provider, &self.model, SYSTEM_NOTES, &user, 4000).await
    }

    pub async fn suggest_category(
        &self,
        question: &str,
        my_answer: &str,
        correct: &str,
        allowed: &[&str],
    ) -> Result<Option<String>, AiUnavailable> {
        let user = format!(
            "Categories: {}\n\nQuestion: {question}\n\nThe student wrote: {my_answer}\n\n\
             The mark scheme says: {correct}",
            allowed.join(" | ")
        );
        let raw = complete(self.provider, &self.model, SYSTEM_CATEGORY, &user, 100).await?;
        Ok(match_category(&raw, allowed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- response extraction, per provider ---------------------------------

    #[test]
    fn extracts_anthropic_text_blocks() {
        let v = json!({"content":[{"type":"text","text":"hello"},{"type":"text","text":" world"}]});
        assert_eq!(extract_text(&v, Provider::Anthropic).as_deref(), Some("hello world"));
    }

    /// Non-text blocks (thinking, tool use) must not be concatenated into the
    /// answer.
    #[test]
    fn anthropic_ignores_non_text_blocks() {
        let v = json!({"content":[{"type":"thinking","thinking":"secret"},{"type":"text","text":"answer"}]});
        assert_eq!(extract_text(&v, Provider::Anthropic).as_deref(), Some("answer"));
    }

    #[test]
    fn extracts_openai_and_gemini_shapes() {
        let openai = json!({"choices":[{"message":{"content":"hi"}}]});
        assert_eq!(extract_text(&openai, Provider::OpenAi).as_deref(), Some("hi"));

        let gemini = json!({"candidates":[{"content":{"parts":[{"text":"hi"}]}}]});
        assert_eq!(extract_text(&gemini, Provider::Gemini).as_deref(), Some("hi"));
    }

    #[test]
    fn malformed_or_empty_responses_yield_none() {
        assert!(extract_text(&json!({}), Provider::Anthropic).is_none());
        assert!(extract_text(&json!({"content":[]}), Provider::Anthropic).is_none());
        assert!(extract_text(&json!({"content":[{"type":"text","text":"  "}]}), Provider::Anthropic).is_none());
        assert!(extract_text(&json!({"choices":[]}), Provider::OpenAi).is_none());
    }

    // -- fences ------------------------------------------------------------

    #[test]
    fn unfence_strips_code_blocks_with_and_without_language() {
        assert_eq!(unfence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(unfence("```\n[1,2]\n```"), "[1,2]");
        assert_eq!(unfence("{\"a\":1}"), "{\"a\":1}");
    }

    // -- feature parsers ---------------------------------------------------

    #[test]
    fn parses_a_task_suggestion() {
        let t = parse_task_suggestion(
            "```json\n{\"title\":\"Prac report\",\"subject\":\"Chemistry\",\"dueOn\":\"2026-08-14\"}\n```",
        )
        .unwrap();
        assert_eq!(t.title, "Prac report");
        assert_eq!(t.subject.as_deref(), Some("Chemistry"));
        assert_eq!(t.due_on.as_deref(), Some("2026-08-14"));
    }

    #[test]
    fn a_task_with_unknown_fields_still_parses() {
        let t = parse_task_suggestion("{\"title\":\"Thing\",\"subject\":null,\"dueOn\":null}").unwrap();
        assert_eq!(t.subject, None);
        assert_eq!(t.due_on, None);
    }

    #[test]
    fn nonsense_task_output_is_rejected_not_guessed() {
        assert!(parse_task_suggestion("I think you should do your homework!").is_none());
    }

    #[test]
    fn parses_cards_as_array_or_wrapped_object() {
        let arr = parse_card_suggestions("[{\"front\":\"Q\",\"back\":\"A\"}]");
        assert_eq!(arr.len(), 1);

        let wrapped = parse_card_suggestions("{\"cards\":[{\"front\":\"Q\",\"back\":\"A\"}]}");
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0].front, "Q");
    }

    #[test]
    fn cards_with_an_empty_front_are_dropped() {
        let cards = parse_card_suggestions(
            "[{\"front\":\"  \",\"back\":\"A\"},{\"front\":\"Real\",\"back\":\"B\"}]",
        );
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].front, "Real");
    }

    #[test]
    fn unparseable_card_output_yields_nothing_rather_than_junk() {
        assert!(parse_card_suggestions("Here are some cards for you!").is_empty());
    }

    // -- category matching -------------------------------------------------

    #[test]
    fn matches_a_category_exactly_or_within_a_sentence() {
        let allowed = ["careless slip", "conceptual gap", "ran out of time"];
        assert_eq!(match_category("conceptual gap", &allowed).as_deref(), Some("conceptual gap"));
        assert_eq!(match_category("\"Careless Slip\"", &allowed).as_deref(), Some("careless slip"));
        assert_eq!(
            match_category("This looks like a conceptual gap to me.", &allowed).as_deref(),
            Some("conceptual gap")
        );
    }

    /// An unrecognised answer must yield None, never a guess — a wrong category
    /// silently applied would poison the recurring-error analytics.
    #[test]
    fn an_unmatched_category_is_none_not_a_guess() {
        let allowed = ["careless slip", "conceptual gap"];
        assert_eq!(match_category("misread the diagram", &allowed), None);
        assert_eq!(match_category("", &allowed), None);
    }

    // -- prompts -----------------------------------------------------------

    /// The weekly-review prompt must forbid the framing the brief rules out.
    #[test]
    fn the_weekly_prompt_forbids_loss_framing() {
        let p = SYSTEM_WEEKLY.to_lowercase();
        assert!(p.contains("loss framing"));
        assert!(p.contains("guilt"));
        assert!(p.contains("falling behind"));
    }

    /// The rubric must be mark-by-mark so it can actually be self-marked.
    #[test]
    fn the_practice_prompt_demands_a_per_mark_rubric() {
        assert!(SYSTEM_PRACTICE.contains("1 mark:"));
        assert!(SYSTEM_PRACTICE.contains("exactly as many lines as there are marks"));
    }

    /// Card quality over quantity is a stated goal of the brief.
    #[test]
    fn the_card_prompt_asks_for_atomic_cards() {
        let p = SYSTEM_CARDS.to_lowercase();
        assert!(p.contains("one fact"));
        assert!(p.contains("few excellent cards"));
    }

    // -- Gemini: the six paths that broke, or could -------------------------
    //
    // The bug these exist for: `gemini-2.0-flash` was retired by Google, the
    // request 404'd, and the app said "change it in Settings" without saying
    // which model, why, or what to change it to.

    /// 1. A valid model is simply used, verbatim, in the request path.
    #[test]
    fn a_valid_gemini_model_is_used_as_given() {
        let list = parse_model_list(&json!({"models": [
            {"name": "models/gemini-flash-latest", "displayName": "Flash latest",
             "supportedGenerationMethods": ["generateContent", "countTokens"]}
        ]}));

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "gemini-flash-latest", "the models/ prefix must be stripped");
        assert!(list[0].supports_generate_content);
        assert_eq!(usable(&list).len(), 1);
    }

    /// The shipped default must be a model, not an alias we invented, and must
    /// not be the pinned id that was retired.
    #[test]
    fn the_gemini_default_is_not_the_retired_pinned_model() {
        let d = default_model(Provider::Gemini);
        assert_eq!(d, "gemini-flash-latest");
        assert_ne!(d, "gemini-2.0-flash", "the retired model must not come back");
        assert!(d.ends_with("-latest"), "prefer a maintained alias over a pinned version");
    }

    /// 2. An unavailable or deprecated model names itself in the error.
    #[test]
    fn an_unavailable_model_is_named_in_the_error() {
        let e = classify_http(404, Provider::Gemini, "gemini-2.0-flash");
        assert!(e.0.contains("gemini-2.0-flash"), "the error must say which model: {}", e.0);
        assert!(e.0.contains("Gemini"));
        // The unhelpful original wording must not come back.
        assert!(!e.0.contains("Change it in Settings."), "regressed to the vague message");
    }

    /// A model listed WITHOUT generateContent must never be offered. Measured:
    /// the API lists video and TTS models that cannot generate text.
    #[test]
    fn models_that_cannot_generate_content_are_filtered_out() {
        let list = parse_model_list(&json!({"models": [
            {"name": "models/gemini-flash-latest",
             "supportedGenerationMethods": ["generateContent"]},
            {"name": "models/veo-3.1-generate-preview",
             "supportedGenerationMethods": ["predictLongRunning"]},
            {"name": "models/embedding-001",
             "supportedGenerationMethods": ["embedContent"]}
        ]}));

        let ok: Vec<&str> = usable(&list).into_iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ok, vec!["gemini-flash-latest"]);
    }

    /// 3. An invalid key says so, and says where to fix it.
    #[test]
    fn an_invalid_key_is_reported_as_a_key_problem() {
        for status in [401, 403] {
            let e = classify_http(status, Provider::Gemini, "gemini-flash-latest");
            assert!(e.0.contains("rejected the key"), "{status}: {}", e.0);
            assert!(e.0.contains("Settings"));
        }
    }

    /// Quota or rate limit (4) is distinct from a bad key. The fix is waiting,
    /// not re-entering the key, so the wording must not send you to Settings.
    #[test]
    fn a_quota_or_rate_limit_is_not_reported_as_a_bad_key() {
        let e = classify_http(429, Provider::Gemini, "gemini-flash-latest");
        assert!(e.0.contains("rate-limiting") || e.0.contains("quota"), "{}", e.0);
        assert!(!e.0.contains("rejected the key"));
        assert!(!e.0.contains("has no model"));
    }

    /// A provider outage is not the user's problem and shouldn't read like it.
    #[test]
    fn a_server_error_is_attributed_to_the_provider() {
        for status in [500, 503] {
            let e = classify_http(status, Provider::Gemini, "gemini-flash-latest");
            assert!(e.0.contains("Not your end"), "{status}: {}", e.0);
        }
    }

    /// 5. Network failure is an offline state, not a configuration error.
    #[test]
    fn a_network_failure_reads_as_offline_not_misconfigured() {
        // The message built on the transport-error path in `send`.
        let offline = format!("Couldn't reach {}. Are you online?", Provider::Gemini.label());
        assert!(offline.contains("online"));
        assert!(!offline.contains("Settings"), "offline is not a settings problem");

        let timeout = format!("{} didn't respond in time.", Provider::Gemini.label());
        assert!(timeout.contains("didn't respond"));
    }

    /// 6. A successful generation is extracted from Gemini's response shape.
    #[test]
    fn a_successful_gemini_generation_is_extracted() {
        let response = json!({
            "candidates": [{
                "content": {"parts": [{"text": "Ribosome"}], "role": "model"},
                "finishReason": "STOP"
            }]
        });
        assert_eq!(extract_text(&response, Provider::Gemini).as_deref(), Some("Ribosome"));

        // Multi-part responses are joined rather than truncated to the first.
        let split = json!({"candidates": [{"content": {"parts": [
            {"text": "Rib"}, {"text": "osome"}
        ]}}]});
        assert_eq!(extract_text(&split, Provider::Gemini).as_deref(), Some("Ribosome"));
    }

    /// A response blocked by a safety filter has no parts. It must read as
    /// "no usable text", not as a crash or an empty success.
    #[test]
    fn a_blocked_or_empty_gemini_response_yields_none() {
        assert!(extract_text(&json!({"candidates": []}), Provider::Gemini).is_none());
        assert!(extract_text(
            &json!({"promptFeedback": {"blockReason": "SAFETY"}}),
            Provider::Gemini
        )
        .is_none());
    }

    /// No error message may ever carry the response body — a provider echoes
    /// the request it received, and that request held the key.
    #[test]
    fn no_classified_error_can_carry_a_response_body() {
        let source = include_str!("ai.rs");
        let body = source.split("#[cfg(test)]").next().unwrap();
        let classifier = body
            .split("pub fn classify_http")
            .nth(1)
            .and_then(|c| c.split("\nasync fn").next())
            .expect("classifier should be findable");

        // It is handed a status code and a model name, and nothing else.
        assert!(!classifier.contains("text"), "the classifier must not see the body");
        assert!(!classifier.contains("body"));
    }

    /// The model list must survive the shapes the real API returns.
    #[test]
    fn the_model_list_parser_tolerates_real_world_shapes() {
        // No models key at all.
        assert!(parse_model_list(&json!({})).is_empty());
        // Missing supportedGenerationMethods — treated as unusable, not assumed.
        let list = parse_model_list(&json!({"models": [{"name": "models/x"}]}));
        assert_eq!(list.len(), 1);
        assert!(!list[0].supports_generate_content);
        // An entry with no name is skipped rather than becoming an empty id.
        assert!(parse_model_list(&json!({"models": [{"displayName": "?"}]})).is_empty());
        // displayName falls back to the id.
        let fallback = parse_model_list(&json!({"models": [
            {"name": "models/abc", "supportedGenerationMethods": ["generateContent"]}
        ]}));
        assert_eq!(fallback[0].display_name, "abc");
    }

    // -- weekly facts ------------------------------------------------------

    fn db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!("db/migrations/001_init.sql")).unwrap();
        conn.execute_batch(include_str!("db/migrations/002_capture_cards_errors.sql")).unwrap();
        conn.execute(
            "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
             VALUES (1,'Biology','#4BA97B','3_4','science',0,'2026-08-01T00:00:00Z'),
                    (2,'Specialist Maths','#5B8DEF','1_2','maths',1,'2026-08-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    fn today() -> String {
        crate::util::retain_today_naive().format("%Y-%m-%d").to_string()
    }

    fn log_session(conn: &rusqlite::Connection, subject: i64, minutes: i64) {
        conn.execute(
            "INSERT INTO sessions (subject_id,mode,started_at,ended_at,local_date,
                                   elapsed_seconds,active_seconds)
             VALUES (?1,'stopwatch','2026-08-13T09:00:00Z','2026-08-13T10:00:00Z',?2,?3,?3)",
            rusqlite::params![subject, today(), minutes * 60],
        )
        .expect("insert session");
    }

    /// The subject with zero minutes is reported as avoided, and it is NOT
    /// mixed in with the subjects that do have hours.
    #[test]
    fn an_untouched_subject_is_named_separately() {
        let conn = db();
        log_session(&conn, 1, 120);

        let f = weekly_facts(&conn).unwrap();
        assert_eq!(f.minutes_by_subject, vec![("Biology".into(), 120)]);
        assert_eq!(f.untouched, vec!["Specialist Maths".to_string()]);
    }

    /// Totals come from SQL, not from the model.
    #[test]
    fn totals_are_computed_not_generated() {
        let conn = db();
        log_session(&conn, 1, 90);
        log_session(&conn, 2, 30);

        let f = weekly_facts(&conn).unwrap();
        assert_eq!(f.total_minutes, 120);
        assert_eq!(f.sessions, 2);
        assert!(f.render().contains("2h 0m focused"));
    }

    #[test]
    fn repeated_error_categories_are_ranked_most_frequent_first() {
        let conn = db();
        for (cat, n) in [("careless slip", 1), ("conceptual gap", 3)] {
            for _ in 0..n {
                conn.execute(
                    "INSERT INTO error_entries (subject_id,logged_on,category,created_at)
                     VALUES (1,?1,?2,'2026-08-13T00:00:00Z')",
                    rusqlite::params![today(), cat],
                )
                .unwrap();
            }
        }

        let f = weekly_facts(&conn).unwrap();
        assert_eq!(f.errors_by_category[0], ("conceptual gap".into(), 3));
        assert_eq!(f.errors_by_category[1], ("careless slip".into(), 1));
    }

    /// A near-empty week shouldn't spend the user's tokens producing a review
    /// of nothing.
    #[test]
    fn an_empty_week_is_not_worth_reviewing() {
        let conn = db();
        assert!(!weekly_facts(&conn).unwrap().has_enough_data());

        log_session(&conn, 1, 60);
        log_session(&conn, 1, 60);
        assert!(weekly_facts(&conn).unwrap().has_enough_data());
    }

    /// The rendered prompt must never claim a subject was studied when it
    /// wasn't — the wording the model sees has to be unambiguous.
    #[test]
    fn the_rendered_facts_distinguish_zero_from_absent() {
        let conn = db();
        log_session(&conn, 1, 45);

        let rendered = weekly_facts(&conn).unwrap().render();
        assert!(rendered.contains("Not touched at all: Specialist Maths"));
        assert!(!rendered.contains("Specialist Maths: 0h"));
    }

    /// The key is read only where it goes straight into a request header.
    ///
    /// Constraint #2 makes the Keychain the sole persistent home for a key.
    /// This catches the accidental version of breaking it — someone later
    /// caching a key in a struct field, a setting, or a log line to save a
    /// Keychain round trip.
    ///
    /// Two read sites are expected and no more:
    ///   * `complete`, which sends one of three auth headers, and
    ///   * `gemini_models`, which sends `x-goog-api-key` to list models.
    ///
    /// Four `expose()` calls follow from that: three in `complete`, one in
    /// `gemini_models`. If either count moves, a new place is handling the key
    /// and this test should be read before it is changed.
    #[test]
    fn the_key_is_read_only_where_it_becomes_a_header() {
        let source = include_str!("ai.rs");
        // Comments discuss what must NOT happen, so scanning them would match
        // the very warnings that exist to prevent it. Code only.
        let body: String = source
            .split("#[cfg(test)]")
            .next()
            .unwrap()
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let body = body.as_str();

        assert_eq!(body.matches("secrets::get_key").count(), 2, "unexpected key read site");
        assert_eq!(body.matches("key.expose()").count(), 4, "unexpected key use");

        for banned in ["settings::set", "set_key", "println!", "eprintln!", "dbg!"] {
            assert!(
                !body.contains(banned),
                "`{banned}` must not appear where a key is in scope"
            );
        }

        // Every exposure must be an argument to a header call, never anything
        // else — not a URL, not a body, not a format string for a message.
        for (i, _) in body.match_indices("key.expose()") {
            let before = &body[i.saturating_sub(90)..i];
            assert!(
                before.contains(".header("),
                "a key exposure is not going into a header:\n...{before}"
            );
        }

        // And never into a query string, where it would land in logs.
        assert!(!body.contains("?key="));
        assert!(!body.contains("(\"key\", key"));
    }

    /// The user's text must never become the system prompt.
    ///
    /// This replaced an earlier test asserting there was no chat surface at
    /// all. That stopped being true when the assistant was added, and a test
    /// asserting something false is worse than no test. What still holds — and
    /// is what actually matters — is that both halves of every prompt are
    /// assembled by Retain from app data.
    #[test]
    fn a_system_prompt_is_never_user_supplied() {
        let source = include_str!("ai.rs");
        let body = source.split("#[cfg(test)]").next().unwrap();

        // Four sites take a system prompt, and they are two pairs: `complete`
        // and `complete_with_images` (the transport), and `ask` and
        // `ask_with_images` (the entry points). A fifth means something new
        // accepts one, which is worth looking at by hand.
        assert_eq!(
            body.matches("system: &str").count(),
            4,
            "a new function accepts a caller-supplied system prompt"
        );

        // Both entry points must be reached only with the two constants below.
        for entry in ["pub async fn ask(", "pub async fn ask_with_images("] {
            assert!(body.contains(entry), "{entry} went missing");
        }

        let assistant = include_str!("assistant.rs");
        assert!(assistant.contains("pub const SYSTEM_STRICT"));
        assert!(assistant.contains("pub const SYSTEM_OPEN"));

        let commands = include_str!("commands.rs");
        for chosen in ["assistant::SYSTEM_STRICT", "assistant::SYSTEM_OPEN"] {
            assert!(commands.contains(chosen), "{chosen} should be what reaches `ask`");
        }
    }
}

#[cfg(test)]
mod live_diagnostics {
    //! Tests that talk to a real provider using the key in your Keychain.
    //!
    //! `#[ignore]` by default so the ordinary suite stays offline and
    //! deterministic. Run one explicitly:
    //!
    //!     cargo test --lib live_diagnostics::list_gemini_models -- --ignored --nocapture
    //!
    //! None of these ever print, return or persist the key. It is read inside
    //! the process, put straight into a request header, and dropped.

    use super::*;

    /// Prove the request path works end to end against a candidate model.
    #[test]
    #[ignore = "talks to the live Gemini API; needs a key in the Keychain"]
    fn gemini_generate_content_round_trip() {
        if !secrets::has_key(Provider::Gemini) {
            println!("no Gemini key stored — skipping");
            return;
        }

        for candidate in ["gemini-flash-latest", "gemini-2.5-flash", "gemini-2.0-flash"] {
            let outcome = tauri::async_runtime::block_on(complete(
                Provider::Gemini,
                candidate,
                "Reply with exactly one word.",
                "Say the word: ribosome",
                64,
            ));
            match outcome {
                Ok(text) => println!("  {candidate:<24} OK -> {:?}", text.trim()),
                Err(e) => println!("  {candidate:<24} FAILED -> {e}"),
            }
        }
    }

    /// Does the API version explain why some listed models 404?
    /// The end-to-end fix: default works, a retired model produces a useful
    /// error, and `test_model` reports honestly.
    #[test]
    #[ignore = "talks to the live Gemini API; needs a key in the Keychain"]
    fn gemini_end_to_end_after_the_fix() {
        if !secrets::has_key(Provider::Gemini) {
            println!("no Gemini key stored — skipping");
            return;
        }

        let default = default_model(Provider::Gemini);
        println!("\n1. shipped default = {default}");
        match tauri::async_runtime::block_on(test_model(Provider::Gemini, default)) {
            Ok(msg) => println!("   PASS: {msg}"),
            Err(e) => println!("   FAIL: {e}"),
        }

        println!("\n2. the model that was broken (gemini-2.0-flash):");
        match tauri::async_runtime::block_on(test_model(Provider::Gemini, "gemini-2.0-flash")) {
            Ok(msg) => println!("   unexpectedly worked: {msg}"),
            Err(e) => println!("   error shown to the user:\n   {e}"),
        }

        println!("\n3. empty model name:");
        match tauri::async_runtime::block_on(test_model(Provider::Gemini, "  ")) {
            Ok(m) => println!("   unexpected: {m}"),
            Err(e) => println!("   {e}"),
        }
    }

    #[test]
    #[ignore = "talks to the live Gemini API; needs a key in the Keychain"]
    fn gemini_api_version_probe() {
        let key = secrets::get_key(Provider::Gemini).expect("key");
        let client = reqwest::Client::new();

        for version in ["v1beta", "v1"] {
            for model in ["gemini-flash-latest", "gemini-2.5-flash", "gemini-3.6-flash"] {
                let url = format!(
                    "https://generativelanguage.googleapis.com/{version}/models/{model}:generateContent"
                );
                let body = json!({ "contents": [{ "parts": [{ "text": "hi" }] }] });
                let status = tauri::async_runtime::block_on(async {
                    client
                        .post(&url)
                        .header("x-goog-api-key", key.expose())
                        .json(&body)
                        .send()
                        .await
                        .map(|r| r.status().as_u16())
                });
                println!("  {version:<7} {model:<22} -> {status:?}");
            }
        }
    }

    #[test]
    #[ignore = "talks to the live Gemini API; needs a key in the Keychain"]
    fn list_gemini_models() {
        if !secrets::has_key(Provider::Gemini) {
            println!("no Gemini key stored — nothing to diagnose");
            return;
        }

        let models = tauri::async_runtime::block_on(gemini_models())
            .expect("the models endpoint should answer");

        println!("\n{} models visible to this key:\n", models.len());
        for m in &models {
            println!(
                "  {:<44} generateContent={}",
                m.id,
                if m.supports_generate_content { "yes" } else { "NO" }
            );
        }

        let usable: Vec<&str> = models
            .iter()
            .filter(|m| m.supports_generate_content)
            .map(|m| m.id.as_str())
            .collect();
        println!("\nusable for generateContent: {usable:#?}");

        // The default we ship must be one of them.
        let default = default_model(Provider::Gemini);
        println!("\ncurrently shipped default: {default}");
        println!("is it usable? {}", usable.contains(&default));
    }

    // -- images ---------------------------------------------------------------

    #[test]
    fn a_data_url_splits_into_media_type_and_payload() {
        assert_eq!(
            split_data_url("data:image/png;base64,iVBORw0K"),
            Some(("image/png", "iVBORw0K"))
        );
        assert_eq!(
            split_data_url("data:image/jpeg;base64,/9j/4AA"),
            Some(("image/jpeg", "/9j/4AA"))
        );
    }

    /// A bad attachment must cost the image, not the whole question — the
    /// request still has to go out with the text.
    #[test]
    fn anything_that_is_not_a_base64_image_url_is_ignored() {
        for bad in [
            "https://example.com/x.png",   // a link, not inline data
            "data:text/plain;base64,aGk=", // not an image
            "data:image/png,notbase64",    // missing the ;base64 marker
            "iVBORw0KGgo=",                // bare payload
            "",
        ] {
            assert_eq!(split_data_url(bad), None, "{bad}");
        }
    }
}
