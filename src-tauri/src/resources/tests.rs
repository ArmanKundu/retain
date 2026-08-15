use super::*;

fn db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::db::run_migrations(&conn).unwrap();
    conn.execute(
        "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
         VALUES (1,'Biology','#4BA97B','3_4','science',0,'2026-08-01T00:00:00Z'),
                (2,'Chemistry','#5B8DEF','1_2','science',1,'2026-08-01T00:00:00Z')",
        [],
    )
    .unwrap();
    let _ = &mut conn;
    conn
}

fn now() -> DateTime<Utc> {
    chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 8, 14, 9, 0, 0).unwrap()
}

// -- text preparation -------------------------------------------------------

#[test]
fn normalise_collapses_pdf_noise_but_keeps_paragraph_breaks() {
    let raw = "Line   one  \r\n\r\n\r\n\r\n   Line two\t\tmore\n\n\nLine three\n";
    let out = normalise(raw);
    assert_eq!(out, "Line one\n\nLine two more\n\nLine three");
}

#[test]
fn normalise_of_nothing_is_empty() {
    assert_eq!(normalise("   \n\n\t  \n"), "");
}

#[test]
fn a_short_document_is_one_chunk() {
    let c = chunk("A short note about ribosomes.");
    assert_eq!(c.len(), 1);
}

#[test]
fn a_long_document_is_split_with_overlap_and_loses_nothing() {
    // Distinct sentences so we can check none vanish at a boundary.
    let doc: String = (0..400)
        .map(|i| format!("Sentence number {i} about cellular respiration. "))
        .collect();

    let chunks = chunk(&normalise(&doc));
    assert!(chunks.len() > 1, "expected several chunks");

    let joined = chunks.join(" ");
    for i in [0, 137, 399] {
        assert!(
            joined.contains(&format!("Sentence number {i} ")),
            "sentence {i} was lost at a chunk boundary"
        );
    }
}

#[test]
fn chunking_always_terminates_on_awkward_input() {
    // No sentence or paragraph breaks at all — the boundary search finds
    // nothing and must fall back rather than loop.
    let doc = "x".repeat(20_000);
    let chunks = chunk(&doc);
    assert!(chunks.len() > 1);
    assert!(chunks.len() < 100, "produced {} chunks", chunks.len());
}

// -- storage ----------------------------------------------------------------

#[test]
fn a_resource_is_stored_and_chunked() {
    let mut conn = db();
    let text: String = (0..300).map(|i| format!("Key knowledge point {i}. ")).collect();

    let id = add(&mut conn, Some(1), "Study design", ResourceKind::StudyDesign,
                 Some("design.pdf"), &text, now()).unwrap();

    let listed = list(&conn, None).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert_eq!(listed[0].kind, ResourceKind::StudyDesign);
    assert_eq!(listed[0].subject_name.as_deref(), Some("Biology"));
    assert!(listed[0].chunk_count > 1);
    assert!(listed[0].word_count > 100);
}

#[test]
fn an_empty_or_image_only_file_is_refused_with_a_useful_message() {
    let mut conn = db();
    let err = add(&mut conn, None, "Scan", ResourceKind::PastPaper, None, "   \n\n ", now())
        .unwrap_err()
        .to_string();
    assert!(err.contains("scanned"), "unhelpful message: {err}");
}

#[test]
fn resources_can_be_filtered_by_subject() {
    let mut conn = db();
    add(&mut conn, Some(1), "Bio", ResourceKind::SchoolNotes, None, "mitochondria", now()).unwrap();
    add(&mut conn, Some(2), "Chem", ResourceKind::SchoolNotes, None, "titration", now()).unwrap();

    assert_eq!(list(&conn, Some(1)).unwrap().len(), 1);
    assert_eq!(list(&conn, Some(2)).unwrap().len(), 1);
    assert_eq!(list(&conn, None).unwrap().len(), 2);
}

// -- retrieval --------------------------------------------------------------

#[test]
fn a_question_finds_the_relevant_excerpt() {
    let mut conn = db();
    add(&mut conn, Some(1), "Notes", ResourceKind::SchoolNotes, None,
        "Photosynthesis occurs in the chloroplast.\n\nRespiration occurs in the mitochondria.\n\n\
         Protein synthesis begins with transcription in the nucleus.",
        now()).unwrap();

    let hits = search(&conn, "where does protein synthesis begin?", None, 5).unwrap();
    assert!(!hits.is_empty(), "expected a match");
    assert!(hits[0].content.contains("Protein synthesis"));
}

/// A question full of punctuation must search, not error. FTS5 treats several
/// characters as operators, which is why the query is built term by term.
#[test]
fn punctuation_in_a_question_does_not_break_the_query() {
    let mut conn = db();
    add(&mut conn, Some(1), "Notes", ResourceKind::SchoolNotes, None,
        "The cell's membrane is semi-permeable.", now()).unwrap();

    for q in [
        "what is the cell's membrane?",
        "semi-permeable \"membrane\"",
        "membrane: how does it work * ^",
        "NOT AND OR",
    ] {
        assert!(search(&conn, q, None, 5).is_ok(), "query failed: {q}");
    }
}

#[test]
fn a_question_of_only_stopwords_retrieves_nothing_rather_than_everything() {
    let mut conn = db();
    add(&mut conn, Some(1), "Notes", ResourceKind::SchoolNotes, None, "Anything at all.", now()).unwrap();

    assert!(to_match_query("what is it and how do you do that").is_none());
    assert!(search(&conn, "what is it", None, 5).unwrap().is_empty());
}

#[test]
fn retrieval_can_be_scoped_to_one_subject() {
    let mut conn = db();
    add(&mut conn, Some(1), "Bio", ResourceKind::SchoolNotes, None, "enzyme catalysis in cells", now()).unwrap();
    add(&mut conn, Some(2), "Chem", ResourceKind::SchoolNotes, None, "enzyme catalysis in industry", now()).unwrap();

    assert_eq!(search(&conn, "enzyme catalysis", None, 10).unwrap().len(), 2);
    let bio = search(&conn, "enzyme catalysis", Some(1), 10).unwrap();
    assert_eq!(bio.len(), 1);
    assert_eq!(bio[0].resource_title, "Bio");
}

/// The bug this guards: FTS5 external-content tables don't clean themselves up.
/// Without the delete trigger, material you removed keeps being fed to the AI.
#[test]
fn deleting_a_resource_removes_it_from_search() {
    let mut conn = db();
    let id = add(&mut conn, Some(1), "Old paper", ResourceKind::PastPaper, None,
                 "Describe the process of osmoregulation in detail.", now()).unwrap();

    assert!(!search(&conn, "osmoregulation", None, 5).unwrap().is_empty());

    delete(&conn, id).unwrap();

    assert!(list(&conn, None).unwrap().is_empty());
    assert!(
        search(&conn, "osmoregulation", None, 5).unwrap().is_empty(),
        "deleted material is still searchable — the FTS delete trigger isn't firing"
    );
    let orphans: i64 = conn
        .query_row("SELECT COUNT(*) FROM resource_chunks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(orphans, 0, "chunks outlived their resource");
}

// -- prompt context ---------------------------------------------------------

#[test]
fn no_excerpts_means_no_context_block() {
    assert!(context_block(&[]).is_none());
}

#[test]
fn the_context_block_labels_each_source_by_kind() {
    let excerpts = vec![
        Excerpt { resource_id: 1, resource_title: "VCAA study design".into(),
                  kind: ResourceKind::StudyDesign, ordinal: 0, content: "dot point text".into() },
        Excerpt { resource_id: 2, resource_title: "2023 exam".into(),
                  kind: ResourceKind::PastPaper, ordinal: 4, content: "question text".into() },
    ];

    let block = context_block(&excerpts).unwrap();
    // The label now states what authority the source carries, not just where it
    // came from — that distinction is the point of the taxonomy.
    assert!(block.contains("authoritative on what is examinable"));
    assert!(block.contains("VCAA study design"));
    assert!(block.contains("From a past exam paper"));
    assert!(block.contains("2023 exam"));
    assert!(block.contains("dot point text"));
    // The model is told the material outranks its own memory.
    assert!(block.contains("authoritative"));
}
