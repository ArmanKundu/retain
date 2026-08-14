//! Turning files and folders into text Retain can index.
//!
//! The Library previously accepted pasted text and `.txt`/`.md` read in the
//! webview. That was honest but thin: VCAA past papers are PDFs, and a year of
//! class notes is a folder, not a file you paste one at a time.
//!
//! So extraction moved to Rust, where it can read a PDF, walk a directory and
//! report per-file what happened. The webview only ever sends a path.
//!
//! ## What it will and won't read
//!
//! Text-shaped formats are read directly. PDFs go through `pdf-extract`, which
//! pulls the text layer. **A scanned PDF has no text layer** — it's page
//! images — and no amount of parsing will find words in it. That case is
//! detected and reported as its own outcome rather than being silently stored
//! as an empty document, because "I imported it and nothing happened" is the
//! worst possible failure here.
//!
//! `.docx` and `.pages` are zip archives of XML. Reading them properly means a
//! zip dependency and an XML parser for a format that changes; exporting to PDF
//! or plain text takes one keystroke in the app that made them. They're
//! reported as unsupported with that instruction rather than half-parsed.

use anyhow::{anyhow, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Files bigger than this are almost certainly not study notes.
const MAX_FILE_BYTES: u64 = 80 * 1024 * 1024;

/// Depth limit when walking a folder. Deep enough for `Biology/Unit 3/AoS 1`,
/// shallow enough that pointing at a home directory doesn't run for an hour.
const MAX_DEPTH: usize = 6;

/// Ceiling on files considered in one import.
const MAX_FILES: usize = 500;

/// What happened to one file.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum Outcome {
    /// Text extracted and ready to store.
    Extracted {
        path: String,
        name: String,
        text: String,
        words: usize,
    },
    /// A PDF with no text layer — pages are images.
    Scanned { path: String, name: String },
    /// A format we deliberately don't parse, with what to do instead.
    Unsupported {
        path: String,
        name: String,
        reason: String,
    },
    /// Something went wrong reading it.
    Failed {
        path: String,
        name: String,
        reason: String,
    },
}

/// Extensions worth opening at all. Anything else in a folder is skipped
/// silently — a folder of notes usually also holds images and `.DS_Store`, and
/// reporting each of those as a failure would bury the real results.
pub fn is_candidate(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_lowercase().as_str(),
        "txt" | "md" | "markdown" | "text" | "csv" | "tsv" | "json" | "html" | "htm"
            | "xml" | "rtf" | "tex" | "org" | "rst" | "log" | "pdf" | "docx" | "pages" | "doc"
            | "odt" | "key" | "pptx"
    )
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string()
}

/// Strip HTML/XML tags, keeping the text between them.
///
/// Not a parser — a tag stripper. It exists so a saved web page or an exported
/// XML note becomes readable prose rather than markup, and it drops `<script>`
/// and `<style>` bodies, which would otherwise dominate the index with code.
pub fn strip_markup(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut skip_until: Option<&str> = None;

    while let Some(c) = chars.next() {
        if c != '<' {
            if skip_until.is_none() {
                out.push(c);
            }
            continue;
        }

        // Collect the tag.
        let mut tag = String::new();
        for t in chars.by_ref() {
            if t == '>' {
                break;
            }
            tag.push(t);
        }

        let lower = tag.trim().to_lowercase();

        if let Some(end) = skip_until {
            if lower.starts_with(end) {
                skip_until = None;
            }
            continue;
        }

        if lower.starts_with("script") {
            skip_until = Some("/script");
        } else if lower.starts_with("style") {
            skip_until = Some("/style");
        } else if lower.starts_with("br")
            || lower.starts_with("/p")
            || lower.starts_with("/div")
            || lower.starts_with("/li")
            || lower.starts_with("/h")
            || lower.starts_with("/tr")
        {
            // Block-level ends become line breaks, so paragraphs survive.
            out.push('\n');
        } else {
            out.push(' ');
        }
    }

    decode_entities(&out)
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&rsquo;", "’")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
}

/// Strip RTF control words, keeping the literal text.
pub fn strip_rtf(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // A control word runs to the first non-alphanumeric character.
                let mut word = String::new();
                while let Some(&n) = chars.peek() {
                    if n.is_ascii_alphanumeric() || n == '-' {
                        word.push(n);
                        chars.next();
                    } else {
                        break;
                    }
                }
                // A single trailing space is part of the control word.
                if chars.peek() == Some(&' ') {
                    chars.next();
                }
                if word.starts_with("par") || word.starts_with("line") {
                    out.push('\n');
                }
            }
            '{' | '}' => {}
            _ => out.push(c),
        }
    }

    out
}

/// Whether extracted text is substantial enough to be real content.
///
/// A PDF whose pages are images still yields a trickle of characters — page
/// numbers, a header from the template. This is what separates that from a
/// document that actually has words in it.
pub fn looks_like_real_text(text: &str) -> bool {
    let words = text.split_whitespace().count();
    if words < 25 {
        return false;
    }
    // Guard against a "text layer" that is mostly ligature noise or symbols.
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    letters * 4 > text.chars().count()
}

/// Read one file.
pub fn extract_file(path: &Path) -> Outcome {
    let name = file_name(path);
    let display = path.to_string_lossy().to_string();

    let unsupported = |reason: &str| Outcome::Unsupported {
        path: display.clone(),
        name: name.clone(),
        reason: reason.to_string(),
    };
    let failed = |reason: String| Outcome::Failed {
        path: display.clone(),
        name: name.clone(),
        reason,
    };

    match std::fs::metadata(path) {
        Ok(m) if m.len() > MAX_FILE_BYTES => {
            return failed(format!("{} MB is too large to index.", m.len() / 1_048_576))
        }
        Err(e) => return failed(e.to_string()),
        _ => {}
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let text = match ext.as_str() {
        "docx" | "doc" | "odt" | "pages" | "key" | "pptx" => {
            return unsupported(
                "Word, Pages and Keynote files are compressed archives Retain doesn't unpack. \
                 Export it as PDF or plain text and add that instead — one keystroke in the app \
                 that made it.",
            )
        }

        "pdf" => match extract_pdf(path) {
            Ok(t) => t,
            Err(e) => return failed(e.to_string()),
        },

        "html" | "htm" | "xml" => match std::fs::read_to_string(path) {
            Ok(raw) => strip_markup(&raw),
            Err(e) => return failed(e.to_string()),
        },

        "rtf" => match std::fs::read_to_string(path) {
            Ok(raw) => strip_rtf(&raw),
            Err(e) => return failed(e.to_string()),
        },

        _ => match std::fs::read(path) {
            // Lossy rather than strict: a stray invalid byte in a notes file
            // shouldn't cost you the whole document.
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) => return failed(e.to_string()),
        },
    };

    let cleaned = crate::resources::normalise(&text);

    if ext == "pdf" && !looks_like_real_text(&cleaned) {
        return Outcome::Scanned {
            path: display,
            name,
        };
    }
    if cleaned.trim().is_empty() {
        return failed("There's no readable text in it.".into());
    }

    let words = cleaned.split_whitespace().count();
    Outcome::Extracted {
        path: display,
        name,
        text: cleaned,
        words,
    }
}

/// Pull the text layer out of a PDF.
///
/// `pdf-extract` panics on some malformed files rather than returning an error,
/// so the call is caught. A single bad PDF in a folder of thirty must not take
/// down the import — or the app.
fn extract_pdf(path: &Path) -> Result<String> {
    let owned = path.to_path_buf();

    let result = std::panic::catch_unwind(move || pdf_extract::extract_text(&owned));

    match result {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(e)) => Err(anyhow!("Couldn't read that PDF: {e}")),
        Err(_) => Err(anyhow!(
            "That PDF is malformed enough that it couldn't be parsed. Opening it in Preview and \
             re-exporting as PDF usually fixes it."
        )),
    }
}

/// Every candidate file under a folder, breadth-first, depth-limited.
///
/// Hidden files and the usual macOS clutter are skipped. Symlinks are not
/// followed: a folder containing a link to its own parent would otherwise walk
/// forever.
pub fn walk(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut queue: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];

    while let Some((dir, depth)) = queue.pop() {
        if depth > MAX_DEPTH || found.len() >= MAX_FILES {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();

            let name = file_name(&path);
            if name.starts_with('.') {
                continue;
            }

            let Ok(meta) = entry.metadata() else { continue };
            // `metadata()` on a DirEntry does not traverse symlinks on macOS,
            // so this is also the symlink guard.
            if meta.is_symlink() {
                continue;
            }

            if meta.is_dir() {
                queue.push((path, depth + 1));
            } else if is_candidate(&path) && found.len() < MAX_FILES {
                found.push(path);
            }
        }
    }

    found.sort();
    found
}

#[cfg(test)]
mod tests;
