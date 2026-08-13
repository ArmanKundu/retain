//! The `app_settings` key/value table, plus typed accessors for the settings
//! that have behaviour attached to them.
//!
//! A key/value table rather than a wide single-row table: settings get added
//! every checkpoint, and this way adding one is a constant, not a migration.

use rusqlite::{Connection, OptionalExtension};

/// Default focused-session bar, in minutes.
///
/// This is a DEFAULT, not a requirement, and the reasoning is in
/// docs/streak-rule.md: the Pomodoro work block is 25 minutes, and the idle
/// detector pauses after 2 minutes of no input, so a genuine 25-minute block that
/// included some reading-without-typing lands around 23. Setting the bar at 25
/// would fail sessions that actually happened. 20 leaves slack for that without
/// letting a token five minutes count.
pub const DEFAULT_FOCUSED_SESSION_MINUTES: i64 = 20;

pub const DEFAULT_POMODORO_WORK_MINUTES: i64 = 25;
pub const DEFAULT_POMODORO_BREAK_MINUTES: i64 = 5;

/// Read a setting. `Ok(None)` means "never set", which callers turn into their
/// own default rather than this module guessing.
pub fn get(conn: &Connection, key: &str) -> anyhow::Result<Option<String>> {
    // `.optional()` turns "no rows" from an error into `None`, which is what we
    // mean here — a missing setting is normal, not a failure.
    let value = conn
        .query_row("SELECT value FROM app_settings WHERE key = ?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .optional()?;
    Ok(value)
}

/// Write a setting, inserting or replacing.
pub fn set(conn: &Connection, key: &str, value: &str) -> anyhow::Result<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

pub fn get_i64(conn: &Connection, key: &str, fallback: i64) -> anyhow::Result<i64> {
    Ok(get(conn, key)?.and_then(|v| v.parse().ok()).unwrap_or(fallback))
}

pub fn get_bool(conn: &Connection, key: &str, fallback: bool) -> anyhow::Result<bool> {
    Ok(get(conn, key)?.map(|v| v == "1" || v == "true").unwrap_or(fallback))
}

/// The streak bar. Clamped to the range Settings offers, so a hand-edited
/// database can't produce a nonsensical target.
pub fn focused_session_minutes(conn: &Connection) -> anyhow::Result<i64> {
    let raw = get_i64(conn, "focused_session_minutes", DEFAULT_FOCUSED_SESSION_MINUTES)?;
    Ok(raw.clamp(5, 120))
}

pub fn onboarding_complete(conn: &Connection) -> anyhow::Result<bool> {
    get_bool(conn, "onboarding_complete", false)
}
