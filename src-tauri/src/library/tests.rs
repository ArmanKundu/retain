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
    chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 8, 14, 9, 0, 0).unwrap()
}

#[test]
fn a_generation_is_saved_and_listed() {
    let conn = db();
    let id = save(&conn, Some(1), ItemKind::Notes, "Photosynthesis",
                  Some("make notes on photosynthesis"), "Light-dependent reactions…",
                  Some("gemini-flash-latest"), now()).unwrap();

    let items = list(&conn, &Filter::default(), 50).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, id);
    assert_eq!(items[0].title, "Photosynthesis");
    assert_eq!(items[0].subject_name.as_deref(), Some("Biology"));
    assert_eq!(items[0].model.as_deref(), Some("gemini-flash-latest"));
    assert!(!items[0].pinned);
}

/// An untitled item takes its title from its first real line — forty items all
/// called "Notes" would be unfindable.
#[test]
fn an_untitled_item_gets_a_title_from_its_content() {
    let conn = db();
    save(&conn, None, ItemKind::Notes, "  ", None,
         "## The role of enzymes\n\nEnzymes lower activation energy.", None, now()).unwrap();

    assert_eq!(list(&conn, &Filter::default(), 10).unwrap()[0].title, "The role of enzymes");
}

#[test]
fn an_empty_body_still_produces_a_usable_title() {
    let conn = db();
    save(&conn, None, ItemKind::WeeklyReview, "", None, "", None, now()).unwrap();
    assert_eq!(list(&conn, &Filter::default(), 10).unwrap()[0].title, "Weekly review");
}

#[test]
fn a_very_long_first_line_is_truncated() {
    let conn = db();
    save(&conn, None, ItemKind::Notes, "", None, &"word ".repeat(80), None, now()).unwrap();
    let t = &list(&conn, &Filter::default(), 10).unwrap()[0].title;
    assert!(t.chars().count() <= 81, "title was {} chars", t.chars().count());
    assert!(t.ends_with('…'));
}

#[test]
fn items_can_be_filtered_and_searched() {
    let conn = db();
    save(&conn, Some(1), ItemKind::Notes, "Mitosis", None, "phases of mitosis", None, now()).unwrap();
    save(&conn, None, ItemKind::PracticeQuestion, "Osmosis Q", None, "explain osmosis", None, now()).unwrap();

    let notes = list(&conn, &Filter { kind: Some("notes".into()), ..Default::default() }, 50).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title, "Mitosis");

    let by_subject = list(&conn, &Filter { subject_id: Some(1), ..Default::default() }, 50).unwrap();
    assert_eq!(by_subject.len(), 1);

    // Search covers the body, not just the title.
    let found = list(&conn, &Filter { search: Some("osmosis".into()), ..Default::default() }, 50).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].title, "Osmosis Q");

    // A blank search is not a filter.
    assert_eq!(list(&conn, &Filter { search: Some("   ".into()), ..Default::default() }, 50).unwrap().len(), 2);
}

#[test]
fn pinned_items_sort_first_and_can_be_filtered() {
    let conn = db();
    save(&conn, None, ItemKind::Notes, "Older", None, "a", None, now()).unwrap();
    let id = save(&conn, None, ItemKind::Notes, "Pinned", None, "b", None, now()).unwrap();

    set_pinned(&conn, id, true).unwrap();

    assert_eq!(list(&conn, &Filter::default(), 50).unwrap()[0].title, "Pinned");
    let only = list(&conn, &Filter { only_pinned: Some(true), ..Default::default() }, 50).unwrap();
    assert_eq!(only.len(), 1);

    set_pinned(&conn, id, false).unwrap();
    assert_eq!(list(&conn, &Filter { only_pinned: Some(true), ..Default::default() }, 50).unwrap().len(), 0);
}

#[test]
fn items_can_be_renamed_and_deleted() {
    let conn = db();
    let id = save(&conn, None, ItemKind::Notes, "Before", None, "x", None, now()).unwrap();

    rename(&conn, id, "After").unwrap();
    assert_eq!(list(&conn, &Filter::default(), 10).unwrap()[0].title, "After");

    // A blank rename is ignored rather than blanking the title.
    rename(&conn, id, "   ").unwrap();
    assert_eq!(list(&conn, &Filter::default(), 10).unwrap()[0].title, "After");

    delete(&conn, id).unwrap();
    assert!(list(&conn, &Filter::default(), 10).unwrap().is_empty());
}

/// Deleting a subject must not delete the notes you made about it.
#[test]
fn removing_a_subject_keeps_the_notes() {
    let conn = db();
    save(&conn, Some(1), ItemKind::Notes, "Bio notes", None, "body", None, now()).unwrap();

    conn.execute("DELETE FROM subjects WHERE id = 1", []).unwrap();

    let items = list(&conn, &Filter::default(), 10).unwrap();
    assert_eq!(items.len(), 1, "the note was deleted with its subject");
    assert_eq!(items[0].subject_id, None);
}

#[test]
fn markdown_export_carries_the_provenance() {
    let conn = db();
    save(&conn, Some(1), ItemKind::Notes, "Enzymes", Some("notes on enzymes"),
         "Enzymes are catalysts.", Some("claude-opus-5"), now()).unwrap();

    let md = to_markdown(&list(&conn, &Filter::default(), 1).unwrap()[0]);

    assert!(md.starts_with("# Enzymes"));
    assert!(md.contains("**Subject:** Biology"));
    assert!(md.contains("**Created:** 2026-08-14"));
    assert!(md.contains("**Generated by:** claude-opus-5"));
    assert!(md.contains("> **Asked:** notes on enzymes"));
    assert!(md.contains("Enzymes are catalysts."));
}
