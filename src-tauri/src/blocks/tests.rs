use super::*;

fn db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::db::run_migrations(&conn).unwrap();
    conn.execute(
        "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
         VALUES (1,'Biology','#4BA97B','3_4','science',0,'2026-08-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn
}

fn now() -> DateTime<Utc> {
    chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 8, 17, 9, 0, 0).unwrap()
}

fn weekly(title: &str, weekday: i64, start: i64, end: i64) -> NewBlock {
    NewBlock {
        title: title.into(),
        kind: "class".into(),
        weekday: Some(weekday),
        on_date: None,
        start_min: start,
        end_min: end,
        available: false,
        subject_id: None,
        note: None,
        link: None,
    }
}

fn block(start: i64, end: i64, available: bool) -> TimeBlock {
    TimeBlock {
        id: 0,
        title: "x".into(),
        kind: "class".into(),
        weekday: Some(0),
        on_date: None,
        start_min: start,
        end_min: end,
        available,
        subject_id: None,
        subject_name: None,
        colour: None,
        note: None,
        link: None,
    }
}

// -- validation -------------------------------------------------------------

#[test]
fn a_block_repeats_weekly_or_happens_once_never_both() {
    let mut b = weekly("Tuition", 1, 16 * 60, 18 * 60);
    assert!(validate(&b).is_ok());

    b.on_date = Some("2026-08-18".into());
    let err = validate(&b).unwrap_err().to_string();
    assert!(err.contains("weekly or happens once"), "{err}");

    b.weekday = None;
    assert!(validate(&b).is_ok(), "dated-only should be fine");

    b.on_date = None;
    assert!(validate(&b).is_err(), "neither is not a block");
}

#[test]
fn a_block_has_to_end_after_it_starts() {
    let mut b = weekly("Backwards", 0, 600, 600);
    assert!(validate(&b).unwrap_err().to_string().contains("end after it starts"));

    b.end_min = 599;
    assert!(validate(&b).is_err());
}

#[test]
fn a_block_has_to_fit_inside_one_day() {
    let mut b = weekly("Overnight", 0, 1380, 1500);
    assert!(validate(&b).unwrap_err().to_string().contains("inside one day"));

    b.end_min = 1440;
    assert!(validate(&b).is_ok(), "ending exactly at midnight is fine");
}

#[test]
fn a_block_needs_a_name() {
    let mut b = weekly("   ", 0, 600, 700);
    assert!(validate(&b).unwrap_err().to_string().contains("name"));
    b.title = "Work".into();
    assert!(validate(&b).is_ok());
}

// -- storage ----------------------------------------------------------------

#[test]
fn blocks_round_trip_with_their_subject() {
    let conn = db();
    let mut b = weekly("Biology", 2, 9 * 60, 10 * 60);
    b.subject_id = Some(1);
    b.available = true;

    create(&conn, &b, now()).unwrap();

    let all = all(&conn).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].title, "Biology");
    assert_eq!(all[0].subject_name.as_deref(), Some("Biology"));
    assert!(all[0].available);
}

/// A weekly block and a one-off on the same day are two commitments, not one.
#[test]
fn a_date_sees_both_its_weekly_and_its_dated_blocks() {
    let conn = db();
    // 18 August 2026 is a Tuesday.
    create(&conn, &weekly("Tuition", 1, 16 * 60, 18 * 60), now()).unwrap();
    create(
        &conn,
        &NewBlock {
            title: "Dentist".into(),
            kind: "other".into(),
            weekday: None,
            on_date: Some("2026-08-18".into()),
            start_min: 10 * 60,
            end_min: 11 * 60,
            available: false,
            subject_id: None,
            note: None,
            link: None,
        },
        now(),
    )
    .unwrap();

    let tuesday = for_date(&conn, NaiveDate::from_ymd_opt(2026, 8, 18).unwrap()).unwrap();
    let titles: Vec<&str> = tuesday.iter().map(|b| b.title.as_str()).collect();
    assert_eq!(titles, vec!["Dentist", "Tuition"], "sorted by start time");

    // Wednesday has neither.
    let wednesday = for_date(&conn, NaiveDate::from_ymd_opt(2026, 8, 19).unwrap()).unwrap();
    assert!(wednesday.is_empty());
}

#[test]
fn a_block_can_be_edited_and_removed() {
    let conn = db();
    let id = create(&conn, &weekly("Wrong", 0, 600, 700), now()).unwrap();

    let mut fixed = weekly("Right", 0, 660, 780);
    fixed.kind = "work".into();
    update(&conn, id, &fixed).unwrap();

    let one = &all(&conn).unwrap()[0];
    assert_eq!(one.title, "Right");
    assert_eq!(one.kind, "work");
    assert_eq!(one.start_min, 660);

    delete(&conn, id).unwrap();
    assert!(all(&conn).unwrap().is_empty());
}

/// Deleting a subject must not delete the class you have for it.
#[test]
fn removing_a_subject_keeps_its_blocks() {
    let conn = db();
    let mut b = weekly("Biology period 1", 0, 9 * 60, 10 * 60);
    b.subject_id = Some(1);
    create(&conn, &b, now()).unwrap();

    conn.execute("DELETE FROM subjects WHERE id = 1", []).unwrap();

    let all = all(&conn).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].subject_id, None);
}

// -- free time --------------------------------------------------------------

/// The bug this exists to prevent: overlapping commitments counted twice would
/// systematically understate free time, which is what makes a planner feel
/// punishing rather than useful.
#[test]
fn overlapping_blocks_are_merged_not_summed() {
    let day_start = 7 * 60;
    let day_end = 22 * 60; // 900 minutes

    let overlapping = vec![
        block(16 * 60, 17 * 60, false),      // 60 min
        block(16 * 60 + 30, 18 * 60, false), // overlaps by 30
    ];
    // Together they consume 16:00–18:00 = 120 minutes, not 150.
    assert_eq!(free_minutes(&overlapping, day_start, day_end), 900 - 120);
}

#[test]
fn adjacent_blocks_do_not_double_count_their_boundary() {
    let blocks = vec![block(600, 660, false), block(660, 720, false)];
    assert_eq!(free_minutes(&blocks, 600, 720), 0);
}

#[test]
fn blocks_you_can_study_in_do_not_consume_time() {
    let blocks = vec![block(9 * 60, 10 * 60, true)];
    assert_eq!(free_minutes(&blocks, 9 * 60, 10 * 60), 60);
}

#[test]
fn a_block_outside_the_waking_window_is_clipped() {
    // A 6am–8am commitment against a 7am start only consumes one hour of it.
    let blocks = vec![block(6 * 60, 8 * 60, false)];
    assert_eq!(free_minutes(&blocks, 7 * 60, 22 * 60), 900 - 60);

    // And one entirely outside consumes nothing.
    let overnight = vec![block(0, 6 * 60, false)];
    assert_eq!(free_minutes(&overnight, 7 * 60, 22 * 60), 900);
}

#[test]
fn an_empty_day_is_entirely_free() {
    assert_eq!(free_minutes(&[], 7 * 60, 22 * 60), 900);
}

// -- the assistant's view ---------------------------------------------------

#[test]
fn the_week_summary_states_commitments_and_free_time() {
    let conn = db();
    create(&conn, &weekly("Tuition", 1, 16 * 60, 18 * 60), now()).unwrap();

    let summary = week_summary(&conn, NaiveDate::from_ymd_opt(2026, 8, 17).unwrap()).unwrap();
    assert!(summary.contains("Tuesday: Tuition 4pm–6pm"), "{summary}");
    assert!(summary.contains("13h 0m free"), "{summary}");
    // The model is told plainly that these are not available.
    assert!(summary.contains("cannot study during these"));
}

#[test]
fn an_empty_week_produces_no_summary_rather_than_an_empty_heading() {
    let conn = db();
    assert_eq!(week_summary(&conn, NaiveDate::from_ymd_opt(2026, 8, 17).unwrap()).unwrap(), "");
}

#[test]
fn clock_reads_the_way_people_say_times() {
    assert_eq!(clock(0), "12am");
    assert_eq!(clock(9 * 60), "9am");
    assert_eq!(clock(12 * 60), "12pm");
    assert_eq!(clock(13 * 60 + 30), "1:30pm");
    assert_eq!(clock(23 * 60 + 5), "11:05pm");
}

// -- meeting links ----------------------------------------------------------

#[test]
fn a_meeting_link_round_trips() {
    let conn = db();
    let mut b = weekly("Tuition", 1, 16 * 60, 18 * 60);
    b.link = Some("https://zoom.us/j/123456".into());
    create(&conn, &b, now()).unwrap();

    assert_eq!(all(&conn).unwrap()[0].link.as_deref(), Some("https://zoom.us/j/123456"));
}

/// Checked here so the message can appear beside the field, and so nothing but
/// an HTTP(S) URL can ever reach the OS opener.
#[test]
fn a_link_that_is_not_http_is_refused() {
    let mut b = weekly("Dodgy", 0, 600, 700);

    for bad in ["file:///etc/passwd", "javascript:alert(1)", "zoom.us/j/1"] {
        b.link = Some(bad.into());
        let err = validate(&b).unwrap_err().to_string();
        assert!(err.contains("https://"), "{bad}: {err}");
    }

    b.link = Some("https://teams.microsoft.com/l/x".into());
    assert!(validate(&b).is_ok());
}

#[test]
fn a_blank_link_is_stored_as_nothing_rather_than_an_empty_string() {
    let conn = db();
    let mut b = weekly("No link", 0, 600, 700);
    b.link = Some("   ".into());

    assert!(validate(&b).is_ok(), "blank should not trip the scheme check");
    create(&conn, &b, now()).unwrap();
    assert_eq!(all(&conn).unwrap()[0].link, None);
}
