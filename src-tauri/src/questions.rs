//! Finding one question in a thousand papers.
//!
//! The library holds over a thousand exams. Searching it returns *papers*, and
//! a paper is twenty pages — which means "show me every calculus question in
//! Specialist" was answerable only by opening PDFs until you found some.
//!
//! # How a paper is cut up
//!
//! On a line of its own, `Question 3`. That is the marker, and it was chosen by
//! looking rather than guessing: of the 1,041 exam resources in the real
//! library, 942 contain it. The alternative — a bare `3.` at the start of a
//! line — is far more common in the text and almost never a question, because
//! every multiple-choice answer grid, every page number and every reference
//! list produces one.
//!
//! Everything before the first marker is front matter: the publisher's address,
//! the instructions, the "you may not bring a calculator". It is dropped.
//!
//! # What this deliberately does not do
//!
//! It does not read the *answers*. A question and its worked solution live in
//! separate documents most of the time, and pairing them by guessing would
//! produce confident wrong pairings. The question is what you search for; the
//! document it came from is one click away.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

/// Below this a "question" is a heading, a page artefact, or a stray line.
const MIN_WORDS: usize = 4;

/// Above this the segmentation has gone wrong — a marker was missed and two or
/// more questions have run together, or the whole paper landed in one span.
/// Better to keep it than drop it, but it is worth capping what gets stored.
const MAX_CHARS: usize = 4_000;

#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub label: String,
    pub number: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: i64,
    pub resource_id: i64,
    pub resource_title: String,
    pub subject_id: Option<i64>,
    pub subject_name: Option<String>,
    pub label: String,
    pub words: i64,
    /// Zero-based page in the original PDF, once located.
    pub page: Option<i64>,
    /// Whether the original file is still where it was imported from — the
    /// only thing that decides if a picture is possible.
    pub has_file: bool,
    pub text: String,
    pub tags: Vec<String>,
    /// Year, publisher and whether this came out of a solutions document —
    /// derived from the paper's title, which is where all of it actually lives.
    #[serde(flatten)]
    pub paper: PaperMeta,
}

/// Whether a line is a question marker, and which number it carries.
///
/// Requires the line to be *only* the marker. "Question 3" alone starts a
/// question; "…as shown in Question 3 above" is a cross-reference inside one,
/// and treating it as a boundary would split a question in half.
fn marker(line: &str) -> Option<(String, i64)> {
    let t = line.trim();
    let rest = t.strip_prefix("Question ").or_else(|| t.strip_prefix("QUESTION "))?;

    // Trailing punctuation and marks in brackets are part of the label, not the
    // number: "Question 12 (4 marks)" is still question 12.
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }

    let after = rest[digits.len()..].trim();
    let is_label_only = after.is_empty()
        || after.starts_with('(')
        || after.starts_with('.')
        || after.starts_with('–')
        || after.starts_with('-')
        || after.starts_with("marks");
    if !is_label_only {
        return None;
    }

    let number: i64 = digits.parse().ok()?;
    // Papers don't have a question 300; a match that big is a page reference or
    // a year that happened to follow the word.
    if !(1..=200).contains(&number) {
        return None;
    }

    Some((t.to_string(), number))
}

/// Split a paper into its questions.
pub fn segment(content: &str) -> Vec<Parsed> {
    let mut out: Vec<Parsed> = Vec::new();
    let mut current: Option<(String, i64, Vec<&str>)> = None;

    for line in content.lines() {
        if let Some((label, number)) = marker(line) {
            if let Some((l, n, body)) = current.take() {
                push(&mut out, l, n, &body);
            }
            current = Some((label, number, Vec::new()));
            continue;
        }
        // Lines before the first marker are the publisher's front matter.
        if let Some((_, _, body)) = current.as_mut() {
            body.push(line);
        }
    }

    if let Some((l, n, body)) = current {
        push(&mut out, l, n, &body);
    }
    out
}

fn push(out: &mut Vec<Parsed>, label: String, number: i64, body: &[&str]) {
    let text = body.join("\n").trim().to_string();
    if text.split_whitespace().count() < MIN_WORDS {
        return;
    }

    let text = if text.chars().count() > MAX_CHARS {
        text.chars().take(MAX_CHARS).collect()
    } else {
        text
    };

    out.push(Parsed { label, number, text });
}

/// How many of a topic's own words a question has to use before it counts.
///
/// Three, paired with the shared-term filter. Four was measurably purer and
/// tagged only 5% of the library; three doubles coverage without the skills
/// sections coming back, because dropping any term shared by two headings has
/// already stripped their generic vocabulary. The old comment about four: their vocabulary is the
/// generic language of scientific method — evidence, data, conclusion — and
/// almost every exam question uses three of those words in passing. Measured
/// against the real library, four is where a tag starts meaning the question is
/// actually about that topic.
const TERMS_TO_MATCH: usize = 3;

/// Tag a question by the vocabulary its topics actually use.
///
/// Matching against topic *names* is what the first version did, and it tagged
/// 37 questions out of 6,529 — a heading reads "Cellular structure and
/// function" while the question says "active transport across the plasma
/// membrane". The words that connect them are in the study design's dot
/// points, which is where these come from.
pub fn auto_tags_by_vocabulary(
    text: &str,
    topics: &[crate::resources::TopicVocabulary],
) -> Vec<String> {
    let haystack = text.to_lowercase();
    // Both sides reduced to a singular stem. The study design writes
    // "inhibitors" and the question writes "inhibitor"; matching them exactly
    // misses, which is the same failure the name matcher had.
    //
    // One pass into a set, rather than scanning the question once per term: a
    // subject has forty topics and several hundred terms between them.
    let words: std::collections::HashSet<String> = haystack
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 5)
        .map(singular)
        .collect();

    let mut scored: Vec<(usize, &str)> = topics
        .iter()
        .filter_map(|t| {
            let hits = t
                .terms
                .iter()
                .filter(|term| words.contains(&singular(term)))
                .count();
            (hits >= TERMS_TO_MATCH).then_some((hits, t.name.as_str()))
        })
        .collect();

    // Strongest first, and at most two: a question belongs to one topic, and a
    // list of six tags is the same as no tags.
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(2);
    scored.into_iter().map(|(_, name)| name.to_string()).collect()
}

/// Drop a plural ending, so "enzymes" and "enzyme" are the same word.
///
/// Only the endings that matter for exam vocabulary. Real stemming would also
/// turn "mitosis" into "mitosi", which is worse than doing nothing.
fn singular(word: &str) -> String {
    if let Some(base) = word.strip_suffix("ies") {
        return format!("{base}y");
    }
    if word.ends_with("ss") {
        return word.to_string();
    }
    word.strip_suffix('s').unwrap_or(word).to_string()
}

/// Index one resource, replacing anything already stored for it.
///
/// Re-runnable: indexing twice leaves the same questions rather than two copies,
/// which matters because the button that triggers it is easy to press twice.
pub fn index_resource(conn: &mut Connection, resource_id: i64) -> Result<usize> {
    let (subject_id, content, kind): (Option<i64>, String, String) = conn.query_row(
        "SELECT subject_id, content, kind FROM resources WHERE id = ?1",
        [resource_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    // Only exam-shaped material. Segmenting a study design on the word
    // "Question" produces nonsense with a question number attached to it.
    if !matches!(kind.as_str(), "past_paper" | "trial_test" | "exam_solution") {
        return Ok(0);
    }

    // The study design's vocabulary, not its topic names — see
    // `auto_tags_by_vocabulary` for why matching on names barely works.
    let topics: Vec<crate::resources::TopicVocabulary> = match subject_id {
        Some(sid) => {
            let mut stmt = conn.prepare(
                "SELECT content FROM resources WHERE subject_id = ?1 AND kind = 'study_design'",
            )?;
            let designs: Vec<String> =
                stmt.query_map([sid], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
            designs
                .iter()
                .flat_map(|d| crate::resources::topic_vocabulary(d))
                .collect()
        }
        None => Vec::new(),
    };

    let parsed = segment(&content);

    let tx = conn.transaction()?;
    // Cascades to tags and, through the trigger, out of the FTS index.
    tx.execute("DELETE FROM questions WHERE resource_id = ?1", [resource_id])?;

    {
        let mut insert = tx.prepare(
            "INSERT INTO questions (resource_id, subject_id, label, number, ordinal, text, words)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
        )?;
        let mut tag = tx.prepare(
            "INSERT OR IGNORE INTO question_tags (question_id, tag, source) VALUES (?1,?2,'auto')",
        )?;

        for (i, q) in parsed.iter().enumerate() {
            insert.execute(rusqlite::params![
                resource_id,
                subject_id,
                q.label,
                q.number,
                i as i64,
                q.text,
                q.text.split_whitespace().count() as i64,
            ])?;
            let id = tx.last_insert_rowid();

            for t in auto_tags_by_vocabulary(&q.text, &topics) {
                tag.execute(rusqlite::params![id, t])?;
            }
        }
    }

    tx.commit()?;
    Ok(parsed.len())
}

/// How many exam resources still have no questions indexed.
pub fn unindexed(conn: &Connection) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT r.id FROM resources r
          WHERE r.kind IN ('past_paper','trial_test','exam_solution')
            AND NOT EXISTS (SELECT 1 FROM questions q WHERE q.resource_id = r.id)
          ORDER BY r.id",
    )?;
    let rows = stmt.query_map([], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

const SELECT: &str = "SELECT q.id, q.resource_id, r.title, q.subject_id, s.name, q.label,
                             q.words, q.text, q.page, r.origin_path
                        FROM questions q
                        JOIN resources r ON r.id = q.resource_id
                        LEFT JOIN subjects s ON s.id = q.subject_id";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Question> {
    Ok(Question {
        id: r.get(0)?,
        resource_id: r.get(1)?,
        resource_title: r.get(2)?,
        subject_id: r.get(3)?,
        subject_name: r.get(4)?,
        label: r.get(5)?,
        words: r.get(6)?,
        text: r.get(7)?,
        page: r.get(8)?,
        // Checked here rather than trusted: `origin_path` records where a file
        // was, and a paper you imported and then moved has text and no file.
        has_file: r
            .get::<_, Option<String>>(9)?
            .is_some_and(|p| std::path::Path::new(&p).is_file()),
        tags: Vec::new(),
        paper: paper_meta(&r.get::<_, String>(2)?),
    })
}

/// Search. Empty query with a tag filter lists that tag.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filters {
    pub subject_id: Option<i64>,
    pub tag: Option<String>,
    /// Inclusive. A paper whose title carries no year is excluded once either
    /// bound is set — "2014 to 2025" should not quietly include undated ones.
    pub from_year: Option<i64>,
    pub to_year: Option<i64>,
    pub source: Option<String>,
    /// Whether to include questions that came out of a solutions document.
    /// Off by default: those are answers, and searching for a topic should
    /// return the questions on it.
    #[serde(default)]
    pub include_solutions: bool,
}

pub fn search(
    conn: &Connection,
    query: &str,
    filters: &Filters,
    limit: i64,
) -> Result<Vec<Question>> {
    let subject_id = filters.subject_id;
    let tag = filters.tag.as_deref();
    let terms = crate::resources::to_match_query(query);

    let mut sql = String::from(SELECT);
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if terms.is_some() {
        sql.push_str(" JOIN questions_fts f ON f.rowid = q.id");
    }
    if tag.is_some() {
        sql.push_str(" JOIN question_tags t ON t.question_id = q.id");
    }
    sql.push_str(" WHERE 1 = 1");

    if let Some(t) = terms.as_deref() {
        sql.push_str(" AND questions_fts MATCH ?");
        binds.push(Box::new(t.to_string()));
    }
    if let Some(t) = tag {
        sql.push_str(" AND t.tag = ?");
        binds.push(Box::new(t.to_string()));
    }
    if let Some(s) = subject_id {
        sql.push_str(" AND q.subject_id = ?");
        binds.push(Box::new(s));
    }

    // Relevance when there's a query, newest-paper-first when there isn't —
    // ordering by bm25 without a MATCH is an error, not just meaningless.
    sql.push_str(if terms.is_some() {
        " ORDER BY bm25(questions_fts) LIMIT ?"
    } else {
        " ORDER BY q.resource_id DESC, q.ordinal LIMIT ?"
    });
    // Over-fetch, because year and source are read from the title in Rust
    // rather than stored — filtering them in SQL would mean a fourth column
    // that has to be kept in step with the parser.
    let needs_post_filter = filters.from_year.is_some()
        || filters.to_year.is_some()
        || filters.source.is_some()
        || !filters.include_solutions;
    binds.push(Box::new(if needs_post_filter { limit * 6 } else { limit }));

    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
    let mut found: Vec<Question> =
        stmt.query_map(params.as_slice(), row)?.collect::<Result<Vec<_>, _>>()?;

    found.retain(|q| {
        if !filters.include_solutions && q.paper.is_solutions {
            return false;
        }
        if let Some(want) = filters.source.as_deref() {
            if q.paper.source.as_deref() != Some(want) {
                return false;
            }
        }
        if filters.from_year.is_some() || filters.to_year.is_some() {
            let Some(year) = q.paper.year else {
                return false;
            };
            if filters.from_year.is_some_and(|f| year < f) {
                return false;
            }
            if filters.to_year.is_some_and(|t| year > t) {
                return false;
            }
        }
        true
    });
    found.truncate(limit as usize);

    // Tags in a second pass. One query per question would be a hundred queries
    // for a screen of results.
    for q in &mut found {
        let mut tags = conn.prepare("SELECT tag FROM question_tags WHERE question_id = ?1 ORDER BY tag")?;
        q.tags = tags.query_map([q.id], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
    }

    Ok(found)
}

/// Every tag in use, most-used first.
pub fn all_tags(conn: &Connection, subject_id: Option<i64>) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT t.tag, COUNT(*) FROM question_tags t
           JOIN questions q ON q.id = t.question_id
          WHERE (?1 IS NULL OR q.subject_id = ?1)
          GROUP BY t.tag ORDER BY COUNT(*) DESC, t.tag LIMIT 60",
    )?;
    let rows = stmt
        .query_map([subject_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn add_tag(conn: &Connection, question_id: i64, tag: &str) -> Result<()> {
    let clean = tag.trim().to_lowercase();
    if clean.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT OR REPLACE INTO question_tags (question_id, tag, source) VALUES (?1, ?2, 'manual')",
        rusqlite::params![question_id, clean],
    )?;
    Ok(())
}

pub fn remove_tag(conn: &Connection, question_id: i64, tag: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM question_tags WHERE question_id = ?1 AND tag = ?2",
        rusqlite::params![question_id, tag.trim().to_lowercase()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// What a paper is
// ---------------------------------------------------------------------------

/// The facts a paper's filename actually carries.
///
/// Every title in the real library follows the same shape — year, then who
/// wrote it, then which paper: `2018 kilbaha exam 1 solutions`, `2016 TSSM
/// Unit 4 Key Topic Test 1`, `2024 vcaa nht solutions`. None of it is stored
/// anywhere, so filtering by year meant reading a thousand titles by eye.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PaperMeta {
    pub year: Option<i64>,
    /// VCAA, Neap, TSSM, Kilbaha… Lowercased, because the titles aren't
    /// consistent about capitals and you'd otherwise get two "Neap" filters.
    pub source: Option<String>,
    /// Whether this document holds the answers.
    pub is_solutions: bool,
}

/// Publishers seen in the real library, plus VCAA itself.
///
/// A fixed list rather than "the second word", because titles like `2015
/// engage a exam 1` have a section letter where a publisher's second word
/// would be, and `2016 vcaa` has nothing after it at all.
const SOURCES: [&str; 14] = [
    "vcaa", "neap", "tssm", "kilbaha", "insight", "heffernan", "itute", "lisachem", "engage",
    "prime", "access", "legac", "stav", "compak",
];

pub fn paper_meta(title: &str) -> PaperMeta {
    let lower = title.to_lowercase();

    // A four-digit number in the plausible range. Taken from anywhere in the
    // title rather than only the start — `Unit 4 2016 exam` happens.
    let year = lower
        .split(|c: char| !c.is_ascii_digit())
        .filter(|w| w.len() == 4)
        .filter_map(|w| w.parse::<i64>().ok())
        .find(|y| (1990..=2100).contains(y));

    let source = SOURCES
        .iter()
        .find(|s| {
            lower
                .match_indices(*s)
                .any(|(at, _)| bounded_word(&lower, at, s.len()))
        })
        .map(|s| s.to_string());

    PaperMeta {
        year,
        source,
        // "report" is VCAA's examiner's report, which is answers with
        // commentary — the same thing for the purpose of "show me the answer".
        is_solutions: ["solution", "answer", "report"]
            .iter()
            .any(|w| lower.contains(w)),
    }
}

/// Whether a match sits on word boundaries. `access` must not match `accessed`.
fn bounded_word(haystack: &str, at: usize, len: usize) -> bool {
    let before = at == 0
        || !haystack[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric());
    let after = at + len >= haystack.len()
        || !haystack[at + len..]
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric());
    before && after
}

/// The solutions document for a paper, if one is in the library.
///
/// Matched on the paper's own title with the solution words stripped, so
/// `2018 kilbaha exam 1` finds `2018 kilbaha exam 1 solutions`. Deliberately
/// exact rather than fuzzy: pairing the wrong solutions to a question is worse
/// than pairing none, because you would revise from the answer to a different
/// question and never notice.
pub fn solutions_for(conn: &Connection, resource_id: i64) -> Result<Option<(i64, String)>> {
    let (title, subject_id): (String, Option<i64>) = conn.query_row(
        "SELECT title, subject_id FROM resources WHERE id = ?1",
        [resource_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let stem = title.to_lowercase();
    if paper_meta(&title).is_solutions {
        return Ok(None); // this *is* the solutions
    }

    let mut stmt = conn.prepare(
        "SELECT id, title FROM resources
          WHERE id != ?1
            AND (?2 IS NULL OR subject_id IS ?2)
            AND lower(title) LIKE ?3
          ORDER BY length(title) LIMIT 1",
    )?;

    let found = stmt
        .query_row(
            rusqlite::params![resource_id, subject_id, format!("{stem} %")],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    // Only accept it if the longer title is actually the answers.
    Ok(found.filter(|(_, t): &(i64, String)| paper_meta(t).is_solutions))
}

/// Find which page each question in a paper is printed on.
///
/// Separate from indexing, and run on demand, because it opens the PDF once
/// per question and a thousand papers of that is minutes rather than seconds.
/// Indexing gives you searchable questions immediately; pictures come after.
#[cfg(target_os = "macos")]
pub fn locate_pages(conn: &Connection, resource_id: i64) -> Result<usize> {
    let origin: Option<String> = conn.query_row(
        "SELECT origin_path FROM resources WHERE id = ?1",
        [resource_id],
        |r| r.get(0),
    )?;

    let Some(path) = origin.filter(|p| std::path::Path::new(p).is_file()) else {
        return Ok(0);
    };
    let pdf = std::path::Path::new(&path);

    let mut stmt = conn.prepare(
        "SELECT id, text FROM questions WHERE resource_id = ?1 AND page IS NULL",
    )?;
    let pending: Vec<(i64, String)> = stmt
        .query_map([resource_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    let mut found = 0;
    for (id, text) in pending {
        // A question whose words aren't in the PDF is left NULL rather than
        // guessed at — a picture of the wrong page is worse than none.
        if let Ok(Some(page)) = crate::pdfpage::find_page(pdf, &text) {
            conn.execute(
                "UPDATE questions SET page = ?2 WHERE id = ?1",
                rusqlite::params![id, page as i64],
            )?;
            found += 1;
        }
    }
    Ok(found)
}

/// Papers that have questions but no located pages yet.
pub fn unlocated(conn: &Connection) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT q.resource_id FROM questions q
           JOIN resources r ON r.id = q.resource_id
          WHERE q.page IS NULL AND r.origin_path IS NOT NULL
          ORDER BY q.resource_id",
    )?;
    let rows = stmt.query_map([], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// The PDF path and page for one question, if both are known.
pub fn page_source(conn: &Connection, question_id: i64) -> Result<Option<(String, i64)>> {
    let found: Option<(Option<String>, Option<i64>)> = conn
        .query_row(
            "SELECT r.origin_path, q.page FROM questions q
               JOIN resources r ON r.id = q.resource_id
              WHERE q.id = ?1",
            [question_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    Ok(match found {
        Some((Some(path), Some(page))) if std::path::Path::new(&path).is_file() => {
            Some((path, page))
        }
        _ => None,
    })
}

#[cfg(test)]
#[path = "questions/tests.rs"]
mod tests;
