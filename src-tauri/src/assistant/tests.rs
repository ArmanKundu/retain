use super::*;
use crate::resources::ResourceKind;

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
    chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 8, 14, 9, 0, 0).unwrap()
}

fn excerpt(title: &str, content: &str) -> Excerpt {
    Excerpt {
        resource_id: 1,
        resource_title: title.into(),
        kind: ResourceKind::StudyDesign,
        ordinal: 0,
        content: content.into(),
    }
}

// -- the grounding contract -------------------------------------------------

/// The rule the whole feature rests on: strict mode must forbid filling gaps
/// from the model's own knowledge, in so many words.
#[test]
fn strict_mode_forbids_answering_from_memory() {
    let p = SYSTEM_STRICT.to_lowercase();
    assert!(p.contains("only from the material"));
    assert!(p.contains("never fill a gap"));
    assert!(p.contains("doesn't cover this"));
}

#[test]
fn open_mode_requires_labelling_what_is_outside_your_material() {
    let p = SYSTEM_OPEN.to_lowercase();
    assert!(p.contains("say so explicitly"));
    assert!(p.contains("isn't in your notes"));
}

/// When retrieval finds nothing in strict mode, the prompt must SAY nothing was
/// found. Silence would read as "no material exists" and invite the model to
/// answer from memory — the exact failure strict mode exists to prevent.
#[test]
fn strict_mode_states_when_nothing_was_retrieved() {
    let prompt = build_prompt(&[], &[], "", &[], "What is osmoregulation?", Grounding::Strict);
    assert!(
        prompt.contains("Nothing in the student's uploaded material matched"),
        "strict prompt must state the absence:\n{prompt}"
    );
}

#[test]
fn open_mode_stays_quiet_when_nothing_was_retrieved() {
    let prompt = build_prompt(&[], &[], "", &[], "What is osmoregulation?", Grounding::Open);
    assert!(!prompt.contains("Nothing in the student's uploaded material matched"));
    assert!(prompt.contains("What is osmoregulation?"));
}

#[test]
fn retrieved_material_is_marked_authoritative() {
    let prompt = build_prompt(
        &[excerpt("VCAA study design", "the role of enzymes")],
        &[],
        "",
        &[],
        "Explain enzymes",
        Grounding::Strict,
    );
    assert!(prompt.contains("authoritative"));
    assert!(prompt.contains("the role of enzymes"));
}

/// The question must be last. Models weight the end of a prompt most heavily,
/// and a question buried above six pages of context gets lost.
#[test]
fn the_question_comes_last() {
    let prompt = build_prompt(
        &[excerpt("Notes", "a".repeat(400).as_str())],
        &[NewAttachment { name: "sheet.txt".into(), content: "b".repeat(400) }],
        "--- The student's Retain data ---\n12 reviews due.\n",
        &[],
        "MY ACTUAL QUESTION",
        Grounding::Strict,
    );

    let q = prompt.find("MY ACTUAL QUESTION").unwrap();
    assert!(q > prompt.find("authoritative").unwrap());
    assert!(q > prompt.find("sheet.txt").unwrap());
    assert!(q > prompt.find("12 reviews due").unwrap());
}

#[test]
fn attachments_are_scoped_to_the_message_that_carried_them() {
    let prompt = build_prompt(
        &[],
        &[NewAttachment { name: "worksheet.txt".into(), content: "Question 4b asks…".into() }],
        "",
        &[],
        "Help with 4b",
        Grounding::Open,
    );
    assert!(prompt.contains("Attached to this message: worksheet.txt"));
    assert!(prompt.contains("Question 4b asks…"));
}

/// Only recent turns are replayed — every one costs tokens.
#[test]
fn only_recent_history_is_replayed() {
    let history: Vec<Message> = (0..30)
        .map(|i| Message {
            id: i,
            role: if i % 2 == 0 { "user".into() } else { "assistant".into() },
            body: format!("turn number {i}"),
            sources: vec![],
            model: None,
            attachments: vec![],
            created_at: "2026-08-14T09:00:00Z".into(),
        })
        .collect();

    let prompt = build_prompt(&[], &[], "", &history, "next", Grounding::Open);

    assert!(prompt.contains("turn number 29"), "the most recent turn must survive");
    assert!(!prompt.contains("turn number 0"), "ancient history should be dropped");
}

// -- conversations ----------------------------------------------------------

#[test]
fn a_conversation_takes_its_title_from_the_first_question() {
    let mut conn = db();
    let id = create(&conn, Some(1), Grounding::Strict, now()).unwrap();
    assert_eq!(list(&conn, 10).unwrap()[0].title, "New conversation");

    add_user_message(&mut conn, id, "How does the sodium potassium pump work?", &[], now()).unwrap();
    assert_eq!(
        list(&conn, 10).unwrap()[0].title,
        "How does the sodium potassium pump work?"
    );

    // A second question does not rename it.
    add_user_message(&mut conn, id, "And what about calcium?", &[], now()).unwrap();
    assert_eq!(
        list(&conn, 10).unwrap()[0].title,
        "How does the sodium potassium pump work?"
    );
}

#[test]
fn a_very_long_first_question_is_truncated_into_a_title() {
    let mut conn = db();
    let id = create(&conn, None, Grounding::Strict, now()).unwrap();
    add_user_message(&mut conn, id, &"explain ".repeat(40), &[], now()).unwrap();

    let t = &list(&conn, 10).unwrap()[0].title;
    assert!(t.chars().count() <= 61, "{} chars", t.chars().count());
    assert!(t.ends_with('…'));
}

#[test]
fn a_turn_round_trips_with_its_sources_and_attachments() {
    let mut conn = db();
    let id = create(&conn, Some(1), Grounding::Strict, now()).unwrap();

    add_user_message(
        &mut conn,
        id,
        "Explain this question",
        &[NewAttachment { name: "q.txt".into(), content: "some question text".into() }],
        now(),
    )
    .unwrap();

    add_assistant_message(
        &conn,
        id,
        "It's asking about osmosis.",
        &[excerpt("VCAA study design", "osmosis dot point")],
        Some("gemini-flash-latest"),
        now(),
    )
    .unwrap();

    let msgs = messages(&conn, id).unwrap();
    assert_eq!(msgs.len(), 2);

    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[0].attachments.len(), 1);
    assert_eq!(msgs[0].attachments[0].name, "q.txt");
    assert_eq!(msgs[0].attachments[0].words, 3);

    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[1].sources.len(), 1);
    assert_eq!(msgs[1].sources[0].resource_title, "VCAA study design");
    assert_eq!(msgs[1].model.as_deref(), Some("gemini-flash-latest"));
}

/// Citations are stored, not recomputed. Retrieval depends on what was in the
/// library at the time; re-running it later would show sources the answer never
/// actually used.
#[test]
fn citations_survive_the_material_being_deleted() {
    let conn = db();
    let id = create(&conn, None, Grounding::Strict, now()).unwrap();
    add_assistant_message(&conn, id, "answer", &[excerpt("Gone soon", "text")], None, now())
        .unwrap();

    conn.execute("DELETE FROM resources", []).unwrap();

    let msgs = messages(&conn, id).unwrap();
    assert_eq!(msgs[0].sources.len(), 1);
    assert_eq!(msgs[0].sources[0].resource_title, "Gone soon");
}

#[test]
fn a_corrupt_citation_blob_costs_the_citations_not_the_message() {
    let conn = db();
    let id = create(&conn, None, Grounding::Strict, now()).unwrap();
    conn.execute(
        "INSERT INTO messages (conversation_id, role, body, sources, created_at)
         VALUES (?1, 'assistant', 'the answer', '{not json', '2026-08-14T09:00:00Z')",
        [id],
    )
    .unwrap();

    let msgs = messages(&conn, id).unwrap();
    assert_eq!(msgs[0].body, "the answer");
    assert!(msgs[0].sources.is_empty());
}

#[test]
fn deleting_a_conversation_takes_its_messages_and_attachments() {
    let mut conn = db();
    let id = create(&conn, None, Grounding::Strict, now()).unwrap();
    add_user_message(
        &mut conn,
        id,
        "q",
        &[NewAttachment { name: "a.txt".into(), content: "x".into() }],
        now(),
    )
    .unwrap();

    delete(&conn, id).unwrap();

    for table in ["conversations", "messages", "message_attachments"] {
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{table} still has rows");
    }
}

#[test]
fn grounding_can_be_changed_per_conversation() {
    let conn = db();
    let id = create(&conn, None, Grounding::Strict, now()).unwrap();
    assert_eq!(list(&conn, 10).unwrap()[0].grounding, Grounding::Strict);

    set_grounding(&conn, id, Grounding::Open).unwrap();
    assert_eq!(list(&conn, 10).unwrap()[0].grounding, Grounding::Open);
}

/// Strict is the default, and must stay that way — it's the safety property.
#[test]
fn strict_is_the_schema_default() {
    let conn = db();
    conn.execute(
        "INSERT INTO conversations (title, created_at, updated_at)
         VALUES ('x', '2026-08-14T09:00:00Z', '2026-08-14T09:00:00Z')",
        [],
    )
    .unwrap();
    let g: String = conn
        .query_row("SELECT grounding FROM conversations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(g, "strict");
}

// -- export -----------------------------------------------------------------

#[test]
fn a_conversation_exports_as_readable_markdown() {
    let mut conn = db();
    let id = create(&conn, Some(1), Grounding::Strict, now()).unwrap();
    add_user_message(&mut conn, id, "What is a codon?", &[], now()).unwrap();
    add_assistant_message(
        &conn,
        id,
        "Three bases coding for one amino acid.",
        &[excerpt("VCAA study design", "codon")],
        Some("claude-opus-5"),
        now(),
    )
    .unwrap();

    let convo = list(&conn, 1).unwrap().remove(0);
    let md = to_markdown(&convo, &messages(&conn, id).unwrap()).unwrap();

    assert!(md.starts_with("# What is a codon?"));
    assert!(md.contains("**Subject:** Biology"));
    assert!(md.contains("only my own material"));
    assert!(md.contains("## Question"));
    assert!(md.contains("## Answer"));
    assert!(md.contains("Three bases coding for one amino acid."));
    assert!(md.contains("*Sources: VCAA study design*"));
}

#[test]
fn exporting_an_empty_conversation_is_refused() {
    let conn = db();
    let id = create(&conn, None, Grounding::Strict, now()).unwrap();
    let convo = list(&conn, 1).unwrap().remove(0);
    assert!(to_markdown(&convo, &messages(&conn, id).unwrap()).is_err());
}

// -- app context ------------------------------------------------------------

/// The assistant's picture of your data is computed from rows, never guessed.
///
/// With no subjects at all there is genuinely nothing to say, and the block is
/// omitted rather than sent as an empty heading.
#[test]
fn app_context_is_empty_when_there_is_nothing_to_say() {
    let conn = db();
    conn.execute("DELETE FROM subjects", []).unwrap();
    assert_eq!(app_context(&conn), "");
}

/// A subject with no time logged this week is worth telling the assistant
/// about — that's how "what have I been avoiding?" becomes answerable.
#[test]
fn app_context_names_a_subject_with_no_time_this_week() {
    let conn = db();
    let ctx = app_context(&conn);
    assert!(ctx.contains("Not studied this week: Biology"), "{ctx}");
}

#[test]
fn app_context_reports_real_numbers() {
    let conn = db();
    conn.execute(
        "INSERT INTO sessions (subject_id,mode,started_at,ended_at,local_date,
                               elapsed_seconds,active_seconds)
         VALUES (1,'stopwatch','2026-08-13T09:00:00Z','2026-08-13T10:00:00Z',?1,3600,3600)",
        [crate::util::retain_today()],
    )
    .unwrap();

    let ctx = app_context(&conn);
    assert!(ctx.contains("Retain data"), "{ctx}");
    assert!(ctx.contains("1h 0m"), "{ctx}");
}
