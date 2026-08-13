//! Quick capture: parsing a scrap of text into a task.
//!
//! The target from the brief is `"chem prac report fri"` → a task titled
//! "prac report", subject Chemistry, due this Friday.
//!
//! Everything here is **offline and deterministic**. No API key, no network, no
//! model. Capture has to work in a classroom with no wifi in under three
//! seconds, and a parser that sometimes phones home cannot promise that.
//!
//! Parsing is deliberately conservative: a phrase it doesn't recognise is left
//! in the title rather than guessed at. A wrong due date filed silently is worse
//! than no due date, because you stop trusting the inbox.

use chrono::{Datelike, NaiveDate, Weekday};
use serde::Serialize;

use crate::util::retain_today_naive;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedCapture {
    /// What's left after removing the bits we understood.
    pub title: String,
    pub subject_id: Option<i64>,
    pub subject_name: Option<String>,
    pub due_on: Option<String>,
    /// The exact words consumed, so the UI can show what it took and let the
    /// user reject an interpretation rather than silently accepting it.
    pub matched: Vec<String>,
}

/// A subject the parser can recognise.
pub struct SubjectHint {
    pub id: i64,
    pub name: String,
}

fn weekday_from(token: &str) -> Option<Weekday> {
    Some(match token {
        "mon" | "monday" => Weekday::Mon,
        "tue" | "tues" | "tuesday" => Weekday::Tue,
        "wed" | "weds" | "wednesday" => Weekday::Wed,
        "thu" | "thur" | "thurs" | "thursday" => Weekday::Thu,
        "fri" | "friday" => Weekday::Fri,
        "sat" | "saturday" => Weekday::Sat,
        "sun" | "sunday" => Weekday::Sun,
        _ => return None,
    })
}

fn month_from(token: &str) -> Option<u32> {
    Some(match token {
        "jan" | "january" => 1,
        "feb" | "february" => 2,
        "mar" | "march" => 3,
        "apr" | "april" => 4,
        "may" => 5,
        "jun" | "june" => 6,
        "jul" | "july" => 7,
        "aug" | "august" => 8,
        "sep" | "sept" | "september" => 9,
        "oct" | "october" => 10,
        "nov" | "november" => 11,
        "dec" | "december" => 12,
        _ => return None,
    })
}

/// The next occurrence of a weekday, counting today.
///
/// Saying "fri" on a Friday means today, not a week away — that's how people
/// speak, and the alternative silently pushes same-day work out by a week.
fn next_weekday(from: NaiveDate, target: Weekday) -> NaiveDate {
    let delta = (target.num_days_from_monday() as i64
        - from.weekday().num_days_from_monday() as i64
        + 7)
        % 7;
    from + chrono::Duration::days(delta)
}

/// Strip trailing punctuation so "friday," still matches.
fn clean(token: &str) -> String {
    token
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '/')
        .to_lowercase()
}

/// Parse a captured line.
///
/// `today` is passed in rather than read from the clock so the parser is
/// deterministic and testable.
pub fn parse(text: &str, subjects: &[SubjectHint], today: NaiveDate) -> ParsedCapture {
    let raw: Vec<&str> = text.split_whitespace().collect();
    let lower: Vec<String> = raw.iter().map(|t| clean(t)).collect();

    let mut consumed = vec![false; raw.len()];
    let mut matched = Vec::new();
    let mut due: Option<NaiveDate> = None;
    let mut subject: Option<(i64, String)> = None;

    // --- subject -----------------------------------------------------------
    //
    // A token matches a subject when it is a prefix of at least three
    // characters ("chem" → Chemistry, "bio" → Biology) or when it appears as a
    // whole word inside the name ("methods" → Maths Methods). Three characters
    // is the floor because two-letter prefixes collide constantly.
    for (i, token) in lower.iter().enumerate() {
        if token.len() < 3 || consumed[i] {
            continue;
        }
        for hint in subjects {
            let name = hint.name.to_lowercase();
            let is_prefix = name.starts_with(token.as_str());
            let is_word = name.split_whitespace().any(|w| w == token);
            if is_prefix || is_word {
                subject = Some((hint.id, hint.name.clone()));
                consumed[i] = true;
                matched.push(raw[i].to_string());
                break;
            }
        }
        if subject.is_some() {
            break;
        }
    }

    // --- date --------------------------------------------------------------
    let mut i = 0;
    while i < lower.len() {
        if consumed[i] || due.is_some() {
            i += 1;
            continue;
        }
        let token = &lower[i];

        // Multi-word forms first, so "next week" isn't read as bare "week".
        if token == "next" && i + 1 < lower.len() {
            let after = &lower[i + 1];
            if let Some(w) = weekday_from(after) {
                // "next friday" means the Friday of next week, not the coming
                // one — otherwise it's indistinguishable from plain "friday".
                // `next_weekday` counts today, so a week past it is also right
                // when today *is* the target weekday.
                due = Some(next_weekday(today, w) + chrono::Duration::days(7));
                consumed[i] = true;
                consumed[i + 1] = true;
                matched.push(format!("{} {}", raw[i], raw[i + 1]));
                i += 2;
                continue;
            }
            if after == "week" {
                due = Some(today + chrono::Duration::days(7));
                consumed[i] = true;
                consumed[i + 1] = true;
                matched.push(format!("{} {}", raw[i], raw[i + 1]));
                i += 2;
                continue;
            }
        }

        // "in 3 days" / "in 2 weeks"
        if token == "in" && i + 2 < lower.len() {
            if let Ok(n) = lower[i + 1].parse::<i64>() {
                let unit = &lower[i + 2];
                let days = if unit.starts_with("day") {
                    Some(n)
                } else if unit.starts_with("week") {
                    Some(n * 7)
                } else {
                    None
                };
                if let Some(d) = days {
                    due = Some(today + chrono::Duration::days(d));
                    consumed[i..=i + 2].fill(true);
                    matched.push(format!("{} {} {}", raw[i], raw[i + 1], raw[i + 2]));
                    i += 3;
                    continue;
                }
            }
        }

        // "25 dec" / "dec 25"
        if i + 1 < lower.len() {
            let pair = (lower[i].parse::<u32>().ok(), month_from(&lower[i + 1]));
            let flipped = (month_from(&lower[i]), lower[i + 1].parse::<u32>().ok());
            let (day, month) = match (pair, flipped) {
                ((Some(d), Some(m)), _) => (Some(d), Some(m)),
                (_, (Some(m), Some(d))) => (Some(d), Some(m)),
                _ => (None, None),
            };
            if let (Some(d), Some(m)) = (day, month) {
                if let Some(date) = resolve_day_month(today, d, m) {
                    due = Some(date);
                    consumed[i] = true;
                    consumed[i + 1] = true;
                    matched.push(format!("{} {}", raw[i], raw[i + 1]));
                    i += 2;
                    continue;
                }
            }
        }

        // Single tokens.
        let single = match token.as_str() {
            "today" | "tonight" => Some(today),
            "tomorrow" | "tmr" | "tmrw" | "tom" => Some(today + chrono::Duration::days(1)),
            _ => weekday_from(token).map(|w| next_weekday(today, w)),
        };
        if let Some(d) = single {
            due = Some(d);
            consumed[i] = true;
            matched.push(raw[i].to_string());
            i += 1;
            continue;
        }

        // Numeric dates. Australian order: day first.
        if token.contains('/') {
            if let Some(d) = parse_slash_date(token, today) {
                due = Some(d);
                consumed[i] = true;
                matched.push(raw[i].to_string());
                i += 1;
                continue;
            }
        }

        i += 1;
    }

    let title = raw
        .iter()
        .enumerate()
        .filter(|(i, _)| !consumed[*i])
        .map(|(_, t)| *t)
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();

    ParsedCapture {
        title,
        subject_id: subject.as_ref().map(|(id, _)| *id),
        subject_name: subject.map(|(_, n)| n),
        due_on: due.map(|d| d.format("%Y-%m-%d").to_string()),
        matched,
    }
}

/// A bare day+month with no year means the next time that date occurs.
fn resolve_day_month(today: NaiveDate, day: u32, month: u32) -> Option<NaiveDate> {
    let this_year = NaiveDate::from_ymd_opt(today.year(), month, day)?;
    if this_year >= today {
        Some(this_year)
    } else {
        NaiveDate::from_ymd_opt(today.year() + 1, month, day)
    }
}

/// `d/m` or `d/m/y`, Australian order.
fn parse_slash_date(token: &str, today: NaiveDate) -> Option<NaiveDate> {
    let parts: Vec<&str> = token.split('/').collect();
    let day: u32 = parts.first()?.parse().ok()?;
    let month: u32 = parts.get(1)?.parse().ok()?;

    match parts.get(2) {
        None => resolve_day_month(today, day, month),
        Some(y) => {
            let year: i32 = y.parse().ok()?;
            // Two-digit years are this century.
            let year = if year < 100 { 2000 + year } else { year };
            NaiveDate::from_ymd_opt(year, month, day)
        }
    }
}

/// Parse using today's Retain day.
pub fn parse_now(text: &str, subjects: &[SubjectHint]) -> ParsedCapture {
    parse(text, subjects, retain_today_naive())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subjects() -> Vec<SubjectHint> {
        vec![
            SubjectHint { id: 1, name: "Biology".into() },
            SubjectHint { id: 2, name: "Chemistry".into() },
            SubjectHint { id: 3, name: "Maths Methods".into() },
            SubjectHint { id: 4, name: "English".into() },
        ]
    }

    /// A Wednesday, so weekday maths is unambiguous in both directions.
    fn wednesday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 12).unwrap()
    }

    /// The brief's own example.
    #[test]
    fn the_brief_example_parses() {
        let out = parse("chem prac report fri", &subjects(), wednesday());
        assert_eq!(out.title, "prac report");
        assert_eq!(out.subject_id, Some(2));
        assert_eq!(out.due_on.as_deref(), Some("2026-08-14")); // that Friday
    }

    #[test]
    fn recognises_subject_prefixes_and_whole_words() {
        assert_eq!(parse("bio essay", &subjects(), wednesday()).subject_id, Some(1));
        assert_eq!(parse("methods sac", &subjects(), wednesday()).subject_id, Some(3));
        assert_eq!(parse("english oral", &subjects(), wednesday()).subject_id, Some(4));
    }

    /// Two-letter tokens must not match — too many false positives.
    #[test]
    fn very_short_tokens_do_not_match_a_subject() {
        let out = parse("do ch homework", &subjects(), wednesday());
        assert_eq!(out.subject_id, None);
        assert_eq!(out.title, "do ch homework");
    }

    #[test]
    fn today_and_tomorrow() {
        assert_eq!(parse("thing today", &subjects(), wednesday()).due_on.as_deref(), Some("2026-08-12"));
        assert_eq!(parse("thing tomorrow", &subjects(), wednesday()).due_on.as_deref(), Some("2026-08-13"));
        assert_eq!(parse("thing tmr", &subjects(), wednesday()).due_on.as_deref(), Some("2026-08-13"));
    }

    /// Naming today's weekday means today, not next week.
    #[test]
    fn a_weekday_that_is_today_means_today() {
        let out = parse("thing wed", &subjects(), wednesday());
        assert_eq!(out.due_on.as_deref(), Some("2026-08-12"));
    }

    #[test]
    fn a_past_weekday_rolls_forward() {
        // Monday already went; "mon" means next Monday.
        let out = parse("thing mon", &subjects(), wednesday());
        assert_eq!(out.due_on.as_deref(), Some("2026-08-17"));
    }

    #[test]
    fn next_weekday_is_a_week_further_than_the_bare_weekday() {
        let bare = parse("x fri", &subjects(), wednesday()).due_on.unwrap();
        let next = parse("x next fri", &subjects(), wednesday()).due_on.unwrap();
        assert_eq!(bare, "2026-08-14");
        assert_eq!(next, "2026-08-21");
    }

    /// On the target weekday itself, the bare form means today and the "next"
    /// form means a week out — the case where the two readings diverge most.
    #[test]
    fn on_the_day_itself_bare_means_today_and_next_means_a_week_out() {
        let friday = NaiveDate::from_ymd_opt(2026, 8, 14).unwrap();
        assert_eq!(parse("x fri", &subjects(), friday).due_on.as_deref(), Some("2026-08-14"));
        assert_eq!(parse("x next fri", &subjects(), friday).due_on.as_deref(), Some("2026-08-21"));
    }

    #[test]
    fn next_week_and_relative_offsets() {
        assert_eq!(parse("x next week", &subjects(), wednesday()).due_on.as_deref(), Some("2026-08-19"));
        assert_eq!(parse("x in 3 days", &subjects(), wednesday()).due_on.as_deref(), Some("2026-08-15"));
        assert_eq!(parse("x in 2 weeks", &subjects(), wednesday()).due_on.as_deref(), Some("2026-08-26"));
    }

    #[test]
    fn month_names_in_either_order() {
        assert_eq!(parse("x 25 dec", &subjects(), wednesday()).due_on.as_deref(), Some("2026-12-25"));
        assert_eq!(parse("x dec 25", &subjects(), wednesday()).due_on.as_deref(), Some("2026-12-25"));
    }

    /// A day/month already past means next year, not a date in the past.
    #[test]
    fn a_past_day_month_rolls_to_next_year() {
        assert_eq!(parse("x 3 jan", &subjects(), wednesday()).due_on.as_deref(), Some("2027-01-03"));
    }

    #[test]
    fn slash_dates_are_day_first() {
        // 5/9 must be 5 September, not 9 May — Australian order.
        assert_eq!(parse("x 5/9", &subjects(), wednesday()).due_on.as_deref(), Some("2026-09-05"));
        assert_eq!(parse("x 25/12/26", &subjects(), wednesday()).due_on.as_deref(), Some("2026-12-25"));
        assert_eq!(parse("x 1/2/2027", &subjects(), wednesday()).due_on.as_deref(), Some("2027-02-01"));
    }

    #[test]
    fn trailing_punctuation_does_not_block_a_match() {
        let out = parse("chem, prac report friday.", &subjects(), wednesday());
        assert_eq!(out.subject_id, Some(2));
        assert_eq!(out.due_on.as_deref(), Some("2026-08-14"));
    }

    /// Anything unrecognised stays in the title rather than being guessed at.
    #[test]
    fn unrecognised_text_is_left_alone() {
        let out = parse("ask mr smith about the thing", &subjects(), wednesday());
        assert_eq!(out.title, "ask mr smith about the thing");
        assert_eq!(out.subject_id, None);
        assert_eq!(out.due_on, None);
        assert!(out.matched.is_empty());
    }

    /// The words consumed are reported so the UI can show its working.
    #[test]
    fn matched_tokens_are_reported() {
        let out = parse("chem prac fri", &subjects(), wednesday());
        assert!(out.matched.iter().any(|m| m == "chem"));
        assert!(out.matched.iter().any(|m| m == "fri"));
    }

    #[test]
    fn only_the_first_date_wins() {
        let out = parse("x fri mon", &subjects(), wednesday());
        assert_eq!(out.due_on.as_deref(), Some("2026-08-14"));
        assert_eq!(out.title, "x mon", "the second date stays as text");
    }

    #[test]
    fn empty_and_whitespace_input_is_safe() {
        let out = parse("   ", &subjects(), wednesday());
        assert_eq!(out.title, "");
        assert_eq!(out.due_on, None);
    }

    #[test]
    fn an_invalid_slash_date_is_left_as_text() {
        let out = parse("x 99/99", &subjects(), wednesday());
        assert_eq!(out.due_on, None);
        assert!(out.title.contains("99/99"));
    }

    #[test]
    fn leap_day_resolves_or_rolls_without_panicking() {
        let out = parse("x 29 feb", &subjects(), wednesday());
        // 2026 and 2027 are not leap years, so this must simply not resolve
        // rather than panic or fabricate a date.
        assert!(out.due_on.is_none() || out.due_on.as_deref() == Some("2028-02-29"));
    }
}
