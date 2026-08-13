# The day boundary

**A Retain day runs from 4am local time to 4am local time the next morning.**

One policy, one implementation, applied to everything that buckets by day. It lives in
`src-tauri/src/util.rs` as `retain_day_of` / `retain_day_naive` / `retain_days_between`, and
nothing in the app is permitted to compute a day bucket any other way.

## What uses it

| Consumer | Where |
| --- | --- |
| Session day (contribution grid) | `timer.rs::start` → `sessions.local_date` |
| Streak qualifying days, freezes, rest days | `streak.rs::reconcile`, `summary`, `grid` |
| FSRS elapsed days | `scheduler.rs::schedule` → `retain_days_between` |
| Weekly goal rings | `streak.rs::week_start` |
| Card due dates, review queue, new-card caps | `cards.due_on` (Checkpoint 2) |
| Error-log revisit dates | `error_entries.revisit_on` |
| Notification daily cap | `notification_log.local_date` |

## Why 4am and not local midnight

A student who finishes at 1am does not think of that as tomorrow's work. Under a midnight
boundary that session would:

- land on **tomorrow's square** in the contribution grid,
- count toward **tomorrow's** streak while leaving today blank — so a late night of study could
  *break* the very streak it should have earned,
- consume **tomorrow's** new-card allowance before tomorrow began,
- and register as a **full day elapsed** to FSRS after two hours, lengthening the next interval
  on a card that was just reviewed.

The 4am boundary makes all four behave the way the person means. It is also Anki's default, so
the elapsed-day counts Retain feeds FSRS match the ecosystem the algorithm was tuned against.

## Why it is a constant, not a setting

Because changing it would rewrite the past. A user who moved the boundary would retroactively
shift which day old sessions belonged to — moving grid squares and potentially breaking a streak
that had already been earned. The value is fixed at `util::DAY_ROLLOVER_HOUR` and documented
here instead.

## What is *not* affected

- **Timestamps.** Every instant is stored as RFC 3339 UTC. The boundary applies only to the
  derived `local_date` / `due_on` columns.
- **Snapshot and export filenames.** `db.rs::snapshot` and `commands.rs::export_to_file` stamp
  filenames with the plain local clock, because a filename is not a day bucket.

## Tests

`util.rs` pins the policy directly: 1am and 3:59am belong to the previous day, 4:00am starts the
new one, and an 11pm → 1am pair is **zero** days elapsed rather than one. That last case is the
one a midnight boundary gets wrong, and it feeds straight into FSRS scheduling.
