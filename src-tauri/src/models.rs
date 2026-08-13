//! The types that cross the Rust ↔ TypeScript boundary.
//!
//! `#[derive(Serialize)]` generates code to turn a struct into JSON on the way
//! out; `Deserialize` does the reverse on the way in. Tauri uses those to move
//! values between the backend and the React side, so every field name here is
//! literally the key the frontend sees.
//!
//! `#[serde(rename_all = "camelCase")]` means Rust keeps its `snake_case`
//! convention and TypeScript gets the `camelCase` it expects — the same value,
//! spelled the way each language prefers.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Subjects
// ---------------------------------------------------------------------------

/// Unit level. Drives behaviour, not just display: 3/4 subjects get exam
/// countdowns, the VCAA topic tree and revision scheduling; 1/2 subjects get
/// timers and notes only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitLevel {
    /// Serialised as the string "1_2" so it matches the CHECK constraint in SQL.
    #[serde(rename = "1_2")]
    UnitsOneTwo,
    #[serde(rename = "3_4")]
    UnitsThreeFour,
}

impl UnitLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            UnitLevel::UnitsOneTwo => "1_2",
            UnitLevel::UnitsThreeFour => "3_4",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "3_4" => UnitLevel::UnitsThreeFour,
            _ => UnitLevel::UnitsOneTwo,
        }
    }
}

/// Type flag. Switches which error-log categories and card templates are offered
/// in later checkpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectType {
    Science,
    Maths,
    English,
    Humanities,
}

impl SubjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            SubjectType::Science => "science",
            SubjectType::Maths => "maths",
            SubjectType::English => "english",
            SubjectType::Humanities => "humanities",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "maths" => SubjectType::Maths,
            "english" => SubjectType::English,
            "humanities" => SubjectType::Humanities,
            _ => SubjectType::Science,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subject {
    pub id: i64,
    pub name: String,
    pub colour: String,
    pub unit_level: UnitLevel,
    pub subject_type: SubjectType,
    /// `Option<T>` is Rust's nullable: `Some(300)` or `None`. Serialises to a
    /// number or `null`.
    pub weekly_goal_minutes: Option<i64>,
    pub sort_order: i64,
    pub archived: bool,
}

/// What the frontend sends when creating or editing a subject. Separate from
/// `Subject` because there is no id yet on create.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectInput {
    pub name: String,
    pub colour: String,
    pub unit_level: UnitLevel,
    pub subject_type: SubjectType,
    pub weekly_goal_minutes: Option<i64>,
}

// ---------------------------------------------------------------------------
// Timer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerMode {
    Stopwatch,
    Pomodoro,
}

/// Which half of a Pomodoro cycle we're in. Stopwatch sessions are always `Work`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Work,
    Break,
}

/// Why the timer is currently not accumulating active time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    /// The user pressed pause.
    Manual,
    /// No input for the idle threshold, so we paused on their behalf.
    Idle,
    /// A Pomodoro break. Not counted as a pause in `pause_count` — breaks are
    /// the design working, not friction.
    Break,
}

impl PauseReason {
    pub fn as_str(self) -> &'static str {
        match self {
            PauseReason::Manual => "manual",
            PauseReason::Idle => "idle",
            PauseReason::Break => "break",
        }
    }
}

/// The whole state of the timer, sent to the UI once a second.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerSnapshot {
    pub session_id: i64,
    pub subject_id: i64,
    pub subject_name: String,
    pub subject_colour: String,
    pub topic_id: Option<i64>,
    pub topic_name: Option<String>,
    pub mode: TimerMode,
    pub phase: Phase,
    /// Wall clock since start.
    pub elapsed_seconds: i64,
    /// Elapsed minus every pause and break. What the streak reads.
    pub active_seconds: i64,
    pub pause_count: i64,
    pub idle_pause_count: i64,
    /// `None` when running, `Some(reason)` when stopped for any reason.
    pub paused_reason: Option<PauseReason>,
    /// Seconds left in the current Pomodoro phase; `None` for stopwatch.
    pub phase_remaining_seconds: Option<i64>,
    pub completed_work_blocks: i64,
}

/// Sent by the UI to start a session.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTimerInput {
    pub subject_id: i64,
    pub topic_id: Option<i64>,
    pub mode: TimerMode,
    /// Pomodoro only. Ignored for stopwatch.
    pub work_minutes: Option<i64>,
    pub break_minutes: Option<i64>,
}

/// What the UI gets back after stopping, so it can offer the note prompt with
/// the session already in hand.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishedSession {
    pub session_id: i64,
    pub subject_name: String,
    pub elapsed_seconds: i64,
    pub active_seconds: i64,
    pub pause_count: i64,
    pub idle_pause_count: i64,
    /// Whether this session on its own met the focused-session bar. Used to tell
    /// the user what they earned, in progress framing.
    pub qualifies_for_streak: bool,
}

// ---------------------------------------------------------------------------
// Contribution grid, streak, goals
// ---------------------------------------------------------------------------

/// One subject's slice of a single day, for the grid hover breakdown.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySubjectSlice {
    pub subject_id: i64,
    pub subject_name: String,
    pub colour: String,
    pub minutes: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GridDay {
    /// Local date, 'YYYY-MM-DD'.
    pub date: String,
    /// Active minutes, summed across subjects.
    pub minutes: i64,
    /// Whether this day met the streak bar. Distinct from `minutes > 0` — twelve
    /// scattered minutes colour the cell but do not earn the day.
    pub qualified: bool,
    pub by_subject: Vec<DaySubjectSlice>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreakSummary {
    pub current: i64,
    pub longest: i64,
    pub freezes_available: i64,
    /// Weekdays nominated as rest days. 0 = Monday.
    pub rest_days: Vec<i64>,
    pub today_qualified: bool,
    pub today_active_minutes: i64,
    /// The bar, so the UI can say "20 min today" without hardcoding it.
    pub threshold_minutes: i64,
}

/// One Apple-Watch-style ring.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyGoalRing {
    pub subject_id: i64,
    pub subject_name: String,
    pub colour: String,
    pub goal_minutes: i64,
    pub done_minutes: i64,
}
