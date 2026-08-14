//! Database setup: where the file lives, how it's configured, how it migrates,
//! and how we take recoverable snapshots of it.
//!
//! A short orientation, since this is the first Rust file in the project:
//!
//! * `pub fn` means the function is visible outside this module (this file).
//! * `-> Result<T>` means the function either succeeds with a `T` or fails with
//!   an error. The `?` operator after a call means "if that failed, stop here and
//!   return the error to my caller" — it is Rust's version of rethrowing.
//! * `&Path` is a borrowed reference to a filesystem path: we can read it but we
//!   don't own it and won't free it. `PathBuf` is the owned version.
//! * `Connection` is one open handle to the SQLite database.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// Migrations, newest last. The number is the schema version we end up at after
/// the file has been applied.
///
/// `include_str!` reads the file at COMPILE time and bakes its contents into the
/// binary as a string. That means the shipped app has no external .sql files to
/// lose, and a typo in the filename is a build error rather than a crash on a
/// user's machine.
const MIGRATIONS: &[(i32, &str)] = &[
    (1, include_str!("db/migrations/001_init.sql")),
    (2, include_str!("db/migrations/002_capture_cards_errors.sql")),
    (3, include_str!("db/migrations/003_library_resources.sql")),
    (4, include_str!("db/migrations/004_assistant.sql")),
];

/// How many automatic snapshots to keep before deleting the oldest.
const SNAPSHOTS_TO_KEEP: usize = 7;

/// Everything the app writes lives under one directory:
///   ~/Library/Application Support/com.armankundu.retain/
///
/// Deliberately NOT in iCloud Drive. See docs/icloud-sqlite-analysis.md — a live
/// SQLite database on a file-granular replicator corrupts, and the analysis walks
/// through exactly how.
pub fn database_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("retain.db")
}

pub fn snapshots_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("snapshots")
}

/// Open the database, configure it, and bring the schema up to date.
pub fn open(app_data_dir: &Path) -> anyhow::Result<Connection> {
    // `create_dir_all` is mkdir -p: makes every missing parent, and is happy if
    // the directory already exists.
    fs::create_dir_all(app_data_dir)?;

    let path = database_path(app_data_dir);
    let conn = Connection::open(&path)?;

    apply_pragmas(&conn)?;
    verify_integrity(&conn)?;
    run_migrations(&conn)?;

    Ok(conn)
}

/// PRAGMAs are SQLite's per-connection settings. These four matter:
fn apply_pragmas(conn: &Connection) -> anyhow::Result<()> {
    // Write-Ahead Logging. Readers don't block the writer, which keeps the UI
    // responsive while the timer's background thread writes.
    //
    // `query_row` rather than `execute` because journal_mode RETURNS the mode it
    // ended up in; SQLite treats it as a query, and `execute` errors on it.
    conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;

    // FULL means SQLite fsyncs on every commit, so a power cut cannot leave a
    // torn transaction. The usual argument against it is write throughput, which
    // is irrelevant here — this app writes a handful of rows per study session.
    conn.execute_batch("PRAGMA synchronous = FULL")?;

    // SQLite ignores REFERENCES clauses unless this is on, per connection. Without
    // it every foreign key in 001_init.sql is decorative.
    conn.execute_batch("PRAGMA foreign_keys = ON")?;

    // If another connection holds the write lock, wait up to 5s rather than
    // failing instantly.
    conn.execute_batch("PRAGMA busy_timeout = 5000")?;

    Ok(())
}

/// Cheap corruption check on startup. `quick_check` skips the expensive index
/// cross-validation that `integrity_check` does, which is the right trade for
/// something that runs on every launch.
///
/// If this fails we refuse to continue rather than writing more data on top of a
/// damaged file — the snapshots in `snapshots/` are the recovery path.
fn verify_integrity(conn: &Connection) -> anyhow::Result<()> {
    let result: String = conn.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if result != "ok" {
        anyhow::bail!(
            "The Retain database failed its integrity check ({result}). \
             It has not been modified. The most recent snapshot in the 'snapshots' \
             folder inside the app's data directory can be restored in its place."
        );
    }
    Ok(())
}

/// Apply any migration newer than the database's current `user_version`.
///
/// `user_version` is a 32-bit integer SQLite stores in the database header for
/// exactly this purpose. It costs nothing and needs no migrations table. With
/// four users and a schema we control end to end, a migration framework would be
/// more moving parts than the problem has.
pub(crate) fn run_migrations(conn: &Connection) -> anyhow::Result<()> {
    let current: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    for (version, sql) in MIGRATIONS {
        if *version > current {
            // Each migration runs inside a transaction: either the whole file
            // applies or none of it does, so a failure halfway through can't
            // leave a half-migrated schema.
            conn.execute_batch("BEGIN")?;

            // `execute_batch` runs multiple statements separated by semicolons.
            match conn.execute_batch(sql) {
                Ok(()) => {
                    // PRAGMA won't accept a bound parameter, so the version is
                    // formatted in. It comes from the MIGRATIONS constant above,
                    // never from user input, so there is nothing to inject.
                    conn.execute_batch(&format!("PRAGMA user_version = {version}"))?;
                    conn.execute_batch("COMMIT")?;
                }
                Err(e) => {
                    conn.execute_batch("ROLLBACK")?;
                    return Err(anyhow::anyhow!("migration {version} failed: {e}"));
                }
            }
        }
    }

    Ok(())
}

/// Write a consistent point-in-time copy of the database.
///
/// `VACUUM INTO` is the important detail. Unlike copying the file, it produces a
/// SINGLE file that is transactionally consistent and carries no `-wal`/`-shm`
/// sidecars. That property is what makes a snapshot safe to move, back up, or
/// (later, if the iCloud handoff gets built) sync — see the analysis doc.
pub fn snapshot(conn: &Connection, app_data_dir: &Path) -> anyhow::Result<PathBuf> {
    let dir = snapshots_dir(app_data_dir);
    fs::create_dir_all(&dir)?;

    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let target = dir.join(format!("retain-{stamp}.db"));

    // VACUUM INTO refuses to overwrite, so a colliding name is an error rather
    // than silent data loss. Second granularity makes collisions essentially
    // impossible for a once-per-launch snapshot.
    conn.execute("VACUUM INTO ?1", [target.to_string_lossy().as_ref()])?;

    prune_snapshots(&dir)?;
    Ok(target)
}

/// Keep the newest `SNAPSHOTS_TO_KEEP` snapshots, delete the rest.
fn prune_snapshots(dir: &Path) -> anyhow::Result<()> {
    let mut snaps: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "db")
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("retain-"))
        })
        .collect();

    // Filenames are timestamped in a sortable format, so lexicographic order is
    // chronological order. Sorting descending puts newest first.
    snaps.sort();
    snaps.reverse();

    for old in snaps.into_iter().skip(SNAPSHOTS_TO_KEEP) {
        // A snapshot we fail to delete is clutter, not a correctness problem, so
        // the error is deliberately swallowed rather than failing app startup.
        let _ = fs::remove_file(old);
    }

    Ok(())
}
