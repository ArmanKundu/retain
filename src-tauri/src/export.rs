//! Complete JSON export and import.
//!
//! The brief's requirement is "so I'm never trapped in my own app", which means
//! the export has to stay complete as the schema grows. So rather than naming
//! tables by hand — a list that would silently fall behind the first time
//! Checkpoint 2 adds one — the exporter asks SQLite what tables exist and dumps
//! all of them. New tables are included automatically.
//!
//! One thing is deliberately absent: API keys. They live in the Keychain, never
//! in the database, so they cannot appear here even by accident. An export is
//! therefore safe to hand to someone; it carries study data, not credentials.

use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde_json::{json, Map, Value};

/// Tables SQLite manages itself, which must not be exported or overwritten.
fn is_internal(table: &str) -> bool {
    table.starts_with("sqlite_")
}

fn table_names(conn: &Connection) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master
          WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
          ORDER BY name",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

    let mut out = Vec::new();
    for r in rows {
        let name = r?;
        if !is_internal(&name) {
            out.push(name);
        }
    }
    Ok(out)
}

/// Turn one SQLite cell into JSON.
///
/// `ValueRef` is SQLite's "whatever type this cell actually holds" — the engine
/// is dynamically typed, so a column can contain different types in different
/// rows and we have to branch on what is really there.
fn cell_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(f) => json!(f),
        ValueRef::Text(bytes) => Value::String(String::from_utf8_lossy(bytes).into_owned()),
        // No BLOB columns exist today. Checkpoint 2's pasted error-log images are
        // the first candidate, and base64 keeps them valid JSON when they arrive.
        ValueRef::Blob(bytes) => Value::String(base64_encode(bytes)),
    }
}

/// Minimal base64 encoder, so a whole crate isn't pulled in for one call site.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;

        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }

    out
}

/// Dump the entire database.
pub fn export_all(conn: &Connection) -> anyhow::Result<Value> {
    let schema_version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    let mut tables = Map::new();

    for table in table_names(conn)? {
        // The table name comes from sqlite_master, not from user input, so
        // formatting it into the query is safe — and bound parameters cannot be
        // used for identifiers in SQL anyway.
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\""))?;
        let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();

        let rows = stmt.query_map([], |row| {
            let mut object = Map::new();
            for (index, name) in columns.iter().enumerate() {
                object.insert(name.clone(), cell_to_json(row.get_ref(index)?));
            }
            Ok(Value::Object(object))
        })?;

        let mut collected = Vec::new();
        for r in rows {
            collected.push(r?);
        }
        tables.insert(table, Value::Array(collected));
    }

    Ok(json!({
        "app": "Retain",
        "exportedAt": crate::util::rfc3339(chrono::Utc::now()),
        "schemaVersion": schema_version,
        "tables": Value::Object(tables),
    }))
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub tables_written: usize,
    pub rows_written: usize,
}

/// Replace the database contents with an exported document.
///
/// This is destructive by design — it is a restore, not a merge. Merging two
/// divergent study histories has no obviously correct answer, and guessing at one
/// silently is how data goes missing. The UI takes an explicit confirmation, and
/// a snapshot is taken immediately before this runs.
pub fn import_all(conn: &mut Connection, document: &Value) -> anyhow::Result<ImportReport> {
    let schema_version = document
        .get("schemaVersion")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| anyhow::anyhow!("This file has no schemaVersion — it isn't a Retain export."))?;

    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if schema_version > current {
        anyhow::bail!(
            "This export was made by a newer version of Retain (schema {schema_version}, \
             this copy understands {current}). Update Retain, then import again."
        );
    }

    let tables = document
        .get("tables")
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("This file has no 'tables' section."))?;

    let existing = table_names(conn)?;

    // Foreign keys are switched off for the duration. Rows arrive in alphabetical
    // table order, which will not respect parent-before-child, so enforcing
    // references mid-import would reject perfectly valid data. Consistency is
    // checked at the end instead, before anything is committed.
    //
    // This PRAGMA is a no-op inside a transaction, so it has to come first.
    conn.execute_batch("PRAGMA foreign_keys = OFF")?;

    let result = (|| -> anyhow::Result<ImportReport> {
        let tx = conn.transaction()?;

        let mut tables_written = 0usize;
        let mut rows_written = 0usize;

        for table in &existing {
            let Some(rows) = tables.get(table).and_then(|v| v.as_array()) else {
                // A table absent from the export (an older file, before that
                // table existed) is left exactly as it is rather than wiped.
                continue;
            };

            tx.execute(&format!("DELETE FROM \"{table}\""), [])?;
            tables_written += 1;

            for row in rows {
                let Some(object) = row.as_object() else { continue };

                let columns: Vec<&String> = object.keys().collect();
                if columns.is_empty() {
                    continue;
                }

                let column_list = columns
                    .iter()
                    .map(|c| format!("\"{c}\""))
                    .collect::<Vec<_>>()
                    .join(", ");
                let placeholders = (1..=columns.len())
                    .map(|i| format!("?{i}"))
                    .collect::<Vec<_>>()
                    .join(", ");

                // Convert each JSON value to something rusqlite can bind.
                let values: Vec<Box<dyn rusqlite::ToSql>> = columns
                    .iter()
                    .map(|c| -> Box<dyn rusqlite::ToSql> {
                        match &object[*c] {
                            Value::Null => Box::new(Option::<i64>::None),
                            Value::Bool(b) => Box::new(if *b { 1i64 } else { 0i64 }),
                            Value::Number(n) => {
                                if let Some(i) = n.as_i64() {
                                    Box::new(i)
                                } else {
                                    Box::new(n.as_f64().unwrap_or(0.0))
                                }
                            }
                            Value::String(s) => Box::new(s.clone()),
                            // Nested structures have no column type to land in;
                            // storing the JSON text keeps the data rather than
                            // dropping the row.
                            other => Box::new(other.to_string()),
                        }
                    })
                    .collect();

                let bindings: Vec<&dyn rusqlite::ToSql> =
                    values.iter().map(|v| v.as_ref()).collect();

                tx.execute(
                    &format!("INSERT INTO \"{table}\" ({column_list}) VALUES ({placeholders})"),
                    bindings.as_slice(),
                )?;
                rows_written += 1;
            }
        }

        // Verify every foreign key now holds. If the import produced orphans, the
        // transaction is rolled back and the database is untouched.
        let violations: i64 =
            tx.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        if violations > 0 {
            anyhow::bail!(
                "That export is internally inconsistent ({violations} broken references). \
                 Nothing was changed."
            );
        }

        tx.commit()?;
        Ok(ImportReport {
            tables_written,
            rows_written,
        })
    })();

    // Restore enforcement whether the import succeeded or failed.
    conn.execute_batch("PRAGMA foreign_keys = ON")?;

    result
}

#[cfg(test)]
mod tests {
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

    fn seed(conn: &Connection) {
        conn.execute(
            "INSERT INTO sessions (subject_id,mode,started_at,ended_at,local_date,
                                   elapsed_seconds,active_seconds)
             VALUES (1,'stopwatch','2026-08-13T09:00:00Z','2026-08-13T10:00:00Z',
                     '2026-08-13',3600,3300)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cards (subject_id,note_type,front,back,state,content_hash,created_at)
             VALUES (1,'basic','What is a codon?','Three bases','new','h1','2026-08-13T09:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO calendar_events (uid,summary,starts_at,all_day,local_date,fetched_at)
             VALUES ('u1','Biology','2026-08-13T23:00:00Z',0,'2026-08-14','2026-08-13T00:00:00Z')",
            [],
        )
        .unwrap();
    }

    /// The export must cover every table the schema has, not a hand-written
    /// list. A table added later and silently omitted is exactly the data loss
    /// the export exists to prevent.
    #[test]
    fn every_table_in_the_schema_appears_in_the_export() {
        let conn = db();
        let doc = export_all(&conn).unwrap();
        let tables = doc.get("tables").unwrap().as_object().unwrap();

        for name in table_names(&conn).unwrap() {
            assert!(tables.contains_key(&name), "table {name} missing from the export");
        }

        // Including the ones added most recently.
        for recent in ["calendar_events", "practice_exams", "error_reattempts", "topic_reviews"] {
            assert!(tables.contains_key(recent), "{recent} missing");
        }
    }

    #[test]
    fn a_round_trip_preserves_the_data() {
        let source = db();
        seed(&source);
        let doc = export_all(&source).unwrap();

        let mut target = db();
        let report = import_all(&mut target, &doc).unwrap();
        assert!(report.rows_written > 0);

        let (front, back): (String, String) = target
            .query_row("SELECT front, back FROM cards", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(front, "What is a codon?");
        assert_eq!(back, "Three bases");

        let active: i64 = target
            .query_row("SELECT active_seconds FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(active, 3300);

        let summary: String = target
            .query_row("SELECT summary FROM calendar_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(summary, "Biology");
    }

    /// An export is meant to be safe to hand to someone. Keys live in the
    /// Keychain, so this asserts the property rather than assuming it.
    #[test]
    fn no_credential_shaped_value_appears_in_an_export() {
        let conn = db();
        seed(&conn);
        crate::settings::set(&conn, "ai_model_anthropic", "claude-opus-5").unwrap();

        let text = serde_json::to_string(&export_all(&conn).unwrap()).unwrap();

        for marker in ["sk-ant-", "sk-proj-", "AIza", "sk-or-v1"] {
            assert!(!text.contains(marker), "export contains something key-shaped: {marker}");
        }
        // The model name is fine — it is not a secret.
        assert!(text.contains("claude-opus-5"));
    }

    /// An older export that predates a table must leave that table alone rather
    /// than emptying it.
    #[test]
    fn a_table_absent_from_the_file_is_left_untouched() {
        let mut conn = db();
        seed(&conn);

        let mut doc = export_all(&conn).unwrap();
        doc.get_mut("tables")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("calendar_events");

        import_all(&mut conn, &doc).unwrap();

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM calendar_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "an absent table was wiped instead of skipped");
    }

    #[test]
    fn a_file_that_is_not_an_export_is_refused() {
        let mut conn = db();
        seed(&conn);

        for bad in [json!({}), json!({"tables": {}}), json!({"schemaVersion": 1})] {
            assert!(import_all(&mut conn, &bad).is_err(), "{bad} should be refused");
        }

        // And nothing was destroyed by the attempts.
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM cards", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    /// An export from a newer schema must be refused rather than half-applied.
    #[test]
    fn an_export_from_a_newer_schema_is_refused() {
        let mut conn = db();
        let mut doc = export_all(&conn).unwrap();
        doc.as_object_mut()
            .unwrap()
            .insert("schemaVersion".into(), json!(9999));

        let err = import_all(&mut conn, &doc).unwrap_err().to_string();
        assert!(err.contains("newer version"), "unhelpful message: {err}");
    }

    /// Foreign keys are disabled during import and re-checked before commit;
    /// a file with a dangling reference must not commit.
    #[test]
    fn foreign_keys_hold_after_an_import() {
        let source = db();
        seed(&source);
        let doc = export_all(&source).unwrap();

        let mut target = db();
        import_all(&mut target, &doc).unwrap();

        let violations: i64 = target
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(violations, 0);

        // And foreign keys are back on afterwards, not left disabled.
        let on: i64 = target.query_row("PRAGMA foreign_keys", [], |r| r.get(0)).unwrap();
        assert_eq!(on, 1, "foreign keys were left switched off after import");
    }

    #[test]
    fn an_empty_database_exports_and_reimports_cleanly() {
        let conn = db();
        let doc = export_all(&conn).unwrap();

        let mut target = db();
        assert!(import_all(&mut target, &doc).is_ok());

        let n: i64 = target.query_row("SELECT COUNT(*) FROM cards", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }
}
