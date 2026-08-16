//! ICS parsing tests.
//!
//! Melbourne is the zone under test throughout, because that's where the app is
//! used and because it has a southern-hemisphere DST schedule: clocks go
//! forward in October and back in April, which is exactly backwards from every
//! northern-hemisphere example a parser tends to be checked against.

use super::*;
use chrono::Timelike;
use chrono_tz::Australia::Melbourne;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 13, 0, 0, 0).unwrap()
}

fn cal(body: &str) -> String {
    format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//EN\r\n{body}\r\nEND:VCALENDAR\r\n")
}

fn parse(body: &str) -> Vec<CalendarEvent> {
    parse_calendar(&cal(body), Melbourne, now()).expect("calendar should parse")
}

// ---------------------------------------------------------------------------
// Line handling
// ---------------------------------------------------------------------------

#[test]
fn folded_lines_are_rejoined_without_the_fold_character() {
    let lines = unfold("SUMMARY:Biology SAC\r\n  part two\r\nUID:1\r\n");
    assert_eq!(lines[0], "SUMMARY:Biology SAC part two");
    assert_eq!(lines[1], "UID:1");
}

#[test]
fn parameters_and_quoted_colons_survive_parsing() {
    let l = parse_line("DTSTART;TZID=\"Australia/Melbourne\":20260813T090000").unwrap();
    assert_eq!(l.name, "DTSTART");
    assert_eq!(l.param("TZID"), Some("Australia/Melbourne"));
    assert_eq!(l.value, "20260813T090000");
}

#[test]
fn escaped_text_is_unescaped() {
    assert_eq!(unescape(r"Line one\nLine two\, with comma"), "Line one\nLine two, with comma");
    assert_eq!(unescape(r"a\\b"), r"a\b");
}

// ---------------------------------------------------------------------------
// Timezones
// ---------------------------------------------------------------------------

#[test]
fn a_tzid_event_converts_to_the_right_instant() {
    // 9am Melbourne in August is AEST (UTC+10), so 23:00 UTC the day before.
    let ev = &parse(
        "BEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Biology\r\n\
         DTSTART;TZID=Australia/Melbourne:20260813T090000\r\n\
         DTEND;TZID=Australia/Melbourne:20260813T100000\r\nEND:VEVENT",
    )[0];

    assert_eq!(ev.starts_at, "2026-08-12T23:00:00Z");
    assert_eq!(ev.ends_at.as_deref(), Some("2026-08-13T00:00:00Z"));
    // The local date is the Melbourne date, not the UTC one.
    assert_eq!(ev.local_date, "2026-08-13");
}

#[test]
fn a_utc_event_is_taken_at_face_value() {
    let ev = &parse(
        "BEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Exam\r\nDTSTART:20260813T233000Z\r\nEND:VEVENT",
    )[0];
    assert_eq!(ev.starts_at, "2026-08-13T23:30:00Z");
    // 23:30 UTC is 9:30am the next morning in Melbourne.
    assert_eq!(ev.local_date, "2026-08-14");
}

#[test]
fn a_floating_time_is_read_as_local() {
    let ev = &parse("BEGIN:VEVENT\r\nUID:c\r\nSUMMARY:x\r\nDTSTART:20260813T090000\r\nEND:VEVENT")[0];
    assert_eq!(ev.starts_at, "2026-08-12T23:00:00Z");
}

#[test]
fn prefixed_and_windows_tzids_still_resolve() {
    assert_eq!(resolve_tzid("Australia/Melbourne"), Some(Melbourne));
    assert_eq!(
        resolve_tzid("/mozilla.org/20070129_1/Australia/Melbourne"),
        Some(Melbourne)
    );
    assert_eq!(resolve_tzid("AUS Eastern Standard Time"), Some(chrono_tz::Australia::Sydney));
    assert_eq!(resolve_tzid("Not A Zone"), None);
}

// ---------------------------------------------------------------------------
// All-day and midnight
// ---------------------------------------------------------------------------

#[test]
fn an_all_day_event_lands_on_its_own_date() {
    let ev = &parse(
        "BEGIN:VEVENT\r\nUID:d\r\nSUMMARY:Athletics day\r\n\
         DTSTART;VALUE=DATE:20260814\r\nDTEND;VALUE=DATE:20260815\r\nEND:VEVENT",
    )[0];

    assert!(ev.all_day);
    assert_eq!(ev.local_date, "2026-08-14");
    // Midnight Melbourne, not midnight UTC — the latter would show it a day early.
    assert_eq!(ev.starts_at, "2026-08-13T14:00:00Z");
}

#[test]
fn an_event_crossing_midnight_keeps_the_day_it_starts_on() {
    let ev = &parse(
        "BEGIN:VEVENT\r\nUID:e\r\nSUMMARY:Late thing\r\n\
         DTSTART;TZID=Australia/Melbourne:20260813T230000\r\n\
         DTEND;TZID=Australia/Melbourne:20260814T010000\r\nEND:VEVENT",
    )[0];

    assert_eq!(ev.local_date, "2026-08-13");
    assert_eq!(ev.ends_at.as_deref(), Some("2026-08-13T15:00:00Z"));
}

#[test]
fn a_duration_is_used_when_there_is_no_dtend() {
    let ev = &parse(
        "BEGIN:VEVENT\r\nUID:f\r\nSUMMARY:Period\r\n\
         DTSTART;TZID=Australia/Melbourne:20260813T090000\r\nDURATION:PT50M\r\nEND:VEVENT",
    )[0];
    assert_eq!(ev.ends_at.as_deref(), Some("2026-08-12T23:50:00Z"));
}

// ---------------------------------------------------------------------------
// DST — the part most likely to be quietly wrong
// ---------------------------------------------------------------------------

/// Melbourne moves to daylight time at 2am on the first Sunday of October 2026
/// (4 October). A weekly 9am class must still be 9am local afterwards, which
/// means its UTC time *changes* from 23:00 to 22:00.
#[test]
fn a_weekly_event_keeps_its_wall_clock_across_the_spring_transition() {
    let events = parse_calendar(
        &cal("BEGIN:VEVENT\r\nUID:dst1\r\nSUMMARY:Biology\r\n\
              DTSTART;TZID=Australia/Melbourne:20260930T090000\r\n\
              DTEND;TZID=Australia/Melbourne:20260930T100000\r\n\
              RRULE:FREQ=WEEKLY;COUNT=4\r\nEND:VEVENT"),
        Melbourne,
        Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(),
    )
    .unwrap();

    assert_eq!(events.len(), 4);

    for e in &events {
        let local = DateTime::parse_from_rfc3339(&e.starts_at)
            .unwrap()
            .with_timezone(&Melbourne);
        assert_eq!(local.time(), NaiveTime::from_hms_opt(9, 0, 0).unwrap(), "{}", e.starts_at);
    }

    // Before the change: UTC+10. After: UTC+11.
    assert_eq!(events[0].starts_at, "2026-09-29T23:00:00Z");
    assert_eq!(events[1].starts_at, "2026-10-06T22:00:00Z");
}

/// The same in the other direction — clocks go back on 5 April 2026.
#[test]
fn a_weekly_event_keeps_its_wall_clock_across_the_autumn_transition() {
    let events = parse_calendar(
        &cal("BEGIN:VEVENT\r\nUID:dst2\r\nSUMMARY:Chem\r\n\
              DTSTART;TZID=Australia/Melbourne:20260401T140000\r\n\
              RRULE:FREQ=WEEKLY;COUNT=3\r\nEND:VEVENT"),
        Melbourne,
        Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap(),
    )
    .unwrap();

    assert_eq!(events[0].starts_at, "2026-04-01T03:00:00Z"); // AEDT, UTC+11
    assert_eq!(events[1].starts_at, "2026-04-08T04:00:00Z"); // AEST, UTC+10
}

/// 2:30am on the spring-forward morning never happens. The event must land
/// somewhere real rather than being dropped or panicking.
#[test]
fn a_time_inside_the_spring_forward_gap_is_moved_not_lost() {
    let events = parse_calendar(
        &cal("BEGIN:VEVENT\r\nUID:gap\r\nSUMMARY:Impossible\r\n\
              DTSTART;TZID=Australia/Melbourne:20261004T023000\r\nEND:VEVENT"),
        Melbourne,
        now(),
    )
    .unwrap();

    assert_eq!(events.len(), 1);
    let local = DateTime::parse_from_rfc3339(&events[0].starts_at)
        .unwrap()
        .with_timezone(&Melbourne);
    // Pushed past the gap into 3am-something, still on the same day.
    assert_eq!(local.date_naive(), NaiveDate::from_ymd_opt(2026, 10, 4).unwrap());
    assert!(local.hour() >= 3, "expected past the gap, got {local}");
}

/// The repeated hour in autumn is ambiguous; the earlier instant is chosen.
#[test]
fn an_ambiguous_autumn_time_resolves_to_the_earlier_instant() {
    let events = parse_calendar(
        &cal("BEGIN:VEVENT\r\nUID:amb\r\nSUMMARY:Twice\r\n\
              DTSTART;TZID=Australia/Melbourne:20260405T023000\r\nEND:VEVENT"),
        Melbourne,
        now(),
    )
    .unwrap();

    // 2:30am AEDT (UTC+11) = 15:30Z on the 4th, which is the earlier of the two.
    assert_eq!(events[0].starts_at, "2026-04-04T15:30:00Z");
}

// ---------------------------------------------------------------------------
// Recurrence
// ---------------------------------------------------------------------------

#[test]
fn a_weekly_byday_rule_produces_one_event_per_listed_day() {
    let events = parse(
        "BEGIN:VEVENT\r\nUID:r1\r\nSUMMARY:Period 1\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\n\
         RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR;COUNT=6\r\nEND:VEVENT",
    );

    assert_eq!(events.len(), 6);
    for e in &events {
        let wd = DateTime::parse_from_rfc3339(&e.starts_at)
            .unwrap()
            .with_timezone(&Melbourne)
            .weekday();
        assert!(
            matches!(wd, chrono::Weekday::Mon | chrono::Weekday::Wed | chrono::Weekday::Fri),
            "unexpected weekday {wd}"
        );
    }
}

#[test]
fn count_and_until_both_bound_a_series() {
    let counted = parse(
        "BEGIN:VEVENT\r\nUID:r2\r\nSUMMARY:x\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\n\
         RRULE:FREQ=DAILY;COUNT=3\r\nEND:VEVENT",
    );
    assert_eq!(counted.len(), 3);

    // UNTIL is an absolute instant, and it is inclusive. 20260819T235900Z is
    // 09:59 on the *20th* in Melbourne, so the 9am occurrence on the 20th is
    // still inside the bound — four events, not three. Reading UNTIL as a local
    // date would drop a legitimate class.
    let bounded = parse(
        "BEGIN:VEVENT\r\nUID:r3\r\nSUMMARY:x\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\n\
         RRULE:FREQ=DAILY;UNTIL=20260819T235900Z\r\nEND:VEVENT",
    );
    let dates: Vec<_> = bounded.iter().map(|e| e.local_date.as_str()).collect();
    assert_eq!(dates, vec!["2026-08-17", "2026-08-18", "2026-08-19", "2026-08-20"]);

    // An UNTIL that falls before the next occurrence does cut it off.
    let tight = parse(
        "BEGIN:VEVENT\r\nUID:r3b\r\nSUMMARY:x\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\n\
         RRULE:FREQ=DAILY;UNTIL=20260818T230000Z\r\nEND:VEVENT",
    );
    let dates: Vec<_> = tight.iter().map(|e| e.local_date.as_str()).collect();
    assert_eq!(dates, vec!["2026-08-17", "2026-08-18", "2026-08-19"]);
}

#[test]
fn interval_skips_the_right_number_of_periods() {
    let events = parse(
        "BEGIN:VEVENT\r\nUID:r4\r\nSUMMARY:Fortnightly\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\n\
         RRULE:FREQ=WEEKLY;INTERVAL=2;COUNT=3\r\nEND:VEVENT",
    );

    let dates: Vec<_> = events.iter().map(|e| e.local_date.as_str()).collect();
    assert_eq!(dates, vec!["2026-08-17", "2026-08-31", "2026-09-14"]);
}

#[test]
fn monthly_recurrence_clamps_a_day_that_does_not_exist() {
    // 31 January + 1 month must not vanish.
    let next = add_months(
        NaiveDate::from_ymd_opt(2026, 1, 31).unwrap().and_hms_opt(9, 0, 0).unwrap(),
        1,
    )
    .unwrap();
    assert_eq!(next.date(), NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
}

#[test]
fn an_excluded_date_is_removed_from_the_series() {
    let events = parse(
        "BEGIN:VEVENT\r\nUID:r5\r\nSUMMARY:x\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\n\
         RRULE:FREQ=DAILY;COUNT=3\r\n\
         EXDATE;TZID=Australia/Melbourne:20260818T090000\r\nEND:VEVENT",
    );

    let dates: Vec<_> = events.iter().map(|e| e.local_date.as_str()).collect();
    assert_eq!(dates, vec!["2026-08-17", "2026-08-19"]);
}

/// One instance of a series moved to a different time replaces that instance —
/// it must not appear twice.
#[test]
fn a_modified_instance_replaces_the_original_rather_than_duplicating() {
    let events = parse(
        "BEGIN:VEVENT\r\nUID:r6\r\nSUMMARY:Biology\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\n\
         RRULE:FREQ=DAILY;COUNT=3\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:r6\r\nSUMMARY:Biology (moved to the lab)\r\n\
         RECURRENCE-ID;TZID=Australia/Melbourne:20260818T090000\r\n\
         DTSTART;TZID=Australia/Melbourne:20260818T140000\r\nEND:VEVENT",
    );

    assert_eq!(events.len(), 3);

    let moved: Vec<_> = events.iter().filter(|e| e.summary.contains("lab")).collect();
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].starts_at, "2026-08-18T04:00:00Z"); // 2pm AEST

    // And the original 9am slot on that day is gone.
    assert!(!events.iter().any(|e| e.starts_at == "2026-08-17T23:00:00Z"
        && e.local_date == "2026-08-18"));
}

#[test]
fn a_cancelled_instance_is_dropped_from_the_series() {
    let events = parse(
        "BEGIN:VEVENT\r\nUID:r7\r\nSUMMARY:Class\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\n\
         RRULE:FREQ=DAILY;COUNT=3\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:r7\r\nSUMMARY:Class\r\nSTATUS:CANCELLED\r\n\
         RECURRENCE-ID;TZID=Australia/Melbourne:20260818T090000\r\n\
         DTSTART;TZID=Australia/Melbourne:20260818T090000\r\nEND:VEVENT",
    );

    assert_eq!(events.len(), 2);
    assert!(!events.iter().any(|e| e.local_date == "2026-08-18"));
}

#[test]
fn a_cancelled_one_off_event_never_appears() {
    let events = parse(
        "BEGIN:VEVENT\r\nUID:r8\r\nSUMMARY:Gone\r\nSTATUS:CANCELLED\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\nEND:VEVENT",
    );
    assert!(events.is_empty());
}

/// A rule with no COUNT or UNTIL repeats forever. It must be bounded by the
/// horizon rather than filling the database.
#[test]
fn an_unbounded_rule_stops_at_the_horizon() {
    let events = parse(
        "BEGIN:VEVENT\r\nUID:r9\r\nSUMMARY:Forever\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\n\
         RRULE:FREQ=DAILY\r\nEND:VEVENT",
    );

    assert!(events.len() <= MAX_OCCURRENCES);
    assert!(events.len() > 300, "expected roughly a year, got {}", events.len());

    let last = DateTime::parse_from_rfc3339(&events.last().unwrap().starts_at).unwrap();
    assert!(last.with_timezone(&Utc) <= now() + Duration::days(HORIZON_DAYS + 1));
}

/// Sub-daily frequencies would expand to tens of thousands of rows and never
/// appear in a school timetable, so the rule is ignored and the event is kept
/// as a single occurrence.
#[test]
fn a_sub_daily_frequency_is_not_expanded() {
    let events = parse(
        "BEGIN:VEVENT\r\nUID:r10\r\nSUMMARY:Nope\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\n\
         RRULE:FREQ=MINUTELY;COUNT=5000\r\nEND:VEVENT",
    );
    assert_eq!(events.len(), 1);
}

// ---------------------------------------------------------------------------
// Robustness
// ---------------------------------------------------------------------------

#[test]
fn an_empty_calendar_parses_to_nothing() {
    assert!(parse("").is_empty());
}

#[test]
fn input_that_is_not_a_calendar_is_rejected_clearly() {
    let err = parse_calendar("<html><body>Login required</body></html>", Melbourne, now())
        .unwrap_err()
        .to_string();
    assert!(err.contains("ICS feed"), "unhelpful message: {err}");
}

/// One broken VEVENT must not cost you the rest of the timetable.
#[test]
fn a_malformed_event_is_skipped_and_the_others_survive() {
    let events = parse(
        "BEGIN:VEVENT\r\nUID:ok1\r\nSUMMARY:Good\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nSUMMARY:No UID at all\r\nDTSTART:garbage\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:ok2\r\nSUMMARY:Also good\r\n\
         DTSTART;TZID=Australia/Melbourne:20260818T090000\r\nEND:VEVENT",
    );

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].summary, "Good");
    assert_eq!(events[1].summary, "Also good");
}

/// A VTIMEZONE block has its own DTSTART lines describing DST rules. Reading
/// those as events would create phantom entries dated 1970.
#[test]
fn vtimezone_blocks_do_not_become_events() {
    let events = parse(
        "BEGIN:VTIMEZONE\r\nTZID:Australia/Melbourne\r\n\
         BEGIN:STANDARD\r\nDTSTART:19700405T030000\r\nTZOFFSETFROM:+1100\r\n\
         TZOFFSETTO:+1000\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
         BEGIN:VEVENT\r\nUID:only\r\nSUMMARY:The only event\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\nEND:VEVENT",
    );

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].summary, "The only event");
}

#[test]
fn an_event_with_no_summary_gets_a_readable_placeholder() {
    let events = parse(
        "BEGIN:VEVENT\r\nUID:nosum\r\nDTSTART;TZID=Australia/Melbourne:20260817T090000\r\nEND:VEVENT",
    );
    assert_eq!(events[0].summary, "(untitled)");
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/// The real migration chain, not a hand-picked subset.
///
/// This used to apply 001 and 002 by hand, which meant the schema these tests
/// ran against silently stopped being the schema the app has — a column added
/// in a later migration was missing here and every calendar test failed on it.
/// A fixture that drifts from production tests the fixture.
fn db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::db::run_migrations(&conn).unwrap();
    conn
}

/// Syncing the same feed twice must leave the same number of rows, not double
/// them. This is the bug that makes a calendar integration useless within a
/// week.
#[test]
fn syncing_twice_does_not_duplicate_anything() {
    let mut conn = db();
    let events = parse(
        "BEGIN:VEVENT\r\nUID:s1\r\nSUMMARY:Biology\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\n\
         RRULE:FREQ=DAILY;COUNT=5\r\nEND:VEVENT",
    );

    let first = store(&mut conn, &events).unwrap();
    let second = store(&mut conn, &events).unwrap();
    assert_eq!(first, second);

    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM calendar_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 5);
}

/// An event removed upstream must disappear locally, or a cancelled SAC stays
/// on the Today screen forever.
#[test]
fn an_event_removed_upstream_disappears_locally() {
    let mut conn = db();

    let before = parse(
        "BEGIN:VEVENT\r\nUID:keep\r\nSUMMARY:Stays\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\nEND:VEVENT\r\n\
         BEGIN:VEVENT\r\nUID:drop\r\nSUMMARY:Goes\r\n\
         DTSTART;TZID=Australia/Melbourne:20260818T090000\r\nEND:VEVENT",
    );
    store(&mut conn, &before).unwrap();

    let after = parse(
        "BEGIN:VEVENT\r\nUID:keep\r\nSUMMARY:Stays\r\n\
         DTSTART;TZID=Australia/Melbourne:20260817T090000\r\nEND:VEVENT",
    );
    store(&mut conn, &after).unwrap();

    let uids: Vec<String> = conn
        .prepare("SELECT uid FROM calendar_events")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(uids, vec!["keep".to_string()]);
}

#[test]
fn status_reports_counts_and_settings() {
    let mut conn = db();
    crate::settings::set(&conn, "ics_enabled", "1").unwrap();
    crate::settings::set(&conn, "ics_url", "https://example.test/feed.ics").unwrap();

    store(
        &mut conn,
        &parse(
            "BEGIN:VEVENT\r\nUID:z\r\nSUMMARY:x\r\n\
             DTSTART;TZID=Australia/Melbourne:20260817T090000\r\nEND:VEVENT",
        ),
    )
    .unwrap();

    let s = status(&conn).unwrap();
    assert!(s.enabled);
    assert_eq!(s.event_count, 1);
    assert_eq!(s.url, "https://example.test/feed.ics");
}

/// `upcoming` must not return events that already happened.
#[test]
fn upcoming_excludes_the_past() {
    let conn = db();
    let today = Utc::now().with_timezone(&local_tz()).date_naive();

    let rows = [
        (today - Duration::days(3), "old"),
        (today + Duration::days(1), "soon"),
        (today + Duration::days(400), "far"),
    ];
    for (d, uid) in rows {
        conn.execute(
            "INSERT INTO calendar_events (uid,summary,starts_at,all_day,local_date,fetched_at)
             VALUES (?1,'x',?2,0,?3,'2026-08-13T00:00:00Z')",
            rusqlite::params![
                uid,
                d.and_hms_opt(9, 0, 0).unwrap().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                d.format("%Y-%m-%d").to_string()
            ],
        )
        .unwrap();
    }

    crate::settings::set(&conn, "ics_enabled", "1").unwrap();

    let up = upcoming(&conn, 30, 20).unwrap();
    let uids: Vec<&str> = up.iter().map(|e| e.uid.as_str()).collect();
    assert_eq!(uids, vec!["soon"]);
}

// ---------------------------------------------------------------------------
// URL handling
// ---------------------------------------------------------------------------

/// A non-HTTP scheme must be refused before any request is attempted — this is
/// the guard that keeps a file:// path or a pasted Compass login page out of
/// the fetcher.
#[test]
fn a_non_http_url_is_refused() {
    for bad in ["file:///etc/passwd", "ftp://example.test/x.ics", "not a url", ""] {
        assert!(normalise_url(bad).is_err(), "{bad} should be refused");
    }
}

/// Compass hands out a webcal:// subscribe link; pasting it in should work.
#[test]
fn a_webcal_url_becomes_https() {
    assert_eq!(
        normalise_url("webcal://compass.example.edu.au/feed.ics").unwrap(),
        "https://compass.example.edu.au/feed.ics"
    );
    assert_eq!(
        normalise_url("  https://x.test/a.ics  ").unwrap(),
        "https://x.test/a.ics"
    );
}

/// `upcoming` must use the real calendar date, not Retain's 4am study day.
///
/// Between midnight and 4am the study day is still yesterday. If the query used
/// it, a class that finished at 3pm yesterday would be listed as upcoming. The
/// stored `local_date` is a calendar date, so the filter has to be one too.
#[test]
fn upcoming_uses_the_calendar_day_not_the_four_am_study_day() {
    let calendar_today = Utc::now().with_timezone(&local_tz()).date_naive();
    let study_today = util::retain_today_naive();

    let conn = db();
    // Dated to the study day, which before 4am is the day before the calendar
    // day. Only the calendar-day filter excludes it correctly.
    let yesterday = calendar_today - Duration::days(1);
    conn.execute(
        "INSERT INTO calendar_events (uid,summary,starts_at,all_day,local_date,fetched_at)
         VALUES ('past','Finished yesterday','2026-08-12T05:00:00Z',0,?1,'2026-08-13T00:00:00Z')",
        [yesterday.format("%Y-%m-%d").to_string()],
    )
    .unwrap();

    crate::settings::set(&conn, "ics_enabled", "1").unwrap();

    let up = upcoming(&conn, 14, 50).unwrap();
    assert!(
        !up.iter().any(|e| e.uid == "past"),
        "yesterday's event leaked into upcoming (study day {study_today}, calendar day {calendar_today})"
    );
}

/// The enable toggle must actually gate the feature. A switch that leaves
/// events on the Today screen after you turn it off is a switch that does
/// nothing.
#[test]
fn turning_the_calendar_off_hides_events_without_deleting_them() {
    let mut conn = db();
    store(
        &mut conn,
        &parse(
            "BEGIN:VEVENT\r\nUID:z\r\nSUMMARY:Class\r\n\
             DTSTART;TZID=Australia/Melbourne:20260817T090000\r\nEND:VEVENT",
        ),
    )
    .unwrap();

    // Off by default.
    assert!(upcoming(&conn, 400, 50).unwrap().is_empty());

    crate::settings::set(&conn, "ics_enabled", "1").unwrap();
    assert_eq!(upcoming(&conn, 400, 50).unwrap().len(), 1);

    crate::settings::set(&conn, "ics_enabled", "0").unwrap();
    assert!(upcoming(&conn, 400, 50).unwrap().is_empty());

    // The rows survive, so turning it back on is instant rather than a resync.
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM calendar_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

// -- decoding a Compass class -----------------------------------------------
//
// Every code below is one of Arman's real ones, taken from the live calendar.

/// The subject list as it actually is: name and colour.
fn subjects() -> Vec<(String, String)> {
    vec![
        ("Biology".into(), "#4BA97B".into()),
        ("Chemistry".into(), "#5B7FD4".into()),
        ("English".into(), "#D0603C".into()),
        ("Accounting".into(), "#8A6FD6".into()),
        ("Specialist Mathematics".into(), "#D08B3C".into()),
        ("Mathematical Methods".into(), "#3CA8D0".into()),
    ]
}

#[test]
fn real_class_codes_resolve_to_the_right_subject() {
    let s = subjects();

    for (code, expected) in [
        ("11CHEU2", "Chemistry"),
        ("12BIOS", "Biology"),
        ("11ENGT2", "English"),
        ("11ACCQ", "Accounting"),
        ("11SMAR", "Specialist Mathematics"),
        ("11MMEP2", "Mathematical Methods"),
    ] {
        let got = match_subject(code, &s).map(|(n, _)| n.as_str());
        assert_eq!(got, Some(expected), "{code}");
    }
}

/// `11ASMEDA` is a year-level assembly. Filing it under whichever subject
/// happens to share two letters would put a class you don't have on your
/// timetable, which is worse than showing the bare code.
#[test]
fn a_code_that_is_not_one_of_your_subjects_matches_nothing() {
    let s = subjects();

    assert!(match_subject("11ASMEDA", &s).is_none());
    assert!(match_subject("VSMF", &s).is_none());
    assert!(match_subject("11", &s).is_none(), "digits alone are not a code");
    assert!(match_subject("", &s).is_none());
}

#[test]
fn the_teacher_comes_out_of_compasss_description_line() {
    assert_eq!(teacher_from_description(Some("Attending Staff : BGY")), Some("BGY".into()));
    assert_eq!(teacher_from_description(Some("Attending Staff : MD14 - BFU")), Some("MD14 - BFU".into()));

    // Anything not in that shape is left alone rather than guessed at.
    assert_eq!(teacher_from_description(Some("Bring your textbook")), None);
    assert_eq!(teacher_from_description(Some("Attending Staff :   ")), None);
    assert_eq!(teacher_from_description(None), None);
}

#[test]
fn a_class_decodes_into_everything_worth_showing() {
    let got = describe("11CHEU2", Some("Attending Staff : BGY"), Some("T3"), &subjects());

    assert_eq!(got.code, "11CHEU2");
    assert_eq!(got.subject_name.as_deref(), Some("Chemistry"));
    assert_eq!(got.colour.as_deref(), Some("#5B7FD4"));
    assert_eq!(got.room.as_deref(), Some("T3"));
    assert_eq!(got.teacher.as_deref(), Some("BGY"));
}

/// A whole-school event has no code, no room and no teacher, and must still
/// come through with its name intact rather than being dropped or mangled.
#[test]
fn an_event_that_is_not_a_class_keeps_its_name() {
    let got = describe("Parent-Student-Teacher Conferences", None, None, &subjects());

    assert_eq!(got.code, "Parent-Student-Teacher Conferences");
    assert_eq!(got.subject_name, None);
    assert_eq!(got.room, None);
    assert_eq!(got.teacher, None);
}

/// LOCATION was parsed nowhere, so the room never reached the database — the
/// one detail you most want at 8:25 on a Monday.
#[test]
fn the_room_is_read_from_the_location_property() {
    let events = parse(
        "BEGIN:VEVENT\r\n\
         UID:class-1\r\n\
         SUMMARY:11CHEU2\r\n\
         LOCATION:T3\r\n\
         DESCRIPTION:Attending Staff : BGY\r\n\
         DTSTART:20260817T012500Z\r\n\
         DTEND:20260817T022500Z\r\n\
         END:VEVENT",
    );
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].location.as_deref(), Some("T3"));
    assert_eq!(events[0].description.as_deref(), Some("Attending Staff : BGY"));
}
