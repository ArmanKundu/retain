use super::*;
use std::fs;

/// A directory of this test's own.
///
/// Keyed on the test name, not the process id: tests run in parallel in one
/// process, so a shared directory means one test walks another's files.
fn tmp(label: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("retain-ingest-{}-{label}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
    let p = dir.join(name);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&p, body).unwrap();
    p
}

// -- which files are even considered ----------------------------------------

#[test]
fn candidates_cover_the_formats_notes_actually_arrive_in() {
    for name in ["a.pdf", "a.txt", "a.md", "a.html", "a.rtf", "a.csv", "A.PDF", "a.docx"] {
        assert!(is_candidate(Path::new(name)), "{name} should be considered");
    }
    for name in ["a.png", "a.mp4", "a.zip", "a.app", "noextension"] {
        assert!(!is_candidate(Path::new(name)), "{name} should be skipped");
    }
}

// -- markup and rtf ---------------------------------------------------------

#[test]
fn html_becomes_readable_prose() {
    let out = strip_markup(
        "<html><head><style>p{color:red}</style><script>alert(1)</script></head>\
         <body><h1>Enzymes</h1><p>They lower activation energy.</p>\
         <p>They are &amp; remain catalysts.</p></body></html>",
    );

    assert!(out.contains("Enzymes"));
    assert!(out.contains("They lower activation energy."));
    assert!(out.contains("& remain"), "entities should decode: {out}");
    // Script and style bodies would otherwise dominate the search index.
    assert!(!out.contains("alert"), "script body leaked: {out}");
    assert!(!out.contains("color:red"), "style body leaked: {out}");
}

#[test]
fn block_ends_become_line_breaks_so_paragraphs_survive() {
    let out = strip_markup("<p>One</p><p>Two</p>");
    assert!(out.contains('\n'), "expected a break between paragraphs: {out:?}");
}

#[test]
fn rtf_control_words_are_stripped_but_text_kept() {
    let out = strip_rtf(r"{\rtf1\ansi\deff0 Mitosis\par has four phases.}");
    assert!(out.contains("Mitosis"));
    assert!(out.contains("has four phases."));
    assert!(!out.contains("rtf1"));
    assert!(!out.contains(r"\par"));
}

// -- the scanned-PDF distinction --------------------------------------------

#[test]
fn a_trickle_of_characters_is_not_real_text() {
    // What a scanned page yields: a header and a page number.
    assert!(!looks_like_real_text("VCAA  2023  Page 4"));
    assert!(!looks_like_real_text(""));
    // Mostly symbols is not text either.
    assert!(!looks_like_real_text(&"◆ ▪ § ¶ ".repeat(40)));
}

#[test]
fn a_real_page_of_prose_is_recognised() {
    let page = "Describe the process by which the sodium potassium pump maintains the resting \
                membrane potential of a neuron, and explain why this process requires ATP in \
                order to move ions against their concentration gradients across the membrane.";
    assert!(looks_like_real_text(page));
}

// -- extraction -------------------------------------------------------------

#[test]
fn a_text_file_is_extracted_with_a_word_count() {
    let d = tmp("text-file");
    let p = write(&d, "notes.txt", "Photosynthesis happens in the chloroplast.");

    match extract_file(&p) {
        Outcome::Extracted { name, text, words, .. } => {
            assert_eq!(name, "notes.txt");
            assert!(text.contains("chloroplast"));
            assert_eq!(words, 5);
        }
        other => panic!("expected extraction, got {other:?}"),
    }
}

#[test]
fn word_documents_are_refused_with_a_way_forward() {
    let d = tmp("docx");
    let p = write(&d, "essay.docx", "PK\u{3}\u{4}not really a docx");

    match extract_file(&p) {
        Outcome::Unsupported { reason, .. } => {
            assert!(reason.contains("Export"), "should say what to do: {reason}");
            assert!(reason.to_lowercase().contains("pdf"));
        }
        other => panic!("expected unsupported, got {other:?}"),
    }
}

#[test]
fn an_empty_file_fails_rather_than_storing_nothing() {
    let d = tmp("empty");
    let p = write(&d, "blank.txt", "   \n\n  ");
    assert!(matches!(extract_file(&p), Outcome::Failed { .. }));
}

#[test]
fn a_missing_file_is_reported_not_panicked() {
    assert!(matches!(
        extract_file(Path::new("/nonexistent/nowhere.txt")),
        Outcome::Failed { .. }
    ));
}

/// A malformed PDF must be reported, not crash the app. `pdf-extract` panics on
/// some inputs, which is why extraction is wrapped.
#[test]
fn a_corrupt_pdf_is_reported_rather_than_bringing_down_the_app() {
    let d = tmp("corrupt-pdf");
    let p = write(&d, "broken.pdf", "%PDF-1.4\nthis is not actually a pdf at all\n%%EOF");

    match extract_file(&p) {
        Outcome::Failed { reason, .. } => assert!(!reason.is_empty()),
        Outcome::Scanned { .. } => {} // also acceptable: no text found
        other => panic!("expected a failure or scanned result, got {other:?}"),
    }
}

// -- folder walking ---------------------------------------------------------

#[test]
fn a_folder_walk_finds_nested_files_and_skips_clutter() {
    let d = tmp("walk");
    write(&d, "Unit 3/AoS 1/dot points.md", "content");
    write(&d, "Unit 3/2023 exam.txt", "content");
    write(&d, "Unit 4/notes.txt", "content");
    write(&d, ".DS_Store", "junk");
    write(&d, "diagram.png", "junk");
    write(&d, ".hidden/secret.txt", "junk");

    let found = walk(&d);
    let names: Vec<String> = found.iter().map(|p| file_name(p)).collect();

    assert_eq!(found.len(), 3, "found {names:?}");
    assert!(names.contains(&"dot points.md".to_string()));
    assert!(names.contains(&"2023 exam.txt".to_string()));
    assert!(names.contains(&"notes.txt".to_string()));
    assert!(!names.iter().any(|n| n == ".DS_Store"));
    assert!(!names.iter().any(|n| n == "diagram.png"));
    assert!(!names.iter().any(|n| n == "secret.txt"));
}

#[test]
fn walking_something_that_is_not_a_folder_returns_nothing() {
    assert!(walk(Path::new("/nonexistent/folder")).is_empty());
}

/// Depth is bounded so pointing at a home directory doesn't run for an hour.
#[test]
fn the_walk_stops_at_a_depth_limit() {
    let d = tmp("depth");
    let deep = (0..MAX_DEPTH + 3).map(|i| format!("l{i}")).collect::<Vec<_>>().join("/");
    write(&d, &format!("{deep}/buried.txt"), "content");
    write(&d, "shallow.txt", "content");

    let names: Vec<String> = walk(&d).iter().map(|p| file_name(p)).collect();
    assert!(names.contains(&"shallow.txt".to_string()));
    assert!(!names.contains(&"buried.txt".to_string()), "walked past the depth limit");
}
