# What earns a streak day

The brief says a streak is earned by "a meaningful action — one focused session OR clearing due
reviews — never by opening the app." That sentence needs to become something a database query can
decide, and the follow-up constraint was explicit that no threshold should be treated as an
established requirement just because I wrote it down once. So here is the rule, with reasoning.

---

## The rule

A local calendar day **D** is a *qualifying day* if **either** branch below is true.

### Branch A — a focused session

There exists a **completed** session with `local_date = D` whose **active seconds** are at least
`focused_session_minutes` (a setting, default 20 — see below).

**Active seconds** is not wall-clock time. It is:

```
active_seconds = (ended_at - started_at)
               - (time spent manually paused)
               - (time spent auto-paused by the idle detector)
```

Every pause interval is stored as its own row in `session_pauses` with a reason of `manual` or
`idle`, and `active_seconds` is computed by summing the gaps between them. A session that ran for
three hours with the laptop untouched contributes nothing, which is the entire point of the brief's
"without this the data is fiction" note about idle detection.

The session must be **completed**. A running timer never earns the day — `active_seconds` is only
final at stop.

### Branch B — reviews genuinely cleared

**All** of the following hold:

1. The number of items **due on D** was greater than zero. You cannot clear an empty queue. A day
   with no reviews due earns nothing through this branch.
2. Every item due on D has a corresponding row in `review_log` with `local_date = D`.
3. Each of those rows carries `presented_at`, `rated_at`, and a `rating` in 1–4.

Point 3 is what stops this being a formality. A review is only logged when the item was actually
**shown** and then **graded** — the row cannot exist without both timestamps, and `duration_ms`
(`rated_at − presented_at`) is stored alongside so the record stays auditable after the fact.

Bumping a due date does not create a `review_log` row. Marking rows complete in bulk does not
create one. Nothing in the scheduler writes to `review_log` except the act of rating a presented
item.

### Neither branch fires on app launch

There is no code path where opening the window, viewing the grid, or navigating to the review
screen writes anything the streak reads. Both branches require a completed action with a duration
attached to it.

---

## Why the default is 20 minutes, and why it is a setting

This is the number I was told not to smuggle in as a requirement, so: it is a default, it is
adjustable in Settings (5–120 minutes), and here is the actual reasoning.

The app's own Pomodoro default is a **25-minute** work block. Anchoring "one focused session" to
one complete Pomodoro is the only non-arbitrary anchor available — it is the unit of work the app
already asks you to do.

But setting the threshold *at* 25 minutes creates a bad failure: the idle detector auto-pauses after
2 minutes of no input, so a Pomodoro where you sat and read a textbook without touching the
trackpad finishes with roughly 23 minutes of active time and **fails**, despite being exactly the
behaviour the app wants to reward. Anchoring at the block length makes idle detection and the
streak fight each other.

So the threshold sits one notch below one complete Pomodoro: **20 minutes**, leaving 5 minutes of
slack for idle pauses inside a session that genuinely happened. Stopwatch users get the same bar.

If 20 turns out to be wrong after two weeks of real use, it is one field in Settings.

---

## Freezes and rest days

These sit *outside* the qualifying-day test — they affect whether a **gap** breaks the run, never
whether a day was earned.

- **Rest days** are weekdays you nominate. A non-qualifying rest day does not break the streak and
  does not consume a freeze. It also does not extend the streak count.
- **Freezes**: up to 2 held at once. A non-qualifying, non-rest day consumes one automatically and
  the run survives. With none available, the run ends. Freezes regenerate slowly (one per 7
  qualifying days, capped at 2), recorded in `streak_freezes` as grant/consume rows so the history
  is inspectable rather than a counter that drifts.

## Framing

The streak surface shows the current run, freezes remaining, and what would earn today. It never
shows what you are about to lose, never counts down to a break, and never uses the word "don't".
Copy is checked against this on every screen that touches streak state.
