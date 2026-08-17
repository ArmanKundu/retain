use super::*;

/// Apply migrations 1..=`through`, leaving the schema at that version.
///
/// Lets a test seed data at the schema an old install actually had, which is the
/// only way to exercise what a later migration does to real rows.
fn apply_through(conn: &Connection, through: i32) {
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= through) {
        conn.execute_batch(sql).unwrap();
        conn.execute_batch(&format!("PRAGMA user_version = {version}")).unwrap();
    }
}

/// The bug this exists to prevent, reproduced exactly.
///
/// Migration 005 rebuilds `resources`. With `foreign_keys = ON` — how the app
/// opens the database, and *not* how the `sqlite3` CLI opens it — SQLite runs an
/// implicit `DELETE FROM` before `DROP TABLE`, which fires the cascade on
/// `resource_chunks` and deletes every chunk of every resource. The FTS
/// `'rebuild'` that followed then rebuilt the index from an empty table and
/// reported success, so the library still listed the files and searching them
/// found nothing.
///
/// Verifying that migration against a copy opened with the CLI was what hid it:
/// the cascade never fires with foreign keys off. So this test seeds real data
/// at the pre-005 schema and migrates with enforcement on, which is the only
/// arrangement that reproduces what the app actually does.
#[test]
fn a_table_rebuild_does_not_cascade_away_the_search_index() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

    // Stop at 4 — the schema as it was before the rebuild.
    apply_through(&conn, 4);

    conn.execute(
        "INSERT INTO resources (id, title, kind, content, word_count, added_at)
         VALUES (1, 'Biology study design', 'notes', 'enzymes catalyse reactions', 4,
                 '2026-08-15T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO resource_chunks (resource_id, ordinal, content)
         VALUES (1, 0, 'enzymes catalyse reactions')",
        [],
    )
    .unwrap();

    run_migrations(&conn).unwrap();

    let chunks: i64 = conn
        .query_row("SELECT COUNT(*) FROM resource_chunks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(chunks, 1, "the rebuild deleted the chunks through the cascade");

    let hits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM resource_chunks_fts WHERE resource_chunks_fts MATCH 'enzymes'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1, "material survived but stopped being searchable");

    let _ = &mut conn;
}

/// The repair for databases that already lost their chunks. Idempotent, because
/// it runs on every launch.
#[test]
fn reindexing_restores_chunks_from_the_text_that_survived() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_migrations(&conn).unwrap();

    conn.execute(
        "INSERT INTO resources (id, title, kind, content, word_count, added_at)
         VALUES (1, 'Chemistry study design', 'study_design', ?1, 3, '2026-08-15T00:00:00Z')",
        [&"Redox reactions transfer electrons. ".repeat(200)],
    )
    .unwrap();

    let repaired = crate::resources::reindex_missing(&mut conn).unwrap();
    assert_eq!(repaired, 1);

    let chunks: i64 = conn
        .query_row("SELECT COUNT(*) FROM resource_chunks", [], |r| r.get(0))
        .unwrap();
    assert!(chunks > 0, "content should have produced chunks");

    let hits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM resource_chunks_fts WHERE resource_chunks_fts MATCH 'redox'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(hits > 0, "restored chunks must be searchable, not just present");

    // A second pass must not duplicate anything.
    assert_eq!(crate::resources::reindex_missing(&mut conn).unwrap(), 0);
    let after: i64 = conn
        .query_row("SELECT COUNT(*) FROM resource_chunks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after, chunks);
}

/// Runs the repair against a copy of a real database when one is present.
///
/// Ignored by default: it needs `RETAIN_TEST_DB` to point at a *copy*. Never
/// point it at the live file — this writes.
#[test]
#[ignore]
fn reindex_repairs_a_real_database_copy() {
    let Ok(path) = std::env::var("RETAIN_TEST_DB") else {
        return;
    };
    let mut conn = Connection::open(path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_migrations(&conn).unwrap();

    let repaired = crate::resources::reindex_missing(&mut conn).unwrap();
    let chunks: i64 = conn
        .query_row("SELECT COUNT(*) FROM resource_chunks", [], |r| r.get(0))
        .unwrap();
    let hits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM resource_chunks_fts WHERE resource_chunks_fts MATCH 'enzyme'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    println!("repaired {repaired} resource(s) -> {chunks} chunks, {hits} hits for \"enzyme\"");
    assert!(chunks > 0);
}

/// Cut real papers into questions and report what came out.
///
/// Ignored by default: needs `RETAIN_TEST_DB` pointing at a *copy*. Segmenting
/// a thousand real papers is the only way to find out whether the marker
/// actually holds across twenty years of different publishers.
#[test]
#[ignore]
fn segmenting_real_papers_produces_sane_questions() {
    let Ok(path) = std::env::var("RETAIN_TEST_DB") else {
        return;
    };
    let mut conn = Connection::open(path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    run_migrations(&conn).unwrap();

    let pending = crate::questions::unindexed(&conn).unwrap();
    println!("exam resources to index: {}", pending.len());

    let mut failed = 0;
    for id in pending.iter().take(400) {
        if crate::questions::index_resource(&mut conn, *id).is_err() {
            failed += 1;
        }
    }

    let (total, papers): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COUNT(DISTINCT resource_id) FROM questions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let median_words: i64 = conn
        .query_row(
            "SELECT words FROM questions ORDER BY words LIMIT 1 OFFSET (SELECT COUNT(*)/2 FROM questions)",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    println!("{total} questions from {papers} papers, {failed} errored, median {median_words} words");

    let hits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM questions_fts WHERE questions_fts MATCH 'enzyme'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    println!("questions matching \"enzyme\": {hits}");

    assert!(total > 0, "no questions came out of real papers");
}
