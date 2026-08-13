//! FSRS-6 scheduling.
//!
//! ## What is and isn't ours
//!
//! **All memory mathematics comes from the `fsrs` crate** (v6.x, BSD-3-Clause) —
//! the reference implementation, carrying the published 21-parameter FSRS-6
//! default weight set. Nothing in this file derives stability, difficulty, or
//! retrievability. We call `next_states()` and use what it returns.
//!
//! What this file owns is the **state machine around** that math — the
//! New/Learning/Review/Relearning transitions and learning steps. That layer is
//! not part of FSRS; Anki implements it on top of FSRS in exactly the same way.
//!
//! ## The three things that are easy to get wrong
//!
//! 1. **A new card must pass `None`, not a zeroed memory state.** In the crate
//!    (`model.rs`), the initial-stability path is gated on `nth == 0 &&
//!    stability == 0.0`, and `next_states` only sets `nth = 0` when the memory
//!    state is `None`. Passing `Some(MemoryState { stability: 0.0, .. })` sets
//!    `nth = 1` and silently takes the review path for a card that has never
//!    been seen.
//!
//! 2. **Same-day reviews must pass `days_elapsed = 0`, not be skipped.** The
//!    crate branches on `delta_t == 0.0` into `stability_short_term` (the w17/
//!    w18/w19 short-term path). Learning-step reviews are same-day by
//!    definition, so they must go through the scheduler like any other review.
//!
//! 3. **`days_elapsed` is time actually elapsed, not the interval we asked
//!    for.** A card scheduled 5 days out but answered on day 9 must pass 9.
//!    Passing the scheduled interval would feed the model a review that didn't
//!    happen when it says it did.

use chrono::{DateTime, Duration, Utc};
use fsrs::{MemoryState, FSRS};
use serde::{Deserialize, Serialize};

/// Anki's four buttons. The numeric values are FSRS's rating scale and are sent
/// to the crate as-is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardState {
    New,
    Learning,
    Review,
    Relearning,
}

impl CardState {
    pub fn as_str(self) -> &'static str {
        match self {
            CardState::New => "new",
            CardState::Learning => "learning",
            CardState::Review => "review",
            CardState::Relearning => "relearning",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "learning" => CardState::Learning,
            "review" => CardState::Review,
            "relearning" => CardState::Relearning,
            _ => CardState::New,
        }
    }
}

/// Scheduler configuration. Everything here is user-adjustable in Settings.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// FSRS target retention. Default 0.90, per the brief.
    pub desired_retention: f32,
    /// Intraday learning steps, in minutes.
    pub learning_steps: Vec<i64>,
    /// Intraday relearning steps, in minutes, used after a lapse.
    pub relearning_steps: Vec<i64>,
    /// Whether to jitter interday intervals. See `fuzz_interval`.
    pub fuzz: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            desired_retention: 0.90,
            // Anki's defaults. Two short steps before a card graduates.
            learning_steps: vec![1, 10],
            relearning_steps: vec![10],
            fuzz: true,
        }
    }
}

/// The parts of a card the scheduler reads.
#[derive(Debug, Clone)]
pub struct CardSnapshot {
    pub state: CardState,
    pub stability: Option<f32>,
    pub difficulty: Option<f32>,
    pub last_review_at: Option<DateTime<Utc>>,
    pub reps: i64,
    pub lapses: i64,
    pub learning_step: i64,
}

/// The parts the scheduler writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Scheduled {
    pub state: CardState,
    pub stability: f32,
    pub difficulty: f32,
    pub due_at: DateTime<Utc>,
    pub reps: i64,
    pub lapses: i64,
    pub learning_step: i64,
    /// Interday interval in whole days; `None` for an intraday (learning) step.
    /// Used only for display — "next review in 12 days".
    pub interval_days: Option<i64>,
}

// Elapsed days come from the app-wide day-boundary policy in `util` — a Retain
// day runs 4am to 4am local. That single definition is what keeps FSRS elapsed
// days, the streak, the grid, the queue and the new-card cap agreeing with each
// other. See the module docs on `crate::util` for why the boundary is 4am.
use crate::util::retain_days_between;

// ---------------------------------------------------------------------------
// Interval fuzz — ported from Anki's reference implementation
// ---------------------------------------------------------------------------
//
// The `fsrs` crate does NOT implement fuzzing: its only mentions of the word are
// doc comments for the simulator's user-supplied hook. Fuzz is unambiguously the
// host application's job, so the reference to match is Anki's
// `rslib/src/scheduler/states/fuzz.rs`, which the functions below port directly.
//
// An earlier version of this file applied a flat per-band percentage. That was
// wrong in two ways that mattered: Anki's delta starts at a **base of 1 day**,
// and the band contributions **accumulate** across ranges rather than one band
// applying to the whole interval. For a 37-day interval the reference yields
// bounds of [33, 41]; the flat version produced [35, 39] — materially too
// narrow. `anki_parity_*` tests below are Anki's own vectors, copied verbatim.

/// A band of interval lengths and how much fuzz each day inside it contributes.
struct FuzzRange {
    start: f32,
    end: f32,
    factor: f32,
}

static FUZZ_RANGES: [FuzzRange; 3] = [
    FuzzRange { start: 2.5, end: 7.0, factor: 0.15 },
    FuzzRange { start: 7.0, end: 20.0, factor: 0.1 },
    FuzzRange { start: 20.0, end: f32::MAX, factor: 0.05 },
];

/// Fuzz to apply in each direction: 1 day, plus each band's factor times the
/// number of the interval's days that fall inside that band.
fn fuzz_delta(interval: f32) -> f32 {
    if interval < 2.5 {
        0.0
    } else {
        FUZZ_RANGES.iter().fold(1.0, |delta, range| {
            delta + range.factor * (interval.min(range.end) - range.start).max(0.0)
        })
    }
}

fn fuzz_bounds(interval: f32) -> (u32, u32) {
    let delta = fuzz_delta(interval);
    (
        (interval - delta).round() as u32,
        (interval + delta).round() as u32,
    )
}

/// Fuzz bounds clamped into `[minimum, maximum]`, widening a collapsed range
/// where the limits allow it.
fn constrained_fuzz_bounds(interval: f32, minimum: u32, maximum: u32) -> (u32, u32) {
    let minimum = minimum.min(maximum);
    let interval = interval.clamp(minimum as f32, maximum as f32);
    let (mut lower, mut upper) = fuzz_bounds(interval);

    lower = lower.clamp(minimum, maximum);
    upper = upper.clamp(minimum, maximum);

    if upper == lower && upper > 2 && upper < maximum {
        upper = lower + 1;
    }

    (lower, upper)
}

/// Pick an interval inside the fuzz range.
///
/// `fuzz_factor` is a uniform value in `[0, 1)`; `None` disables fuzz and rounds
/// the raw interval. The selection formula (`lower + f * (1 + upper - lower)`,
/// floored) is Anki's, and preserves a uniform distribution across the inclusive
/// range.
fn with_review_fuzz(fuzz_factor: Option<f32>, interval: f32, minimum: u32, maximum: u32) -> u32 {
    match fuzz_factor {
        Some(f) => {
            let (lower, upper) = constrained_fuzz_bounds(interval, minimum, maximum);
            (lower as f32 + f * ((1 + upper - lower) as f32)).floor() as u32
        }
        None => (interval.round() as u32).clamp(minimum, maximum),
    }
}

/// Anki's default maximum review interval: 100 years.
const MAX_REVIEW_INTERVAL: u32 = 36_500;

/// Derive the uniform fuzz factor from the same inputs Anki seeds with.
///
/// Anki uses `StdRng::seed_from_u64(card_id + reps).random_range(0.0..1.0)`.
/// We deliberately do **not** try to reproduce that exact random stream:
/// `rand`'s `StdRng` is documented as not reproducible across `rand` releases,
/// so byte-parity with Anki's *chosen* interval is not achievable in a stable
/// way — Anki does not even guarantee it against its own past versions.
///
/// What is reproducible, and what the parity tests below pin, is the **bounds**
/// (pure deterministic arithmetic) and the selection formula. We match Anki's
/// seed *inputs* — card id and rep count, so the jitter varies per review rather
/// than biasing one card the same direction forever — and use a documented
/// integer hash to land in `[0, 1)`.
fn fuzz_factor_for(card_id: i64, reps: i64) -> f32 {
    let seed = (card_id as u64).wrapping_add(reps as u64);
    // SplitMix64 finaliser: a well-distributed integer hash.
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    // Top 24 bits → [0,1), which is plenty of resolution for a day count.
    ((z >> 40) as f32) / ((1u64 << 24) as f32)
}

/// Fuzz an interday interval and return whole days.
fn fuzz_interval(interval: f32, card_id: i64, reps: i64, enabled: bool) -> i64 {
    let factor = enabled.then(|| fuzz_factor_for(card_id, reps));
    with_review_fuzz(factor, interval, 1, MAX_REVIEW_INTERVAL) as i64
}

/// Apply a rating to a card and return its new state.
///
/// `now` is passed in rather than read from the clock so the whole scheduler is
/// deterministic and testable.
pub fn schedule(
    fsrs: &FSRS,
    card_id: i64,
    card: &CardSnapshot,
    rating: Rating,
    now: DateTime<Utc>,
    config: &SchedulerConfig,
) -> anyhow::Result<Scheduled> {
    // --- 1. Ask FSRS for the new memory state -----------------------------
    //
    // `None` for a card that has never been reviewed. See note 1 in the module
    // docs: this is what selects the initial-stability path inside the crate.
    let memory = match (card.stability, card.difficulty) {
        (Some(s), Some(d)) if card.state != CardState::New => {
            Some(MemoryState { stability: s, difficulty: d })
        }
        _ => None,
    };

    // Actual elapsed days, not the interval we asked for. Zero for a card being
    // seen for the first time and for same-day learning steps — which is exactly
    // what routes the crate to its short-term path.
    let days_elapsed = match card.last_review_at {
        Some(last) => retain_days_between(last, now),
        None => 0,
    };

    let next = fsrs
        .next_states(memory, config.desired_retention, days_elapsed)
        .map_err(|e| anyhow::anyhow!("FSRS could not schedule this card: {e:?}"))?;

    let item = match rating {
        Rating::Again => next.again,
        Rating::Hard => next.hard,
        Rating::Good => next.good,
        Rating::Easy => next.easy,
    };

    let stability = item.memory.stability;
    let difficulty = item.memory.difficulty;
    let reps = card.reps + 1;

    // --- 2. Advance the state machine -------------------------------------
    //
    // The memory state above updates on EVERY review, including intraday
    // learning steps. What differs by state is only when the card comes back.

    // `card.reps` is the count BEFORE this answer, matching Anki's seed input,
    // so the jitter differs on each successive review of the same card.
    let graduate = |interval: f32| -> (CardState, DateTime<Utc>, i64, Option<i64>) {
        let days = fuzz_interval(interval, card_id, card.reps, config.fuzz);
        (CardState::Review, now + Duration::days(days), 0, Some(days))
    };

    let (state, due_at, learning_step, interval_days) = match card.state {
        CardState::New | CardState::Learning => {
            let steps = &config.learning_steps;
            let current = card.learning_step.clamp(0, steps.len() as i64 - 1);

            match rating {
                // Back to the first step.
                Rating::Again => (
                    CardState::Learning,
                    now + Duration::minutes(steps[0]),
                    0,
                    None,
                ),
                // Hold position — another go at the same step.
                Rating::Hard => (
                    CardState::Learning,
                    now + Duration::minutes(steps[current as usize]),
                    current,
                    None,
                ),
                // Advance one step; graduate if that was the last one.
                Rating::Good => {
                    let next_step = current + 1;
                    if next_step >= steps.len() as i64 {
                        graduate(item.interval)
                    } else {
                        (
                            CardState::Learning,
                            now + Duration::minutes(steps[next_step as usize]),
                            next_step,
                            None,
                        )
                    }
                }
                // Skip the remaining steps entirely.
                Rating::Easy => graduate(item.interval),
            }
        }

        CardState::Review => match rating {
            // A lapse. This is the only transition that increments `lapses`.
            Rating::Again => {
                let steps = &config.relearning_steps;
                (
                    CardState::Relearning,
                    now + Duration::minutes(steps[0]),
                    0,
                    None,
                )
            }
            _ => graduate(item.interval),
        },

        CardState::Relearning => {
            let steps = &config.relearning_steps;
            let current = card.learning_step.clamp(0, steps.len() as i64 - 1);

            match rating {
                Rating::Again => (
                    CardState::Relearning,
                    now + Duration::minutes(steps[0]),
                    0,
                    None,
                ),
                Rating::Hard => (
                    CardState::Relearning,
                    now + Duration::minutes(steps[current as usize]),
                    current,
                    None,
                ),
                Rating::Good => {
                    let next_step = current + 1;
                    if next_step >= steps.len() as i64 {
                        graduate(item.interval)
                    } else {
                        (
                            CardState::Relearning,
                            now + Duration::minutes(steps[next_step as usize]),
                            next_step,
                            None,
                        )
                    }
                }
                Rating::Easy => graduate(item.interval),
            }
        }
    };

    // Only a Review card falling back counts as a lapse. Failing a card that is
    // still in learning is not a lapse — it never graduated in the first place.
    let lapses = if card.state == CardState::Review && rating == Rating::Again {
        card.lapses + 1
    } else {
        card.lapses
    };

    Ok(Scheduled {
        state,
        stability,
        difficulty,
        due_at,
        reps,
        lapses,
        learning_step,
        interval_days,
    })
}

/// Build the FSRS engine with the published FSRS-6 defaults.
///
/// An empty parameter slice tells the crate to fill in `DEFAULT_PARAMETERS` —
/// the 21-weight FSRS-6 set. We never train or optimise: the brief rules the
/// optimizer out of v1, and hand-tuning weights would be worse than the
/// published defaults on a single user's data anyway.
pub fn engine() -> anyhow::Result<FSRS> {
    FSRS::new(&[]).map_err(|e| anyhow::anyhow!("could not initialise FSRS: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
    }

    fn new_card() -> CardSnapshot {
        CardSnapshot {
            state: CardState::New,
            stability: None,
            difficulty: None,
            last_review_at: None,
            reps: 0,
            lapses: 0,
            learning_step: 0,
        }
    }

    /// The crate ships FSRS-6, which is a 21-weight model. FSRS-5 had 19.
    /// If this ever reads 19 we have silently downgraded a hard requirement.
    #[test]
    fn uses_the_fsrs6_21_parameter_set() {
        assert_eq!(
            fsrs::DEFAULT_PARAMETERS.len(),
            21,
            "expected the FSRS-6 21-parameter set"
        );
    }

    #[test]
    fn a_new_card_gets_real_memory_state_from_the_init_path() {
        let f = engine().unwrap();
        let out = schedule(
            &f,
            1,
            &new_card(),
            Rating::Good,
            at(2026, 8, 12, 9),
            &SchedulerConfig::default(),
        )
        .unwrap();

        // The init path must produce a positive stability. A zero here would
        // mean we took the review branch for a card with no history.
        assert!(out.stability > 0.0, "stability was {}", out.stability);
        assert!(out.difficulty > 0.0);
        assert_eq!(out.reps, 1);
    }

    /// Rating a new card Again must not count as a lapse — it never graduated.
    #[test]
    fn failing_a_new_card_is_not_a_lapse() {
        let f = engine().unwrap();
        let out = schedule(&f, 1, &new_card(), Rating::Again, at(2026, 8, 12, 9), &SchedulerConfig::default()).unwrap();
        assert_eq!(out.lapses, 0);
        assert_eq!(out.state, CardState::Learning);
    }

    /// Failing a graduated card IS a lapse, and sends it to relearning.
    #[test]
    fn failing_a_review_card_is_a_lapse_and_relearns() {
        let f = engine().unwrap();
        let card = CardSnapshot {
            state: CardState::Review,
            stability: Some(20.0),
            difficulty: Some(5.0),
            last_review_at: Some(at(2026, 8, 1, 9)),
            reps: 6,
            lapses: 1,
            learning_step: 0,
        };
        let out = schedule(&f, 1, &card, Rating::Again, at(2026, 8, 12, 9), &SchedulerConfig::default()).unwrap();

        assert_eq!(out.state, CardState::Relearning);
        assert_eq!(out.lapses, 2);
        assert_eq!(out.interval_days, None, "relearning is intraday");
        // Stability must fall after a lapse.
        assert!(out.stability < 20.0, "stability rose after a lapse: {}", out.stability);
    }

    /// Learning steps walk forward on Good and graduate off the last step.
    #[test]
    fn learning_steps_advance_then_graduate() {
        let f = engine().unwrap();
        let cfg = SchedulerConfig::default(); // steps [1, 10]
        let now = at(2026, 8, 12, 9);

        let first = schedule(&f, 1, &new_card(), Rating::Good, now, &cfg).unwrap();
        assert_eq!(first.state, CardState::Learning);
        assert_eq!(first.learning_step, 1);
        assert_eq!(first.due_at, now + Duration::minutes(10));

        let mid = CardSnapshot {
            state: CardState::Learning,
            stability: Some(first.stability),
            difficulty: Some(first.difficulty),
            last_review_at: Some(now),
            reps: first.reps,
            lapses: 0,
            learning_step: 1,
        };
        let second = schedule(&f, 1, &mid, Rating::Good, now + Duration::minutes(10), &cfg).unwrap();
        assert_eq!(second.state, CardState::Review, "should graduate off the last step");
        assert!(second.interval_days.unwrap() >= 1);
    }

    /// Easy skips the remaining learning steps.
    #[test]
    fn easy_graduates_immediately() {
        let f = engine().unwrap();
        let out = schedule(&f, 1, &new_card(), Rating::Easy, at(2026, 8, 12, 9), &SchedulerConfig::default()).unwrap();
        assert_eq!(out.state, CardState::Review);
        assert!(out.interval_days.is_some());
    }

    /// A same-day second review must take the crate's short-term path rather
    /// than being treated as a zero-day interday review. The observable
    /// signature is that it still produces a usable state and doesn't panic or
    /// collapse stability to the floor.
    #[test]
    fn same_day_review_uses_the_short_term_path() {
        let f = engine().unwrap();
        let now = at(2026, 8, 12, 9);
        let card = CardSnapshot {
            state: CardState::Learning,
            stability: Some(3.0),
            difficulty: Some(5.0),
            last_review_at: Some(now), // same instant → 0 days elapsed
            reps: 1,
            lapses: 0,
            learning_step: 0,
        };
        let out = schedule(&f, 1, &card, Rating::Good, now + Duration::minutes(5), &SchedulerConfig::default()).unwrap();
        assert!(out.stability > 0.0);
    }

    /// Elapsed days must come from the app-wide Retain-day policy, so a late
    /// evening review followed by an after-midnight one is ZERO days elapsed
    /// (same Retain day) rather than one. Under a midnight boundary FSRS would
    /// be told a full day passed after a couple of hours.
    #[test]
    fn elapsed_days_follow_the_retain_day_boundary() {
        use chrono::{Local, TimeZone};
        let evening = Local.with_ymd_and_hms(2026, 8, 12, 23, 0, 0).single().unwrap().with_timezone(&Utc);
        let after_midnight = Local.with_ymd_and_hms(2026, 8, 13, 1, 0, 0).single().unwrap().with_timezone(&Utc);

        assert_eq!(retain_days_between(evening, after_midnight), 0);
        assert_eq!(retain_days_between(after_midnight, evening), 0, "never negative");

        let next_afternoon = Local.with_ymd_and_hms(2026, 8, 13, 15, 0, 0).single().unwrap().with_timezone(&Utc);
        assert_eq!(retain_days_between(evening, next_afternoon), 1);
    }

    // -----------------------------------------------------------------------
    // Fuzz parity with Anki's reference implementation.
    //
    // Every vector below is copied verbatim from Anki's own test in
    // rslib/src/scheduler/states/fuzz.rs (`fn with_review_fuzz`). If our port
    // drifts from the reference, these fail.
    // -----------------------------------------------------------------------

    /// Anki's `assert_lower_middle_upper!` macro: factor 0.0 hits the bottom of
    /// the range, 0.5 the middle, 0.99 the top.
    fn lmu(interval: f32, minimum: u32, maximum: u32) -> (u32, u32, u32) {
        (
            with_review_fuzz(Some(0.0), interval, minimum, maximum),
            with_review_fuzz(Some(0.5), interval, minimum, maximum),
            with_review_fuzz(Some(0.99), interval, minimum, maximum),
        )
    }

    #[test]
    fn anki_parity_no_fuzz_factor_just_rounds_and_clamps() {
        assert_eq!(with_review_fuzz(None, 1.5, 1, 100), 2);
        assert_eq!(with_review_fuzz(None, 0.1, 1, 100), 1);
        assert_eq!(with_review_fuzz(None, 101.0, 1, 100), 100);
    }

    #[test]
    fn anki_parity_short_intervals_are_not_fuzzed() {
        assert_eq!(lmu(1.0, 1, 1000), (1, 1, 1));
        assert_eq!(lmu(2.49, 1, 1000), (2, 2, 2));
    }

    /// The base 1 day plus cumulative band contributions. These are exactly the
    /// values the earlier flat-percentage implementation got wrong.
    #[test]
    fn anki_parity_band_accumulation() {
        assert_eq!(lmu(2.5, 1, 1000), (2, 3, 4));
        assert_eq!(lmu(7.0, 1, 1000), (5, 7, 9));
        assert_eq!(lmu(17.0, 1, 1000), (14, 17, 20));
        // The regression case: flat ±5% gave [35, 39] here.
        assert_eq!(lmu(37.0, 1, 1000), (33, 37, 41));
    }

    #[test]
    fn anki_parity_band_transitions_are_continuous() {
        assert_eq!(lmu(6.9, 3, 1000), (5, 7, 9));
        assert_eq!(lmu(7.0, 3, 1000), (5, 7, 9));
        assert_eq!(lmu(7.1, 3, 1000), (5, 7, 9));
        assert_eq!(lmu(19.9, 3, 1000), (17, 20, 23));
        assert_eq!(lmu(20.0, 3, 1000), (17, 20, 23));
        assert_eq!(lmu(20.1, 3, 1000), (17, 20, 23));
    }

    #[test]
    fn anki_parity_minimum_widens_a_collapsed_range() {
        assert_eq!(lmu(2.0, 2, 1000), (2, 2, 2));
        assert_eq!(lmu(2.0, 3, 1000), (3, 4, 4));
        assert_eq!(lmu(2.0, 3, 3), (3, 3, 3));
    }

    #[test]
    fn anki_parity_limits_preserve_uniform_distribution() {
        assert_eq!(lmu(100.0, 101, 1000), (101, 105, 108));
        assert_eq!(lmu(100.0, 1, 99), (92, 96, 99));
        assert_eq!(lmu(100.0, 97, 103), (97, 100, 103));
    }

    #[test]
    fn anki_parity_invalid_bounds_do_not_panic() {
        let _ = constrained_fuzz_bounds(1.0, 3, 2);
    }

    /// Our own layer on top: the factor must be uniform-ish in [0,1), stable for
    /// a given (card, rep), and must CHANGE as reps advance — a factor keyed on
    /// card id alone would bias every review of a card the same direction.
    #[test]
    fn fuzz_factor_is_deterministic_but_varies_per_rep() {
        for id in 1..50i64 {
            for reps in 0..5i64 {
                let f = fuzz_factor_for(id, reps);
                assert!((0.0..1.0).contains(&f), "factor {f} out of range");
                assert_eq!(f, fuzz_factor_for(id, reps), "must be deterministic");
            }
        }

        let varies = (0..40i64).any(|id| fuzz_factor_for(id, 0) != fuzz_factor_for(id, 1));
        assert!(varies, "fuzz must change as reps advance");

        // Rough uniformity: over many cards, both halves get used.
        let low = (0..2000i64).filter(|i| fuzz_factor_for(*i, 0) < 0.5).count();
        assert!((700..1300).contains(&low), "factor looks badly skewed: {low}/2000 below 0.5");
    }

    #[test]
    fn fuzz_can_be_disabled_for_exact_intervals() {
        assert_eq!(fuzz_interval(30.0, 7, 3, false), 30);
        assert_eq!(fuzz_interval(1.0, 7, 3, true), 1);
    }

    /// Higher desired retention must produce shorter intervals — the core
    /// tuning knob has to actually do something, in the right direction.
    #[test]
    fn higher_retention_shortens_intervals() {
        let f = engine().unwrap();
        let card = CardSnapshot {
            state: CardState::Review,
            stability: Some(25.0),
            difficulty: Some(5.0),
            last_review_at: Some(at(2026, 8, 1, 9)),
            reps: 5,
            lapses: 0,
            learning_step: 0,
        };
        let now = at(2026, 8, 20, 9);

        let relaxed = schedule(&f, 1, &card, Rating::Good, now,
            &SchedulerConfig { desired_retention: 0.80, fuzz: false, ..Default::default() }).unwrap();
        let strict = schedule(&f, 1, &card, Rating::Good, now,
            &SchedulerConfig { desired_retention: 0.95, fuzz: false, ..Default::default() }).unwrap();

        assert!(
            strict.interval_days.unwrap() < relaxed.interval_days.unwrap(),
            "0.95 gave {:?}, 0.80 gave {:?}",
            strict.interval_days,
            relaxed.interval_days
        );
    }

    /// Button ordering must hold: Again ≤ Hard ≤ Good ≤ Easy in interval.
    #[test]
    fn button_intervals_are_ordered() {
        let f = engine().unwrap();
        let card = CardSnapshot {
            state: CardState::Review,
            stability: Some(15.0),
            difficulty: Some(5.0),
            last_review_at: Some(at(2026, 8, 1, 9)),
            reps: 4,
            lapses: 0,
            learning_step: 0,
        };
        let now = at(2026, 8, 16, 9);
        let cfg = SchedulerConfig { fuzz: false, ..Default::default() };

        let hard = schedule(&f, 1, &card, Rating::Hard, now, &cfg).unwrap();
        let good = schedule(&f, 1, &card, Rating::Good, now, &cfg).unwrap();
        let easy = schedule(&f, 1, &card, Rating::Easy, now, &cfg).unwrap();

        assert!(hard.interval_days <= good.interval_days);
        assert!(good.interval_days <= easy.interval_days);
        // And difficulty should move the right way.
        assert!(easy.difficulty <= good.difficulty);
        assert!(good.difficulty <= hard.difficulty);
    }
}
