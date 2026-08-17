use super::*;

/// A real STAV paper's shape, trimmed. The front matter is verbatim from the
/// kind of thing actually sitting in the library.
const PAPER: &str = "\
Published by STAV Publishing Pty Ltd. STAV House, 5 Munro Street, Coburg VIC 3058.
BIOLOGY
Unit 4
Trial Examination
Students are NOT permitted to bring blank sheets of paper.

Question 1
Which of the following best describes the role of an enzyme in a metabolic pathway?
A. It raises the activation energy.
B. It lowers the activation energy.

Question 2 (4 marks)
Explain how a competitive inhibitor differs from a non-competitive inhibitor.
Refer to the active site in your answer.

Question 3
Short.
";

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
    conn.execute(
        "INSERT INTO topics (id,subject_id,name,sort_order)
         VALUES (10,1,'Enzymes',0),(11,1,'Photosynthesis',1),(12,1,'Cell',2)",
        [],
    )
    .unwrap();
    conn
}

fn add_paper(conn: &Connection, id: i64, title: &str, kind: &str, content: &str) {
    conn.execute(
        "INSERT INTO resources (id,subject_id,title,kind,content,word_count,added_at)
         VALUES (?1,1,?2,?3,?4,10,'2026-08-01T00:00:00Z')",
        rusqlite::params![id, title, kind, content],
    )
    .unwrap();
}

// -- cutting a paper up ------------------------------------------------------

#[test]
fn a_paper_splits_on_its_question_markers() {
    let got = segment(PAPER);

    assert_eq!(got.len(), 2, "question 3 is too short to be a question");
    assert_eq!(got[0].label, "Question 1");
    assert_eq!(got[0].number, 1);
    assert!(got[0].text.starts_with("Which of the following"));
    assert!(got[0].text.contains("B. It lowers"), "the options belong to the question");
}

/// Everything before the first marker is the publisher's address and the
/// instructions. It is not question 1.
#[test]
fn front_matter_is_dropped() {
    let got = segment(PAPER);
    assert!(!got.iter().any(|q| q.text.contains("STAV House")));
    assert!(!got.iter().any(|q| q.text.contains("blank sheets of paper")));
}

#[test]
fn marks_in_brackets_stay_part_of_the_label() {
    let got = segment(PAPER);
    assert_eq!(got[1].label, "Question 2 (4 marks)");
    assert_eq!(got[1].number, 2);
    assert!(got[1].text.starts_with("Explain how"));
}

/// The failure that would halve a question: a paper referring to an earlier one
/// mid-sentence must not start a new span.
#[test]
fn a_cross_reference_is_not_a_boundary() {
    let text = "\
Question 5
Using the graph from Question 3 above, calculate the rate.
Show all working for full marks.
";
    let got = segment(text);

    assert_eq!(got.len(), 1);
    assert!(got[0].text.contains("Question 3 above"), "it stays inside question 5");
}

/// A bare `3.` at the start of a line is far more common than a real question —
/// every answer grid, page number and reference list produces one.
#[test]
fn a_bare_number_is_not_a_question() {
    assert!(marker("3. A B C D").is_none());
    assert!(marker("12").is_none());
    assert!(marker("Questions 1 to 5 refer to the diagram").is_none());
    assert!(marker("").is_none());
    // Nor is a number too big to be one.
    assert!(marker("Question 2005").is_none());
    assert!(marker("Question 0").is_none());
}

#[test]
fn the_marker_is_recognised_in_the_forms_papers_use() {
    assert_eq!(marker("Question 7").unwrap().1, 7);
    assert_eq!(marker("  Question 7  ").unwrap().1, 7);
    assert_eq!(marker("QUESTION 7").unwrap().1, 7);
    assert_eq!(marker("Question 7 (10 marks)").unwrap().1, 7);
    assert_eq!(marker("Question 7.").unwrap().1, 7);
}

// -- tagging -----------------------------------------------------------------

/// The only automatic tagging done. A built-in keyword list would mean Retain
/// deciding what counts as a topic in VCE Biology, which is inventing
/// curriculum — these are the student's own topic names.
#[test]
fn auto_tags_come_from_the_students_own_topics() {
    let topics = vec!["Enzymes".to_string(), "Photosynthesis".to_string()];

    // Singular text, plural topic. This is how topic lists are actually
    // written, and matching literally tagged nothing at all.
    let tags = auto_tags("Explain how an enzyme lowers activation energy.", &topics);
    assert_eq!(tags, vec!["Enzymes"]);
    assert_eq!(auto_tags("Enzymes are catalysts.", &topics), vec!["Enzymes"]);

    assert!(auto_tags("Describe the light-independent stage.", &topics).is_empty());
}

/// "Cell" tagging every question containing "excellent" is the failure that
/// makes automatic tags worthless.
#[test]
fn a_topic_inside_a_longer_word_is_not_a_match() {
    let topics = vec!["Cell".to_string()];

    assert!(auto_tags("An excellent answer would mention this.", &topics).is_empty());
    assert!(auto_tags("Cellular respiration occurs here.", &topics).is_empty());
    assert_eq!(auto_tags("Describe the cell membrane.", &topics), vec!["Cell"]);
    // The plural of the topic is still the topic.
    assert_eq!(auto_tags("The cells divide by mitosis.", &topics), vec!["Cell"]);
}

#[test]
fn very_short_topic_names_are_ignored() {
    // A two-letter topic would match half the paper.
    assert!(auto_tags("pH is measured on a scale.", &["pH".to_string()]).is_empty());
}

// -- indexing and searching --------------------------------------------------

#[test]
fn indexing_a_paper_stores_its_questions_with_tags() {
    let mut conn = db();
    add_paper(&conn, 1, "2005 STAV Unit 4", "trial_test", PAPER);

    let n = index_resource(&mut conn, 1).unwrap();
    assert_eq!(n, 2);

    let found = search(&conn, "inhibitor", None, None, 10).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].label, "Question 2 (4 marks)");
    assert_eq!(found[0].resource_title, "2005 STAV Unit 4");
    assert_eq!(found[0].subject_name.as_deref(), Some("Biology"));
    // Question 2 talks about inhibitors and the active site, and never says
    // "enzyme" — so it gets no automatic tag, which is the honest answer.
    assert!(found[0].tags.is_empty());

    let q1 = search(&conn, "activation", None, None, 10).unwrap();
    assert_eq!(q1[0].tags, vec!["Enzymes"], "tagged from the topic list");
}

/// Segmenting a study design on the word "Question" produces nonsense with a
/// question number attached to it.
#[test]
fn only_exam_shaped_material_is_indexed() {
    let mut conn = db();
    add_paper(&conn, 1, "Study design", "study_design", PAPER);
    add_paper(&conn, 2, "My notes", "personal_notes", PAPER);

    assert_eq!(index_resource(&mut conn, 1).unwrap(), 0);
    assert_eq!(index_resource(&mut conn, 2).unwrap(), 0);
    assert!(search(&conn, "enzyme", None, None, 10).unwrap().is_empty());
}

/// The button that triggers this is easy to press twice.
#[test]
fn indexing_twice_does_not_double_the_questions() {
    let mut conn = db();
    add_paper(&conn, 1, "Paper", "past_paper", PAPER);

    index_resource(&mut conn, 1).unwrap();
    index_resource(&mut conn, 1).unwrap();

    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM questions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 2);
}

/// External-content FTS does not clean up after itself. Without the triggers a
/// deleted question stays searchable forever.
#[test]
fn deleting_a_paper_takes_its_questions_out_of_the_index() {
    let mut conn = db();
    add_paper(&conn, 1, "Paper", "past_paper", PAPER);
    index_resource(&mut conn, 1).unwrap();
    assert_eq!(search(&conn, "enzyme", None, None, 10).unwrap().len(), 1);

    conn.execute("DELETE FROM resources WHERE id = 1", []).unwrap();

    assert!(search(&conn, "enzyme", None, None, 10).unwrap().is_empty());
    let orphans: i64 = conn
        .query_row("SELECT COUNT(*) FROM question_tags", [], |r| r.get(0))
        .unwrap();
    assert_eq!(orphans, 0);
}

#[test]
fn a_tag_filter_works_with_and_without_a_query() {
    let mut conn = db();
    add_paper(&conn, 1, "Paper", "past_paper", PAPER);
    index_resource(&mut conn, 1).unwrap();

    // Tag alone: no MATCH, so ordering must not touch bm25.
    let by_tag = search(&conn, "", None, Some("Enzymes"), 10).unwrap();
    assert_eq!(by_tag.len(), 1);

    // Query and tag are an AND. "inhibitor" is question 2, which carries no
    // Enzymes tag, so the pair matches nothing — and that is the point of
    // combining them.
    assert!(search(&conn, "inhibitor", None, Some("Enzymes"), 10).unwrap().is_empty());
    assert_eq!(search(&conn, "activation", None, Some("Enzymes"), 10).unwrap().len(), 1);

    assert!(search(&conn, "", None, Some("Nothing"), 10).unwrap().is_empty());
}

#[test]
fn a_manual_tag_can_be_added_and_taken_off() {
    let mut conn = db();
    add_paper(&conn, 1, "Paper", "past_paper", PAPER);
    index_resource(&mut conn, 1).unwrap();
    let id = search(&conn, "inhibitor", None, None, 1).unwrap()[0].id;

    add_tag(&conn, id, "  Hard  ").unwrap();
    let tags = &search(&conn, "inhibitor", None, None, 1).unwrap()[0].tags;
    assert!(tags.contains(&"hard".to_string()), "normalised: {tags:?}");

    remove_tag(&conn, id, "HARD").unwrap();
    assert!(!search(&conn, "inhibitor", None, None, 1).unwrap()[0]
        .tags
        .contains(&"hard".to_string()));
}

#[test]
fn tags_are_listed_most_used_first() {
    let mut conn = db();
    add_paper(&conn, 1, "Paper", "past_paper", PAPER);
    index_resource(&mut conn, 1).unwrap();

    let ids: Vec<i64> = search(&conn, "", None, None, 10).unwrap().iter().map(|q| q.id).collect();
    add_tag(&conn, ids[0], "sac").unwrap();

    let tags = all_tags(&conn, None).unwrap();
    assert!(tags.contains(&("Enzymes".to_string(), 1)));
    assert!(tags.contains(&("sac".to_string(), 1)));
}

#[test]
fn a_paper_with_no_questions_yet_is_reported_as_unindexed() {
    let mut conn = db();
    add_paper(&conn, 1, "Paper", "past_paper", PAPER);
    add_paper(&conn, 2, "Notes", "personal_notes", PAPER);

    assert_eq!(unindexed(&conn).unwrap(), vec![1], "notes are not exam material");

    index_resource(&mut conn, 1).unwrap();
    assert!(unindexed(&conn).unwrap().is_empty());
}
