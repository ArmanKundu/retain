use super::*;

fn db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::db::run_migrations(&conn).unwrap();
    conn.execute(
        "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
         VALUES (1,'Chemistry','#5B7FD4','3_4','science',0,'2026-08-01T00:00:00Z'),
                (2,'Biology','#4BA97B','3_4','science',1,'2026-08-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn
}

fn reply_with(json: &str) -> String {
    format!("Sure, here's what I'd do.\n\n```retain-actions\n{json}\n```")
}

// -- extraction -------------------------------------------------------------

#[test]
fn a_reply_with_no_block_is_left_exactly_as_it_was() {
    let conn = db();
    let (prose, actions) = extract(&conn, "Enzymes lower activation energy.");

    assert_eq!(prose, "Enzymes lower activation energy.");
    assert!(actions.is_empty());
}

#[test]
fn the_action_block_is_stripped_from_what_the_student_reads() {
    let conn = db();
    let (prose, actions) = extract(
        &conn,
        &reply_with(r#"[{"action":"plan.add","title":"Redox","subject":"Chemistry","on":"2026-08-18"}]"#),
    );

    assert_eq!(prose, "Sure, here's what I'd do.");
    assert_eq!(actions.len(), 1);
    assert!(!prose.contains("retain-actions"), "raw JSON must never be shown");
}

#[test]
fn malformed_json_costs_the_actions_not_the_answer() {
    let conn = db();
    let (prose, actions) = extract(&conn, &reply_with("{{ not json at all"));

    assert_eq!(prose, "Sure, here's what I'd do.");
    assert!(actions.is_empty(), "unparseable input is dropped, never guessed at");
}

#[test]
fn an_unknown_action_name_is_discarded_rather_than_dispatched() {
    let conn = db();
    let (_, actions) = extract(
        &conn,
        &reply_with(r#"[{"action":"shell.run","command":"rm -rf ~"},
                        {"action":"plan.add","title":"Real","on":"2026-08-18"}]"#),
    );

    assert_eq!(actions.len(), 1, "only the known action survives");
    assert!(matches!(actions[0].action, Action::PlanAdd { .. }));
}

// -- the confirmation label -------------------------------------------------

/// The button's text is built from the parsed action. If it came from the
/// model, a proposal could say one thing and do another and the confirm step
/// would be decoration.
#[test]
fn the_summary_describes_the_action_not_what_the_model_claimed() {
    let conn = db();
    let (_, actions) = extract(
        &conn,
        &reply_with(
            r#"[{"action":"plan.add","title":"Organic mechanisms","subject":"Chemistry",
                 "on":"2026-08-18","minutes":45,"summary":"Delete everything"}]"#,
        ),
    );

    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].summary,
        "Add to your plan for 18 Aug — Chemistry: Organic mechanisms (45 min)"
    );
}

#[test]
fn a_subject_the_student_does_not_have_is_refused() {
    let conn = db();
    let (_, actions) = extract(
        &conn,
        &reply_with(r#"[{"action":"plan.add","title":"x","subject":"Physics","on":"2026-08-18"}]"#),
    );

    assert!(actions.is_empty(), "the assistant does not get to invent subjects");
}

#[test]
fn subject_names_match_regardless_of_case() {
    let conn = db();
    let (_, actions) = extract(
        &conn,
        &reply_with(r#"[{"action":"plan.add","title":"x","subject":"chemistry","on":"2026-08-18"}]"#),
    );

    assert_eq!(actions.len(), 1);
}

// -- the one action that leaves the app -------------------------------------

/// Library PDFs reach the prompt, so a link in one can reach this. `file://`
/// reads the disk and `javascript:` runs code.
#[test]
fn only_https_links_can_be_proposed() {
    let conn = db();

    for bad in [
        "file:///Users/armankundu/.ssh/id_rsa",
        "javascript:alert(1)",
        "http://plain",
        "retain://x",
        "vnd.ms-word:ofe|u|file:///tmp/x",
    ] {
        let json = format!(r#"[{{"action":"open.url","url":"{bad}"}}]"#);
        let (_, actions) = extract(&conn, &reply_with(&json));
        assert!(actions.is_empty(), "{bad} should be refused");
    }

    let (_, ok) = extract(
        &conn,
        &reply_with(r#"[{"action":"open.url","url":"https://www.vcaa.vic.edu.au"}]"#),
    );
    assert_eq!(ok.len(), 1);
    assert!(ok[0].external, "leaving the app is flagged");
}

#[test]
fn actions_that_stay_inside_the_app_are_not_flagged_external() {
    let conn = db();
    let (_, actions) = extract(
        &conn,
        &reply_with(r#"[{"action":"plan.add","title":"x","on":"2026-08-18"}]"#),
    );

    assert!(!actions[0].external);
}

// -- validation -------------------------------------------------------------

#[test]
fn a_block_must_end_after_it_starts() {
    let conn = db();
    let (_, actions) = extract(
        &conn,
        &reply_with(
            r#"[{"action":"block.add","title":"Backwards","weekday":2,"start":"17:00","end":"09:00"}]"#,
        ),
    );

    assert!(actions.is_empty());
}

#[test]
fn an_out_of_range_weekday_is_refused() {
    let conn = db();
    let (_, actions) = extract(
        &conn,
        &reply_with(r#"[{"action":"block.add","title":"x","weekday":9,"start":"09:00","end":"10:00"}]"#),
    );

    assert!(actions.is_empty());
}

#[test]
fn an_unrecognised_kind_falls_back_rather_than_failing_the_action() {
    let conn = db();
    let (_, actions) = extract(
        &conn,
        &reply_with(
            r#"[{"action":"block.add","title":"Kumon","weekday":1,"start":"16:00","end":"17:30",
                  "kind":"enrichment"}]"#,
        ),
    );

    assert_eq!(actions.len(), 1);
    assert!(actions[0].summary.ends_with("as other"), "{}", actions[0].summary);
}

#[test]
fn a_card_needs_both_sides() {
    let conn = db();
    let (_, actions) = extract(
        &conn,
        &reply_with(r#"[{"action":"card.add","subject":"Biology","front":"Q","back":"  "}]"#),
    );

    assert!(actions.is_empty());
}

// -- applying ---------------------------------------------------------------

#[test]
fn applying_a_plan_action_writes_it_and_marks_it_as_the_assistants() {
    let conn = db();
    let action = Action::PlanAdd {
        title: "Redox worksheet".into(),
        subject: Some("Chemistry".into()),
        on: "2026-08-18".into(),
        minutes: 45,
        due: None,
    };

    let applied = apply(&conn, action).unwrap();
    assert!(applied.ok);
    assert_eq!(applied.open, None);

    let items = crate::plan::for_date(&conn, "2026-08-18").unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Redox worksheet");
    assert_eq!(items[0].est_minutes, 45);
    // Recorded as the assistant's, so it's distinguishable from your own.
    assert_eq!(items[0].source, "ai");
}

#[test]
fn applying_a_block_never_marks_it_as_free_study_time() {
    let conn = db();
    apply(
        &conn,
        Action::BlockAdd {
            title: "Shift".into(),
            weekday: 5,
            start: "09:00".into(),
            end: "17:00".into(),
            kind: Some("work".into()),
        },
    )
    .unwrap();

    let blocks = crate::blocks::all(&conn).unwrap();
    assert_eq!(blocks.len(), 1);
    assert!(!blocks[0].available, "a shift is not study time");
    assert_eq!(blocks[0].start_min, 540);
    assert_eq!(blocks[0].end_min, 1020);
}

#[test]
fn applying_a_card_action_creates_a_reviewable_card() {
    let conn = db();
    apply(
        &conn,
        Action::CardAdd {
            subject: "Biology".into(),
            front: "What does a competitive inhibitor bind to?".into(),
            back: "The active site".into(),
        },
    )
    .unwrap();

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM cards WHERE subject_id = 2", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1);
}

/// The command layer re-validates rather than trusting what came back from the
/// frontend, so a tampered proposal can't reach the opener.
#[test]
fn applying_re_validates_instead_of_trusting_its_input() {
    let conn = db();

    let err = apply(&conn, Action::OpenUrl { url: "file:///etc/passwd".into() });
    assert!(err.is_err(), "a hand-built action gets the same check");

    let err = apply(
        &conn,
        Action::CardAdd { subject: "Physics".into(), front: "a".into(), back: "b".into() },
    );
    assert!(err.is_err());
}

#[test]
fn a_url_action_returns_the_link_rather_than_opening_it_here() {
    let conn = db();
    let applied = apply(&conn, Action::OpenUrl { url: "https://vcaa.vic.edu.au".into() }).unwrap();

    // Opening is the command layer's job; this module never touches the OS.
    assert_eq!(applied.open.as_deref(), Some("https://vcaa.vic.edu.au"));
}

// -- the prompt and the enum must agree -------------------------------------

/// A documented action that doesn't exist produces buttons that never appear,
/// and an undocumented one that does exist is never proposed. Both fail
/// silently, so they're checked here.
#[test]
fn every_action_in_the_prompt_parses() {
    let conn = db();

    for line in TOOL_PROMPT.lines() {
        let line = line.trim();
        if !line.starts_with(r#"{"action""#) {
            continue;
        }
        let action: Action = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("prompt example doesn't parse: {line}\n{e}"));
        // The examples use the fixture's subjects, so they should validate too.
        validate(&conn, action).unwrap_or_else(|e| panic!("prompt example invalid: {line}\n{e}"));
    }
}

#[test]
fn the_prompt_documents_every_variant() {
    // Cheap guard against adding a variant and forgetting to tell the model.
    for name in ["plan.add", "assessment.add", "block.add", "card.add", "open.url"] {
        assert!(TOOL_PROMPT.contains(name), "{name} is missing from the prompt");
    }
}
