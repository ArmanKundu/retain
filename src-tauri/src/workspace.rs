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
             Each subject has folders for the different kinds of material:\n\n\
               Study design           what VCAA says is examinable\n\
               Past papers            VCAA exams\n\
               Solutions and reports  marking schemes, examiner's reports\n\
               Textbook               chapters and extracts\n\n\
             Those four cover the whole unit sequence, so they sit directly under the\n\
             subject. The things you keep per unit have a folder each:\n\n\
               Unit 3/School notes    from your teacher\n\
               Unit 3/My notes        your own\n\
               Unit 3/Trial tests     your school's practice exams\n\
               Unit 4/...             the same again\n\n\
             Drop files into the matching folder, then open Retain and press Sync on the\n\
             Library screen. The folder tells Retain both the subject and what the document\n\
             is, so there is nothing to fill in.\n\n\
             The distinction matters: the assistant treats the study design as authoritative\n\
             about what is examinable, and your own notes as a record of what you understood\n\
             at the time.\n\n\
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

        build_tree(conn, id, &path)?;

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

/// The folder tree for one subject.
///
/// Shaped by which distinctions are real. VCAA material — the study design, the
/// exams, the reports — and the textbook cover the whole unit sequence, so they
/// sit directly under the subject. The things you genuinely keep per unit get a
/// `Unit 3` / `Unit 4` folder each:
///
///     Chemistry/
///       Study design/            what VCAA says is examinable, both units
///       Past papers/             VCAA exams, both units
///       Solutions and reports/   marking schemes, examiner's reports
///       Textbook/
///       Unit 3/
///         School notes/  My notes/  Trial tests/
///       Unit 4/
///         School notes/  My notes/  Trial tests/
///
/// The alternative — a unit folder above everything — would ask you which unit
/// a study design belongs to, and there is no answer. A tree that asks
/// unanswerable questions is one you stop filing things in.
fn build_tree(conn: &Connection, subject_id: i64, path: &Path) -> Result<()> {
    use crate::resources::ResourceKind;

    for kind in ResourceKind::all() {
        // "Other" is a fallback, not somewhere to file things.
        if kind == ResourceKind::Other || kind.per_unit() {
            continue;
        }
        let _ = std::fs::create_dir_all(path.join(kind.folder()));
    }

    for unit in units_for(conn, subject_id)? {
        let unit_dir = path.join(format!("Unit {unit}"));
        for kind in ResourceKind::all().into_iter().filter(|k| k.per_unit()) {
            let _ = std::fs::create_dir_all(unit_dir.join(kind.folder()));
        }
    }

    Ok(())
}

/// The two units a subject is currently running, from its unit level.
fn units_for(conn: &Connection, subject_id: i64) -> Result<[u8; 2]> {
    let level: String = conn
        .query_row("SELECT unit_level FROM subjects WHERE id = ?1", [subject_id], |r| r.get(0))
        .unwrap_or_else(|_| "3_4".to_string());

    Ok(match level.as_str() {
        "1_2" => [1, 2],
        _ => [3, 4],
    })
}

/// Which unit a file's path puts it in, if any.
///
/// `…/Chemistry/Unit 3/My notes/kinetics.pdf` is Unit 3. Anything not under a
/// unit folder spans the sequence, and that is a real answer rather than a
/// missing one — see `ResourceKind::per_unit`.
pub fn unit_from_path(path: &Path) -> Option<i64> {
    path.components().rev().find_map(|c| {
        let name = c.as_os_str().to_string_lossy();
        let rest = name.strip_prefix("Unit ").or_else(|| name.strip_prefix("unit "))?;
        rest.trim().parse::<i64>().ok().filter(|u| (1..=4).contains(u))
    })
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

/// What kind of material a file is, from where it sits.
///
/// The folder is the answer. `Retain/Biology/Past papers/2023 exam.pdf` is a
/// past paper because of the folder it's in, not because of a keyword in its
/// name — which is what turns filing from a per-file form into the one drag you
/// were going to do anyway.
///
/// The filename is only consulted for files sitting loose in a subject folder,
/// or picked from somewhere else entirely.
pub fn kind_for(path: &Path) -> crate::resources::ResourceKind {
    use crate::resources::ResourceKind;

    // Walk up looking for a folder whose name matches a kind.
    for ancestor in path.ancestors().skip(1) {
        let Some(folder) = ancestor.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if let Some(kind) = ResourceKind::all()
            .into_iter()
            .find(|k| k.folder().eq_ignore_ascii_case(folder))
        {
            return kind;
        }
    }

    guess_kind(&path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase())
}

/// Fall back to the filename when the folder says nothing.
pub fn guess_kind(name: &str) -> crate::resources::ResourceKind {
    use crate::resources::ResourceKind;
    let n = name.to_lowercase();

    if n.contains("study design") || n.contains("studydesign") || n.contains("curriculum") {
        ResourceKind::StudyDesign
    } else if n.contains("solution")
        || n.contains("answer")
        || n.contains("marking")
        || n.contains("examiner")
        || n.contains("report")
    {
        // Checked before "exam", or "2023 exam solutions.pdf" files as a paper.
        ResourceKind::ExamSolution
    } else if n.contains("trial") || n.contains("practice exam") {
        // Checked before "exam": a school trial is not a VCAA paper and must
        // not be weighted as evidence of what VCAA actually asks.
        ResourceKind::TrialTest
    } else if n.contains("exam") || n.contains("paper") || n.contains("vcaa") || n.contains("sac") {
        ResourceKind::PastPaper
    } else if n.contains("textbook") || n.contains("chapter") {
        ResourceKind::Textbook
    } else if n.contains("note") || n.contains("summary") || n.contains("revision") {
        ResourceKind::SchoolNotes
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
    fn a_files_kind_is_guessed_from_its_name_when_the_folder_says_nothing() {
        assert_eq!(guess_kind("vcaa biology study design.pdf"), ResourceKind::StudyDesign);
        assert_eq!(guess_kind("2023 vcaa exam 1.pdf"), ResourceKind::PastPaper);
        assert_eq!(guess_kind("unit 3 sac.docx"), ResourceKind::PastPaper);
        assert_eq!(guess_kind("cell biology notes.md"), ResourceKind::SchoolNotes);
        assert_eq!(guess_kind("random thing.txt"), ResourceKind::Other);

        // Solutions are checked before papers, or "2023 exam solutions" files
        // as the paper it answers.
        assert_eq!(guess_kind("2023 exam solutions.pdf"), ResourceKind::ExamSolution);
        assert_eq!(guess_kind("examiners report 2022.pdf"), ResourceKind::ExamSolution);
    }

    /// The folder is the real answer — that's what makes filing one drag.
    #[test]
    fn the_folder_a_file_sits_in_decides_its_kind() {
        let base = Path::new("/Users/x/Documents/Retain/Biology");

        assert_eq!(
            kind_for(&base.join("Past papers/anything at all.pdf")),
            ResourceKind::PastPaper
        );
        assert_eq!(
            kind_for(&base.join("Study design/notes about exams.pdf")),
            ResourceKind::StudyDesign,
            "the folder must win over misleading words in the filename"
        );
        assert_eq!(
            kind_for(&base.join("My notes/week 3.md")),
            ResourceKind::PersonalNotes
        );
        assert_eq!(
            kind_for(&base.join("Solutions and reports/2023.pdf")),
            ResourceKind::ExamSolution
        );

        // Loose in the subject folder: fall back to the filename.
        assert_eq!(kind_for(&base.join("2023 vcaa exam.pdf")), ResourceKind::PastPaper);
    }

    /// Ordering by authority is what stops a lucky keyword match in your own
    /// notes outranking the study design paragraph that defines the term.
    #[test]
    fn the_study_design_outranks_your_own_notes() {
        let mut kinds = [
            ResourceKind::PersonalNotes,
            ResourceKind::StudyDesign,
            ResourceKind::SchoolNotes,
            ResourceKind::ExamSolution,
        ];
        kinds.sort_by_key(|k| k.authority());

        assert_eq!(kinds[0], ResourceKind::StudyDesign);
        assert_eq!(kinds[1], ResourceKind::ExamSolution);
        assert_eq!(kinds[3], ResourceKind::PersonalNotes);
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

    /// VCAA material and the textbook cover the whole sequence, so they sit
    /// directly under the subject. Asking which unit a study design belongs to
    /// has no answer, and a tree that asks unanswerable questions is one you
    /// stop filing things in.
    #[test]
    fn material_that_spans_the_units_sits_above_them() {
        let conn = db();
        let docs = tmp("kinds");
        ensure(&conn, &docs).unwrap();

        let bio = docs.join("Retain/Biology");
        for name in ["Study design", "Past papers", "Solutions and reports", "Textbook"] {
            assert!(bio.join(name).is_dir(), "missing {name}");
        }

        // These are per-unit, so they must NOT exist at the subject level —
        // two places to put your Unit 3 notes is worse than one.
        for name in ["School notes", "My notes", "Trial tests"] {
            assert!(!bio.join(name).exists(), "{name} should be inside a unit");
        }

        // "Other" is a fallback, not somewhere to file things.
        assert!(!bio.join("Other").exists());
    }

    #[test]
    fn the_things_you_keep_per_unit_get_a_folder_each() {
        let conn = db();
        let docs = tmp("units");
        ensure(&conn, &docs).unwrap();

        let bio = docs.join("Retain/Biology");
        for unit in ["Unit 3", "Unit 4"] {
            for name in ["School notes", "My notes", "Trial tests"] {
                assert!(bio.join(unit).join(name).is_dir(), "missing {unit}/{name}");
            }
        }
    }

    /// A 1/2 subject gets Units 1 and 2, not 3 and 4.
    #[test]
    fn the_unit_folders_follow_the_subjects_own_level() {
        let conn = db();
        conn.execute(
            "INSERT INTO subjects (id,name,colour,unit_level,subject_type,sort_order,created_at)
             VALUES (90,'Methods','#D08B3C','1_2','maths',9,'2026-08-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let docs = tmp("level");
        ensure(&conn, &docs).unwrap();

        let methods = docs.join("Retain/Methods");
        assert!(methods.join("Unit 1/My notes").is_dir());
        assert!(methods.join("Unit 2/My notes").is_dir());
        assert!(!methods.join("Unit 3").exists());
    }

    #[test]
    fn a_files_unit_comes_from_the_folder_it_was_dropped_in() {
        let base = Path::new("/Users/x/Documents/Retain/Chemistry");

        assert_eq!(unit_from_path(&base.join("Unit 3/My notes/redox.pdf")), Some(3));
        assert_eq!(unit_from_path(&base.join("Unit 4/Trial tests/2025.pdf")), Some(4));

        // Material above the unit folders spans the sequence. `None` is the
        // right answer, not a missing one.
        assert_eq!(unit_from_path(&base.join("Study design/2022.pdf")), None);
        assert_eq!(unit_from_path(&base.join("Past papers/2023 exam.pdf")), None);

        // Not a unit folder, however much it looks like one.
        assert_eq!(unit_from_path(&base.join("Unit conversions/notes.pdf")), None);
        assert_eq!(unit_from_path(&base.join("Unit 9/notes.pdf")), None);
    }

    /// A school trial exam is a prediction of a VCAA paper, not evidence of what
    /// VCAA asks, so it must not be filed as one.
    #[test]
    fn a_school_trial_is_not_filed_as_a_vcaa_paper() {
        assert_eq!(guess_kind("2025 trial exam.pdf"), crate::resources::ResourceKind::TrialTest);
        assert_eq!(guess_kind("practice exam 1.pdf"), crate::resources::ResourceKind::TrialTest);
        assert_eq!(guess_kind("2023 vcaa exam.pdf"), crate::resources::ResourceKind::PastPaper);
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
