//! The study timer.
//!
//! This lives in Rust rather than in React for one reason: the menu bar has to
//! keep counting when the window is closed, and a React timer dies with its
//! window. The backend owns the truth; the UI is a view of it.
//!
//! ## How active time is accounted for
//!
//! A session is a series of **stretches** of genuine work, separated by pauses.
//!
//! ```text
//!   start ├──── stretch ────┤  paused  ├──── stretch ────┤  break  ├── stretch ──┤ stop
//!         └─ counted ───────┘          └─ counted ───────┘         └─ counted ───┘
//! ```
//!
//! `accumulated_active` banks each finished stretch. `stretch_start` marks the
//! open one. Active time is the bank plus the open stretch. Anything between
//! stretches — a manual pause, an idle auto-pause, a Pomodoro break — is simply
//! not counted, because no stretch is open.
//!
//! This is why the streak reads `active_seconds` and never `elapsed_seconds`.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;

use crate::models::{FinishedSession, Phase, PauseReason, TimerMode, TimerSnapshot};
use crate::util::{retain_day_of, rfc3339};

/// No input for this long and we pause on the user's behalf. The brief specifies
/// two minutes.
pub const IDLE_THRESHOLD_SECONDS: i64 = 120;

/// The live session. `None` in the shared state means no timer is running.
pub struct ActiveTimer {
    pub session_id: i64,
    pub subject_id: i64,
    pub subject_name: String,
    pub subject_colour: String,
    pub topic_id: Option<i64>,
    pub topic_name: Option<String>,

    pub mode: TimerMode,
    pub started_at: DateTime<Utc>,

    // --- Pomodoro configuration. Unused by stopwatch sessions. ---
    pub work_seconds: i64,
    pub break_seconds: i64,
    pub phase: Phase,
    pub phase_started_at: DateTime<Utc>,
    pub completed_work_blocks: i64,

    // --- Active-time accounting (see the module docs above) ---
    /// Seconds banked from stretches that have already closed.
    pub accumulated_active: i64,
    /// When the currently-open stretch began. `None` while paused.
    pub stretch_start: Option<DateTime<Utc>>,

    // --- Pause bookkeeping ---
    pub paused_reason: Option<PauseReason>,
    /// Row id in `session_pauses` for the pause we're inside, so we can close it.
    pub open_pause_id: Option<i64>,
    /// When the current pause began, so a Pomodoro block can have its countdown
    /// shifted forward by however long the pause lasted.
    pub paused_at: Option<DateTime<Utc>>,
    /// Manual + idle pauses. Breaks are deliberately excluded: the brief wants
    /// visible friction to discourage quitting, and taking the break the app
    /// told you to take is not friction.
    pub pause_count: i64,
    pub idle_pause_count: i64,
}

/// Shared handle. `Arc` = atomically reference-counted pointer, so several
/// threads can hold the same value; `Mutex` = only one may touch it at a time.
/// Together they are the standard way to share mutable state across threads.
pub type SharedTimer = Arc<Mutex<Option<ActiveTimer>>>;

impl ActiveTimer {
    /// Wall clock since the session started.
    pub fn elapsed_seconds(&self, now: DateTime<Utc>) -> i64 {
        (now - self.started_at).num_seconds().max(0)
    }

    /// Banked stretches plus the open one, if any.
    pub fn active_seconds(&self, now: DateTime<Utc>) -> i64 {
        let open = match self.stretch_start {
            Some(start) => (now - start).num_seconds().max(0),
            None => 0,
        };
        self.accumulated_active + open
    }

    /// Seconds left in the current Pomodoro phase. `None` for stopwatch.
    pub fn phase_remaining(&self, now: DateTime<Utc>) -> Option<i64> {
        if self.mode != TimerMode::Pomodoro {
            return None;
        }
        let target = match self.phase {
            Phase::Work => self.work_seconds,
            Phase::Break => self.break_seconds,
        };
        let spent = (now - self.phase_started_at).num_seconds().max(0);
        Some((target - spent).max(0))
    }

    /// Close the open stretch and bank it.
    ///
    /// `at` is passed in rather than read from the clock because the idle
    /// detector needs to **backdate**: when we notice at 14:02 that there has
    /// been no input since 14:00, the two idle minutes must not be counted. See
    /// `maybe_auto_pause`.
    fn bank_stretch(&mut self, at: DateTime<Utc>) {
        if let Some(start) = self.stretch_start.take() {
            // Clamp so a backdated `at` earlier than the stretch start can never
            // produce negative time.
            let seconds = (at - start).num_seconds().max(0);
            self.accumulated_active += seconds;
        }
    }

    pub fn snapshot(&self, now: DateTime<Utc>) -> TimerSnapshot {
        TimerSnapshot {
            session_id: self.session_id,
            subject_id: self.subject_id,
            subject_name: self.subject_name.clone(),
            subject_colour: self.subject_colour.clone(),
            topic_id: self.topic_id,
            topic_name: self.topic_name.clone(),
            mode: self.mode,
            phase: self.phase,
            elapsed_seconds: self.elapsed_seconds(now),
            active_seconds: self.active_seconds(now),
            pause_count: self.pause_count,
            idle_pause_count: self.idle_pause_count,
            paused_reason: self.paused_reason,
            phase_remaining_seconds: self.phase_remaining(now),
            completed_work_blocks: self.completed_work_blocks,
        }
    }
}

// ---------------------------------------------------------------------------
// Transitions. Each one writes to the database as well as mutating memory, so a
// crash mid-session still leaves an accurate record of what happened.
// ---------------------------------------------------------------------------

/// Create the session row and return a live timer.
pub fn start(
    conn: &Connection,
    subject_id: i64,
    topic_id: Option<i64>,
    mode: TimerMode,
    work_minutes: i64,
    break_minutes: i64,
) -> anyhow::Result<ActiveTimer> {
    let now = Utc::now();

    // Pull the subject's display fields once, so the menu bar and the snapshot
    // don't need a database round trip every second.
    let (subject_name, subject_colour): (String, String) = conn.query_row(
        "SELECT name, colour FROM subjects WHERE id = ?1",
        [subject_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let topic_name: Option<String> = match topic_id {
        Some(id) => conn
            .query_row("SELECT name FROM topics WHERE id = ?1", [id], |row| row.get(0))
            .ok(),
        None => None,
    };

    let mode_str = match mode {
        TimerMode::Stopwatch => "stopwatch",
        TimerMode::Pomodoro => "pomodoro",
    };

    conn.execute(
        "INSERT INTO sessions (subject_id, topic_id, mode, started_at, local_date)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            subject_id,
            topic_id,
            mode_str,
            rfc3339(now),
            retain_day_of(now),
        ],
    )?;

    let session_id = conn.last_insert_rowid();

    Ok(ActiveTimer {
        session_id,
        subject_id,
        subject_name,
        subject_colour,
        topic_id,
        topic_name,
        mode,
        started_at: now,
        work_seconds: work_minutes * 60,
        break_seconds: break_minutes * 60,
        phase: Phase::Work,
        phase_started_at: now,
        completed_work_blocks: 0,
        accumulated_active: 0,
        // A session begins with an open stretch: we're working immediately.
        stretch_start: Some(now),
        paused_reason: None,
        open_pause_id: None,
        paused_at: None,
        pause_count: 0,
        idle_pause_count: 0,
    })
}

/// Stop accumulating active time and record why.
pub fn pause(
    conn: &Connection,
    timer: &mut ActiveTimer,
    at: DateTime<Utc>,
    reason: PauseReason,
) -> anyhow::Result<()> {
    // Already paused: nothing to do. Guards against the ticker and a user click
    // racing to pause the same session.
    if timer.paused_reason.is_some() {
        return Ok(());
    }

    timer.bank_stretch(at);

    conn.execute(
        "INSERT INTO session_pauses (session_id, paused_at, reason) VALUES (?1, ?2, ?3)",
        rusqlite::params![timer.session_id, rfc3339(at), reason.as_str()],
    )?;
    timer.open_pause_id = Some(conn.last_insert_rowid());
    timer.paused_reason = Some(reason);
    timer.paused_at = Some(at);

    // Breaks are expected and don't count as friction; manual and idle pauses do.
    match reason {
        PauseReason::Manual => timer.pause_count += 1,
        PauseReason::Idle => {
            timer.pause_count += 1;
            timer.idle_pause_count += 1;
        }
        PauseReason::Break => {}
    }

    persist_progress(conn, timer, at)?;
    Ok(())
}

/// Reopen a stretch and close the pause row.
pub fn resume(conn: &Connection, timer: &mut ActiveTimer, at: DateTime<Utc>) -> anyhow::Result<()> {
    if timer.paused_reason.is_none() {
        return Ok(());
    }

    if let Some(pause_id) = timer.open_pause_id.take() {
        conn.execute(
            "UPDATE session_pauses SET resumed_at = ?1 WHERE id = ?2",
            rusqlite::params![rfc3339(at), pause_id],
        )?;
    }

    // Push the Pomodoro block's countdown forward by however long we were paused.
    //
    // Without this, a block's clock keeps running while the session is paused, so
    // stepping away for ten minutes mid-block means the break fires the instant
    // you sit back down — the app would hand you a break for the work you just
    // interrupted. Breaks are excluded because a break's own countdown is
    // supposed to elapse.
    if timer.mode == TimerMode::Pomodoro && timer.paused_reason != Some(PauseReason::Break) {
        if let Some(paused_at) = timer.paused_at {
            let paused_for = at - paused_at;
            if paused_for > Duration::zero() {
                timer.phase_started_at += paused_for;
            }
        }
    }

    timer.paused_reason = None;
    timer.paused_at = None;
    timer.stretch_start = Some(at);
    Ok(())
}

/// End the session and write the final numbers.
pub fn stop(
    conn: &Connection,
    timer: &mut ActiveTimer,
    focused_threshold_minutes: i64,
) -> anyhow::Result<FinishedSession> {
    let now = Utc::now();

    // Close whatever is open — either a stretch or a pause, never both.
    timer.bank_stretch(now);
    if let Some(pause_id) = timer.open_pause_id.take() {
        conn.execute(
            "UPDATE session_pauses SET resumed_at = ?1 WHERE id = ?2",
            rusqlite::params![rfc3339(now), pause_id],
        )?;
    }

    let elapsed = timer.elapsed_seconds(now);
    let active = timer.accumulated_active;

    conn.execute(
        "UPDATE sessions
            SET ended_at = ?1,
                elapsed_seconds = ?2,
                active_seconds = ?3,
                pause_count = ?4,
                idle_pause_count = ?5
          WHERE id = ?6",
        rusqlite::params![
            rfc3339(now),
            elapsed,
            active,
            timer.pause_count,
            timer.idle_pause_count,
            timer.session_id
        ],
    )?;

    Ok(FinishedSession {
        session_id: timer.session_id,
        subject_name: timer.subject_name.clone(),
        elapsed_seconds: elapsed,
        active_seconds: active,
        pause_count: timer.pause_count,
        idle_pause_count: timer.idle_pause_count,
        qualifies_for_streak: active >= focused_threshold_minutes * 60,
    })
}

/// Keep the session row roughly current while it runs.
///
/// Without this, force-quitting mid-session would leave a row with zeroes in it.
/// With it, the worst case is losing the last few seconds.
pub fn persist_progress(
    conn: &Connection,
    timer: &ActiveTimer,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE sessions
            SET elapsed_seconds = ?1, active_seconds = ?2,
                pause_count = ?3, idle_pause_count = ?4
          WHERE id = ?5",
        rusqlite::params![
            timer.elapsed_seconds(now),
            timer.active_seconds(now),
            timer.pause_count,
            timer.idle_pause_count,
            timer.session_id
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Automatic behaviour, driven once a second by the ticker thread in lib.rs
// ---------------------------------------------------------------------------

/// Pause if the machine has gone idle, resume if it has woken up.
///
/// The backdating is the part worth reading twice. When we notice at 14:02:00
/// that there has been no input for 120 seconds, the user actually stopped at
/// 14:00:00. Passing `now - idle_seconds` as the pause instant means those two
/// minutes never enter `accumulated_active`. Pausing at `now` instead would
/// silently credit every idle period with a free two minutes — which, over a
/// term, is exactly the kind of quiet inflation that makes the numbers useless.
pub fn maybe_auto_pause_or_resume(
    conn: &Connection,
    timer: &mut ActiveTimer,
    idle_seconds: f64,
) -> anyhow::Result<()> {
    let now = Utc::now();

    match timer.paused_reason {
        // Running, and the machine has gone quiet: pause, backdated.
        None => {
            if idle_seconds >= IDLE_THRESHOLD_SECONDS as f64 {
                let went_idle_at = now - Duration::seconds(idle_seconds as i64);
                pause(conn, timer, went_idle_at, PauseReason::Idle)?;
            }
        }
        // We paused for idleness and input has returned: resume.
        Some(PauseReason::Idle) => {
            if idle_seconds < IDLE_THRESHOLD_SECONDS as f64 {
                resume(conn, timer, now)?;
            }
        }
        // A manual pause stays until the user says otherwise, and a break runs
        // to its own schedule. Neither is the idle detector's business.
        Some(PauseReason::Manual) | Some(PauseReason::Break) => {}
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Sleep, wake, and process suspension
// ---------------------------------------------------------------------------

/// A gap between ticks longer than this means the process wasn't running.
///
/// The ticker fires every second. Ninety seconds is far beyond any scheduling
/// hiccup, GC pause or momentary load spike, but well under the two-minute idle
/// threshold, so a genuine suspension is caught before it can be mistaken for
/// ordinary inactivity.
pub const SUSPENSION_GAP_SECONDS: i64 = 90;

/// Did the wall clock jump between two ticks?
///
/// Returns the size of the gap in seconds when it did.
///
/// ## Why this exists, and why the idle detector isn't enough
///
/// When the Mac sleeps, our process is suspended: the ticker stops, then resumes
/// on wake with the clock hours ahead. The open stretch's `stretch_start` is
/// still set, so `active_seconds` would count every one of those sleeping hours
/// as study.
///
/// The idle detector does not catch this. `CGEventSourceSecondsSinceLastEventType`
/// reports time since the last *input event*, and the keypress or trackpad touch
/// that woke the machine **is** an input event — so on wake it reads close to
/// zero and no idle pause fires. The eight hours are banked silently.
///
/// Watching the clock instead catches every version of the problem with one
/// mechanism: sleep, hibernate, lid close, process suspension, and a machine
/// that was simply too loaded to run the ticker. It needs no macOS API, which
/// also makes it deterministic and testable.
pub fn detect_suspension(last_tick: DateTime<Utc>, now: DateTime<Utc>) -> Option<i64> {
    let gap = (now - last_tick).num_seconds();
    (gap >= SUSPENSION_GAP_SECONDS).then_some(gap)
}

/// Close out a session across a suspension, crediting only observed time.
///
/// The stretch is banked **at the last tick we actually saw**, not at `now`.
/// Everything after that instant is time the machine was asleep and nobody was
/// studying, so it is excluded from active time by construction.
///
/// The session is left paused with reason `Idle`; when input resumes, the normal
/// idle handling picks it back up, exactly as it would after any other break.
pub fn handle_suspension(
    conn: &Connection,
    timer: &mut ActiveTimer,
    last_tick: DateTime<Utc>,
) -> anyhow::Result<()> {
    // Already paused: the stretch is closed, so nothing was accruing and there
    // is nothing to correct.
    if timer.paused_reason.is_some() {
        return Ok(());
    }
    pause(conn, timer, last_tick, PauseReason::Idle)
}

/// What a Pomodoro phase boundary asks the app to do, so the caller can fire a
/// notification without this module needing to know about Tauri.
pub enum PhaseChange {
    None,
    StartedBreak { after_blocks: i64 },
    StartedWork,
}

/// Advance the Pomodoro cycle if the current phase has run out.
pub fn advance_pomodoro(conn: &Connection, timer: &mut ActiveTimer) -> anyhow::Result<PhaseChange> {
    if timer.mode != TimerMode::Pomodoro {
        return Ok(PhaseChange::None);
    }

    // A paused session freezes the cycle — otherwise it would march through
    // breaks it never took. This covers idle pauses as well as manual ones: the
    // machine going quiet mid-block should suspend the block, not let it expire
    // while nobody is there. (A `Break` pause is excluded, since ending the break
    // is exactly what this function is for.)
    if matches!(
        timer.paused_reason,
        Some(PauseReason::Manual) | Some(PauseReason::Idle)
    ) {
        return Ok(PhaseChange::None);
    }

    let now = Utc::now();
    let remaining = timer.phase_remaining(now).unwrap_or(1);
    if remaining > 0 {
        return Ok(PhaseChange::None);
    }

    match timer.phase {
        Phase::Work => {
            timer.completed_work_blocks += 1;
            // Entering a break closes the work stretch. Break time is recorded
            // as a pause with reason 'break' so the history is complete, but it
            // does not increment pause_count.
            pause(conn, timer, now, PauseReason::Break)?;
            timer.phase = Phase::Break;
            timer.phase_started_at = now;
            Ok(PhaseChange::StartedBreak {
                after_blocks: timer.completed_work_blocks,
            })
        }
        Phase::Break => {
            resume(conn, timer, now)?;
            timer.phase = Phase::Work;
            timer.phase_started_at = now;
            Ok(PhaseChange::StartedWork)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An in-memory database with the real schema, so these exercise the actual
    /// SQL the app runs rather than a mock.
    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!("db/migrations/001_init.sql"))
            .expect("apply schema");
        conn.execute(
            "INSERT INTO subjects (id, name, colour, unit_level, subject_type, sort_order, created_at)
             VALUES (1, 'Biology', '#4BA97B', '3_4', 'science', 0, '2026-08-12T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    fn instant(offset_seconds: i64) -> DateTime<Utc> {
        chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 8, 12, 20, 0, 0).unwrap()
            + Duration::seconds(offset_seconds)
    }

    /// A running stopwatch session whose open stretch began at `started`.
    fn running_timer(conn: &Connection, started: DateTime<Utc>) -> ActiveTimer {
        conn.execute(
            "INSERT INTO sessions (subject_id, mode, started_at, local_date)
             VALUES (1, 'stopwatch', ?1, '2026-08-12')",
            [rfc3339(started)],
        )
        .unwrap();

        ActiveTimer {
            session_id: conn.last_insert_rowid(),
            subject_id: 1,
            subject_name: "Biology".into(),
            subject_colour: "#4BA97B".into(),
            topic_id: None,
            topic_name: None,
            mode: TimerMode::Stopwatch,
            started_at: started,
            work_seconds: 1500,
            break_seconds: 300,
            phase: Phase::Work,
            phase_started_at: started,
            completed_work_blocks: 0,
            accumulated_active: 0,
            stretch_start: Some(started),
            paused_reason: None,
            open_pause_id: None,
            paused_at: None,
            pause_count: 0,
            idle_pause_count: 0,
        }
    }

    #[test]
    fn detect_suspension_ignores_ordinary_ticks() {
        assert_eq!(detect_suspension(instant(0), instant(1)), None);
        assert_eq!(detect_suspension(instant(0), instant(30)), None);
        // Just under the threshold is still normal.
        assert_eq!(detect_suspension(instant(0), instant(89)), None);
    }

    #[test]
    fn detect_suspension_catches_a_clock_jump() {
        assert_eq!(detect_suspension(instant(0), instant(90)), Some(90));
        assert_eq!(detect_suspension(instant(0), instant(28_800)), Some(28_800));
    }

    /// A clock that goes backwards (NTP correction) must not be read as a gap.
    #[test]
    fn detect_suspension_ignores_a_backwards_clock() {
        assert_eq!(detect_suspension(instant(500), instant(0)), None);
    }

    /// THE regression test.
    ///
    /// Session starts, one minute of real study happens, then the Mac sleeps for
    /// eight hours and wakes. Active time must be the one minute that actually
    /// happened — not eight hours and one minute.
    #[test]
    fn sleeping_does_not_fabricate_study_time() {
        let conn = test_db();
        let started = instant(0);
        let mut timer = running_timer(&conn, started);

        let last_tick = instant(60); // one minute of genuine study
        let wake = instant(60 + 8 * 3600); // eight hours of sleep

        // Without the fix, this is what would have been recorded:
        assert_eq!(
            timer.active_seconds(wake),
            60 + 8 * 3600,
            "precondition: an open stretch does count sleep until corrected"
        );

        handle_suspension(&conn, &mut timer, last_tick).unwrap();

        assert_eq!(
            timer.active_seconds(wake),
            60,
            "only observed time may be credited"
        );
        assert_eq!(timer.paused_reason, Some(PauseReason::Idle));
        assert_eq!(timer.idle_pause_count, 1);

        // And the pause interval is recorded in the database, backdated to the
        // last tick rather than to the wake-up.
        let paused_at: String = conn
            .query_row(
                "SELECT paused_at FROM session_pauses WHERE session_id = ?1",
                [timer.session_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(paused_at, rfc3339(last_tick));
    }

    /// After waking, the session must be resumable and continue accruing from
    /// the moment input returns — not from where it left off.
    #[test]
    fn resuming_after_sleep_continues_from_the_wake_moment() {
        let conn = test_db();
        let started = instant(0);
        let mut timer = running_timer(&conn, started);

        handle_suspension(&conn, &mut timer, instant(60)).unwrap();
        let wake = instant(60 + 8 * 3600);
        resume(&conn, &mut timer, wake).unwrap();

        // Five more minutes of real study after waking.
        let later = wake + Duration::minutes(5);
        assert_eq!(timer.active_seconds(later), 60 + 300);
    }

    /// A session already paused when the machine slept must not be
    /// double-counted or have a second pause row opened.
    #[test]
    fn suspension_while_already_paused_is_a_no_op() {
        let conn = test_db();
        let mut timer = running_timer(&conn, instant(0));
        pause(&conn, &mut timer, instant(30), PauseReason::Manual).unwrap();

        let banked = timer.accumulated_active;
        handle_suspension(&conn, &mut timer, instant(60)).unwrap();

        assert_eq!(timer.accumulated_active, banked);
        assert_eq!(timer.pause_count, 1, "must not open a second pause");
        assert_eq!(timer.paused_reason, Some(PauseReason::Manual));
    }
}
