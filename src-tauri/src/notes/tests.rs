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
    chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 8, 16, 9, 0, 0).unwrap()
}

/// Positions in order, which is the invariant everything else depends on.
fn order(conn: &Connection, note: i64) -> Vec<(i64, String)> {
    blocks(conn, note)
        .unwrap()
        .into_iter()
        .map(|b| (b.position, b.text))
        .collect()
}

fn add(conn: &mut Connection, note: i64, after: Option<i64>, text: &str) -> i64 {
    insert_block(conn, note, after, "paragraph", text, now()).unwrap()
}

// -- starting a note --------------------------------------------------------

/// An editor that needs a click before it accepts a keystroke is one you stop
/// reaching for, so a new note always has somewhere to type.
#[test]
fn a_new_note_opens_with_one_empty_paragraph() {
    let conn = db();
    let id = create(&conn, Some(1), "  ", None, now()).unwrap();

    let note = get(&conn, id).unwrap();
    assert_eq!(note.title, "Untitled", "a blank title becomes Untitled, not empty");
    assert_eq!(note.blocks.len(), 1);
    assert_eq!(note.blocks[0].kind, "paragraph");
    assert_eq!(note.blocks[0].position, 0);
}

#[test]
fn an_unknown_block_type_is_refused() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();

    assert!(insert_block(&mut conn, id, None, "table", "", now()).is_err());
    assert!(update_block(&conn, 1, "spreadsheet", "", false, None, now()).is_err());
}

// -- ordering ---------------------------------------------------------------

#[test]
fn blocks_come_back_in_the_order_they_were_written() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();
    let first = get(&conn, id).unwrap().blocks[0].id;

    let a = add(&mut conn, id, Some(first), "A");
    add(&mut conn, id, Some(a), "B");

    assert_eq!(
        order(&conn, id),
        vec![(0, "".into()), (1, "A".into()), (2, "B".into())]
    );
}

/// Inserting in the middle is where a naive implementation collides with the
/// unique index on (note_id, position).
#[test]
fn inserting_between_two_blocks_shifts_everything_below_it() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();
    let first = get(&conn, id).unwrap().blocks[0].id;

    let a = add(&mut conn, id, Some(first), "A");
    add(&mut conn, id, Some(a), "C");
    add(&mut conn, id, Some(a), "B"); // between A and C

    assert_eq!(
        order(&conn, id),
        vec![(0, "".into()), (1, "A".into()), (2, "B".into()), (3, "C".into())]
    );
}

#[test]
fn deleting_a_block_closes_the_gap_rather_than_leaving_a_hole() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();
    let first = get(&conn, id).unwrap().blocks[0].id;

    let a = add(&mut conn, id, Some(first), "A");
    let b = add(&mut conn, id, Some(a), "B");
    add(&mut conn, id, Some(b), "C");

    delete_block(&mut conn, b, now()).unwrap();

    assert_eq!(
        order(&conn, id),
        vec![(0, "".into()), (1, "A".into()), (2, "C".into())],
        "positions stay dense and contiguous"
    );
}

/// Backspacing through everything must not leave a document with nowhere to put
/// the cursor.
#[test]
fn the_last_block_is_emptied_rather_than_deleted() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();
    let only = get(&conn, id).unwrap().blocks[0].id;

    update_block(&conn, only, "h1", "Something", false, None, now()).unwrap();
    delete_block(&mut conn, only, now()).unwrap();

    let note = get(&conn, id).unwrap();
    assert_eq!(note.blocks.len(), 1, "a note is never left with no blocks");
    assert_eq!(note.blocks[0].kind, "paragraph", "and it resets to a plain one");
    assert_eq!(note.blocks[0].text, "");
}

#[test]
fn a_block_swaps_with_its_neighbour() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();
    let first = get(&conn, id).unwrap().blocks[0].id;

    let a = add(&mut conn, id, Some(first), "A");
    let b = add(&mut conn, id, Some(a), "B");

    move_block(&mut conn, b, -1, now()).unwrap();
    assert_eq!(
        order(&conn, id),
        vec![(0, "".into()), (1, "B".into()), (2, "A".into())]
    );

    move_block(&mut conn, b, 1, now()).unwrap();
    assert_eq!(
        order(&conn, id),
        vec![(0, "".into()), (1, "A".into()), (2, "B".into())]
    );
}

/// Holding the shortcut at the top of a note is not an error.
#[test]
fn moving_past_either_end_does_nothing_quietly() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();
    let first = get(&conn, id).unwrap().blocks[0].id;
    let a = add(&mut conn, id, Some(first), "A");

    move_block(&mut conn, first, -1, now()).unwrap();
    move_block(&mut conn, a, 1, now()).unwrap();

    assert_eq!(order(&conn, id), vec![(0, "".into()), (1, "A".into())]);
}

/// Fifty inserts in the same spot is what breaks a fractional index. Dense
/// renumbering has to survive it.
#[test]
fn repeatedly_inserting_in_one_place_keeps_the_order_exact() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();
    let first = get(&conn, id).unwrap().blocks[0].id;
    let last = add(&mut conn, id, Some(first), "LAST");

    for i in 0..50 {
        add(&mut conn, id, Some(first), &format!("{i}"));
    }

    let got = blocks(&conn, id).unwrap();
    assert_eq!(got.len(), 52);
    // Contiguous, no duplicates, no gaps.
    assert_eq!(
        got.iter().map(|b| b.position).collect::<Vec<_>>(),
        (0..52).collect::<Vec<_>>()
    );
    // Each insert went directly below the first block, so they read backwards.
    assert_eq!(got[1].text, "49");
    assert_eq!(got.last().unwrap().id, last);
}

// -- content ----------------------------------------------------------------

#[test]
fn a_checkbox_keeps_its_state() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();
    let b = add(&mut conn, id, None, "Read chapter 4");

    update_block(&conn, b, "todo", "Read chapter 4", true, None, now()).unwrap();

    let block = blocks(&conn, id).unwrap().into_iter().find(|x| x.id == b).unwrap();
    assert!(block.checked);
    assert_eq!(block.kind, "todo");
}

#[test]
fn deleting_a_note_takes_its_blocks_with_it() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();
    add(&mut conn, id, None, "A");

    delete(&conn, id).unwrap();

    let left: i64 = conn
        .query_row("SELECT COUNT(*) FROM note_blocks WHERE note_id = ?1", [id], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 0, "orphaned blocks would accumulate forever");
}

/// A list of rows all saying "Untitled" is unusable, and most notes never get a
/// title typed into them.
#[test]
fn the_list_previews_the_first_block_that_has_anything_in_it() {
    let mut conn = db();
    let id = create(&conn, Some(1), "", None, now()).unwrap();
    let first = get(&conn, id).unwrap().blocks[0].id;

    // Leading empties are skipped rather than previewed as blank.
    let h = add(&mut conn, id, Some(first), "");
    update_block(&conn, h, "h1", "Enzymes", false, None, now()).unwrap();

    let summary = &list(&conn, None, 10).unwrap()[0];
    assert_eq!(summary.preview, "Enzymes");
    assert_eq!(summary.subject_name.as_deref(), Some("Biology"));
    assert_eq!(summary.block_count, 2);
}

// -- markdown ---------------------------------------------------------------

#[test]
fn markdown_renders_every_block_type() {
    let mut conn = db();
    let id = create(&conn, None, "Enzymes", None, now()).unwrap();
    let first = get(&conn, id).unwrap().blocks[0].id;
    delete_block(&mut conn, first, now()).unwrap();

    let mut prev = get(&conn, id).unwrap().blocks[0].id;
    for (kind, text, checked) in [
        ("h1", "Structure", false),
        ("paragraph", "A protein catalyst.", false),
        ("bullet", "Lowers activation energy", false),
        ("todo", "Read chapter 4", true),
        ("todo", "Do the worksheet", false),
        ("quote", "Not in your material.", false),
        ("divider", "", false),
    ] {
        prev = insert_block(&mut conn, id, Some(prev), kind, text, now()).unwrap();
        if checked {
            update_block(&conn, prev, kind, text, true, None, now()).unwrap();
        }
    }

    let md = to_markdown(&get(&conn, id).unwrap());

    assert!(md.starts_with("# Enzymes\n"));
    assert!(md.contains("## Structure"));
    assert!(md.contains("- Lowers activation energy"));
    assert!(md.contains("- [x] Read chapter 4"), "{md}");
    assert!(md.contains("- [ ] Do the worksheet"));
    assert!(md.contains("> Not in your material."));
    assert!(md.contains("---"));
}

/// Numbering is derived at render time, not stored. A run that's broken by
/// another block has to start again rather than continuing from before it.
#[test]
fn numbered_lists_restart_after_something_interrupts_them() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();
    let mut prev = get(&conn, id).unwrap().blocks[0].id;

    for (kind, text) in [
        ("numbered", "First"),
        ("numbered", "Second"),
        ("paragraph", "An aside."),
        ("numbered", "Restarted"),
    ] {
        prev = insert_block(&mut conn, id, Some(prev), kind, text, now()).unwrap();
    }

    let md = to_markdown(&get(&conn, id).unwrap());
    assert!(md.contains("1. First"));
    assert!(md.contains("2. Second"));
    assert!(md.contains("1. Restarted"), "the run restarts:\n{md}");
}

/// An exported note has to stand alone — a relative path breaks the moment the
/// file moves.
#[test]
fn an_image_block_exports_with_its_data_inline() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();
    let b = add(&mut conn, id, None, "The board");

    update_block(&conn, b, "image", "The board", false, Some("data:image/png;base64,iVBOR"), now())
        .unwrap();

    let md = to_markdown(&get(&conn, id).unwrap());
    assert!(md.contains("![The board](data:image/png;base64,iVBOR)"), "{md}");
}

#[test]
fn an_image_block_that_lost_its_data_says_so_rather_than_rendering_broken() {
    let mut conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();
    let b = add(&mut conn, id, None, "Diagram");
    update_block(&conn, b, "image", "Diagram", false, None, now()).unwrap();

    assert!(to_markdown(&get(&conn, id).unwrap()).contains("image missing"));
}

// -- stickies ---------------------------------------------------------------

/// Closing a sticky must keep where it was. Reopening one you closed yesterday
/// should put it back, not in the middle of the screen.
#[test]
fn closing_a_sticky_keeps_its_position() {
    let conn = db();
    let id = create(&conn, None, "Homework", None, now()).unwrap();

    set_sticky_open(&conn, id, true).unwrap();
    set_sticky_geometry(&conn, id, 1200.0, 340.0, 300.0, 260.0).unwrap();
    set_sticky_open(&conn, id, false).unwrap();

    let s = sticky(&conn, id).unwrap();
    assert_eq!(s.x, Some(1200.0));
    assert_eq!(s.y, Some(340.0));
    assert!(open_stickies(&conn).unwrap().is_empty(), "closed, but not forgotten");

    set_sticky_open(&conn, id, true).unwrap();
    assert_eq!(open_stickies(&conn).unwrap()[0].x, Some(1200.0));
}

#[test]
fn a_new_note_is_not_a_sticky_until_it_is_put_on_the_desktop() {
    let conn = db();
    create(&conn, None, "Just a note", None, now()).unwrap();

    assert!(open_stickies(&conn).unwrap().is_empty());
}

#[test]
fn only_the_paper_colours_are_accepted() {
    let conn = db();
    let id = create(&conn, None, "N", None, now()).unwrap();

    assert_eq!(sticky(&conn, id).unwrap().colour, "amber", "a default, not empty");

    set_sticky_colour(&conn, id, "mint").unwrap();
    assert_eq!(sticky(&conn, id).unwrap().colour, "mint");

    // An arbitrary string would reach a CSS class name.
    assert!(set_sticky_colour(&conn, id, "#ff0000").is_err());
    assert!(set_sticky_colour(&conn, id, "puce").is_err());
    assert_eq!(sticky(&conn, id).unwrap().colour, "mint", "unchanged after a refusal");
}

/// A sticky is a note, so it keeps working like one — it has blocks, it is in
/// the notes list, and it exports as Markdown.
#[test]
fn a_sticky_is_still_an_ordinary_note() {
    let conn = db();
    let id = create(&conn, Some(1), "Ask Mr B", None, now()).unwrap();
    set_sticky_open(&conn, id, true).unwrap();

    let first = get(&conn, id).unwrap().blocks[0].id;
    update_block(&conn, first, "todo", "Ask about Q4b", false, None, now()).unwrap();

    assert_eq!(list(&conn, None, 10).unwrap().len(), 1, "still in the notes list");
    assert!(to_markdown(&get(&conn, id).unwrap()).contains("- [ ] Ask about Q4b"));
}

#[test]
fn deleting_a_sticky_takes_it_off_the_desktop() {
    let conn = db();
    let id = create(&conn, None, "Temp", None, now()).unwrap();
    set_sticky_open(&conn, id, true).unwrap();

    delete(&conn, id).unwrap();
    assert!(open_stickies(&conn).unwrap().is_empty());
}
