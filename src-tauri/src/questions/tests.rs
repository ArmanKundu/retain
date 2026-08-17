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

    // Tagging reads the study design's vocabulary, not the topic list — see
    // `auto_tags_by_vocabulary`. A subject with topic rows and no study design
    // has nothing to match against, which is exactly the state the real
    // library was in and why nothing was ever tagged.
    conn.execute(
        "INSERT INTO resources (id,subject_id,title,kind,content,word_count,added_at)
         VALUES (99,1,'Biology SD','study_design',?1,10,'2026-08-01T00:00:00Z')",
        [DESIGN],
    )
    .unwrap();
    conn
}

/// Two headings with enough distinctive vocabulary to be told apart. The
/// bullet is the private-use character a real study design contains.
const DESIGN: &str = "\
Key knowledge

Enzymes and metabolism

\u{F0B7} enzymes as protein catalysts, including the active site and competitive inhibitors
\u{F0B7} activation energy, denaturation and the effect of inhibitors on reaction rates

Photosynthesis and respiration

\u{F0B7} chloroplasts, thylakoids, the Calvin cycle and glucose production
\u{F0B7} mitochondria, glycolysis and the electron transport chain
";

/// Default filters: no year range, no source, and solutions kept in — the
/// tests are about segmentation and tagging, not the filter surface.
fn any() -> Filters {
    Filters { include_solutions: true, ..Default::default() }
}

fn tagged(tag: &str) -> Filters {
    Filters { tag: Some(tag.into()), include_solutions: true, ..Default::default() }
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

/// Vocabulary matching, which replaced matching on topic names.
///
/// Names barely worked: measured against the real library, whole-name matching
/// tagged 37 questions out of 6,529, because a heading reads "Cellular
/// structure and function" while the question says "active transport across
/// the plasma membrane". The words that connect them are the study design's
/// own dot points.
#[test]
fn a_question_is_tagged_by_the_words_its_topic_uses() {
    let vocab = crate::resources::topic_vocabulary(DESIGN);

    let tags = auto_tags_by_vocabulary(
        "Describe how a competitive inhibitor affects the active site and the reaction rate.",
        &vocab,
    );
    assert_eq!(tags, vec!["Enzymes and metabolism"]);

    let other = auto_tags_by_vocabulary(
        "Explain the role of chloroplasts and the Calvin cycle in glucose production.",
        &vocab,
    );
    assert_eq!(other, vec!["Photosynthesis and respiration"]);
}

/// A question that happens to use two or three words from a topic is not about
/// that topic. Three distinct terms is the line.
#[test]
fn a_passing_mention_is_not_a_tag() {
    let vocab = crate::resources::topic_vocabulary(DESIGN);

    assert!(auto_tags_by_vocabulary("Define energy.", &vocab).is_empty());
    assert!(
        auto_tags_by_vocabulary("The reaction produced energy.", &vocab).is_empty(),
        "two words is a coincidence"
    );
}

/// The design writes "inhibitors" and the question writes "inhibitor".
/// Matching them exactly misses, and that is most of the vocabulary.
#[test]
fn plural_and_singular_are_the_same_word() {
    let vocab = crate::resources::topic_vocabulary(DESIGN);

    assert_eq!(
        auto_tags_by_vocabulary(
            "One inhibitor binds the active site; the enzyme catalyst is blocked.",
            &vocab,
        ),
        vec!["Enzymes and metabolism"]
    );
}

/// A question belongs to one topic. Six tags is the same as none.
#[test]
fn at_most_two_tags_are_given() {
    let vocab = crate::resources::topic_vocabulary(DESIGN);
    let both = "enzymes catalysts activation energy chloroplasts thylakoids glucose mitochondria";

    assert!(auto_tags_by_vocabulary(both, &vocab).len() <= 2);
}

/// A subject with no study design uploaded has nothing to match against — the
/// state the whole library was in, and why nothing was tagged.
#[test]
fn no_study_design_means_no_tags_rather_than_wrong_ones() {
    assert!(auto_tags_by_vocabulary("Describe the active site.", &[]).is_empty());
}

// -- indexing and searching --------------------------------------------------

#[test]
fn indexing_a_paper_stores_its_questions_with_tags() {
    let mut conn = db();
    add_paper(&conn, 1, "2005 STAV Unit 4", "trial_test", PAPER);

    let n = index_resource(&mut conn, 1).unwrap();
    assert_eq!(n, 2);

    let found = search(&conn, "inhibitor", &any(), 10).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].label, "Question 2 (4 marks)");
    assert_eq!(found[0].resource_title, "2005 STAV Unit 4");
    assert_eq!(found[0].subject_name.as_deref(), Some("Biology"));
    // Question 2 uses "competitive", "inhibitor" and "active site" — the
    // study design's own words for that topic, which is the whole point of
    // matching on vocabulary rather than on the heading's name.
    assert_eq!(found[0].tags, vec!["Enzymes and metabolism"]);
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
    assert!(search(&conn, "enzyme", &any(), 10).unwrap().is_empty());
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
    assert_eq!(search(&conn, "enzyme", &any(), 10).unwrap().len(), 1);

    conn.execute("DELETE FROM resources WHERE id = 1", []).unwrap();

    assert!(search(&conn, "enzyme", &any(), 10).unwrap().is_empty());
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

    // Tag alone: no MATCH, so ordering must not touch bm25. Both questions are
    // about enzymes, so both carry the tag.
    let by_tag = search(&conn, "", &tagged("Enzymes and metabolism"), 10).unwrap();
    assert_eq!(by_tag.len(), 2);

    // Query and tag are an AND.
    assert_eq!(
        search(&conn, "inhibitor", &tagged("Enzymes and metabolism"), 10).unwrap().len(),
        1
    );
    assert!(search(&conn, "photosynthesis", &tagged("Enzymes and metabolism"), 10)
        .unwrap()
        .is_empty());

    assert!(search(&conn, "", &tagged("Nothing"), 10).unwrap().is_empty());
}

#[test]
fn a_manual_tag_can_be_added_and_taken_off() {
    let mut conn = db();
    add_paper(&conn, 1, "Paper", "past_paper", PAPER);
    index_resource(&mut conn, 1).unwrap();
    let id = search(&conn, "inhibitor", &any(), 1).unwrap()[0].id;

    add_tag(&conn, id, "  Hard  ").unwrap();
    let tags = &search(&conn, "inhibitor", &any(), 1).unwrap()[0].tags;
    assert!(tags.contains(&"hard".to_string()), "normalised: {tags:?}");

    remove_tag(&conn, id, "HARD").unwrap();
    assert!(!search(&conn, "inhibitor", &any(), 1).unwrap()[0]
        .tags
        .contains(&"hard".to_string()));
}

#[test]
fn tags_are_listed_most_used_first() {
    let mut conn = db();
    add_paper(&conn, 1, "Paper", "past_paper", PAPER);
    index_resource(&mut conn, 1).unwrap();

    let ids: Vec<i64> = search(&conn, "", &any(), 10).unwrap().iter().map(|q| q.id).collect();
    add_tag(&conn, ids[0], "sac").unwrap();

    let tags = all_tags(&conn, None).unwrap();
    assert!(tags.iter().any(|(t, _)| t == "Enzymes and metabolism"), "{tags:?}");
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

// -- what a paper's title actually says --------------------------------------
//
// Every title below is real, taken from the library.

#[test]
fn a_title_gives_up_its_year_publisher_and_whether_it_is_the_answers() {
    let cases = [
        ("2018 kilbaha exam 1 solutions", Some(2018), Some("kilbaha"), true),
        ("2016 TSSM Unit 4 Key Topic Test 1", Some(2016), Some("tssm"), false),
        ("2024 vcaa nht solutions", Some(2024), Some("vcaa"), true),
        ("2000 vcaa unit 4 report", Some(2000), Some("vcaa"), true),
        ("2015 engage a exam 1", Some(2015), Some("engage"), false),
        ("2022 neap unit 2", Some(2022), Some("neap"), false),
        ("2016 vcaa", Some(2016), Some("vcaa"), false),
        ("2019 access solutions", Some(2019), Some("access"), true),
    ];

    for (title, year, source, solutions) in cases {
        let m = paper_meta(title);
        assert_eq!(m.year, year, "{title}");
        assert_eq!(m.source.as_deref(), source, "{title}");
        assert_eq!(m.is_solutions, solutions, "{title}");
    }
}

/// An examiner's report is answers with commentary, which is the same thing for
/// the purpose of "show me the answer".
#[test]
fn an_examiners_report_counts_as_solutions() {
    assert!(paper_meta("2018 vcaa nht report").is_solutions);
}

#[test]
fn a_title_with_nothing_in_it_yields_nothing_rather_than_a_guess() {
    let m = paper_meta("Chapter notes");
    assert_eq!(m.year, None);
    assert_eq!(m.source, None);
    assert!(!m.is_solutions);
}

/// A publisher name inside a longer word is not that publisher.
#[test]
fn a_source_must_be_a_whole_word() {
    assert_eq!(paper_meta("2019 accessed later").source, None);
    assert_eq!(paper_meta("2019 access exam").source.as_deref(), Some("access"));
}

#[test]
fn a_year_is_found_wherever_it_sits_in_the_title() {
    assert_eq!(paper_meta("Unit 4 2016 exam").year, Some(2016));
    // A question count or a page number is not a year.
    assert_eq!(paper_meta("exam 1 of 40").year, None);
    assert_eq!(paper_meta("9999 something").year, None);
}

// -- pairing a paper with its answers ----------------------------------------

/// Pairing the wrong solutions to a question is worse than pairing none: you
/// would revise from the answer to a different question and never notice.
#[test]
fn a_paper_finds_its_own_solutions_and_nothing_elses() {
    let conn = db();
    add_paper(&conn, 1, "2018 kilbaha exam 1", "past_paper", PAPER);
    add_paper(&conn, 2, "2018 kilbaha exam 1 solutions", "exam_solution", PAPER);
    add_paper(&conn, 3, "2018 kilbaha exam 2 solutions", "exam_solution", PAPER);

    let found = solutions_for(&conn, 1).unwrap();
    assert_eq!(found.unwrap().0, 2, "exam 2's answers are not exam 1's");
}

#[test]
fn a_paper_with_no_solutions_in_the_library_pairs_with_nothing() {
    let conn = db();
    add_paper(&conn, 1, "2018 kilbaha exam 1", "past_paper", PAPER);
    add_paper(&conn, 2, "2019 neap exam 1 solutions", "exam_solution", PAPER);

    assert_eq!(solutions_for(&conn, 1).unwrap(), None);
}

/// A solutions document doesn't have solutions of its own.
#[test]
fn the_answers_do_not_pair_with_themselves() {
    let conn = db();
    add_paper(&conn, 1, "2018 kilbaha exam 1 solutions", "exam_solution", PAPER);
    assert_eq!(solutions_for(&conn, 1).unwrap(), None);
}

/// A longer title that isn't the answers must not be accepted as them.
#[test]
fn a_longer_title_is_not_automatically_the_answers() {
    let conn = db();
    add_paper(&conn, 1, "2018 kilbaha exam 1", "past_paper", PAPER);
    add_paper(&conn, 2, "2018 kilbaha exam 1 section b", "past_paper", PAPER);

    assert_eq!(solutions_for(&conn, 1).unwrap(), None);
}

// -- filtering ---------------------------------------------------------------

fn two_years(conn: &mut Connection) {
    add_paper(conn, 1, "2014 vcaa exam 1", "past_paper", PAPER);
    add_paper(conn, 2, "2024 neap exam 1", "past_paper", PAPER);
    add_paper(conn, 3, "2024 neap exam 1 solutions", "exam_solution", PAPER);
    for id in 1..=3 {
        index_resource(conn, id).unwrap();
    }
}

#[test]
fn a_year_range_narrows_to_the_papers_in_it() {
    let mut conn = db();
    two_years(&mut conn);

    let recent = Filters { from_year: Some(2020), include_solutions: true, ..Default::default() };
    let years: Vec<Option<i64>> =
        search(&conn, "enzyme", &recent, 50).unwrap().iter().map(|q| q.paper.year).collect();
    assert!(years.iter().all(|y| *y == Some(2024)), "{years:?}");

    let old = Filters { to_year: Some(2015), include_solutions: true, ..Default::default() };
    assert!(search(&conn, "enzyme", &old, 50)
        .unwrap()
        .iter()
        .all(|q| q.paper.year == Some(2014)));
}

/// "2014 to 2025" should not quietly include papers with no year in the title.
#[test]
fn an_undated_paper_drops_out_once_a_year_bound_is_set() {
    let mut conn = db();
    add_paper(&conn, 1, "Some untitled practice", "past_paper", PAPER);
    index_resource(&mut conn, 1).unwrap();

    let unfiltered = Filters { include_solutions: true, ..Default::default() };
    assert!(!search(&conn, "enzyme", &unfiltered, 50).unwrap().is_empty());

    let dated = Filters { from_year: Some(2000), include_solutions: true, ..Default::default() };
    assert!(search(&conn, "enzyme", &dated, 50).unwrap().is_empty());
}

/// Searching a topic should return the questions on it, not the answers to
/// them — so solutions are out unless asked for.
#[test]
fn solutions_are_excluded_by_default() {
    let mut conn = db();
    two_years(&mut conn);

    let default = Filters::default();
    assert!(search(&conn, "enzyme", &default, 50)
        .unwrap()
        .iter()
        .all(|q| !q.paper.is_solutions));

    let with = Filters { include_solutions: true, ..Default::default() };
    assert!(search(&conn, "enzyme", &with, 50)
        .unwrap()
        .iter()
        .any(|q| q.paper.is_solutions));
}

#[test]
fn a_source_filter_keeps_only_that_publisher() {
    let mut conn = db();
    two_years(&mut conn);

    let neap = Filters {
        source: Some("neap".into()),
        include_solutions: true,
        ..Default::default()
    };
    let found = search(&conn, "enzyme", &neap, 50).unwrap();
    assert!(!found.is_empty());
    assert!(found.iter().all(|q| q.paper.source.as_deref() == Some("neap")));
}

/// Filtering happens after the SQL, so the limit has to be applied after it
/// too — otherwise a filtered search returns fewer rows than asked for while
/// matches sit just past the cut.
#[test]
fn the_limit_counts_what_survives_the_filter() {
    let mut conn = db();
    two_years(&mut conn);

    let one = Filters { include_solutions: true, ..Default::default() };
    assert_eq!(search(&conn, "enzyme", &one, 1).unwrap().len(), 1);
}
