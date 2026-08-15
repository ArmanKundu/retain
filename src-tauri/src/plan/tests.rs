use super::*;

fn db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::db::run_migrations(&conn).unwrap();
    conn.execute(
        "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
         VALUES (1,'Chemistry','#5B7FD4','3_4','science',0,'2026-08-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn
}

fn now() -> DateTime<Utc> {
    chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 8, 17, 9, 0, 0).unwrap()
}

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, d).unwrap()
}

fn iso(d: u32) -> String {
    day(d).format("%Y-%m-%d").to_string()
}

fn item(title: &str, on: u32, minutes: i64) -> NewPlanItem {
    NewPlanItem {
        subject_id: Some(1),
        title: title.into(),
        detail: None,
        planned_on: iso(on),
        est_minutes: minutes,
        due_on: None,
        source: None,
    }
}

/// Fill a date so that only `free` minutes remain in the 7am–10pm window.
fn occupy(conn: &Connection, on: u32, free: i64) {
    let end = DAY_END - free;
    conn.execute(
        "INSERT INTO time_blocks (title, kind, on_date, start_min, end_min, available, created_at)
         VALUES ('Busy','work',?1,?2,?3,0,'2026-08-01T00:00:00Z')",
        params![iso(on), DAY_START, end],
    )
    .unwrap();
}

// -- basics -----------------------------------------------------------------

#[test]
fn an_item_records_where_it_started() {
    let conn = db();
    let id = create(&conn, &item("Redox worksheet", 17, 45), now()).unwrap();

    let got = &for_date(&conn, &iso(17)).unwrap()[0];
    assert_eq!(got.id, id);
    assert_eq!(got.planned_on, iso(17));
    assert_eq!(got.first_planned_on, iso(17));
    assert_eq!(got.moves, 0);
    assert_eq!(got.subject_name.as_deref(), Some("Chemistry"));
}

#[test]
fn a_deadline_before_the_planned_day_is_refused() {
    let conn = db();
    let mut i = item("Too late", 17, 30);
    i.due_on = Some(iso(16));

    let err = create(&conn, &i, now()).unwrap_err().to_string();
    assert!(err.contains("due before"), "{err}");
}

#[test]
fn an_estimate_is_clamped_rather_than_stored_absurd() {
    let conn = db();
    create(&conn, &item("Marathon", 17, 100_000), now()).unwrap();
    create(&conn, &item("Instant", 18, 0), now()).unwrap();

    assert_eq!(for_date(&conn, &iso(17)).unwrap()[0].est_minutes, DAILY_CAP_MIN);
    assert_eq!(for_date(&conn, &iso(18)).unwrap()[0].est_minutes, 5);
}

// -- rollover: the point of the module ---------------------------------------

#[test]
fn work_missed_yesterday_lands_on_today() {
    let conn = db();
    create(&conn, &item("Chemistry: rates", 16, 60), now()).unwrap();

    let out = rollover(&conn, day(17)).unwrap();

    assert_eq!(out.moved.len(), 1);
    assert_eq!(out.moved[0].from, iso(16));
    assert_eq!(out.moved[0].to, iso(17));
    assert_eq!(out.moved[0].moves, 1);
    assert!(out.stuck.is_empty());
    assert_eq!(for_date(&conn, &iso(17)).unwrap()[0].title, "Chemistry: rates");
}

/// The original date survives every move, which is what lets the UI say
/// "you've been meaning to do this since the 10th".
#[test]
fn the_first_planned_date_is_never_rewritten() {
    let conn = db();
    create(&conn, &item("Slipping", 10, 30), now()).unwrap();

    rollover(&conn, day(15)).unwrap();
    conn.execute("UPDATE plan_items SET planned_on = ?1", [iso(15)]).unwrap();
    rollover(&conn, day(17)).unwrap();

    let got = &for_date(&conn, &iso(17)).unwrap()[0];
    assert_eq!(got.first_planned_on, iso(10));
    assert_eq!(got.moves, 2);
}

/// The failure this module exists to prevent: a bad Tuesday producing a
/// Wednesday nobody would attempt.
#[test]
fn a_days_worth_of_slippage_spills_across_days_rather_than_stacking() {
    let conn = db();
    // Four hours missed, but only ninety free minutes a day for the next week.
    for d in 17..=23 {
        occupy(&conn, d, 90);
    }
    for (n, title) in ["Rates", "Redox", "Organic", "Equilibrium"].iter().enumerate() {
        create(&conn, &item(title, 16, 60), now()).unwrap();
        let _ = n;
    }

    let out = rollover(&conn, day(17)).unwrap();

    assert_eq!(out.moved.len(), 4);
    assert!(out.stuck.is_empty());
    // 90 free minutes takes one 60-minute item, not four.
    let per_day: Vec<usize> = (17..=20).map(|d| for_date(&conn, &iso(d)).unwrap().len()).collect();
    assert_eq!(per_day, vec![1, 1, 1, 1], "one hour-long item per ninety-minute day");
}

#[test]
fn a_day_that_is_already_full_is_skipped_over() {
    let conn = db();
    occupy(&conn, 17, 60);
    occupy(&conn, 18, 60);
    // Today already has an hour on it, so its free hour is spoken for.
    create(&conn, &item("Already planned", 17, 60), now()).unwrap();
    create(&conn, &item("Missed", 16, 60), now()).unwrap();

    rollover(&conn, day(17)).unwrap();

    assert_eq!(for_date(&conn, &iso(17)).unwrap().len(), 1, "today stays at one hour");
    assert_eq!(for_date(&conn, &iso(18)).unwrap()[0].title, "Missed");
}

/// Revision for Thursday's SAC must never be rescheduled to Friday.
#[test]
fn nothing_is_moved_past_its_own_deadline() {
    let conn = db();
    occupy(&conn, 17, 30);
    occupy(&conn, 18, 30);

    let mut i = item("SAC revision", 16, 120);
    i.due_on = Some(iso(18));
    create(&conn, &i, now()).unwrap();

    let out = rollover(&conn, day(17)).unwrap();

    assert!(out.moved.is_empty());
    assert_eq!(out.stuck.len(), 1);
    assert!(out.stuck[0].reason.contains("before it's due"), "{:?}", out.stuck[0]);
    // Left where it was rather than moved somewhere useless.
    assert_eq!(for_date(&conn, &iso(16)).unwrap().len(), 1);
}

#[test]
fn a_deadline_that_has_already_passed_is_reported_not_rescheduled() {
    let conn = db();
    let mut i = item("Assignment", 14, 60);
    i.due_on = Some(iso(15));
    create(&conn, &i, now()).unwrap();

    let out = rollover(&conn, day(17)).unwrap();

    assert!(out.moved.is_empty());
    assert_eq!(out.stuck[0].reason, "Its deadline has passed.");
}

#[test]
fn work_with_nowhere_to_go_is_reported_rather_than_parked_out_of_sight() {
    let conn = db();
    // Every day in the horizon is completely committed.
    for d in 17..=31 {
        occupy(&conn, d, 0);
    }
    create(&conn, &item("Nowhere", 16, 60), now()).unwrap();

    let out = rollover(&conn, day(17)).unwrap();

    assert!(out.moved.is_empty());
    assert!(out.stuck[0].reason.contains("14 days"), "{:?}", out.stuck[0]);
}

#[test]
fn done_and_skipped_work_is_left_alone() {
    let conn = db();
    let a = create(&conn, &item("Finished", 16, 30), now()).unwrap();
    let b = create(&conn, &item("Decided against", 16, 30), now()).unwrap();
    set_status(&conn, a, "done", now()).unwrap();
    set_status(&conn, b, "skipped", now()).unwrap();

    let out = rollover(&conn, day(17)).unwrap();

    assert_eq!(out, Rollover::default());
    assert_eq!(for_date(&conn, &iso(16)).unwrap().len(), 2);
}

/// Opening the app twice must not walk the plan forward twice.
#[test]
fn rollover_is_idempotent_within_a_day() {
    let conn = db();
    create(&conn, &item("Once", 16, 60), now()).unwrap();

    let first = rollover(&conn, day(17)).unwrap();
    let second = rollover(&conn, day(17)).unwrap();

    assert_eq!(first.moved.len(), 1);
    assert_eq!(second, Rollover::default(), "second pass found nothing overdue");
    assert_eq!(for_date(&conn, &iso(17)).unwrap()[0].moves, 1);
}

/// Two runs from the same starting state produce the same plan — otherwise the
/// schedule you looked at this morning isn't the one you get tonight.
#[test]
fn rollover_is_deterministic() {
    let arrange = || {
        let conn = db();
        for d in 17..=20 {
            occupy(&conn, d, 60);
        }
        for title in ["A", "B", "C", "D"] {
            create(&conn, &item(title, 16, 45), now()).unwrap();
        }
        conn
    };

    let one = arrange();
    let two = arrange();
    let a = rollover(&one, day(17)).unwrap();
    let b = rollover(&two, day(17)).unwrap();

    assert_eq!(a, b);
}

#[test]
fn the_stamp_records_that_today_has_been_rolled() {
    let conn = db();
    assert!(!rolled_today(&conn, day(17)).unwrap());

    create(&conn, &item("x", 16, 30), now()).unwrap();
    rollover(&conn, day(17)).unwrap();

    assert!(rolled_today(&conn, day(17)).unwrap());
    assert!(!rolled_today(&conn, day(18)).unwrap(), "tomorrow still needs a pass");
}

// -- summary ----------------------------------------------------------------

#[test]
fn the_summary_names_what_keeps_slipping() {
    let conn = db();
    let id = create(&conn, &item("Organic mechanisms", 17, 45), now()).unwrap();
    conn.execute("UPDATE plan_items SET moves = 3 WHERE id = ?1", [id]).unwrap();

    let text = summary(&conn, day(17)).unwrap();

    assert!(text.contains("Chemistry: Organic mechanisms (45 min)"), "{text}");
    assert!(text.contains("moved 3 times since 2026-08-17"), "{text}");
}

#[test]
fn an_empty_day_says_so_plainly() {
    let conn = db();
    assert_eq!(summary(&conn, day(17)).unwrap(), "Nothing planned for today.");
}
