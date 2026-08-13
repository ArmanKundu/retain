//! Paste-to-import, matching Anki's text format so cards move in and out freely.
//!
//! Anki's exported text format is *almost* CSV but not quite:
//!
//!   * The delimiter varies — Tab by default, but Semicolon and Comma appear.
//!   * Files may carry `#separator:`, `#tags:`, `#notetype:` header directives.
//!   * Fields may be quoted, with a literal quote written as `""` inside.
//!   * A cloze note `{{c1::...}} {{c2::...}}` becomes one card per distinct cN.
//!
//! Everything here is pure text → structs, with no database or clock involved,
//! which is what makes it worth testing properly.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

// `Deserialize` as well as `Serialize`: the UI sends this back when the user
// overrides auto-detection from the import dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delimiter {
    Tab,
    Semicolon,
    Comma,
}

impl Delimiter {
    pub fn ch(self) -> char {
        match self {
            Delimiter::Tab => '\t',
            Delimiter::Semicolon => ';',
            Delimiter::Comma => ',',
        }
    }

    fn from_directive(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "tab" | "\\t" => Some(Delimiter::Tab),
            "semicolon" => Some(Delimiter::Semicolon),
            "comma" => Some(Delimiter::Comma),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteType {
    Basic,
    Cloze,
    /// The English card type from the brief: quote → source/context → theme.
    ///
    /// Flashcards suit English poorly because the useful unit isn't a fact, it's
    /// a quotation you can deploy. So the three fields are the quote, where it
    /// comes from and what's happening around it, and the theme it serves.
    Quote,
}

/// One card ready to be inserted. A cloze note expands to several of these.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedCard {
    pub note_type: NoteType,
    pub front: String,
    pub back: String,
    /// For cloze: the original text with all deletions intact, so the card can
    /// be re-rendered or edited later.
    pub extra: Option<String>,
    pub cloze_index: Option<i64>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub delimiter: Delimiter,
    pub cards: Vec<ParsedCard>,
    /// Lines we could not make a card from, with why. Surfaced rather than
    /// silently dropped — a paste that half-worked should say so.
    pub skipped: Vec<SkippedLine>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedLine {
    pub line_number: usize,
    pub text: String,
    pub reason: String,
}

/// Pick the delimiter, preferring Tab, then Semicolon, then Comma.
///
/// The preference order is from the brief and it is the right one: tabs almost
/// never appear inside card text, whereas commas appear constantly in ordinary
/// prose. Choosing by "most frequent character" would shred a deck of sentences
/// containing commas.
///
/// A candidate only wins if it actually splits lines into a consistent number
/// of fields — a lone comma inside one card's text shouldn't make Comma win.
fn detect_delimiter(lines: &[&str]) -> Delimiter {
    let body: Vec<&&str> = lines
        .iter()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .take(20)
        .collect();

    if body.is_empty() {
        return Delimiter::Tab;
    }

    for candidate in [Delimiter::Tab, Delimiter::Semicolon, Delimiter::Comma] {
        let counts: Vec<usize> = body
            .iter()
            .map(|l| split_fields(l, candidate.ch()).len())
            .collect();

        // Every sampled line must yield at least two fields, and they must agree
        // on how many — that's what distinguishes a real delimiter from a
        // character that merely happens to occur.
        let first = counts[0];
        if first >= 2 && counts.iter().all(|c| *c == first) {
            return candidate;
        }
    }

    Delimiter::Tab
}

/// Split one line on `delim`, honouring quoted fields.
///
/// Inside a quoted field, `""` is a literal quote — the CSV convention Anki
/// follows. Everything is `char`-based rather than byte-based so multi-byte
/// UTF-8 (which VCE subjects are full of: β, °, →, μ) survives intact.
fn split_fields(line: &str, delim: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    // Escaped quote: consume the second one, emit one.
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(c);
            }
        } else if c == '"' && current.trim().is_empty() {
            // A quote only opens a field at its start; a stray quote mid-text
            // is just a character.
            current.clear();
            in_quotes = true;
        } else if c == delim {
            fields.push(current.trim().to_string());
            current = String::new();
        } else {
            current.push(c);
        }
    }
    fields.push(current.trim().to_string());
    fields
}

/// Every distinct `N` in `{{cN::...}}`, in ascending order.
///
/// Returns a set, because `{{c1::a}} ... {{c1::b}}` is one card with two
/// deletions revealed together, not two cards.
fn cloze_indices(text: &str) -> BTreeSet<i64> {
    let mut found = BTreeSet::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i + 4 < bytes.len() {
        if bytes[i] == '{' && bytes[i + 1] == '{' && bytes[i + 2] == 'c' {
            let mut j = i + 3;
            let mut digits = String::new();
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                digits.push(bytes[j]);
                j += 1;
            }
            // Must be `{{c<digits>::`
            if !digits.is_empty() && j + 1 < bytes.len() && bytes[j] == ':' && bytes[j + 1] == ':' {
                if let Ok(n) = digits.parse::<i64>() {
                    if n > 0 {
                        found.insert(n);
                    }
                }
            }
        }
        i += 1;
    }

    found
}

/// Parse a pasted block into cards.
///
/// `forced` overrides delimiter auto-detection when the user picks one manually.
pub fn parse(input: &str, forced: Option<Delimiter>) -> ImportPreview {
    // Strip a UTF-8 BOM — Anki exports on Windows often carry one, and left in
    // place it becomes part of the first field's text.
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);

    let raw_lines: Vec<&str> = input.lines().collect();

    // --- header directives -------------------------------------------------
    let mut directive_delim: Option<Delimiter> = None;
    let mut global_tags: Vec<String> = Vec::new();
    let mut directive_notetype: Option<String> = None;

    for line in &raw_lines {
        let Some(rest) = line.strip_prefix('#') else { continue };
        let Some((key, value)) = rest.split_once(':') else { continue };
        match key.trim().to_ascii_lowercase().as_str() {
            "separator" => directive_delim = Delimiter::from_directive(value),
            "tags" => {
                global_tags = value.split_whitespace().map(|t| t.to_string()).collect();
            }
            "notetype" => directive_notetype = Some(value.trim().to_ascii_lowercase()),
            _ => {}
        }
    }

    let delimiter = forced
        .or(directive_delim)
        .unwrap_or_else(|| detect_delimiter(&raw_lines));

    // A `#notetype:` naming cloze forces cloze handling even for a row whose
    // text happens to contain no `{{cN::}}`.
    let notetype_is_cloze = directive_notetype
        .as_deref()
        .is_some_and(|n| n.contains("cloze"));

    let mut cards = Vec::new();
    let mut skipped = Vec::new();

    for (index, line) in raw_lines.iter().enumerate() {
        let line_number = index + 1;

        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }

        let fields = split_fields(line, delimiter.ch());
        let front = fields.first().map(|s| s.as_str()).unwrap_or("").trim();

        if front.is_empty() {
            skipped.push(SkippedLine {
                line_number,
                text: line.trim().to_string(),
                reason: "No text in the first column.".into(),
            });
            continue;
        }

        // Third column, if present, is tags — appended to any global ones.
        let mut tags = global_tags.clone();
        if let Some(t) = fields.get(2) {
            tags.extend(t.split_whitespace().map(|s| s.to_string()));
        }
        tags.sort();
        tags.dedup();

        let indices = cloze_indices(front);

        if !indices.is_empty() {
            // Cloze: one card per distinct cN.
            for n in indices {
                cards.push(ParsedCard {
                    note_type: NoteType::Cloze,
                    front: front.to_string(),
                    back: fields.get(1).cloned().unwrap_or_default(),
                    extra: Some(front.to_string()),
                    cloze_index: Some(n),
                    tags: tags.clone(),
                });
            }
            continue;
        }

        if notetype_is_cloze {
            skipped.push(SkippedLine {
                line_number,
                text: line.trim().to_string(),
                reason: "Declared as a cloze note but contains no {{c1::…}} deletion.".into(),
            });
            continue;
        }

        let back = fields.get(1).cloned().unwrap_or_default();
        if back.trim().is_empty() {
            skipped.push(SkippedLine {
                line_number,
                text: line.trim().to_string(),
                reason: "Only one column — a basic card needs a front and a back.".into(),
            });
            continue;
        }

        cards.push(ParsedCard {
            note_type: NoteType::Basic,
            front: front.to_string(),
            back,
            extra: None,
            cloze_index: None,
            tags,
        });
    }

    ImportPreview {
        delimiter,
        cards,
        skipped,
    }
}

/// Parse a paste as English quote cards.
///
/// Deliberately a separate entry point rather than a flag on `parse`, because
/// the column meanings genuinely differ: in the standard format the third column
/// is whitespace-separated *tags*, whereas here it is a single **theme** that
/// routinely contains spaces ("power and control"). Splitting that on whitespace
/// would turn one theme into three meaningless tags, so the two formats cannot
/// share a parser without silently corrupting one of them.
///
/// Columns: quote → source/context → theme.
pub fn parse_quotes(input: &str, forced: Option<Delimiter>) -> ImportPreview {
    let base = parse(input, forced);
    let delimiter = base.delimiter;

    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut cards = Vec::new();
    let mut skipped = Vec::new();

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }

        let fields = split_fields(line, delimiter.ch());
        let quote = fields.first().map(|s| s.trim()).unwrap_or("");
        let source = fields.get(1).map(|s| s.trim()).unwrap_or("");

        if quote.is_empty() {
            skipped.push(SkippedLine {
                line_number,
                text: line.trim().to_string(),
                reason: "No quote in the first column.".into(),
            });
            continue;
        }
        if source.is_empty() {
            skipped.push(SkippedLine {
                line_number,
                text: line.trim().to_string(),
                reason: "A quote card needs a source or context in the second column.".into(),
            });
            continue;
        }

        cards.push(ParsedCard {
            note_type: NoteType::Quote,
            front: quote.to_string(),
            back: source.to_string(),
            // The whole third column, unsplit — this is a theme, not tags.
            extra: fields.get(2).map(|t| t.trim().to_string()).filter(|t| !t.is_empty()),
            cloze_index: None,
            tags: Vec::new(),
        });
    }

    ImportPreview {
        delimiter,
        cards,
        skipped,
    }
}

/// Stable identity for a card, used to skip duplicates on re-import.
///
/// FNV-1a over the fields that define the card. Not cryptographic — it only has
/// to be stable and well-spread enough that two different cards don't collide.
pub fn content_hash(front: &str, back: &str, cloze_index: Option<i64>) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |s: &str| {
        for byte in s.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    feed(front.trim());
    feed("\u{1}");
    feed(back.trim());
    feed("\u{1}");
    feed(&cloze_index.unwrap_or(0).to_string());
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_tab_over_comma_in_prose() {
        // Both characters appear, but only Tab splits consistently into 2.
        let input = "What is a codon?\tThree bases, coding for one amino acid\n\
                     What is a nucleotide?\tA sugar, a phosphate, and a base";
        let out = parse(input, None);
        assert_eq!(out.delimiter, Delimiter::Tab);
        assert_eq!(out.cards.len(), 2);
        assert_eq!(out.cards[0].back, "Three bases, coding for one amino acid");
    }

    #[test]
    fn falls_back_to_semicolon_then_comma() {
        let semi = parse("a;b\nc;d", None);
        assert_eq!(semi.delimiter, Delimiter::Semicolon);

        let comma = parse("a,b\nc,d", None);
        assert_eq!(comma.delimiter, Delimiter::Comma);
    }

    /// A single comma inside one card's text must not make Comma the delimiter
    /// when the field counts don't agree.
    #[test]
    fn inconsistent_field_counts_do_not_win() {
        let input = "one line with, a comma\nanother line with, two, commas";
        let out = parse(input, None);
        assert_ne!(out.delimiter, Delimiter::Comma);
    }

    #[test]
    fn honours_quoted_fields_with_escaped_quotes() {
        let input = "\"He said \"\"hello\"\" loudly\"\tA greeting";
        let out = parse(input, None);
        assert_eq!(out.cards.len(), 1);
        assert_eq!(out.cards[0].front, "He said \"hello\" loudly");
    }

    #[test]
    fn quoted_field_may_contain_the_delimiter() {
        let input = "\"a, b, c\",back";
        let out = parse(input, Some(Delimiter::Comma));
        assert_eq!(out.cards[0].front, "a, b, c");
        assert_eq!(out.cards[0].back, "back");
    }

    #[test]
    fn cloze_makes_one_card_per_distinct_index() {
        let input = "{{c1::Transcription}} happens in the {{c2::nucleus}}\t\tbio genetics";
        let out = parse(input, None);
        assert_eq!(out.cards.len(), 2);
        assert_eq!(out.cards[0].cloze_index, Some(1));
        assert_eq!(out.cards[1].cloze_index, Some(2));
        assert_eq!(out.cards[0].note_type, NoteType::Cloze);
        // Tags from the third column land on every generated card.
        assert_eq!(out.cards[0].tags, vec!["bio", "genetics"]);
    }

    /// Two deletions with the SAME index are one card, not two.
    #[test]
    fn repeated_cloze_index_is_one_card() {
        let out = parse("{{c1::A}} and {{c1::B}}\t", None);
        assert_eq!(out.cards.len(), 1);
        assert_eq!(out.cards[0].cloze_index, Some(1));
    }

    #[test]
    fn non_sequential_cloze_indices_are_preserved() {
        let out = parse("{{c1::a}} {{c5::b}}\t", None);
        assert_eq!(out.cards.len(), 2);
        assert_eq!(out.cards[0].cloze_index, Some(1));
        assert_eq!(out.cards[1].cloze_index, Some(5));
    }

    #[test]
    fn header_directives_are_honoured() {
        let input = "#separator:Semicolon\n#tags:biology unit3\n\
                     What is ATP?;Adenosine triphosphate";
        let out = parse(input, None);
        assert_eq!(out.delimiter, Delimiter::Semicolon);
        assert_eq!(out.cards.len(), 1);
        assert_eq!(out.cards[0].tags, vec!["biology", "unit3"]);
    }

    #[test]
    fn manual_override_beats_directive_and_detection() {
        let input = "#separator:Tab\na,b";
        let out = parse(input, Some(Delimiter::Comma));
        assert_eq!(out.delimiter, Delimiter::Comma);
        assert_eq!(out.cards[0].front, "a");
    }

    #[test]
    fn one_column_rows_are_reported_not_silently_dropped() {
        let out = parse("just a front with no back", None);
        assert_eq!(out.cards.len(), 0);
        assert_eq!(out.skipped.len(), 1);
        assert!(out.skipped[0].reason.contains("front and a back"));
    }

    #[test]
    fn utf8_survives_intact() {
        let out = parse("β-galactosidase\tAn enzyme — cleaves lactose (≈540 kDa)", None);
        assert_eq!(out.cards[0].front, "β-galactosidase");
        assert!(out.cards[0].back.contains("≈540 kDa"));
    }

    #[test]
    fn strips_utf8_bom() {
        let out = parse("\u{feff}front\tback", None);
        assert_eq!(out.cards[0].front, "front");
    }

    #[test]
    fn blank_lines_and_comments_are_ignored_without_being_flagged() {
        let out = parse("a\tb\n\n#notetype:Basic\n\nc\td", None);
        assert_eq!(out.cards.len(), 2);
        assert_eq!(out.skipped.len(), 0);
    }

    // -- English quote cards ----------------------------------------------

    #[test]
    fn quote_mode_keeps_a_multi_word_theme_intact() {
        let out = parse_quotes(
            "\"I am fire and air\"\tCleopatra, Act V sc ii\tpower and transcendence",
            None,
        );
        assert_eq!(out.cards.len(), 1);
        let c = &out.cards[0];
        assert_eq!(c.note_type, NoteType::Quote);
        assert_eq!(c.front, "I am fire and air");
        assert_eq!(c.back, "Cleopatra, Act V sc ii");
        // The whole theme, NOT split into ["power","and","transcendence"].
        assert_eq!(c.extra.as_deref(), Some("power and transcendence"));
        assert!(c.tags.is_empty(), "quote mode has no tag column");
    }

    #[test]
    fn quote_without_a_source_is_reported_not_dropped() {
        let out = parse_quotes("a quote with no source", None);
        assert!(out.cards.is_empty());
        assert_eq!(out.skipped.len(), 1);
        assert!(out.skipped[0].reason.contains("source or context"));
    }

    #[test]
    fn quote_theme_is_optional() {
        let out = parse_quotes("quote\tsource", None);
        assert_eq!(out.cards.len(), 1);
        assert_eq!(out.cards[0].extra, None);
    }

    /// The same paste read as basic vs quote must differ — proving the two
    /// parsers really are distinct and the third column is treated differently.
    #[test]
    fn quote_mode_differs_from_basic_mode() {
        let text = "quote\tsource\ttheme with spaces";
        let basic = parse(text, None);
        let quotes = parse_quotes(text, None);

        assert_eq!(basic.cards[0].tags, vec!["spaces", "theme", "with"]);
        assert_eq!(quotes.cards[0].extra.as_deref(), Some("theme with spaces"));
    }

    #[test]
    fn content_hash_is_stable_and_discriminating() {
        let a = content_hash("front", "back", None);
        assert_eq!(a, content_hash("  front  ", "back", None), "must ignore surrounding space");
        assert_ne!(a, content_hash("front", "other", None));
        assert_ne!(a, content_hash("front", "back", Some(1)));
        assert_ne!(
            content_hash("front", "back", Some(1)),
            content_hash("front", "back", Some(2)),
            "cloze cards from one note must not collide"
        );
    }
}
