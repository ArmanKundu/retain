use super::*;
use chrono::TimeZone;

fn db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn.execute_batch(include_str!("../db/migrations/001_init.sql")).unwrap();
    conn.execute_batch(include_str!("../db/migrations/002_capture_cards_errors.sql")).unwrap();
    conn.execute(
        "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
         VALUES (1,'Biology','#4BA97B','3_4','science',0,'2026-08-01T00:00:00Z'),
                (2,'Maths Methods','#5B8DEF','1_2','maths',1,'2026-08-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn
}

fn at(h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 13, h, m, 0).unwrap()
}

// ---------------------------------------------------------------------------
// Scope
// ---------------------------------------------------------------------------

/// The Biology categories must not leak onto other subjects — that was the
/// explicit constraint, and it's the difference between a useful picker and a
/// list of forty irrelevant options.
#[test]
fn biology_categories_apply_only_to_biology_three_four() {
    assert!(applies_to("Biology", "3_4"));
    assert!(applies_to("  biology  ", "3_4"));

    assert!(!applies_to("Biology", "1_2"));
    assert!(!applies_to("Chemistry", "3_4"));
    assert!(!applies_to("Maths Methods", "3_4"));
    assert!(!applies_to("Biological Science", "3_4"));
}

/// Command words live in `errors` — one table, shared by every 3/4 subject.
#[test]
fn command_words_cover_the_ones_that_get_confused() {
    let table = crate::errors::COMMAND_WORDS;
    let find = |w: &str| table.iter().find(|(t, _)| *t == w).map(|(_, d)| *d);

    for w in ["describe", "explain", "compare", "analyse", "evaluate", "discuss", "justify"] {
        assert!(find(w).is_some(), "missing {w}");
    }

    // "compare" is the one people lose marks on by giving only differences.
    assert!(find("compare").unwrap().to_lowercase().contains("similarities"));

    // No duplicates — the picker would show the same word twice.
    let mut words: Vec<&str> = table.iter().map(|(w, _)| *w).collect();
    let before = words.len();
    words.sort();
    words.dedup();
    assert_eq!(words.len(), before);
}

/// The module must not ship study-design content — that's the honesty
/// constraint, and a future edit that pastes dot points in should fail here.
#[test]
fn no_vcaa_content_is_baked_into_the_binary() {
    let source = include_str!("../biology.rs");
    let body = source.split("#[cfg(test)]").next().unwrap().to_lowercase();

    for claim in ["key knowledge", "area of study 1", "unit 3 aos", "study design states"] {
        assert!(!body.contains(claim), "looks like invented VCAA content: {claim}");
    }
}

// ---------------------------------------------------------------------------
// Outline import
// ---------------------------------------------------------------------------

#[test]
fn an_indented_outline_becomes_a_hierarchy() {
    let rows = parse_outline(
        "Unit 3\n  Area of Study 1\n    the structure of nucleic acids\n    protein synthesis\n  Area of Study 2\nUnit 4\n",
    );

    let shape: Vec<(usize, &str)> = rows.iter().map(|r| (r.depth, r.name.as_str())).collect();
    assert_eq!(
        shape,
        vec![
            (0, "Unit 3"),
            (1, "Area of Study 1"),
            (2, "the structure of nucleic acids"),
            (2, "protein synthesis"),
            (1, "Area of Study 2"),
            (0, "Unit 4"),
        ]
    );
    assert_eq!(rows[0].kind, "unit");
    assert_eq!(rows[1].kind, "aos");
    assert_eq!(rows[2].kind, "dot_point");
}

#[test]
fn tabs_and_spaces_both_indent() {
    let rows = parse_outline("Unit 3\n\tAoS 1\n\t\tsomething\n");
    assert_eq!(rows.iter().map(|r| r.depth).collect::<Vec<_>>(), vec![0, 1, 2]);
}

#[test]
fn bullets_and_numbering_are_stripped() {
    let rows = parse_outline("- Unit 3\n  * AoS 1\n    1. first point\n    2) second point\n");
    let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["Unit 3", "AoS 1", "first point", "second point"]);
}

/// A dot point can legitimately contain a full stop or a number; only a short
/// leading marker should be removed.
#[test]
fn text_that_merely_contains_punctuation_is_left_alone() {
    let rows = parse_outline("the role of ATP. and ADP in energy transfer\nDNA replication in S phase\n");
    assert_eq!(rows[0].name, "the role of ATP. and ADP in energy transfer");
    assert_eq!(rows[1].name, "DNA replication in S phase");
}

#[test]
fn blank_lines_are_ignored() {
    assert_eq!(parse_outline("\n\n  \n\nUnit 3\n\n").len(), 1);
    assert!(parse_outline("").is_empty());
}

#[test]
fn importing_an_outline_builds_a_real_tree() {
    let mut conn = db();
    let rows = parse_outline("Unit 3\n  AoS 1\n    point one\n    point two\n  AoS 2\n");
    assert_eq!(import_outline(&mut conn, 1, &rows).unwrap(), 5);

    let tree = tree(&conn, 1).unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Unit 3");
    assert_eq!(tree[0].children.len(), 2);
    assert_eq!(tree[0].children[0].name, "AoS 1");
    assert_eq!(tree[0].children[0].children.len(), 2);
    assert_eq!(tree[0].children[0].children[1].name, "point two");
}

#[test]
fn reimporting_replaces_rather_than_accumulating() {
    let mut conn = db();
    import_outline(&mut conn, 1, &parse_outline("Unit 3\n  AoS 1\n")).unwrap();
    import_outline(&mut conn, 1, &parse_outline("Unit 3\n  AoS 1\n  AoS 2\n")).unwrap();

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM topics WHERE subject_id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3);
}

#[test]
fn importing_one_subject_leaves_another_untouched() {
    let mut conn = db();
    import_outline(&mut conn, 2, &parse_outline("Methods\n  Calculus\n")).unwrap();
    import_outline(&mut conn, 1, &parse_outline("Unit 3\n")).unwrap();

    let methods: i64 = conn
        .query_row("SELECT COUNT(*) FROM topics WHERE subject_id = 2", [], |r| r.get(0))
        .unwrap();
    assert_eq!(methods, 2);
}

#[test]
fn an_empty_import_is_refused_rather_than_wiping_the_tree() {
    let mut conn = db();
    import_outline(&mut conn, 1, &parse_outline("Unit 3\n  AoS 1\n")).unwrap();

    assert!(import_outline(&mut conn, 1, &[]).is_err());

    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM topics WHERE subject_id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2, "a rejected import must not have deleted anything");
}

// ---------------------------------------------------------------------------
// Tree progress
// ---------------------------------------------------------------------------

#[test]
fn progress_attaches_to_the_topic_that_earned_it() {
    let mut conn = db();
    import_outline(&mut conn, 1, &parse_outline("Unit 3\n  AoS 1\n")).unwrap();

    let aos: i64 = conn
        .query_row("SELECT id FROM topics WHERE name = 'AoS 1'", [], |r| r.get(0))
        .unwrap();

    conn.execute(
        "INSERT INTO topic_reviews (topic_id, confidence, reviewed_at, local_date)
         VALUES (?1, 4, '2026-08-12T10:00:00Z', '2026-08-12')",
        [aos],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cards (subject_id, topic_id, note_type, front, back, state,
                            content_hash, created_at)
         VALUES (1, ?1, 'basic', 'q', 'a', 'new', 'h1', '2026-08-12T10:00:00Z')",
        [aos],
    )
    .unwrap();

    let tree = tree(&conn, 1).unwrap();
    let unit = &tree[0];
    let child = &unit.children[0];

    assert_eq!(child.confidence, Some(4));
    assert_eq!(child.last_reviewed_on.as_deref(), Some("2026-08-12"));
    assert_eq!(child.card_count, 1);

    // The parent must NOT inherit it — a unit isn't revised because one dot
    // point was.
    assert_eq!(unit.confidence, None);
    assert_eq!(unit.card_count, 0);
}

/// Same-second reviews are reachable because timestamps are whole seconds; the
/// newest by id must win.
#[test]
fn the_latest_confidence_wins_even_within_the_same_second() {
    let mut conn = db();
    import_outline(&mut conn, 1, &parse_outline("Unit 3\n")).unwrap();
    let t: i64 = conn.query_row("SELECT id FROM topics LIMIT 1", [], |r| r.get(0)).unwrap();

    for c in [1, 5] {
        conn.execute(
            "INSERT INTO topic_reviews (topic_id, confidence, reviewed_at, local_date)
             VALUES (?1, ?2, '2026-08-12T10:00:00Z', '2026-08-12')",
            rusqlite::params![t, c],
        )
        .unwrap();
    }

    assert_eq!(tree(&conn, 1).unwrap()[0].confidence, Some(5));
}

#[test]
fn a_subject_with_no_topics_yields_an_empty_tree() {
    let conn = db();
    assert!(tree(&conn, 1).unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// Exam simulation
// ---------------------------------------------------------------------------

#[test]
fn the_phases_use_the_specified_durations() {
    assert_eq!(READING_SECONDS, 900);
    assert_eq!(WRITING_SECONDS, 9000);

    assert_eq!(phase_for(0), Phase::Reading);
    assert_eq!(phase_for(899), Phase::Reading);
    // The transition is exactly at 15 minutes.
    assert_eq!(phase_for(900), Phase::Writing);
    assert_eq!(phase_for(9899), Phase::Writing);
    assert_eq!(phase_for(9900), Phase::Finished);
}

#[test]
fn a_run_transitions_from_reading_to_writing_on_the_clock() {
    let conn = db();
    let started = start(&conn, 1, "2024 VCAA", at(9, 0)).unwrap();
    assert_eq!(started.phase, Phase::Reading);
    assert_eq!(started.remaining_seconds, 900);

    let run = load(&conn).unwrap().unwrap();
    assert_eq!(state_at(&run, at(9, 14)).unwrap().phase, Phase::Reading);
    assert_eq!(state_at(&run, at(9, 15)).unwrap().phase, Phase::Writing);
    assert_eq!(state_at(&run, at(11, 44)).unwrap().phase, Phase::Writing);
    assert_eq!(state_at(&run, at(11, 45)).unwrap().phase, Phase::Finished);
}

/// The whole point of storing only a start instant: quitting mid-exam and
/// reopening must resume at the right moment, not restart.
#[test]
fn a_run_survives_a_restart_and_resumes_at_the_right_point() {
    let conn = db();
    start(&conn, 1, "Paper", at(9, 0)).unwrap();

    // Simulating a relaunch: nothing in memory, everything re-read.
    let reloaded = load(&conn).unwrap().expect("the run should persist");
    let state = state_at(&reloaded, at(10, 30)).unwrap();

    assert_eq!(state.phase, Phase::Writing);
    assert_eq!(state.elapsed_seconds, 90 * 60);
}

#[test]
fn paused_time_does_not_count_towards_the_exam() {
    let conn = db();
    start(&conn, 1, "Paper", at(9, 0)).unwrap();

    set_paused(&conn, true, at(9, 5)).unwrap();
    let resumed = set_paused(&conn, false, at(9, 35)).unwrap();

    // Half an hour of wall clock, five minutes of exam.
    assert_eq!(resumed.elapsed_seconds, 5 * 60);
    assert_eq!(resumed.phase, Phase::Reading);
    assert!(!resumed.paused);

    let run = load(&conn).unwrap().unwrap();
    assert_eq!(run.paused_seconds, 30 * 60);
}

#[test]
fn the_clock_is_frozen_while_paused() {
    let conn = db();
    start(&conn, 1, "Paper", at(9, 0)).unwrap();
    set_paused(&conn, true, at(9, 5)).unwrap();

    let run = load(&conn).unwrap().unwrap();
    assert_eq!(state_at(&run, at(9, 10)).unwrap().elapsed_seconds, 5 * 60);
    assert_eq!(state_at(&run, at(9, 50)).unwrap().elapsed_seconds, 5 * 60);
    assert!(state_at(&run, at(9, 50)).unwrap().paused);
}

#[test]
fn pausing_twice_is_not_double_counted() {
    let conn = db();
    start(&conn, 1, "Paper", at(9, 0)).unwrap();
    set_paused(&conn, true, at(9, 5)).unwrap();
    set_paused(&conn, true, at(9, 20)).unwrap(); // already paused
    let resumed = set_paused(&conn, false, at(9, 35)).unwrap();

    assert_eq!(resumed.elapsed_seconds, 5 * 60);
}

#[test]
fn only_one_exam_can_run_at_a_time() {
    let conn = db();
    start(&conn, 1, "One", at(9, 0)).unwrap();
    assert!(start(&conn, 1, "Two", at(9, 1)).is_err());
}

#[test]
fn finishing_logs_the_real_time_spent_and_clears_the_run() {
    let conn = db();
    start(&conn, 1, "2024 VCAA", at(9, 0)).unwrap();
    let id = finish(&conn, at(10, 0)).unwrap();

    assert!(load(&conn).unwrap().is_none(), "the run should be cleared");

    let exams = history(&conn, 1, 10).unwrap();
    assert_eq!(exams.len(), 1);
    assert_eq!(exams[0].id, id);
    assert_eq!(exams[0].name, "2024 VCAA");
    // 15 minutes reading, 45 writing — not the full paper.
    assert_eq!(exams[0].reading_seconds, Some(900));
    assert_eq!(exams[0].writing_seconds, Some(45 * 60));
    assert_eq!(exams[0].section_a_max, 40);
    assert_eq!(exams[0].section_b_max, 80);
}

/// Stopping during reading time must not record writing time that never
/// happened.
#[test]
fn finishing_during_reading_logs_no_writing_time() {
    let conn = db();
    start(&conn, 1, "Aborted", at(9, 0)).unwrap();
    finish(&conn, at(9, 5)).unwrap();

    let e = &history(&conn, 1, 10).unwrap()[0];
    assert_eq!(e.reading_seconds, Some(300));
    assert_eq!(e.writing_seconds, Some(0));
}

#[test]
fn cancelling_clears_the_run_without_logging_it() {
    let conn = db();
    start(&conn, 1, "Nope", at(9, 0)).unwrap();
    cancel(&conn).unwrap();

    assert!(load(&conn).unwrap().is_none());
    assert!(history(&conn, 1, 10).unwrap().is_empty());
}

#[test]
fn finishing_or_pausing_with_no_run_is_an_error_not_a_panic() {
    let conn = db();
    assert!(finish(&conn, at(9, 0)).is_err());
    assert!(set_paused(&conn, true, at(9, 0)).is_err());
    assert!(load(&conn).unwrap().is_none());
}

/// A clock that jumps backwards (NTP correction, manual change) must not send
/// the phase back to reading or produce a negative time.
#[test]
fn a_backwards_clock_does_not_produce_negative_elapsed_time() {
    let conn = db();
    start(&conn, 1, "Paper", at(10, 0)).unwrap();

    let run = load(&conn).unwrap().unwrap();
    let state = state_at(&run, at(9, 0)).unwrap();
    assert_eq!(state.elapsed_seconds, 0);
    assert_eq!(state.phase, Phase::Reading);
}

#[test]
fn a_corrupt_run_blob_is_treated_as_no_run() {
    let conn = db();
    crate::settings::set(&conn, "exam_sim_run", "{not json").unwrap();
    assert!(load(&conn).unwrap().is_none());
    // And a new exam can still be started.
    assert!(start(&conn, 1, "Fresh", at(9, 0)).is_ok());
}

#[test]
fn scores_can_be_recorded_after_the_fact() {
    let conn = db();
    start(&conn, 1, "Paper", at(9, 0)).unwrap();
    let id = finish(&conn, at(11, 45)).unwrap();

    score(&conn, id, Some(34), Some(61)).unwrap();

    let e = &history(&conn, 1, 10).unwrap()[0];
    assert_eq!(e.section_a_score, Some(34));
    assert_eq!(e.section_b_score, Some(61));
}

// ---------------------------------------------------------------------------
// Terminology deck
// ---------------------------------------------------------------------------

#[test]
fn the_terminology_summary_counts_only_tagged_cards() {
    let conn = db();
    let today = util::retain_today();

    conn.execute(
        "INSERT INTO cards (subject_id, note_type, front, back, tags, state,
                            content_hash, created_at)
         VALUES (1,'basic','a','b','terminology bio','new','h1','2026-08-01T00:00:00Z'),
                (1,'basic','c','d','terminology','new','h2','2026-08-01T00:00:00Z'),
                (1,'basic','e','f','genetics','new','h3','2026-08-01T00:00:00Z')",
        [],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO cards (subject_id, note_type, front, back, tags, state, due_on,
                            content_hash, created_at)
         VALUES (1,'basic','g','h','terminology','review',?1,'h4','2026-08-01T00:00:00Z')",
        [&today],
    )
    .unwrap();

    let s = terminology_summary(&conn, 1).unwrap();
    assert_eq!(s.total, 3, "the untagged card should not be counted");
    assert_eq!(s.new, 2);
    assert_eq!(s.due, 1);
}

#[test]
fn terminology_counts_are_per_subject() {
    let conn = db();
    conn.execute(
        "INSERT INTO cards (subject_id, note_type, front, back, tags, state,
                            content_hash, created_at)
         VALUES (2,'basic','a','b','terminology','new','h9','2026-08-01T00:00:00Z')",
        [],
    )
    .unwrap();

    assert_eq!(terminology_summary(&conn, 1).unwrap().total, 0);
    assert_eq!(terminology_summary(&conn, 2).unwrap().total, 1);
}

/// A realistic study-design paste.
///
/// The *shape* only — headings, outcome lines, lettered and numbered key
/// knowledge, wrapped lines. No VCAA text: the point is that whatever the user
/// pastes from their own copy survives the importer.
#[test]
fn a_realistically_shaped_study_design_paste_imports_correctly() {
    let pasted = "\
Unit 3
\tArea of Study 1
\t\tOutcome 1
\t\t\t1. the first key knowledge point
\t\t\t2. the second key knowledge point
\tArea of Study 2
\t\tOutcome 2
\t\t\t- a point written with a dash
\t\t\t- another point
Unit 4
\tArea of Study 1
\t\tOutcome 1
\t\t\ta) a lettered point
";

    let rows = parse_outline(pasted);
    assert_eq!(rows.len(), 13);

    // Depth is preserved to four levels, not flattened.
    assert_eq!(rows.iter().filter(|r| r.depth == 0).count(), 2, "two units");
    assert_eq!(rows.iter().filter(|r| r.depth == 1).count(), 3, "three areas of study");
    assert_eq!(rows.iter().filter(|r| r.depth == 2).count(), 3, "three outcomes");
    assert_eq!(rows.iter().filter(|r| r.depth == 3).count(), 5, "five key knowledge points");

    // Markers are stripped, content is not.
    assert_eq!(rows[3].name, "the first key knowledge point");
    assert_eq!(rows[7].name, "a point written with a dash");
    assert_eq!(rows[12].name, "a lettered point");

    let mut conn = db();
    assert_eq!(import_outline(&mut conn, 1, &rows).unwrap(), 13);

    let tree = tree(&conn, 1).unwrap();
    assert_eq!(tree.len(), 2, "Unit 3 and Unit 4 are the roots");
    assert_eq!(tree[0].children.len(), 2, "Unit 3 has two areas of study");
    assert_eq!(tree[0].children[0].children[0].children.len(), 2, "outcome holds its points");
}

/// Text that merely contains a colon or a semicolon (very common in study
/// designs) must not be treated as structure.
#[test]
fn punctuation_inside_a_dot_point_is_not_treated_as_structure() {
    let rows = parse_outline("Unit 3\n\tthe structure of DNA: nucleotides, bases; and bonding\n");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].name, "the structure of DNA: nucleotides, bases; and bonding");
}

/// Importing must survive a large paste without complaint.
#[test]
fn a_large_outline_imports_intact() {
    let mut text = String::from("Unit 3\n");
    for i in 0..300 {
        text.push_str(&format!("\tdot point number {i}\n"));
    }

    let rows = parse_outline(&text);
    assert_eq!(rows.len(), 301);

    let mut conn = db();
    assert_eq!(import_outline(&mut conn, 1, &rows).unwrap(), 301);
    assert_eq!(tree(&conn, 1).unwrap()[0].children.len(), 300);
}
