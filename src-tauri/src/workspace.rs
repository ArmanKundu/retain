//! A folder per subject, on disk, so material can just be dropped in.
//!
//! Retain creates `~/Documents/Retain/<Subject>/` for each of your subjects.
//! Drop a PDF into the Biology folder, press Sync, and it's indexed — no file
//! picker, no per-file tagging. The subject is inferred from which folder the
//! file was in, which is the one piece of filing you were going to do anyway.
//!
//! The folders are ordinary Finder folders. Nothing is moved, copied or
//! rewritten: Retain reads them and stores the extracted text. Deleting the
//! folder loses nothing but the convenience.

use anyhow::{anyhow, Result};
use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubjectFolder {
    pub subject_id: i64,
    pub subject_name: String,
    pub colour: String,
    pub path: String,
    /// Files sitting in the folder that Retain could read.
    pub file_count: usize,
    /// How many of those are already in the library.
    pub imported_count: usize,
}

/// Strip characters that make a poor folder name.
///
/// Slashes especially: a subject called "Maths/Methods" would otherwise create
/// a nested folder rather than one named for the subject.
pub fn safe_folder_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c == '/' || c == ':' || c == '\\' { '-' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "Subject".to_string()
    } else {
        trimmed.to_string()
    }
}

/// The root Retain folder inside Documents.
pub fn root(documents: &Path) -> PathBuf {
    documents.join("Retain")
}

/// Create a folder for every active subject and remember where each one is.
///
/// Idempotent: run it again after adding a subject and only the new folder
/// appears. Existing folders are never emptied or renamed — someone may have
/// put a term's work in one.
pub fn ensure(conn: &Connection, documents: &Path) -> Result<Vec<SubjectFolder>> {
    let base = root(documents);
    std::fs::create_dir_all(&base)
        .map_err(|e| anyhow!("Couldn't create {}: {e}", base.display()))?;

    // A short note so the folder isn't mysterious when found in Finder later.
    let readme = base.join("About these folders.txt");
    if !readme.exists() {
        let _ = std::fs::write(
            &readme,
            "Retain made these folders, one per subject.\n\n\
             Put your notes, past papers and the study design into the matching folder, then\n\
             open Retain and press Sync on the Library screen. It reads the text and indexes it\n\
             so the assistant can answer from your own material.\n\n\
             Nothing here is moved or modified. Deleting a folder loses nothing but the\n\
             convenience of dropping files into it.\n",
        );
    }

    let mut stmt = conn.prepare(
        "SELECT id, name, colour FROM subjects WHERE archived = 0 ORDER BY sort_order, id",
    )?;
    let rows: Vec<(i64, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;

    let mut out = Vec::new();
    for (id, name, colour) in rows {
        let path = base.join(safe_folder_name(&name));
        std::fs::create_dir_all(&path)
            .map_err(|e| anyhow!("Couldn't create {}: {e}", path.display()))?;

        let display = path.to_string_lossy().to_string();
        conn.execute(
            "UPDATE subjects SET folder_path = ?2 WHERE id = ?1",
            rusqlite::params![id, display],
        )?;

        let files = crate::ingest::walk(&path);
        let imported = files
            .iter()
            .filter(|f| already_imported(conn, f).unwrap_or(false))
            .count();

        out.push(SubjectFolder {
            subject_id: id,
            subject_name: name,
            colour,
            path: display,
            file_count: files.len(),
            imported_count: imported,
        });
    }

    Ok(out)
}

/// Whether a file at this path has already been indexed.
///
/// Matched on the full path, which is what makes Sync re-runnable: pressing it
/// twice doesn't produce two copies of every document.
pub fn already_imported(conn: &Connection, path: &Path) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM resources WHERE origin_path = ?1",
        [path.to_string_lossy().to_string()],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Guess what a file is from its name.
///
/// A guess, and the UI lets you change it — but "2023 VCAA Exam 1.pdf" landing
/// under Past paper without being asked is the difference between filing thirty
/// documents and filing none.
pub fn guess_kind(name: &str) -> crate::resources::ResourceKind {
    use crate::resources::ResourceKind;
    let n = name.to_lowercase();

    if n.contains("study design") || n.contains("studydesign") || n.contains("curriculum") {
        ResourceKind::StudyDesign
    } else if n.contains("exam")
        || n.contains("paper")
        || n.contains("vcaa")
        || n.contains("sac")
        || n.contains("trial")
    {
        ResourceKind::PastPaper
    } else if n.contains("note") || n.contains("summary") || n.contains("revision") {
        ResourceKind::Notes
    } else {
        ResourceKind::Other
    }
}

/// A readable title from a filename: no extension, separators as spaces.
pub fn title_from_filename(name: &str) -> String {
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    let spaced = stem.replace(['_', '-'], " ");
    let collapsed = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        name.to_string()
    } else {
        collapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::ResourceKind;

    #[test]
    fn folder_names_survive_awkward_subjects() {
        assert_eq!(safe_folder_name("Biology"), "Biology");
        assert_eq!(safe_folder_name("Maths/Methods"), "Maths-Methods");
        assert_eq!(safe_folder_name("  Chemistry  "), "Chemistry");
        assert_eq!(safe_folder_name("..."), "Subject");
        assert_eq!(safe_folder_name(""), "Subject");
    }

    #[test]
    fn a_files_kind_is_guessed_from_its_name() {
        assert_eq!(guess_kind("VCAA Biology Study Design.pdf"), ResourceKind::StudyDesign);
        assert_eq!(guess_kind("2023 VCAA Exam 1.pdf"), ResourceKind::PastPaper);
        assert_eq!(guess_kind("unit 3 sac.docx"), ResourceKind::PastPaper);
        assert_eq!(guess_kind("cell biology notes.md"), ResourceKind::Notes);
        assert_eq!(guess_kind("random thing.txt"), ResourceKind::Other);
    }

    #[test]
    fn titles_come_out_readable() {
        assert_eq!(title_from_filename("2023_VCAA_Exam-1.pdf"), "2023 VCAA Exam 1");
        assert_eq!(title_from_filename("notes.md"), "notes");
        assert_eq!(title_from_filename("no extension"), "no extension");
    }

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::db::run_migrations(&conn).unwrap();
        conn.execute(
            "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,archived,created_at)
             VALUES (1,'Biology','#1','3_4','science',0,0,'2026-08-01T00:00:00Z'),
                    (2,'Maths/Methods','#2','1_2','maths',1,0,'2026-08-01T00:00:00Z'),
                    (3,'Dropped','#3','1_2','humanities',2,1,'2026-08-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    fn tmp(label: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("retain-ws-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_folder_is_created_for_every_active_subject() {
        let conn = db();
        let docs = tmp("ensure");

        let folders = ensure(&conn, &docs).unwrap();

        assert_eq!(folders.len(), 2, "archived subjects should be skipped");
        assert!(docs.join("Retain/Biology").is_dir());
        assert!(docs.join("Retain/Maths-Methods").is_dir());
        assert!(!docs.join("Retain/Dropped").exists());
        assert!(docs.join("Retain/About these folders.txt").is_file());

        // The path is remembered, so Sync doesn't have to ask again.
        let stored: Option<String> = conn
            .query_row("SELECT folder_path FROM subjects WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert!(stored.unwrap().ends_with("Retain/Biology"));
    }

    #[test]
    fn running_it_twice_changes_nothing_and_keeps_your_files() {
        let conn = db();
        let docs = tmp("idempotent");

        ensure(&conn, &docs).unwrap();
        let dropped_in = docs.join("Retain/Biology/my notes.txt");
        std::fs::write(&dropped_in, "content").unwrap();

        let folders = ensure(&conn, &docs).unwrap();

        assert!(dropped_in.is_file(), "an existing folder was emptied");
        let bio = folders.iter().find(|f| f.subject_name == "Biology").unwrap();
        assert_eq!(bio.file_count, 1);
        assert_eq!(bio.imported_count, 0, "nothing has been imported yet");
    }

    #[test]
    fn an_imported_file_is_recognised_on_the_next_sync() {
        let conn = db();
        let docs = tmp("imported");
        ensure(&conn, &docs).unwrap();

        let file = docs.join("Retain/Biology/paper.txt");
        std::fs::write(&file, "content").unwrap();

        assert!(!already_imported(&conn, &file).unwrap());

        conn.execute(
            "INSERT INTO resources (title,kind,content,word_count,added_at,origin_path)
             VALUES ('paper','past_paper','content',1,'2026-08-14T00:00:00Z',?1)",
            [file.to_string_lossy().to_string()],
        )
        .unwrap();

        assert!(already_imported(&conn, &file).unwrap());
        let folders = ensure(&conn, &docs).unwrap();
        let bio = folders.iter().find(|f| f.subject_name == "Biology").unwrap();
        assert_eq!(bio.imported_count, 1);
    }
}
