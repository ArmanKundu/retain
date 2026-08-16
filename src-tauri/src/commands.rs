//! Every function the React side can call.
//!
//! `#[tauri::command]` marks a function as callable from TypeScript. Tauri
//! generates the plumbing: the frontend calls `invoke("start_timer", {...})`, the
//! arguments arrive as JSON, get deserialised into these parameter types, and the
//! return value goes back the same way.
//!
//! Two conventions used throughout:
//!
//!   * `State<'_, AppState>` is Tauri handing us the shared application state it
//!     is holding. We never construct it here.
//!   * Every command returns `CmdResult<T>`, so a failure surfaces in TypeScript
//!     as a rejected promise carrying a readable message.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::models::*;
use crate::timer::{self, ActiveTimer, SharedTimer};
use crate::tray::TrayHandles;
use crate::scheduler;
use crate::{ai, assessments, assistant, biology, blocks, capture, ics, ingest, library, resources, update, workspace, cards, errors, export, inbox, notifications, plan, provider, screen, secrets, settings, streak, subjects, tools};

/// Shared state, created in `lib.rs` and handed to every command.
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub timer: SharedTimer,
    pub tray: Mutex<Option<TrayHandles>>,
}

/// An error a Tauri command can return.
///
/// `anyhow::Error` carries useful context but cannot be serialised to JSON, so
/// this flattens it to a message the UI can display.
pub struct CommandError(String);

impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

/// Lets `?` convert any `anyhow::Error` into a `CommandError` automatically.
impl From<anyhow::Error> for CommandError {
    fn from(e: anyhow::Error) -> Self {
        CommandError(e.to_string())
    }
}

/// An AI feature that can't run is not an app failure — the UI turns this
/// message into an offer to add a key.
impl From<ai::AiUnavailable> for CommandError {
    fn from(e: ai::AiUnavailable) -> Self {
        CommandError(e.0)
    }
}

impl From<rusqlite::Error> for CommandError {
    fn from(e: rusqlite::Error) -> Self {
        CommandError(e.to_string())
    }
}

pub type CmdResult<T> = Result<T, CommandError>;

/// Lock order used everywhere: **timer first, then database.**
///
/// Two threads taking two locks in opposite orders is the classic deadlock, and
/// the ticker thread and the command handlers both need both locks. Keeping one
/// order throughout the file makes that impossible.
fn db(state: &AppState) -> std::sync::MutexGuard<'_, Connection> {
    state.db.lock().expect("database mutex poisoned")
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub onboarding_complete: bool,
    pub user_name: String,
    pub subjects: Vec<Subject>,
    pub focused_session_minutes: i64,
    pub pomodoro_work_minutes: i64,
    pub pomodoro_break_minutes: i64,
    pub theme: String,
    pub app_version: String,
}

/// Everything the UI needs on first paint, in one round trip.
#[tauri::command]
pub fn get_bootstrap(state: State<'_, AppState>) -> CmdResult<Bootstrap> {
    let conn = db(&state);
    Ok(Bootstrap {
        onboarding_complete: settings::onboarding_complete(&conn)?,
        user_name: settings::get(&conn, "user_name")?.unwrap_or_default(),
        subjects: subjects::list(&conn, false)?,
        focused_session_minutes: settings::focused_session_minutes(&conn)?,
        pomodoro_work_minutes: settings::get_i64(
            &conn,
            "pomodoro_work_minutes",
            settings::DEFAULT_POMODORO_WORK_MINUTES,
        )?,
        pomodoro_break_minutes: settings::get_i64(
            &conn,
            "pomodoro_break_minutes",
            settings::DEFAULT_POMODORO_BREAK_MINUTES,
        )?,
        theme: settings::get(&conn, "theme")?.unwrap_or_else(|| "dark".into()),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[tauri::command]
pub fn complete_onboarding(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    let conn = db(&state);
    settings::set(&conn, "user_name", name.trim())?;
    settings::set(&conn, "onboarding_complete", "1")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_setting(state: State<'_, AppState>, key: String) -> CmdResult<Option<String>> {
    Ok(settings::get(&db(&state), &key)?)
}

#[tauri::command]
pub fn set_setting(state: State<'_, AppState>, key: String, value: String) -> CmdResult<()> {
    settings::set(&db(&state), &key, &value)?;
    Ok(())
}

#[tauri::command]
pub fn set_rest_days(state: State<'_, AppState>, weekdays: Vec<i64>) -> CmdResult<()> {
    let conn = db(&state);
    conn.execute("DELETE FROM rest_days", [])?;
    for w in weekdays.iter().filter(|w| (0..=6).contains(*w)) {
        conn.execute("INSERT INTO rest_days (weekday) VALUES (?1)", [w])?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Subjects
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_subjects(state: State<'_, AppState>, include_archived: bool) -> CmdResult<Vec<Subject>> {
    Ok(subjects::list(&db(&state), include_archived)?)
}

#[tauri::command]
pub fn create_subject(state: State<'_, AppState>, input: SubjectInput) -> CmdResult<Subject> {
    Ok(subjects::create(&db(&state), input)?)
}

#[tauri::command]
pub fn update_subject(
    state: State<'_, AppState>,
    id: i64,
    input: SubjectInput,
) -> CmdResult<Subject> {
    Ok(subjects::update(&db(&state), id, input)?)
}

#[tauri::command]
pub fn archive_subject(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    subjects::archive(&db(&state), id)?;
    Ok(())
}

#[tauri::command]
pub fn unarchive_subject(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    subjects::unarchive(&db(&state), id)?;
    Ok(())
}

#[tauri::command]
pub fn reorder_subjects(state: State<'_, AppState>, ordered_ids: Vec<i64>) -> CmdResult<()> {
    subjects::reorder(&db(&state), ordered_ids)?;
    Ok(())
}

#[tauri::command]
pub fn set_weekly_goal(
    state: State<'_, AppState>,
    id: i64,
    minutes: Option<i64>,
) -> CmdResult<()> {
    subjects::set_weekly_goal(&db(&state), id, minutes)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Timer
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn start_timer(
    state: State<'_, AppState>,
    input: StartTimerInput,
) -> CmdResult<TimerSnapshot> {
    let mut slot = state.timer.lock().expect("timer mutex poisoned");
    if slot.is_some() {
        return Err(CommandError("A session is already running.".into()));
    }

    let conn = db(&state);

    let work = input
        .work_minutes
        .unwrap_or(settings::DEFAULT_POMODORO_WORK_MINUTES)
        .clamp(1, 180);
    let brk = input
        .break_minutes
        .unwrap_or(settings::DEFAULT_POMODORO_BREAK_MINUTES)
        .clamp(1, 60);

    let active = timer::start(&conn, input.subject_id, input.topic_id, input.mode, work, brk)?;
    let snapshot = active.snapshot(chrono::Utc::now());
    *slot = Some(active);

    Ok(snapshot)
}

#[tauri::command]
pub fn pause_timer(state: State<'_, AppState>) -> CmdResult<Option<TimerSnapshot>> {
    let mut slot = state.timer.lock().expect("timer mutex poisoned");
    let conn = db(&state);

    let Some(active) = slot.as_mut() else {
        return Ok(None);
    };
    timer::pause(&conn, active, chrono::Utc::now(), PauseReason::Manual)?;
    Ok(Some(active.snapshot(chrono::Utc::now())))
}

#[tauri::command]
pub fn resume_timer(state: State<'_, AppState>) -> CmdResult<Option<TimerSnapshot>> {
    let mut slot = state.timer.lock().expect("timer mutex poisoned");
    let conn = db(&state);

    let Some(active) = slot.as_mut() else {
        return Ok(None);
    };
    timer::resume(&conn, active, chrono::Utc::now())?;
    Ok(Some(active.snapshot(chrono::Utc::now())))
}

#[tauri::command]
pub fn stop_timer(state: State<'_, AppState>) -> CmdResult<Option<FinishedSession>> {
    let mut slot = state.timer.lock().expect("timer mutex poisoned");
    let conn = db(&state);

    let Some(active) = slot.as_mut() else {
        return Ok(None);
    };

    let threshold = settings::focused_session_minutes(&conn)?;
    let finished = timer::stop(&conn, active, threshold)?;

    // Clear the slot only after the database write succeeded, so a failure leaves
    // the session recoverable rather than orphaned.
    *slot = None;
    Ok(Some(finished))
}

/// The current timer, or `None`. Used on window focus to resync after the UI has
/// been closed and reopened.
#[tauri::command]
pub fn get_timer(state: State<'_, AppState>) -> CmdResult<Option<TimerSnapshot>> {
    let slot = state.timer.lock().expect("timer mutex poisoned");
    Ok(slot.as_ref().map(|t| t.snapshot(chrono::Utc::now())))
}

/// Attach the one-line note. Offered every time, always dismissible — passing
/// `None` or an empty string simply leaves it blank.
#[tauri::command]
pub fn set_session_note(
    state: State<'_, AppState>,
    session_id: i64,
    note: Option<String>,
) -> CmdResult<()> {
    let conn = db(&state);
    let cleaned = note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    conn.execute(
        "UPDATE sessions SET note = ?1 WHERE id = ?2",
        rusqlite::params![cleaned, session_id],
    )?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentSession {
    pub id: i64,
    pub subject_name: String,
    pub colour: String,
    pub topic_name: Option<String>,
    pub started_at: String,
    pub active_seconds: i64,
    pub pause_count: i64,
    pub idle_pause_count: i64,
    pub note: Option<String>,
}

#[tauri::command]
pub fn recent_sessions(state: State<'_, AppState>, limit: i64) -> CmdResult<Vec<RecentSession>> {
    let conn = db(&state);
    let mut stmt = conn.prepare(
        "SELECT s.id, subj.name, subj.colour, t.name, s.started_at,
                s.active_seconds, s.pause_count, s.idle_pause_count, s.note
           FROM sessions s
           JOIN subjects subj ON subj.id = s.subject_id
           LEFT JOIN topics t ON t.id = s.topic_id
          WHERE s.ended_at IS NOT NULL
          ORDER BY s.started_at DESC
          LIMIT ?1",
    )?;

    let rows = stmt.query_map([limit.clamp(1, 500)], |row| {
        Ok(RecentSession {
            id: row.get(0)?,
            subject_name: row.get(1)?,
            colour: row.get(2)?,
            topic_name: row.get(3)?,
            started_at: row.get(4)?,
            active_seconds: row.get(5)?,
            pause_count: row.get(6)?,
            idle_pause_count: row.get(7)?,
            note: row.get(8)?,
        })
    })?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Grid, streak, goals
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_grid(state: State<'_, AppState>, from: String, to: String) -> CmdResult<Vec<GridDay>> {
    Ok(streak::grid(&db(&state), &from, &to)?)
}

#[tauri::command]
pub fn get_streak(state: State<'_, AppState>) -> CmdResult<StreakSummary> {
    Ok(streak::summary(&db(&state))?)
}

#[tauri::command]
pub fn get_weekly_rings(state: State<'_, AppState>) -> CmdResult<Vec<WeeklyGoalRing>> {
    Ok(streak::weekly_rings(&db(&state))?)
}

// ---------------------------------------------------------------------------
// Flashcards
// ---------------------------------------------------------------------------

/// Parse a paste without writing anything, so the UI can show what it found —
/// delimiter, card count, cloze expansion, and which lines were skipped and why.
/// `quote_mode` selects the English card format (quote → source/context →
/// theme) instead of the standard Basic/Cloze one. It is a separate parser
/// because the third column means different things in each.
#[tauri::command]
pub fn preview_card_import(
    text: String,
    delimiter: Option<crate::anki_import::Delimiter>,
    quote_mode: Option<bool>,
) -> crate::anki_import::ImportPreview {
    if quote_mode.unwrap_or(false) {
        crate::anki_import::parse_quotes(&text, delimiter)
    } else {
        crate::anki_import::parse(&text, delimiter)
    }
}

#[tauri::command]
pub fn import_cards(
    state: State<'_, AppState>,
    subject_id: i64,
    topic_id: Option<i64>,
    text: String,
    delimiter: Option<crate::anki_import::Delimiter>,
    quote_mode: Option<bool>,
) -> CmdResult<cards::ImportResult> {
    let parsed = if quote_mode.unwrap_or(false) {
        crate::anki_import::parse_quotes(&text, delimiter)
    } else {
        crate::anki_import::parse(&text, delimiter)
    };
    Ok(cards::import(&db(&state), subject_id, topic_id, &parsed.cards)?)
}

#[tauri::command]
pub fn review_queue(
    state: State<'_, AppState>,
    subject_id: Option<i64>,
    limit: Option<i64>,
) -> CmdResult<Vec<cards::QueueItem>> {
    Ok(cards::queue(
        &db(&state),
        subject_id,
        limit.unwrap_or(200).clamp(1, 1000),
    )?)
}

#[tauri::command]
pub fn review_counts(
    state: State<'_, AppState>,
    subject_id: Option<i64>,
) -> CmdResult<cards::QueueCounts> {
    Ok(cards::counts(&db(&state), subject_id)?)
}

/// Rate a card.
///
/// `presented_at` comes from the UI — the moment the card was actually shown —
/// so the review log records genuine thinking time. Defaulting it to "now" would
/// make every review look instantaneous and quietly devalue the audit trail the
/// streak depends on.
#[tauri::command]
pub fn answer_card(
    state: State<'_, AppState>,
    card_id: i64,
    rating: crate::scheduler::Rating,
    presented_at: Option<String>,
) -> CmdResult<cards::AnswerResult> {
    let now = chrono::Utc::now();
    let presented = presented_at
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|d| d.with_timezone(&chrono::Utc))
        // Clamp: a client clock ahead of ours must not produce negative
        // thinking time, and one absurdly behind must not claim hours.
        .filter(|p| *p <= now && (now - *p) < chrono::Duration::hours(1))
        .unwrap_or(now);

    Ok(cards::answer(&db(&state), card_id, rating, presented, now)?)
}

/// Projected review load, so the debt being taken on is visible.
#[tauri::command]
pub fn review_forecast(
    state: State<'_, AppState>,
    days: Option<i64>,
) -> CmdResult<Vec<(String, i64)>> {
    Ok(cards::future_load(&db(&state), days.unwrap_or(30))?)
}

// ---------------------------------------------------------------------------
// Error log
//
// Note the shape of the blind re-attempt commands: `start_error_reattempt`
// returns a `BlindPrompt`, which has no field for the correct answer, and
// `reveal_error_answer` is the only way to obtain it — refusing until a blind
// answer has been committed.
// ---------------------------------------------------------------------------

/// The categories offered when logging an error against a subject.
///
/// Takes a subject id rather than a type so Biology 3/4 can be given its
/// course-specific categories on top of the generic Science ones.
#[tauri::command]
pub fn error_categories(state: State<'_, AppState>, subject_id: i64) -> CmdResult<Vec<String>> {
    Ok(errors::categories_for_subject(&db(&state), subject_id)?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandWord {
    pub word: String,
    pub meaning: String,
}

/// The command-word reference.
///
/// Plain-English study guidance written for this app — deliberately not
/// presented as a reproduction of any examination authority's glossary.
#[tauri::command]
pub fn command_words() -> Vec<CommandWord> {
    errors::COMMAND_WORDS
        .iter()
        .map(|(w, m)| CommandWord {
            word: w.to_string(),
            meaning: m.to_string(),
        })
        .collect()
}

#[tauri::command]
pub fn create_error_entry(
    state: State<'_, AppState>,
    input: errors::ErrorEntryInput,
) -> CmdResult<i64> {
    Ok(errors::create(&db(&state), input)?)
}

#[tauri::command]
pub fn list_error_entries(
    state: State<'_, AppState>,
    filter: errors::EntryFilter,
) -> CmdResult<Vec<errors::ErrorEntry>> {
    Ok(errors::list(&db(&state), &filter)?)
}

#[tauri::command]
pub fn delete_error_entry(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    errors::delete(&db(&state), id)?;
    Ok(())
}

#[tauri::command]
pub fn error_entry_image(state: State<'_, AppState>, id: i64) -> CmdResult<Option<String>> {
    Ok(errors::image(&db(&state), id)?)
}

#[tauri::command]
pub fn due_error_reattempts(
    state: State<'_, AppState>,
    subject_id: Option<i64>,
) -> CmdResult<Vec<i64>> {
    Ok(errors::due_reattempts(&db(&state), subject_id)?)
}

#[tauri::command]
pub fn start_error_reattempt(
    state: State<'_, AppState>,
    entry_id: i64,
) -> CmdResult<errors::BlindPrompt> {
    Ok(errors::start_reattempt(&db(&state), entry_id)?)
}

#[tauri::command]
pub fn commit_error_reattempt(
    state: State<'_, AppState>,
    reattempt_id: i64,
    blind_answer: String,
) -> CmdResult<()> {
    errors::commit_reattempt(&db(&state), reattempt_id, &blind_answer)?;
    Ok(())
}

/// The ONLY path to the mark scheme. Errors if nothing has been committed.
#[tauri::command]
pub fn reveal_error_answer(
    state: State<'_, AppState>,
    reattempt_id: i64,
) -> CmdResult<Option<String>> {
    Ok(errors::reveal_reattempt(&db(&state), reattempt_id)?)
}

#[tauri::command]
pub fn assess_error_reattempt(
    state: State<'_, AppState>,
    reattempt_id: i64,
    assessment: errors::SelfAssessment,
    marks_awarded: Option<i64>,
) -> CmdResult<bool> {
    Ok(errors::assess_reattempt(
        &db(&state),
        reattempt_id,
        assessment,
        marks_awarded,
    )?)
}

#[tauri::command]
pub fn recurring_errors(
    state: State<'_, AppState>,
    subject_id: Option<i64>,
    days: Option<i64>,
) -> CmdResult<Vec<errors::CategoryCount>> {
    Ok(errors::recurring(&db(&state), subject_id, days.unwrap_or(30))?)
}

// ---------------------------------------------------------------------------
// API keys — Keychain only. Note what is NOT here: any command that returns a key.
// ---------------------------------------------------------------------------

/// Check a pasted key with its provider, and store it only if the provider
/// accepts it.
///
/// This is the normal path for adding a key. The key reaching this function is
/// held in memory for the duration of one HTTPS request and is written to the
/// Keychain only on success — a rejected key is never persisted anywhere.
#[tauri::command]
pub async fn secret_verify_and_store(
    provider: secrets::Provider,
    key: String,
) -> CmdResult<provider::KeyCheck> {
    let outcome = provider::check(provider, &key).await;

    // Only a definite yes gets stored. `Unreachable` deliberately does not fall
    // through to storing: the UI offers that as an explicit choice instead, so
    // an unverified key is always something the user opted into.
    if matches!(outcome, provider::KeyCheck::Valid { .. }) {
        secrets::set_key(provider, &key)?;
    }

    Ok(outcome)
}

/// Store a key without checking it.
///
/// The escape hatch for being offline. The UI only offers this after a check has
/// come back `Unreachable`, never after a rejection — the app has to be fully
/// usable without a network, and refusing to save a key because the wifi is down
/// would be the wrong failure.
#[tauri::command]
pub fn secret_store_unverified(provider: secrets::Provider, key: String) -> CmdResult<()> {
    secrets::set_key(provider, &key)?;
    Ok(())
}

/// Re-check a key that's already in the Keychain — the "test connection" button.
///
/// The key is read from the Keychain inside Rust and never crosses back to the
/// frontend; only the verdict does.
#[tauri::command]
pub async fn secret_test_stored(provider: secrets::Provider) -> CmdResult<provider::KeyCheck> {
    let Ok(stored) = secrets::get_key(provider) else {
        return Ok(provider::KeyCheck::Invalid {
            message: "There's no key saved for this provider.".into(),
        });
    };

    Ok(provider::check(provider, stored.expose()).await)
}

#[tauri::command]
pub fn secret_set(provider: secrets::Provider, key: String) -> CmdResult<()> {
    secrets::set_key(provider, &key)?;
    Ok(())
}

#[tauri::command]
pub fn secret_has(provider: secrets::Provider) -> bool {
    secrets::has_key(provider)
}

#[tauri::command]
pub fn secret_delete(provider: secrets::Provider) -> CmdResult<()> {
    secrets::delete_key(provider)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Export / import
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn export_json(state: State<'_, AppState>) -> CmdResult<String> {
    let conn = db(&state);
    let document = export::export_all(&conn)?;
    serde_json::to_string_pretty(&document).map_err(|e| CommandError(e.to_string()))
}

/// Write the export straight to ~/Downloads and return the path.
///
/// Doing the file write in Rust avoids pulling in the dialog and filesystem
/// plugins just to save one file. Import goes the other way — a plain
/// `<input type="file">` in the UI, which needs no plugin either.
#[tauri::command]
pub fn export_to_file(app: AppHandle, state: State<'_, AppState>) -> CmdResult<String> {
    let json = {
        let conn = db(&state);
        let document = export::export_all(&conn)?;
        serde_json::to_string_pretty(&document).map_err(|e| CommandError(e.to_string()))?
    };

    let dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .map_err(|e| CommandError(format!("Couldn't find a folder to save into: {e}")))?;

    let name = format!(
        "retain-export-{}.json",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let path = dir.join(name);

    std::fs::write(&path, json).map_err(|e| CommandError(format!("Couldn't write the file: {e}")))?;

    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn import_json(
    app: AppHandle,
    state: State<'_, AppState>,
    contents: String,
) -> CmdResult<export::ImportReport> {
    let document: serde_json::Value =
        serde_json::from_str(&contents).map_err(|e| CommandError(format!("Not valid JSON: {e}")))?;

    let mut conn = db(&state);

    // Snapshot first. An import replaces everything, so the pre-import state has
    // to be recoverable before a single row is touched.
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = crate::db::snapshot(&conn, &dir);
    }

    Ok(export::import_all(&mut conn, &document)?)
}

// ---------------------------------------------------------------------------
// Called from the tray menu, which has an AppHandle rather than State
// ---------------------------------------------------------------------------

pub fn tray_toggle_pause(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut slot = state.timer.lock().expect("timer mutex poisoned");
    let conn = state.db.lock().expect("database mutex poisoned");

    if let Some(active) = slot.as_mut() {
        let now = chrono::Utc::now();
        let _ = if active.paused_reason.is_some() {
            timer::resume(&conn, active, now)
        } else {
            timer::pause(&conn, active, now, PauseReason::Manual)
        };
        let _ = app.emit("timer:changed", active.snapshot(now));
    }
}

pub fn tray_stop(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut slot = state.timer.lock().expect("timer mutex poisoned");
    let conn = state.db.lock().expect("database mutex poisoned");

    if let Some(active) = slot.as_mut() {
        let threshold = settings::focused_session_minutes(&conn).unwrap_or(20);
        if let Ok(finished) = timer::stop(&conn, active, threshold) {
            *slot = None;
            // Bring the window up so the note prompt is actually seen — stopping
            // from the menu bar usually means the window is buried.
            crate::tray::show_main_window(app);
            let _ = app.emit("timer:finished", finished);
        }
    }
}

/// Helper for the ticker thread in lib.rs.
pub fn snapshot_of(slot: &Option<ActiveTimer>) -> Option<TimerSnapshot> {
    slot.as_ref().map(|t| t.snapshot(chrono::Utc::now()))
}

// ---------------------------------------------------------------------------
// Quick capture and the inbox
// ---------------------------------------------------------------------------

/// Parse a line WITHOUT storing it, for the live hint in the capture bar.
///
/// Separate from `save_capture` so typing never writes rows: the hint updates on
/// every keystroke, and an inbox full of half-typed fragments would be worse
/// than no hint at all.
#[tauri::command]
pub fn preview_capture(
    state: State<'_, AppState>,
    text: String,
) -> CmdResult<crate::capture::ParsedCapture> {
    let conn = db(&state);
    let mut stmt = conn.prepare("SELECT id, name FROM subjects WHERE archived = 0")?;
    let rows = stmt.query_map([], |r| {
        Ok(crate::capture::SubjectHint { id: r.get(0)?, name: r.get(1)? })
    })?;
    let mut hints = Vec::new();
    for r in rows {
        hints.push(r?);
    }
    Ok(crate::capture::parse_now(&text, &hints))
}

/// Store a captured line and return what the parser made of it.
///
/// Runs entirely offline — no key, no network. See `capture.rs`.
#[tauri::command]
pub fn save_capture(
    state: State<'_, AppState>,
    text: String,
) -> CmdResult<crate::capture::ParsedCapture> {
    Ok(inbox::save(&db(&state), &text)?)
}

/// Hide the capture window. Called the moment a capture is saved so the
/// keystroke-to-gone path is: type → Enter → window disappears.
#[tauri::command]
pub fn hide_capture_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window("capture") {
        let _ = w.hide();
    }
}

#[tauri::command]
pub fn list_inbox(state: State<'_, AppState>) -> CmdResult<Vec<inbox::Capture>> {
    Ok(inbox::list_untriaged(&db(&state))?)
}

#[tauri::command]
pub fn inbox_count(state: State<'_, AppState>) -> CmdResult<i64> {
    Ok(inbox::untriaged_count(&db(&state))?)
}

#[tauri::command]
pub fn triage_capture_to_task(
    state: State<'_, AppState>,
    capture_id: i64,
    title: String,
    subject_id: Option<i64>,
    due_on: Option<String>,
) -> CmdResult<i64> {
    Ok(inbox::triage_to_task(
        &db(&state),
        capture_id,
        &title,
        subject_id,
        due_on.as_deref(),
    )?)
}

#[tauri::command]
pub fn triage_capture(
    state: State<'_, AppState>,
    capture_id: i64,
    destination: String,
) -> CmdResult<()> {
    inbox::triage_to(&db(&state), capture_id, &destination)?;
    Ok(())
}

#[tauri::command]
pub fn list_tasks(state: State<'_, AppState>, include_done: bool) -> CmdResult<Vec<inbox::Task>> {
    Ok(inbox::list_tasks(&db(&state), include_done)?)
}

#[tauri::command]
pub fn set_task_done(state: State<'_, AppState>, id: i64, done: bool) -> CmdResult<()> {
    inbox::set_task_done(&db(&state), id, done)?;
    Ok(())
}

#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    inbox::delete_task(&db(&state), id)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Assessments and retrospective revision
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn create_assessment(
    state: State<'_, AppState>,
    input: assessments::AssessmentInput,
) -> CmdResult<i64> {
    Ok(assessments::create(&db(&state), input)?)
}

#[tauri::command]
pub fn list_assessments(
    state: State<'_, AppState>,
    include_past: bool,
) -> CmdResult<Vec<assessments::Assessment>> {
    Ok(assessments::list(&db(&state), include_past)?)
}

#[tauri::command]
pub fn delete_assessment(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    assessments::delete(&db(&state), id)?;
    Ok(())
}

/// Record a self-test on a topic. This is the entire retrospective model —
/// a log of what was actually revised, never a plan for what will be.
#[tauri::command]
pub fn log_topic_review(
    state: State<'_, AppState>,
    topic_id: i64,
    confidence: i64,
    note: Option<String>,
) -> CmdResult<()> {
    assessments::log_topic_review(&db(&state), topic_id, confidence, note.as_deref())?;
    Ok(())
}

/// Rank topics by what deserves attention now. Recomputed every call, stored
/// nowhere — that's what keeps it retrospective rather than a timetable.
#[tauri::command]
pub fn surface_topics(
    state: State<'_, AppState>,
    subject_id: Option<i64>,
    limit: Option<i64>,
) -> CmdResult<Vec<assessments::TopicStatus>> {
    Ok(assessments::surface(&db(&state), subject_id, limit.unwrap_or(25))?)
}

/// Create a topic. Until the VCAA tree lands, this is how topics exist at all.
#[tauri::command]
pub fn create_topic(
    state: State<'_, AppState>,
    subject_id: i64,
    name: String,
) -> CmdResult<i64> {
    let conn = db(&state);
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError("A topic needs a name.".into()));
    }
    conn.execute(
        "INSERT INTO topics (subject_id, name, sort_order)
         VALUES (?1, ?2, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM topics WHERE subject_id = ?1))",
        rusqlite::params![subject_id, name],
    )?;
    Ok(conn.last_insert_rowid())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicRow {
    pub id: i64,
    pub subject_id: i64,
    pub name: String,
}

#[tauri::command]
pub fn list_topics(state: State<'_, AppState>, subject_id: Option<i64>) -> CmdResult<Vec<TopicRow>> {
    let conn = db(&state);
    let mut stmt = conn.prepare(
        "SELECT id, subject_id, name FROM topics
          WHERE (?1 IS NULL OR subject_id = ?1) ORDER BY subject_id, sort_order, id",
    )?;
    let rows = stmt.query_map(rusqlite::params![subject_id], |r| {
        Ok(TopicRow { id: r.get(0)?, subject_id: r.get(1)?, name: r.get(2)? })
    })?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

#[tauri::command]
pub fn delete_topic(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    db(&state).execute("DELETE FROM topics WHERE id = ?1", [id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn notification_settings(
    state: State<'_, AppState>,
) -> CmdResult<notifications::NotificationSettings> {
    Ok(notifications::load_settings(&db(&state))?)
}

#[tauri::command]
pub fn set_notification_settings(
    state: State<'_, AppState>,
    settings: notifications::NotificationSettings,
) -> CmdResult<()> {
    let conn = db(&state);
    let b = |v: bool| if v { "1" } else { "0" };
    settings_set(&conn, "notify_enabled", b(settings.enabled))?;
    settings_set(&conn, "notify_quiet_from", &settings.quiet_from_hour.to_string())?;
    settings_set(&conn, "notify_quiet_to", &settings.quiet_to_hour.to_string())?;
    settings_set(&conn, "notify_daily_cap", &settings.daily_cap.to_string())?;
    settings_set(&conn, "notify_reviews", b(settings.reviews))?;
    settings_set(&conn, "notify_assessments", b(settings.assessments))?;
    settings_set(&conn, "notify_topic_decay", b(settings.topic_decay))?;
    settings_set(&conn, "notify_streak", b(settings.streak))?;
    Ok(())
}

fn settings_set(conn: &Connection, key: &str, value: &str) -> CmdResult<()> {
    settings::set(conn, key, value)?;
    Ok(())
}

#[tauri::command]
pub fn notifications_sent_today(state: State<'_, AppState>) -> CmdResult<i64> {
    Ok(notifications::sent_today(&db(&state))?)
}

/// What the rules would say right now, WITHOUT sending or recording anything.
/// Lets Settings show exactly what you'd receive before you enable a category.
#[tauri::command]
pub fn preview_notifications(
    state: State<'_, AppState>,
) -> CmdResult<Vec<notifications::Candidate>> {
    Ok(notifications::evaluate(&db(&state), chrono::Utc::now())?)
}

// ---------------------------------------------------------------------------
// AI — optional, never required
// ---------------------------------------------------------------------------
//
// Every command here can fail with "no key configured", and the UI renders that
// as an offer to set one up rather than as an error. Nothing else in the app
// calls into these; Retain is fully functional with no key at all.
//
// A note on locking: `db()` returns a MutexGuard, which cannot be held across an
// `.await`. Each command therefore reads everything it needs into owned values,
// drops the guard, makes the network call, and only then re-locks to write.

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    /// None means no key anywhere — the UI shows the "add a key" state.
    pub provider: Option<secrets::Provider>,
    pub model: String,
    /// Which providers have a key, so Settings can offer a choice.
    pub available: Vec<secrets::Provider>,
}

#[tauri::command]
pub fn ai_status(state: State<'_, AppState>) -> CmdResult<AiStatus> {
    let conn = db(&state);
    let provider = ai::active_provider(&conn);

    Ok(AiStatus {
        provider,
        model: provider.map(|p| ai::model_name(&conn, p)).unwrap_or_default(),
        available: [
            secrets::Provider::Anthropic,
            secrets::Provider::OpenAi,
            secrets::Provider::Gemini,
            secrets::Provider::OpenRouter,
        ]
        .into_iter()
        .filter(|p| secrets::has_key(*p))
        .collect(),
    })
}

#[tauri::command]
pub fn ai_set_provider(state: State<'_, AppState>, provider: secrets::Provider) -> CmdResult<()> {
    settings_set(&db(&state), "ai_provider", ai::slug(provider))
}

#[tauri::command]
pub fn ai_set_model(
    state: State<'_, AppState>,
    provider: secrets::Provider,
    model: String,
) -> CmdResult<()> {
    settings_set(&db(&state), ai::model_setting_key(provider), model.trim())
}

/// Feature 1 — a messy captured note becomes a structured task.
///
/// Returns a *suggestion*. Nothing is written; the Inbox screen shows it in an
/// editable form and the user confirms.
#[tauri::command]
pub async fn ai_task_from_note(
    state: State<'_, AppState>,
    note: String,
) -> CmdResult<ai::TaskSuggestion> {
    let (client, subjects, today) = {
        let conn = db(&state);
        let subjects: Vec<String> = subjects::list(&conn, false)?.into_iter().map(|s| s.name).collect();
        (ai::Ai::from(&conn)?, subjects, crate::util::retain_today())
    };

    Ok(client.task_from_note(&note, &subjects, &today).await?)
}

/// Feature 2 — pasted notes become cards.
///
/// Also returns suggestions only. The Import screen shows every generated card
/// for review, and nothing reaches the deck until the user accepts it — an
/// unreviewed generated card is exactly the kind of card that wastes reviews
/// for months.
#[tauri::command]
pub async fn ai_cards_from_notes(
    state: State<'_, AppState>,
    subject_id: i64,
    notes: String,
    count: usize,
) -> CmdResult<Vec<ai::CardSuggestion>> {
    let (client, subject) = {
        let conn = db(&state);
        let subject = subjects::get(&conn, subject_id)?.name;
        (ai::Ai::from(&conn)?, subject)
    };

    Ok(client.cards_from_notes(&notes, &subject, count.clamp(1, 40)).await?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyReview {
    pub facts: ai::WeeklyFacts,
    /// None when there wasn't enough logged to be worth reviewing.
    pub prose: Option<String>,
}

/// Feature 3 — the weekly review.
///
/// The facts are always returned, key or no key, so the screen is useful
/// offline. Only the written summary needs a provider.
#[tauri::command]
pub async fn ai_weekly_review(state: State<'_, AppState>) -> CmdResult<WeeklyReview> {
    let (facts, client) = {
        let conn = db(&state);
        (ai::weekly_facts(&conn)?, ai::Ai::from(&conn))
    };

    if !facts.has_enough_data() {
        return Ok(WeeklyReview { facts, prose: None });
    }

    let prose = match client {
        Ok(c) => Some(c.weekly_review(&facts.render()).await?),
        Err(e) => return Err(CommandError(e.to_string())),
    };

    if let Some(text) = &prose {
        let conn = db(&state);
        let _ = library::save(
            &conn,
            None,
            library::ItemKind::WeeklyReview,
            &format!("Week of {}", facts.from),
            None,
            text,
            Some(&client_model(&conn)),
            chrono::Utc::now(),
        );
    }

    Ok(WeeklyReview { facts, prose })
}

/// The facts alone — no network, no key. Backs the offline view of the screen.
#[tauri::command]
pub fn weekly_facts(state: State<'_, AppState>) -> CmdResult<ai::WeeklyFacts> {
    Ok(ai::weekly_facts(&db(&state))?)
}

/// Feature 4 — a VCAA-style practice question for a Biology 3/4 dot point.
#[tauri::command]
pub async fn ai_practice_question(
    state: State<'_, AppState>,
    dot_point: String,
    marks: i64,
    subject_id: Option<i64>,
) -> CmdResult<GroundedText> {
    // Retrieve first, then drop the lock — the network call can't hold it.
    let (client, model, context, excerpts) = {
        let conn = db(&state);
        let client = ai::Ai::from(&conn)?;
        let found = resources::by_authority(
            resources::search(&conn, &dot_point, subject_id, 4).unwrap_or_default(),
        );
        let block = resources::context_block(&found);
        (client, client_model(&conn), block, found)
    };

    let body = client
        .practice_question(&dot_point, marks.clamp(1, 10), context.as_deref())
        .await?;

    // Archived automatically. A practice question you can't find again is one
    // you'll ask for twice.
    {
        let conn = db(&state);
        let _ = library::save(
            &conn,
            subject_id,
            library::ItemKind::PracticeQuestion,
            &format!("Practice: {}", truncate(&dot_point, 60)),
            Some(&dot_point),
            &body,
            Some(&model),
            chrono::Utc::now(),
        );
    }

    Ok(GroundedText { body, sources: excerpts })
}

/// Text produced by the AI, plus the excerpts of your own material that shaped
/// it. Showing the sources is what makes a bad retrieval visible rather than
/// silently wrong.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundedText {
    pub body: String,
    pub sources: Vec<resources::Excerpt>,
}

fn truncate(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= n {
        return t.to_string();
    }
    format!("{}…", t.chars().take(n).collect::<String>().trim_end())
}

/// The model currently in use, for attributing a saved item.
fn client_model(conn: &Connection) -> String {
    ai::active_provider(conn)
        .map(|p| ai::model_name(conn, p))
        .unwrap_or_default()
}

/// Study notes on a topic, grounded in your own material where it covers it.
#[tauri::command]
pub async fn ai_notes(
    state: State<'_, AppState>,
    topic: String,
    subject_id: Option<i64>,
) -> CmdResult<GroundedText> {
    let (client, model, context, excerpts) = {
        let conn = db(&state);
        let client = ai::Ai::from(&conn)?;
        let found = resources::by_authority(
            resources::search(&conn, &topic, subject_id, 5).unwrap_or_default(),
        );
        let block = resources::context_block(&found);
        (client, client_model(&conn), block, found)
    };

    let body = client.notes(&topic, context.as_deref()).await?;

    {
        let conn = db(&state);
        let _ = library::save(
            &conn,
            subject_id,
            library::ItemKind::Notes,
            &truncate(&topic, 70),
            Some(&topic),
            &body,
            Some(&model),
            chrono::Utc::now(),
        );
    }

    Ok(GroundedText { body, sources: excerpts })
}

/// Feature 5 — suggest an error category.
///
/// Suggestion only. The returned value pre-selects a radio button in the error
/// form; it is never written to `error_entries` without the user submitting.
/// `None` means the model didn't produce anything in the allowed list, which is
/// treated as "no suggestion" rather than as a failure.
#[tauri::command]
pub async fn ai_suggest_category(
    state: State<'_, AppState>,
    subject_id: i64,
    question: String,
    my_answer: String,
    correct_answer: String,
) -> CmdResult<Option<String>> {
    // The same list the form offers. Suggesting a category the user can't then
    // select would be worse than suggesting nothing.
    let (client, allowed) = {
        let conn = db(&state);
        (ai::Ai::from(&conn)?, errors::categories_for_subject(&conn, subject_id)?)
    };

    let refs: Vec<&str> = allowed.iter().map(String::as_str).collect();
    Ok(client
        .suggest_category(&question, &my_answer, &correct_answer, &refs)
        .await?)
}

// ---------------------------------------------------------------------------
// Calendar (ICS subscription)
// ---------------------------------------------------------------------------
//
// Compass publishes an ICS subscription URL under its calendar settings; that
// URL is the entire integration. Nothing here logs in, scrapes HTML, or talks
// to an unofficial endpoint, and there is no code path that could.
//
// Calendar failure is never allowed to affect anything else: a sync error is
// recorded in settings and shown on the calendar screen, and every other part
// of the app carries on unaware.

#[tauri::command]
pub fn calendar_status(state: State<'_, AppState>) -> CmdResult<ics::SyncStatus> {
    Ok(ics::status(&db(&state))?)
}

#[tauri::command]
pub fn set_calendar_settings(
    state: State<'_, AppState>,
    enabled: bool,
    url: String,
) -> CmdResult<ics::SyncStatus> {
    let conn = db(&state);

    // Validate before storing, so a bad address is rejected while the user is
    // still looking at the field rather than at the next sync.
    let cleaned = if url.trim().is_empty() {
        String::new()
    } else {
        ics::normalise_url(&url)?
    };

    settings::set(&conn, "ics_enabled", if enabled { "1" } else { "0" })?;
    settings::set(&conn, "ics_url", &cleaned)?;
    Ok(ics::status(&conn)?)
}

/// Fetch, parse and store the feed.
///
/// Returns `Ok` with the error recorded in the status rather than `Err` on a
/// network failure: an unreachable calendar is an expected state for an
/// offline-first app, not an exception. Only a programming-level failure
/// propagates.
#[tauri::command]
pub async fn sync_calendar(state: State<'_, AppState>) -> CmdResult<ics::SyncStatus> {
    // The DB guard can't be held across an await, so read the URL, drop it,
    // do the network work, then re-lock to write.
    let (url, enabled) = {
        let conn = db(&state);
        (
            settings::get(&conn, "ics_url")?.unwrap_or_default(),
            settings::get_bool(&conn, "ics_enabled", false)?,
        )
    };

    if url.trim().is_empty() {
        return Err(CommandError("Add a calendar address first.".into()));
    }
    if !enabled {
        return Err(CommandError("Turn the calendar on first.".into()));
    }

    let outcome: anyhow::Result<Vec<ics::CalendarEvent>> = async {
        let body = ics::fetch(&url).await?;
        ics::parse_calendar(&body, ics::local_tz(), chrono::Utc::now())
    }
    .await;

    let mut guard = state.db.lock().expect("database mutex poisoned");

    match outcome {
        Ok(events) => {
            ics::store(&mut guard, &events)?;
            settings::set(&guard, "ics_last_sync", &crate::util::rfc3339(chrono::Utc::now()))?;
            settings::set(&guard, "ics_last_error", "")?;
        }
        Err(e) => {
            // The previously-synced events are deliberately left in place. A
            // timetable you fetched yesterday is far more useful than an empty
            // screen because the wifi dropped.
            settings::set(&guard, "ics_last_error", &e.to_string())?;
        }
    }

    Ok(ics::status(&guard)?)
}

#[tauri::command]
pub fn upcoming_events(
    state: State<'_, AppState>,
    days: i64,
    limit: i64,
) -> CmdResult<Vec<ics::CalendarEvent>> {
    Ok(ics::upcoming(&db(&state), days.clamp(1, 60), limit.clamp(1, 200))?)
}

#[tauri::command]
pub fn clear_calendar(state: State<'_, AppState>) -> CmdResult<ics::SyncStatus> {
    let conn = db(&state);
    conn.execute("DELETE FROM calendar_events", [])?;
    settings::set(&conn, "ics_last_sync", "")?;
    settings::set(&conn, "ics_last_error", "")?;
    Ok(ics::status(&conn)?)
}

// ---------------------------------------------------------------------------
// Biology 3/4
// ---------------------------------------------------------------------------
//
// No study-design content is shipped in the binary — see the module docs on
// `biology`. The topic tree is a structure the user fills from their own copy
// of the study design, via `import_topic_outline`.

#[tauri::command]
pub fn topic_tree(state: State<'_, AppState>, subject_id: i64) -> CmdResult<Vec<biology::TopicNode>> {
    Ok(biology::tree(&db(&state), subject_id)?)
}

/// Preview a pasted outline without writing anything.
#[tauri::command]
pub fn preview_topic_outline(text: String) -> Vec<biology::OutlineRow> {
    biology::parse_outline(&text)
}

/// Replace a subject's topics with a pasted outline.
///
/// Destructive by design and labelled as such in the UI: cards and error
/// entries survive (`ON DELETE SET NULL`) but lose their topic link.
#[tauri::command]
pub fn import_topic_outline(
    state: State<'_, AppState>,
    subject_id: i64,
    text: String,
) -> CmdResult<usize> {
    let rows = biology::parse_outline(&text);
    let mut conn = state.db.lock().expect("database mutex poisoned");
    Ok(biology::import_outline(&mut conn, subject_id, &rows)?)
}

#[tauri::command]
pub fn terminology_summary(
    state: State<'_, AppState>,
    subject_id: i64,
) -> CmdResult<biology::DeckSummary> {
    Ok(biology::terminology_summary(&db(&state), subject_id)?)
}

// --- exam simulation ---

/// The running exam, if any. Everything is derived from the stored start
/// instant, so quitting mid-exam and reopening resumes rather than restarts.
#[tauri::command]
pub fn exam_state(state: State<'_, AppState>) -> CmdResult<Option<biology::ExamState>> {
    let conn = db(&state);
    match biology::load(&conn)? {
        Some(run) => Ok(Some(biology::state_at(&run, chrono::Utc::now())?)),
        None => Ok(None),
    }
}

#[tauri::command]
pub fn start_exam(
    state: State<'_, AppState>,
    subject_id: i64,
    name: String,
) -> CmdResult<biology::ExamState> {
    Ok(biology::start(&db(&state), subject_id, &name, chrono::Utc::now())?)
}

#[tauri::command]
pub fn set_exam_paused(
    state: State<'_, AppState>,
    paused: bool,
) -> CmdResult<biology::ExamState> {
    Ok(biology::set_paused(&db(&state), paused, chrono::Utc::now())?)
}

/// Finish and log the attempt. Records the real time spent, not the full paper.
#[tauri::command]
pub fn finish_exam(state: State<'_, AppState>) -> CmdResult<i64> {
    Ok(biology::finish(&db(&state), chrono::Utc::now())?)
}

#[tauri::command]
pub fn cancel_exam(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(biology::cancel(&db(&state))?)
}

#[tauri::command]
pub fn score_exam(
    state: State<'_, AppState>,
    exam_id: i64,
    section_a: Option<i64>,
    section_b: Option<i64>,
) -> CmdResult<()> {
    Ok(biology::score(&db(&state), exam_id, section_a, section_b)?)
}

#[tauri::command]
pub fn exam_history(
    state: State<'_, AppState>,
    subject_id: i64,
    limit: i64,
) -> CmdResult<Vec<biology::PracticeExam>> {
    Ok(biology::history(&db(&state), subject_id, limit.clamp(1, 100))?)
}

// ---------------------------------------------------------------------------
// Update check
// ---------------------------------------------------------------------------
//
// Reads GitHub Releases and reports. Downloads nothing, installs nothing,
// modifies nothing, and sends nothing about the user or the machine. Failure is
// a normal state — see the `update` module docs.

/// The last known result, from cache. Never touches the network, so the
/// Settings screen renders instantly and works offline.
#[tauri::command]
pub fn update_status(state: State<'_, AppState>) -> CmdResult<update::UpdateReport> {
    let conn = db(&state);
    Ok(update::cached(&conn, env!("CARGO_PKG_VERSION"))?)
}

/// Ask GitHub now — the "Check for updates" button.
#[tauri::command]
pub async fn check_for_updates(state: State<'_, AppState>) -> CmdResult<update::UpdateReport> {
    let current = env!("CARGO_PKG_VERSION");
    let status = update::check(current).await;

    let conn = state.db.lock().expect("database mutex poisoned");
    update::store(&conn, &status, chrono::Utc::now())?;
    Ok(update::cached(&conn, current)?)
}

/// Open a release page in the default browser.
///
/// The URL is re-validated here rather than trusted from the frontend: this is
/// the one command that hands a string to the OS, so it only ever accepts a
/// github.com HTTPS link.
#[tauri::command]
pub fn open_release_page(app: AppHandle, url: Option<String>) -> CmdResult<()> {
    let target = url.unwrap_or_else(|| update::releases_page().to_string());
    update::is_safe_release_url(&target)?;

    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_url(target, None::<&str>)
        .map_err(|e| CommandError(e.to_string()))?;
    Ok(())
}

/// Models the configured key can see.
///
/// Only Gemini publishes a usable list for an ordinary key, so other providers
/// return empty and the Settings field stays free text for them. An empty list
/// is a normal answer, not an error.
#[tauri::command]
pub async fn list_ai_models(provider: secrets::Provider) -> CmdResult<Vec<ai::ModelOption>> {
    if provider != secrets::Provider::Gemini {
        return Ok(Vec::new());
    }
    Ok(ai::gemini_models().await?)
}

/// Try a model for real — the "Test" button in Settings.
///
/// Performs an actual minimal generation, because a model can be listed and
/// still refuse to run. Returns the reply so the result is unambiguous.
#[tauri::command]
pub async fn test_ai_model(
    state: State<'_, AppState>,
    provider: Option<secrets::Provider>,
    model: Option<String>,
) -> CmdResult<String> {
    let (provider, model) = {
        let conn = db(&state);
        let p = match provider {
            Some(p) => p,
            None => ai::active_provider(&conn)
                .ok_or_else(|| CommandError("No API key is set up.".into()))?,
        };
        let m = match model {
            Some(m) if !m.trim().is_empty() => m,
            _ => ai::model_name(&conn, p),
        };
        (p, m)
    };

    Ok(ai::test_model(provider, &model).await?)
}

/// The next interval for each rating, so the Review screen can show them under
/// the buttons. Read-only — nothing is scheduled or recorded.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntervalPreview {
    pub rating: scheduler::Rating,
    /// Whole days, or null for an intraday learning step.
    pub interval_days: Option<i64>,
}

#[tauri::command]
pub fn preview_intervals(
    state: State<'_, AppState>,
    card_id: i64,
) -> CmdResult<Vec<IntervalPreview>> {
    Ok(cards::preview(&db(&state), card_id, chrono::Utc::now())?
        .into_iter()
        .map(|(rating, interval_days)| IntervalPreview { rating, interval_days })
        .collect())
}

// ---------------------------------------------------------------------------
// Resources — your own study material
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_resources(
    state: State<'_, AppState>,
    subject_id: Option<i64>,
) -> CmdResult<Vec<resources::Resource>> {
    Ok(resources::list(&db(&state), subject_id)?)
}

/// Import text the frontend has already read from a file.
///
/// The file is read in the webview rather than Rust so the OS file picker and
/// drag-and-drop both work without a new plugin. Only the text crosses over —
/// Retain stores what it can search, not a copy of your PDF.
#[tauri::command]
pub fn add_resource(
    state: State<'_, AppState>,
    subject_id: Option<i64>,
    title: String,
    kind: resources::ResourceKind,
    unit: Option<i64>,
    source: Option<String>,
    content: String,
) -> CmdResult<i64> {
    let mut conn = state.db.lock().expect("database mutex poisoned");
    Ok(resources::add(
        &mut conn,
        subject_id,
        &title,
        kind,
        // A file dragged in from a subject folder brings its unit with it;
        // `source` is the original path.
        unit.or_else(|| source.as_deref().and_then(|p| workspace::unit_from_path(Path::new(p)))),
        source.as_deref(),
        &content,
        chrono::Utc::now(),
    )?)
}

#[tauri::command]
pub fn delete_resource(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    Ok(resources::delete(&db(&state), id)?)
}

/// What would be retrieved for a question, without asking the model.
///
/// Lets you check whether your material actually covers a topic — and costs
/// nothing, since no request is made.
#[tauri::command]
pub fn search_resources(
    state: State<'_, AppState>,
    question: String,
    subject_id: Option<i64>,
    limit: i64,
) -> CmdResult<Vec<resources::Excerpt>> {
    Ok(resources::search(&db(&state), &question, subject_id, limit.clamp(1, 20))?)
}

// ---------------------------------------------------------------------------
// Library — saved AI output
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_library(
    state: State<'_, AppState>,
    filter: library::Filter,
    limit: i64,
) -> CmdResult<Vec<library::Item>> {
    Ok(library::list(&db(&state), &filter, limit.clamp(1, 500))?)
}

#[tauri::command]
pub fn set_library_pinned(state: State<'_, AppState>, id: i64, pinned: bool) -> CmdResult<()> {
    Ok(library::set_pinned(&db(&state), id, pinned)?)
}

#[tauri::command]
pub fn rename_library_item(state: State<'_, AppState>, id: i64, title: String) -> CmdResult<()> {
    Ok(library::rename(&db(&state), id, &title)?)
}

#[tauri::command]
pub fn delete_library_item(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    Ok(library::delete(&db(&state), id)?)
}

/// One item as Markdown, for saving or printing.
#[tauri::command]
pub fn library_item_markdown(state: State<'_, AppState>, id: i64) -> CmdResult<String> {
    let conn = db(&state);
    let items = library::list(&conn, &library::Filter::default(), 500)?;
    let item = items
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| CommandError("That item no longer exists.".into()))?;
    Ok(library::to_markdown(&item))
}

/// Save an item to disk as Markdown, returning the path written.
#[tauri::command]
pub fn export_library_item(
    state: State<'_, AppState>,
    app: AppHandle,
    id: i64,
) -> CmdResult<String> {
    let markdown = library_item_markdown(state, id)?;

    // Downloads, so it lands somewhere you'll actually find it.
    let dir = app
        .path()
        .download_dir()
        .map_err(|_| CommandError("Couldn't find your Downloads folder.".into()))?;

    let first_line = markdown.lines().next().unwrap_or("note").trim_start_matches('#').trim();
    let safe: String = first_line
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '-' })
        .collect();
    let safe = safe.trim().replace("  ", " ");
    let name = format!("{}.md", if safe.is_empty() { "retain-note".into() } else { safe });

    let path = dir.join(name);
    std::fs::write(&path, markdown).map_err(|e| CommandError(format!("Couldn't write it: {e}")))?;
    Ok(path.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Workspace folders and folder import
// ---------------------------------------------------------------------------

/// Create (or find) a folder per subject under ~/Documents/Retain.
#[tauri::command]
pub fn ensure_subject_folders(
    state: State<'_, AppState>,
    app: AppHandle,
) -> CmdResult<Vec<workspace::SubjectFolder>> {
    let docs = app
        .path()
        .document_dir()
        .map_err(|_| CommandError("Couldn't find your Documents folder.".into()))?;
    Ok(workspace::ensure(&db(&state), &docs)?)
}

/// Reveal a folder in Finder.
#[tauri::command]
pub fn reveal_folder(app: AppHandle, path: String) -> CmdResult<()> {
    // Only ever a folder Retain created, checked before it reaches the OS.
    let docs = app
        .path()
        .document_dir()
        .map_err(|_| CommandError("Couldn't find your Documents folder.".into()))?;
    let root = workspace::root(&docs);

    if !std::path::Path::new(&path).starts_with(&root) {
        return Err(CommandError("That isn't a Retain folder.".into()));
    }

    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_path(path, None::<&str>)
        .map_err(|e| CommandError(e.to_string()))?;
    Ok(())
}

/// What one file produced, once stored.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFile {
    pub name: String,
    pub outcome: ingest::Outcome,
    /// Set when it was stored.
    pub resource_id: Option<i64>,
    pub skipped_duplicate: bool,
}

/// Read every readable file in a folder and index what it finds.
///
/// Re-runnable: a file already imported from the same path is skipped rather
/// than duplicated, so pressing Sync after dropping in two new PDFs costs two
/// files of work, not thirty.
#[tauri::command]
pub fn import_folder(
    state: State<'_, AppState>,
    path: String,
    subject_id: Option<i64>,
) -> CmdResult<Vec<ImportedFile>> {
    let dir = std::path::PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(CommandError("That isn't a folder.".into()));
    }

    let files = ingest::walk(&dir);
    let mut out = Vec::with_capacity(files.len());

    for file in files {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        {
            let conn = db(&state);
            if workspace::already_imported(&conn, &file).unwrap_or(false) {
                out.push(ImportedFile {
                    name,
                    outcome: ingest::Outcome::Extracted {
                        path: file.to_string_lossy().to_string(),
                        name: file.file_name().unwrap_or_default().to_string_lossy().to_string(),
                        text: String::new(),
                        words: 0,
                    },
                    resource_id: None,
                    skipped_duplicate: true,
                });
                continue;
            }
        }

        // Extraction happens without the database lock held — a folder of PDFs
        // takes real time and would otherwise freeze the whole app.
        let outcome = ingest::extract_file(&file);

        let resource_id = if let ingest::Outcome::Extracted { text, .. } = &outcome {
            let mut conn = state.db.lock().expect("database mutex poisoned");
            resources::add_from_file(
                &mut conn,
                subject_id,
                &workspace::title_from_filename(&name),
                workspace::kind_for(&file),
                Some(&name),
                text,
                Some(&file.to_string_lossy()),
                chrono::Utc::now(),
            )
            .ok()
        } else {
            None
        };

        out.push(ImportedFile {
            name,
            outcome,
            resource_id,
            skipped_duplicate: false,
        });
    }

    Ok(out)
}

/// Read one file chosen in the picker — used for chat attachments too.
#[tauri::command]
pub fn read_file_text(path: String) -> CmdResult<ingest::Outcome> {
    Ok(ingest::extract_file(std::path::Path::new(&path)))
}

// ---------------------------------------------------------------------------
// The assistant
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_conversations(
    state: State<'_, AppState>,
    limit: i64,
) -> CmdResult<Vec<assistant::Conversation>> {
    Ok(assistant::list(&db(&state), limit.clamp(1, 200))?)
}

#[tauri::command]
pub fn create_conversation(
    state: State<'_, AppState>,
    subject_id: Option<i64>,
    grounding: assistant::Grounding,
) -> CmdResult<i64> {
    Ok(assistant::create(&db(&state), subject_id, grounding, chrono::Utc::now())?)
}

#[tauri::command]
pub fn conversation_messages(
    state: State<'_, AppState>,
    conversation_id: i64,
) -> CmdResult<Vec<assistant::Message>> {
    Ok(assistant::messages(&db(&state), conversation_id)?)
}

#[tauri::command]
pub fn set_conversation_grounding(
    state: State<'_, AppState>,
    conversation_id: i64,
    grounding: assistant::Grounding,
) -> CmdResult<()> {
    Ok(assistant::set_grounding(&db(&state), conversation_id, grounding)?)
}

#[tauri::command]
pub fn delete_conversation(state: State<'_, AppState>, conversation_id: i64) -> CmdResult<()> {
    Ok(assistant::delete(&db(&state), conversation_id)?)
}

/// Ask a question. The whole turn: store it, retrieve, answer, store that.
///
/// The user's message is written before the model is called, so a failed or
/// slow request never loses what you typed.
#[tauri::command]
pub async fn ask_assistant(
    state: State<'_, AppState>,
    conversation_id: i64,
    question: String,
    attachments: Vec<assistant::NewAttachment>,
) -> CmdResult<AssistantTurn> {
    let now = chrono::Utc::now();

    // --- everything that needs the database, before any network work --------
    let (client, model, grounding, subject_id, prompt, excerpts) = {
        let mut conn = state.db.lock().expect("database mutex poisoned");

        let convo = assistant::list(&conn, 200)?
            .into_iter()
            .find(|c| c.id == conversation_id)
            .ok_or_else(|| CommandError("That conversation no longer exists.".into()))?;

        let history = assistant::messages(&conn, conversation_id)?;
        assistant::add_user_message(&mut conn, conversation_id, &question, &attachments, now)?;

        let client = ai::Ai::from(&conn)?;
        let model = client_model(&conn);

        let excerpts = resources::by_authority(
            resources::search(&conn, &question, convo.subject_id, assistant::RETRIEVE)
                .unwrap_or_default(),
        );
        let app_data = assistant::app_context(&conn);

        let prompt = assistant::build_prompt(
            &excerpts,
            &attachments,
            &app_data,
            &history,
            &question,
            convo.grounding,
        );

        (client, model, convo.grounding, convo.subject_id, prompt, excerpts)
    };
    let _ = subject_id;

    // The action vocabulary is appended to the system prompt, never to the user
    // half — an instruction about what the assistant may do must not be
    // something a retrieved PDF can appear to be part of.
    let system = format!(
        "{}\n{}",
        match grounding {
            assistant::Grounding::Strict => assistant::SYSTEM_STRICT,
            assistant::Grounding::Open => assistant::SYSTEM_OPEN,
        },
        tools::TOOL_PROMPT
    );

    // Images ride along on the user turn. Only inline data URLs — an attachment
    // is something you picked or captured, never a URL the model can reach out
    // to on its own.
    let images: Vec<String> = attachments
        .iter()
        .filter_map(|a| a.image_data_url.clone())
        .collect();

    let answer = if images.is_empty() {
        client.ask(&system, &prompt).await?
    } else {
        client.ask_with_images(&system, &prompt, &images).await?
    };

    let conn = db(&state);

    // The action block is stripped before the reply is stored, so the saved
    // conversation and its Markdown export read as prose rather than as JSON.
    let (prose, proposals) = tools::extract(&conn, &answer);

    let id = assistant::add_assistant_message(
        &conn,
        conversation_id,
        &prose,
        &excerpts,
        Some(&model),
        chrono::Utc::now(),
    )?;

    let message = assistant::messages(&conn, conversation_id)?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| CommandError("The answer couldn't be saved.".into()))?;

    Ok(AssistantTurn { message, proposals })
}

/// One turn: the stored reply, plus anything the assistant offered to do.
///
/// Proposals are deliberately not stored with the message. They are an offer
/// attached to this moment; a button that is still sitting in a conversation
/// three weeks later, next to a date that has passed, is a trap.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantTurn {
    pub message: assistant::Message,
    pub proposals: Vec<tools::Proposal>,
}

/// Perform an action the assistant proposed and the student confirmed.
///
/// Re-validated in `tools::apply` rather than trusted: this arrives from the
/// frontend, and the whole point of the confirmation step is lost if the
/// confirmed thing isn't checked again on the way through.
#[tauri::command]
pub fn apply_assistant_action(
    state: State<'_, AppState>,
    app: AppHandle,
    action: tools::Action,
) -> CmdResult<tools::Applied> {
    let applied = {
        let conn = db(&state);
        tools::apply(&conn, action)?
    };

    // Only ever an https URL — `tools::validate` is what guarantees that, and it
    // has just run again inside `apply`.
    if let Some(url) = &applied.open {
        tauri_plugin_opener::OpenerExt::opener(&app)
            .open_url(url.clone(), None::<&str>)
            .map_err(|e| CommandError(e.to_string()))?;
    }

    Ok(applied)
}

/// Grab the screen for the next assistant message.
///
/// Explicit, one shot, and nothing is stored: the image comes straight back to
/// the window that asked, as an attachment you can see and remove before you
/// send it. See `screen` for why there is no watching mode.
#[tauri::command]
pub fn capture_screen(app: AppHandle) -> CmdResult<String> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| CommandError(format!("No cache directory: {e}")))?;
    std::fs::create_dir_all(&dir).map_err(|e| CommandError(e.to_string()))?;

    let png = screen::capture_png(&dir).map_err(|e| CommandError(e.to_string()))?;
    Ok(screen::to_data_url(&png))
}

/// A conversation as Markdown, for saving or printing.
#[tauri::command]
pub fn conversation_markdown(
    state: State<'_, AppState>,
    conversation_id: i64,
) -> CmdResult<String> {
    let conn = db(&state);
    let convo = assistant::list(&conn, 200)?
        .into_iter()
        .find(|c| c.id == conversation_id)
        .ok_or_else(|| CommandError("That conversation no longer exists.".into()))?;
    Ok(assistant::to_markdown(&convo, &assistant::messages(&conn, conversation_id)?)?)
}

/// Throw a just-finished session away instead of logging it.
///
/// The stop dialog offers this because not every timer run is study: you start
/// one, get pulled away, and come back to twenty minutes that would quietly
/// inflate your week. A tracker you don't trust is one you stop reading, so
/// discarding has to be as easy as keeping.
///
/// Only a session that has already ended can be discarded, and its pauses go
/// with it — a half-deleted session would leave orphaned pause rows skewing the
/// active-time sums.
#[tauri::command]
pub fn discard_session(state: State<'_, AppState>, session_id: i64) -> CmdResult<()> {
    let mut conn = state.db.lock().expect("database mutex poisoned");
    let tx = conn.transaction()?;

    let ended: Option<String> = tx
        .query_row("SELECT ended_at FROM sessions WHERE id = ?1", [session_id], |r| r.get(0))
        .map_err(|_| CommandError("That session no longer exists.".into()))?;

    if ended.is_none() {
        return Err(CommandError("That session is still running.".into()));
    }

    tx.execute("DELETE FROM session_pauses WHERE session_id = ?1", [session_id])?;
    tx.execute("DELETE FROM sessions WHERE id = ?1", [session_id])?;
    tx.commit()?;
    Ok(())
}

/// How one day was actually spent, broken down by subject.
///
/// The contribution grid shows *how much*; this answers *on what*. Clicking a
/// day and seeing "Biology 40m, Methods 25m" is the question the grid always
/// prompted and never answered.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySubject {
    pub subject_id: i64,
    pub subject_name: String,
    pub colour: String,
    pub minutes: i64,
    pub sessions: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayDetail {
    pub local_date: String,
    pub total_minutes: i64,
    pub session_count: i64,
    pub qualified: bool,
    pub by_subject: Vec<DaySubject>,
    /// Notes written that day, so a day is legible months later.
    pub notes: Vec<String>,
}

#[tauri::command]
pub fn day_detail(state: State<'_, AppState>, local_date: String) -> CmdResult<DayDetail> {
    let conn = db(&state);

    let by_subject: Vec<DaySubject> = conn
        .prepare(
            "SELECT s.id, s.name, s.colour,
                    COALESCE(SUM(x.active_seconds), 0) / 60, COUNT(x.id)
               FROM sessions x JOIN subjects s ON s.id = x.subject_id
              WHERE x.local_date = ?1 AND x.ended_at IS NOT NULL
              GROUP BY s.id ORDER BY 4 DESC, s.sort_order",
        )?
        .query_map([&local_date], |r| {
            Ok(DaySubject {
                subject_id: r.get(0)?,
                subject_name: r.get(1)?,
                colour: r.get(2)?,
                minutes: r.get(3)?,
                sessions: r.get(4)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    let notes: Vec<String> = conn
        .prepare(
            "SELECT note FROM sessions
              WHERE local_date = ?1 AND note IS NOT NULL AND TRIM(note) != ''
              ORDER BY started_at",
        )?
        .query_map([&local_date], |r| r.get(0))?
        .collect::<Result<_, _>>()?;

    let total_minutes = by_subject.iter().map(|s| s.minutes).sum();
    let session_count = by_subject.iter().map(|s| s.sessions).sum();

    Ok(DayDetail {
        // A day counts if it met the threshold or cleared its reviews — the
        // same rule the streak uses, read from the same place rather than
        // re-derived here where it could drift.
        qualified: settings::focused_session_minutes(&conn)
            .and_then(|threshold| streak::qualifying_days(&conn, threshold))
            .map(|days| days.contains(&local_date))
            .unwrap_or(false),
        local_date,
        total_minutes,
        session_count,
        by_subject,
        notes,
    })
}

// ---------------------------------------------------------------------------
// Time blocks — when you can't study
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_blocks(state: State<'_, AppState>) -> CmdResult<Vec<blocks::TimeBlock>> {
    Ok(blocks::all(&db(&state))?)
}

/// Blocks that apply on one date: its weekly ones plus anything dated to it.
#[tauri::command]
pub fn blocks_for_date(
    state: State<'_, AppState>,
    local_date: String,
) -> CmdResult<Vec<blocks::TimeBlock>> {
    let date = local_date
        .parse::<chrono::NaiveDate>()
        .map_err(|_| CommandError("That isn't a date.".into()))?;
    Ok(blocks::for_date(&db(&state), date)?)
}

#[tauri::command]
pub fn create_block(state: State<'_, AppState>, block: blocks::NewBlock) -> CmdResult<i64> {
    Ok(blocks::create(&db(&state), &block, chrono::Utc::now())?)
}

#[tauri::command]
pub fn update_block(
    state: State<'_, AppState>,
    id: i64,
    block: blocks::NewBlock,
) -> CmdResult<()> {
    Ok(blocks::update(&db(&state), id, &block)?)
}

#[tauri::command]
pub fn delete_block(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    Ok(blocks::delete(&db(&state), id)?)
}

/// A screenshot or file attached to a capture.
///
/// Images arrive as a base64 data URL from the webview's paste handler and are
/// stored as bytes. Text arrives already extracted by `ingest`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCaptureAttachment {
    pub name: String,
    /// A `data:image/...;base64,...` URL, for a pasted or dropped image.
    pub image_data_url: Option<String>,
    pub text: Option<String>,
}

/// Save a capture with anything attached to it.
///
/// One command rather than save-then-attach: a capture is meant to take four
/// seconds, and a two-step write that can half-fail would leave a screenshot
/// with no note or a note with no screenshot.
#[tauri::command]
pub fn save_capture_with_attachments(
    state: State<'_, AppState>,
    text: String,
    attachments: Vec<NewCaptureAttachment>,
) -> CmdResult<i64> {
    let mut conn = state.db.lock().expect("database mutex poisoned");
    let now = chrono::Utc::now();

    let subjects: Vec<capture::SubjectHint> = subjects::list(&conn, false)?
        .into_iter()
        .map(|s| capture::SubjectHint { id: s.id, name: s.name })
        .collect();

    let tx = conn.transaction()?;
    let parsed = capture::parse(&text, &subjects, crate::util::retain_today_naive());

    tx.execute(
        "INSERT INTO captures
           (raw_text, created_at, local_date, suggested_subject_id, suggested_due_on, suggested_title)
         VALUES (?1,?2,?3,?4,?5,?6)",
        rusqlite::params![
            text,
            crate::util::rfc3339(now),
            crate::util::retain_today(),
            parsed.subject_id,
            parsed.due_on,
            parsed.title,
        ],
    )?;
    let id = tx.last_insert_rowid();

    {
        let mut stmt = tx.prepare(
            "INSERT INTO capture_attachments (capture_id, name, kind, image, text, created_at)
             VALUES (?1,?2,?3,?4,?5,?6)",
        )?;

        for a in &attachments {
            if let Some(url) = &a.image_data_url {
                // Strip the `data:…;base64,` prefix; anything else is not an
                // image we put in the database.
                let Some(b64) = url.split(",").nth(1) else { continue };
                let Ok(bytes) = decode_base64(b64) else { continue };
                stmt.execute(rusqlite::params![
                    id,
                    a.name,
                    "image",
                    bytes,
                    None::<String>,
                    crate::util::rfc3339(now)
                ])?;
            } else if let Some(t) = &a.text {
                if t.trim().is_empty() {
                    continue;
                }
                stmt.execute(rusqlite::params![
                    id,
                    a.name,
                    "text",
                    None::<Vec<u8>>,
                    t,
                    crate::util::rfc3339(now)
                ])?;
            }
        }
    }

    tx.commit()?;
    Ok(id)
}

/// Decode standard base64. Hand-rolled to avoid a dependency for one use.
fn decode_base64(input: &str) -> Result<Vec<u8>, ()> {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut lookup = [255u8; 256];
    for (i, c) in TABLE.iter().enumerate() {
        lookup[*c as usize] = i as u8;
    }

    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;

    for byte in input.bytes() {
        if byte == b'=' || byte.is_ascii_whitespace() {
            continue;
        }
        let value = lookup[byte as usize];
        if value == 255 {
            return Err(());
        }
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }

    Ok(out)
}

/// Attachment names on a capture, so triage can show what came with it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureAttachment {
    pub id: i64,
    pub name: String,
    pub kind: String,
    /// Present for images, as a data URL ready to render.
    pub image_data_url: Option<String>,
    pub text: Option<String>,
}

#[tauri::command]
pub fn capture_attachments(
    state: State<'_, AppState>,
    capture_id: i64,
) -> CmdResult<Vec<CaptureAttachment>> {
    let conn = db(&state);
    let mut stmt = conn.prepare(
        "SELECT id, name, kind, image, text FROM capture_attachments
          WHERE capture_id = ?1 ORDER BY id",
    )?;

    let rows = stmt
        .query_map([capture_id], |r| {
            let kind: String = r.get(2)?;
            let image: Option<Vec<u8>> = r.get(3)?;
            Ok(CaptureAttachment {
                id: r.get(0)?,
                name: r.get(1)?,
                image_data_url: image.map(|b| format!("data:image/png;base64,{}", encode_base64(&b))),
                text: r.get(4)?,
                kind,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;

        out.push(TABLE[(n >> 18 & 63) as usize] as char);
        out.push(TABLE[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[(n & 63) as usize] as char } else { '=' });
    }
    out
}

/// Open a meeting link from a time block.
///
/// The URL is re-checked against the stored row rather than trusted from the
/// frontend — this hands a string to the OS, so it only ever accepts an HTTP(S)
/// link that Retain itself saved.
#[tauri::command]
pub fn open_block_link(state: State<'_, AppState>, app: AppHandle, id: i64) -> CmdResult<()> {
    let link: Option<String> = {
        let conn = db(&state);
        conn.query_row("SELECT link FROM time_blocks WHERE id = ?1", [id], |r| r.get(0))
            .map_err(|_| CommandError("That block no longer exists.".into()))?
    };

    let link = link
        .filter(|l| l.starts_with("https://") || l.starts_with("http://"))
        .ok_or_else(|| CommandError("That block has no meeting link.".into()))?;

    tauri_plugin_opener::OpenerExt::opener(&app)
        .open_url(link, None::<&str>)
        .map_err(|e| CommandError(e.to_string()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The plan — what you meant to do, and where it goes when a day slips
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn plan_for_date(state: State<'_, AppState>, local_date: String) -> CmdResult<Vec<plan::PlanItem>> {
    Ok(plan::for_date(&db(&state), &local_date)?)
}

#[tauri::command]
pub fn plan_between(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> CmdResult<Vec<plan::PlanItem>> {
    Ok(plan::between(&db(&state), &from, &to)?)
}

#[tauri::command]
pub fn create_plan_item(state: State<'_, AppState>, item: plan::NewPlanItem) -> CmdResult<i64> {
    Ok(plan::create(&db(&state), &item, chrono::Utc::now())?)
}

#[tauri::command]
pub fn set_plan_status(state: State<'_, AppState>, id: i64, status: String) -> CmdResult<()> {
    Ok(plan::set_status(&db(&state), id, &status, chrono::Utc::now())?)
}

#[tauri::command]
pub fn move_plan_item(state: State<'_, AppState>, id: i64, local_date: String) -> CmdResult<()> {
    Ok(plan::move_to(&db(&state), id, &local_date)?)
}

#[tauri::command]
pub fn delete_plan_item(state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    Ok(plan::delete(&db(&state), id)?)
}

/// Walk anything left behind onto days that can take it.
///
/// `force` re-runs a pass that has already happened today, which is what the
/// "reshuffle" button in Today does — you've just added a shift on Thursday and
/// want the plan to account for it without waiting until tomorrow.
#[tauri::command]
pub fn run_rollover(state: State<'_, AppState>, force: bool) -> CmdResult<plan::Rollover> {
    let conn = db(&state);
    // The real calendar date, not the 4am study day. Rollover is about which
    // days on a calendar have room in them; at 2am on Wednesday the work you
    // missed "today" is Tuesday's, and Tuesday is genuinely over.
    let today = chrono::Local::now().date_naive();

    if !force && plan::rolled_today(&conn, today)? {
        return Ok(plan::Rollover::default());
    }
    Ok(plan::rollover(&conn, today)?)
}

/// One day's timetable, decoded into something worth reading.
///
/// The Today screen previously listed seven days of raw class codes — a wall of
/// `11ENGT2`, `11ACCQ`, `12BIOS` with times beside them and nothing else. Every
/// detail that makes a timetable useful (which subject, which room, which
/// teacher) was either dropped at import or never joined to your subjects.
#[tauri::command]
pub fn day_schedule(state: State<'_, AppState>, local_date: String) -> CmdResult<Vec<ScheduledClass>> {
    let conn = db(&state);

    // Name and colour, so a class can be shown in its subject's colour rather
    // than as another grey row.
    let subjects: Vec<(String, String)> = {
        let mut stmt =
            conn.prepare("SELECT name, colour FROM subjects WHERE archived = 0 ORDER BY sort_order")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let mut stmt = conn.prepare(
        "SELECT summary, description, location, starts_at, ends_at, all_day
           FROM calendar_events
          WHERE local_date = ?1
          ORDER BY all_day DESC, starts_at, id",
    )?;

    let rows = stmt
        .query_map([&local_date], |r| {
            let summary: String = r.get(0)?;
            let description: Option<String> = r.get(1)?;
            let location: Option<String> = r.get(2)?;
            Ok((
                summary,
                description,
                location,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)? == 1,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows
        .into_iter()
        .map(|(summary, description, location, starts_at, ends_at, all_day)| {
            let detail = ics::describe(&summary, description.as_deref(), location.as_deref(), &subjects);
            ScheduledClass { detail, starts_at, ends_at, all_day }
        })
        .collect())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledClass {
    #[serde(flatten)]
    pub detail: ics::ClassDetail,
    pub starts_at: String,
    pub ends_at: Option<String>,
    pub all_day: bool,
}
