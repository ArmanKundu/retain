//! Calendar subscription: fetch an ICS URL, parse it, store the events.
//!
//! ## What this is and isn't
//!
//! This reads a **published ICS subscription URL** — the one Compass gives you
//! under its calendar feed settings. That is the whole integration. There is no
//! login, no scraping, no unofficial API, and no password or session handling
//! anywhere in this file or reachable from it. If Compass ever stops publishing
//! ICS, this feature stops working, and that is the correct outcome.
//!
//! ## Why the parser is hand-written
//!
//! ICS is a simple line format, but the parts that actually matter for a school
//! timetable — VTIMEZONE, recurring events across a DST boundary, and instances
//! of a series that were individually moved — are exactly the parts a generic
//! parser tends to get wrong or leave to the caller. Doing it here means the
//! timezone rules are visible and testable rather than implied.
//!
//! ## The timezone rule that matters most
//!
//! A recurring event is expanded **in its own local timezone**, and only then
//! converted to UTC. Melbourne moves its clocks twice a year; a Wednesday 9am
//! class must stay 9am on both sides of that. Expanding in UTC and adding
//! 7-day offsets would silently shift every occurrence by an hour for half the
//! year, which is the sort of bug you only notice by turning up late.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::util;

/// How far ahead a recurring series is expanded. A school timetable repeats
/// indefinitely; storing it forever would be pointless, and the app only ever
/// looks a few weeks out.
const HORIZON_DAYS: i64 = 400;

/// Hard ceiling on occurrences from one rule, so a malformed or hostile RRULE
/// can't spin forever.
const MAX_OCCURRENCES: usize = 1500;

const FETCH_TIMEOUT_SECS: u64 = 25;

/// Roughly 8 MB. A school calendar is a few hundred KB; anything far past that
/// is a misconfigured URL, and we'd rather fail than buffer it.
const MAX_BYTES: usize = 8 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Line unfolding and property parsing
// ---------------------------------------------------------------------------

/// An ICS content line: name, parameters, value.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub value: String,
}

impl Line {
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
}

/// Undo RFC 5545 line folding.
///
/// Long lines are split with CRLF followed by a single space or tab. The
/// continuation character is part of the folding, not the content, so it is
/// dropped rather than kept as whitespace.
pub fn unfold(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    for line in raw.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);

        if let Some(rest) = line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')) {
            if let Some(last) = out.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        out.push(line.to_string());
    }

    out.retain(|l| !l.trim().is_empty());
    out
}

/// Split `NAME;PARAM=value;PARAM2="quoted:value":the value` into its parts.
pub fn parse_line(line: &str) -> Option<Line> {
    // Find the colon that ends the name+params, skipping any inside quotes.
    let mut in_quotes = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let (head, value) = line.split_at(colon);
    let value = &value[1..];

    let mut parts = split_unquoted(head, ';');
    let name = parts.remove(0).trim().to_uppercase();
    if name.is_empty() {
        return None;
    }

    let params = parts
        .into_iter()
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            Some((
                k.trim().to_uppercase(),
                v.trim().trim_matches('"').to_string(),
            ))
        })
        .collect();

    Some(Line {
        name,
        params,
        value: value.to_string(),
    })
}

fn split_unquoted(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;

    for c in s.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                cur.push(c);
            }
            _ if c == sep && !in_quotes => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Unescape an ICS TEXT value: `\n`, `\,`, `\;`, `\\`.
pub fn unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();

    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(',') => out.push(','),
            Some(';') => out.push(';'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Date and time
// ---------------------------------------------------------------------------

/// A parsed DTSTART/DTEND, before it becomes an absolute instant.
///
/// The three cases are genuinely different and collapsing them loses meaning:
/// a date-only value is an all-day event, a `Z` value is a fixed instant, and a
/// TZID value is a wall-clock time in a named zone that has to survive DST.
#[derive(Debug, Clone, PartialEq)]
pub enum IcsTime {
    /// `VALUE=DATE` — an all-day event. No time, no zone.
    Date(NaiveDate),
    /// Trailing `Z` — already absolute.
    Utc(DateTime<Utc>),
    /// `TZID=...`, or floating with no zone at all (treated as local).
    Zoned { naive: NaiveDateTime, tz: Option<Tz> },
}

impl IcsTime {
    /// The absolute instant this represents.
    ///
    /// All-day events anchor to midnight in `fallback`, which is the local
    /// zone — an all-day event is a statement about a calendar day, and it
    /// should land on that day for the person reading it.
    pub fn to_utc(&self, fallback: Tz) -> DateTime<Utc> {
        match self {
            IcsTime::Utc(dt) => *dt,
            IcsTime::Date(d) => resolve_local(d.and_time(NaiveTime::MIN), fallback),
            IcsTime::Zoned { naive, tz } => resolve_local(*naive, tz.unwrap_or(fallback)),
        }
    }

    pub fn is_date(&self) -> bool {
        matches!(self, IcsTime::Date(_))
    }

    /// The naive wall-clock time, for expanding a recurrence in local terms.
    pub fn naive(&self, fallback: Tz) -> NaiveDateTime {
        match self {
            IcsTime::Date(d) => d.and_time(NaiveTime::MIN),
            IcsTime::Zoned { naive, .. } => *naive,
            IcsTime::Utc(dt) => dt.with_timezone(&fallback).naive_local(),
        }
    }

    pub fn tz(&self, fallback: Tz) -> Tz {
        match self {
            IcsTime::Zoned { tz, .. } => tz.unwrap_or(fallback),
            _ => fallback,
        }
    }
}

/// Convert a wall-clock time in a zone to an instant, handling the two days a
/// year when that isn't a simple question.
///
/// * **Spring forward** leaves a gap — 2am–3am simply doesn't happen. A time
///   inside it isn't a real instant, so we step forward past the gap rather
///   than failing and losing the event.
/// * **Fall back** repeats an hour, so the same wall clock happens twice. We
///   take the earlier one, which is what calendar clients do.
pub fn resolve_local(naive: NaiveDateTime, tz: Tz) -> DateTime<Utc> {
    use chrono::LocalResult;

    match tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => dt.with_timezone(&Utc),
        LocalResult::Ambiguous(earlier, _later) => earlier.with_timezone(&Utc),
        LocalResult::None => {
            // Walk forward in ten-minute steps until we're past the gap. Gaps
            // are an hour at most in every real zone, so this terminates fast.
            let mut probe = naive;
            for _ in 0..12 {
                probe += Duration::minutes(10);
                if let LocalResult::Single(dt) = tz.from_local_datetime(&probe) {
                    return dt.with_timezone(&Utc);
                }
                if let LocalResult::Ambiguous(dt, _) = tz.from_local_datetime(&probe) {
                    return dt.with_timezone(&Utc);
                }
            }
            // Shouldn't be reachable; treating it as UTC beats dropping it.
            Utc.from_utc_datetime(&naive)
        }
    }
}

/// Parse an ICS date/time value, using the line's own parameters.
pub fn parse_time(line: &Line) -> Option<IcsTime> {
    let v = line.value.trim();

    if line.param("VALUE").map(|p| p.eq_ignore_ascii_case("DATE")) == Some(true) || v.len() == 8 {
        return NaiveDate::parse_from_str(v, "%Y%m%d").ok().map(IcsTime::Date);
    }

    if let Some(stripped) = v.strip_suffix('Z') {
        let naive = NaiveDateTime::parse_from_str(stripped, "%Y%m%dT%H%M%S").ok()?;
        return Some(IcsTime::Utc(Utc.from_utc_datetime(&naive)));
    }

    let naive = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
    Some(IcsTime::Zoned {
        naive,
        tz: line.param("TZID").and_then(resolve_tzid),
    })
}

/// Map a TZID onto an IANA zone.
///
/// Real feeds are inconsistent here: some emit a clean `Australia/Melbourne`,
/// others a Windows name, others a `/mozilla.org/...` prefix. Recognising the
/// common shapes is worth it, because falling back to the local zone is only
/// correct by luck.
pub fn resolve_tzid(tzid: &str) -> Option<Tz> {
    let cleaned = tzid.trim().trim_matches('"');

    if let Ok(tz) = cleaned.parse::<Tz>() {
        return Some(tz);
    }

    // `/mozilla.org/20070129_1/Australia/Melbourne` and similar prefixed forms:
    // walk back from the end for the longest suffix that parses.
    let parts: Vec<&str> = cleaned.split('/').collect();
    for start in 0..parts.len() {
        if let Ok(tz) = parts[start..].join("/").parse::<Tz>() {
            return Some(tz);
        }
    }

    // The Windows names an Outlook-exported feed uses.
    let lower = cleaned.to_lowercase();
    let mapped = if lower.contains("aus eastern") || lower.contains("e. australia") {
        "Australia/Sydney"
    } else if lower.contains("cen. australia") || lower.contains("aus central") {
        "Australia/Adelaide"
    } else if lower.contains("w. australia") {
        "Australia/Perth"
    } else if lower.contains("new zealand") {
        "Pacific/Auckland"
    } else if lower == "utc" || lower == "gmt" {
        "UTC"
    } else {
        return None;
    };

    mapped.parse::<Tz>().ok()
}

// ---------------------------------------------------------------------------
// Recurrence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Rrule {
    pub freq: Freq,
    pub interval: u32,
    pub count: Option<u32>,
    pub until: Option<IcsTime>,
    /// Weekday numbers, Monday = 0.
    pub by_day: Vec<u32>,
    pub by_month_day: Vec<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

pub fn parse_rrule(value: &str) -> Option<Rrule> {
    let mut freq = None;
    let mut interval = 1;
    let mut count = None;
    let mut until = None;
    let mut by_day = Vec::new();
    let mut by_month_day = Vec::new();

    for part in value.split(';') {
        let Some((k, v)) = part.split_once('=') else { continue };
        match k.trim().to_uppercase().as_str() {
            "FREQ" => {
                freq = match v.trim().to_uppercase().as_str() {
                    "DAILY" => Some(Freq::Daily),
                    "WEEKLY" => Some(Freq::Weekly),
                    "MONTHLY" => Some(Freq::Monthly),
                    "YEARLY" => Some(Freq::Yearly),
                    // HOURLY/MINUTELY/SECONDLY don't appear in a school
                    // timetable and expanding them would be a denial of service
                    // on our own database.
                    _ => None,
                };
            }
            "INTERVAL" => interval = v.trim().parse().unwrap_or(1).max(1),
            "COUNT" => count = v.trim().parse().ok(),
            "UNTIL" => {
                until = parse_time(&Line {
                    name: "UNTIL".into(),
                    params: vec![],
                    value: v.trim().to_string(),
                })
            }
            "BYDAY" => {
                by_day = v
                    .split(',')
                    .filter_map(|d| {
                        // Strip an ordinal prefix like `2TU` or `-1FR`; the
                        // weekday is the last two characters.
                        let code = d.trim().trim_end_matches(char::is_whitespace);
                        let code = &code[code.len().saturating_sub(2)..];
                        Some(match code.to_uppercase().as_str() {
                            "MO" => 0,
                            "TU" => 1,
                            "WE" => 2,
                            "TH" => 3,
                            "FR" => 4,
                            "SA" => 5,
                            "SU" => 6,
                            _ => return None,
                        })
                    })
                    .collect();
            }
            "BYMONTHDAY" => {
                by_month_day = v.split(',').filter_map(|d| d.trim().parse().ok()).collect();
            }
            _ => {}
        }
    }

    Some(Rrule {
        freq: freq?,
        interval,
        count,
        until,
        by_day,
        by_month_day,
    })
}

/// Expand a rule into local wall-clock start times.
///
/// Works entirely in naive local time on purpose — see the module docs. The
/// caller converts each result through the event's zone, so an occurrence keeps
/// its wall-clock time across a DST change instead of drifting by an hour.
pub fn expand(
    rule: &Rrule,
    start: NaiveDateTime,
    horizon: NaiveDateTime,
    tz: Tz,
) -> Vec<NaiveDateTime> {
    let mut out = Vec::new();
    let until_naive = rule
        .until
        .as_ref()
        .map(|u| u.to_utc(tz).with_timezone(&tz).naive_local());

    let mut cursor = start;
    let mut emitted = 0usize;
    let mut guard = 0usize;

    while out.len() < MAX_OCCURRENCES && guard < MAX_OCCURRENCES * 8 {
        guard += 1;

        if cursor > horizon {
            break;
        }
        if let Some(u) = until_naive {
            if cursor > u {
                break;
            }
        }
        if let Some(c) = rule.count {
            if emitted >= c as usize {
                break;
            }
        }

        // For WEEKLY with BYDAY, each interval-week yields one occurrence per
        // listed weekday rather than a single one.
        if rule.freq == Freq::Weekly && !rule.by_day.is_empty() {
            let week_start = cursor - Duration::days(cursor.weekday().num_days_from_monday() as i64);
            for &d in &rule.by_day {
                let candidate = week_start + Duration::days(d as i64);
                if candidate < start || candidate > horizon {
                    continue;
                }
                if let Some(u) = until_naive {
                    if candidate > u {
                        continue;
                    }
                }
                if rule.count.is_some_and(|c| emitted >= c as usize) {
                    break;
                }
                if !out.contains(&candidate) {
                    out.push(candidate);
                    emitted += 1;
                }
            }
        } else if !rule.by_month_day.is_empty() && rule.freq == Freq::Monthly {
            for &md in &rule.by_month_day {
                if md < 1 {
                    continue;
                }
                if let Some(c) = NaiveDate::from_ymd_opt(cursor.year(), cursor.month(), md as u32) {
                    let candidate = c.and_time(cursor.time());
                    if candidate >= start && candidate <= horizon && !out.contains(&candidate) {
                        if rule.count.is_some_and(|cc| emitted >= cc as usize) {
                            break;
                        }
                        out.push(candidate);
                        emitted += 1;
                    }
                }
            }
        } else {
            out.push(cursor);
            emitted += 1;
        }

        cursor = match advance(cursor, rule) {
            Some(next) => next,
            None => break,
        };
    }

    out.sort();
    out.dedup();
    out.truncate(MAX_OCCURRENCES);
    out
}

fn advance(from: NaiveDateTime, rule: &Rrule) -> Option<NaiveDateTime> {
    let i = rule.interval as i64;

    Some(match rule.freq {
        Freq::Daily => from + Duration::days(i),
        Freq::Weekly => from + Duration::weeks(i),
        Freq::Monthly => add_months(from, i as i32)?,
        Freq::Yearly => add_months(from, (i * 12) as i32)?,
    })
}

/// Add months, clamping the day so 31 January + 1 month lands on 28/29 February
/// rather than vanishing.
fn add_months(dt: NaiveDateTime, months: i32) -> Option<NaiveDateTime> {
    let total = dt.year() * 12 + dt.month0() as i32 + months;
    let (y, m0) = (total.div_euclid(12), total.rem_euclid(12));

    let last = days_in_month(y, m0 as u32 + 1);
    NaiveDate::from_ymd_opt(y, m0 as u32 + 1, dt.day().min(last)).map(|d| d.and_time(dt.time()))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 30,
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub uid: String,
    pub recurrence_id: Option<String>,
    pub summary: String,
    pub description: Option<String>,
    /// The room, from LOCATION.
    pub location: Option<String>,
    pub starts_at: String,
    pub ends_at: Option<String>,
    pub all_day: bool,
    pub local_date: String,
}

/// One VEVENT before recurrence expansion.
#[derive(Debug, Clone)]
struct RawEvent {
    uid: String,
    summary: String,
    description: Option<String>,
    /// The room. Compass puts it in LOCATION, which was previously discarded.
    location: Option<String>,
    start: IcsTime,
    end: Option<IcsTime>,
    rrule: Option<Rrule>,
    exdates: Vec<IcsTime>,
    recurrence_id: Option<IcsTime>,
    cancelled: bool,
}

/// Parse a whole calendar into concrete, dated events.
///
/// Never returns an error for a single bad VEVENT — a school feed with one
/// malformed entry should still give you the other three hundred. Only an input
/// that isn't a calendar at all is rejected.
pub fn parse_calendar(raw: &str, local_tz: Tz, now: DateTime<Utc>) -> Result<Vec<CalendarEvent>> {
    let lines = unfold(raw);

    if !lines.iter().any(|l| {
        let u = l.trim().to_uppercase();
        u == "BEGIN:VCALENDAR" || u.starts_with("BEGIN:VCALENDAR")
    }) {
        return Err(anyhow!(
            "That doesn't look like a calendar file. Check the URL points at an ICS feed."
        ));
    }

    let mut raws: Vec<RawEvent> = Vec::new();
    let mut current: Option<RawEvent> = None;
    let mut depth_vtimezone = false;

    for line in &lines {
        let Some(p) = parse_line(line) else { continue };

        match (p.name.as_str(), p.value.trim().to_uppercase().as_str()) {
            ("BEGIN", "VTIMEZONE") => depth_vtimezone = true,
            ("END", "VTIMEZONE") => depth_vtimezone = false,
            ("BEGIN", "VEVENT") => {
                current = Some(RawEvent {
                    uid: String::new(),
                    summary: String::new(),
                    description: None,
                    location: None,
                    start: IcsTime::Utc(now),
                    end: None,
                    rrule: None,
                    exdates: Vec::new(),
                    recurrence_id: None,
                    cancelled: false,
                });
            }
            ("END", "VEVENT") => {
                if let Some(ev) = current.take() {
                    // A VEVENT with no UID has no stable identity and would
                    // duplicate on every sync, so it is dropped rather than
                    // given a synthetic one.
                    if !ev.uid.is_empty() {
                        raws.push(ev);
                    }
                }
            }
            _ => {}
        }

        // VTIMEZONE blocks contain their own DTSTART lines describing the DST
        // rules. Those are not events, and reading them as such would create
        // phantom entries in 1970.
        if depth_vtimezone {
            continue;
        }

        let Some(ev) = current.as_mut() else { continue };

        match p.name.as_str() {
            "UID" => ev.uid = p.value.trim().to_string(),
            "SUMMARY" => ev.summary = unescape(&p.value).trim().to_string(),
            "DESCRIPTION" => {
                let d = unescape(&p.value).trim().to_string();
                if !d.is_empty() {
                    ev.description = Some(d);
                }
            }
            "LOCATION" => {
                let l = unescape(&p.value).trim().to_string();
                if !l.is_empty() {
                    ev.location = Some(l);
                }
            }
            "DTSTART" => {
                if let Some(t) = parse_time(&p) {
                    ev.start = t;
                }
            }
            "DTEND" => ev.end = parse_time(&p),
            "DURATION" => {
                if ev.end.is_none() {
                    if let Some(d) = parse_duration(&p.value) {
                        let s = ev.start.to_utc(local_tz);
                        ev.end = Some(IcsTime::Utc(s + d));
                    }
                }
            }
            "RRULE" => ev.rrule = parse_rrule(&p.value),
            "EXDATE" => {
                // EXDATE can carry several comma-separated values on one line.
                for v in p.value.split(',') {
                    let sub = Line {
                        name: "EXDATE".into(),
                        params: p.params.clone(),
                        value: v.trim().to_string(),
                    };
                    if let Some(t) = parse_time(&sub) {
                        ev.exdates.push(t);
                    }
                }
            }
            "RECURRENCE-ID" => ev.recurrence_id = parse_time(&p),
            "STATUS" => {
                if p.value.trim().eq_ignore_ascii_case("CANCELLED") {
                    ev.cancelled = true;
                }
            }
            _ => {}
        }
    }

    Ok(materialise(raws, local_tz, now))
}

/// `PT1H30M` and friends.
fn parse_duration(v: &str) -> Option<Duration> {
    let v = v.trim().trim_start_matches('+');
    let (neg, v) = match v.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, v),
    };
    let v = v.strip_prefix('P')?;

    let mut total = Duration::zero();
    let mut num = String::new();
    let mut in_time = false;

    for c in v.chars() {
        match c {
            'T' => in_time = true,
            '0'..='9' => num.push(c),
            unit => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += match (unit, in_time) {
                    ('W', _) => Duration::weeks(n),
                    ('D', _) => Duration::days(n),
                    ('H', true) => Duration::hours(n),
                    ('M', true) => Duration::minutes(n),
                    ('S', true) => Duration::seconds(n),
                    _ => return None,
                };
            }
        }
    }

    Some(if neg { -total } else { total })
}

/// Expand recurrences, apply overrides and exclusions, and produce final rows.
fn materialise(raws: Vec<RawEvent>, tz: Tz, now: DateTime<Utc>) -> Vec<CalendarEvent> {
    let horizon = (now + Duration::days(HORIZON_DAYS)).with_timezone(&tz).naive_local();

    // An instance of a series that was moved or cancelled arrives as a separate
    // VEVENT sharing the UID and carrying RECURRENCE-ID. Those are applied on
    // top of the expansion rather than alongside it, or you get both the
    // original slot and the moved one.
    let (overrides, bases): (Vec<_>, Vec<_>) =
        raws.into_iter().partition(|e| e.recurrence_id.is_some());

    let mut out: Vec<CalendarEvent> = Vec::new();

    for base in bases {
        if base.cancelled {
            continue;
        }

        let duration = base
            .end
            .as_ref()
            .map(|e| e.to_utc(tz) - base.start.to_utc(tz))
            .filter(|d| *d >= Duration::zero());

        let all_day = base.start.is_date();
        let event_tz = base.start.tz(tz);

        let starts: Vec<NaiveDateTime> = match &base.rrule {
            None => vec![base.start.naive(tz)],
            Some(rule) => expand(rule, base.start.naive(tz), horizon, event_tz),
        };

        let excluded: Vec<DateTime<Utc>> = base.exdates.iter().map(|e| e.to_utc(tz)).collect();

        for naive in starts {
            let start_utc = resolve_local(naive, event_tz);

            if excluded.contains(&start_utc) {
                continue;
            }

            // Only a recurring event gets a recurrence id; a one-off keeps NULL
            // so its identity in the database stays exactly its UID.
            let rec_id = base
                .rrule
                .as_ref()
                .map(|_| start_utc.format("%Y%m%dT%H%M%SZ").to_string());

            // An individually-modified or cancelled instance replaces this one.
            if let Some(ov) = overrides.iter().find(|o| {
                o.uid == base.uid
                    && o.recurrence_id
                        .as_ref()
                        .map(|r| r.to_utc(tz)) == Some(start_utc)
            }) {
                if ov.cancelled {
                    continue;
                }
                out.push(build(ov, ov.start.to_utc(tz), duration, all_day, rec_id, tz));
                continue;
            }

            out.push(build(&base, start_utc, duration, all_day, rec_id, tz));
        }
    }

    // Overrides whose base slot wasn't produced (series ended, or the base
    // wasn't in the feed) still belong on the calendar.
    for ov in &overrides {
        if ov.cancelled {
            continue;
        }
        let start = ov.start.to_utc(tz);
        let rec_id = ov.recurrence_id.as_ref().map(|r| {
            r.to_utc(tz).format("%Y%m%dT%H%M%SZ").to_string()
        });
        if !out
            .iter()
            .any(|e| e.uid == ov.uid && e.recurrence_id == rec_id)
        {
            let dur = ov.end.as_ref().map(|e| e.to_utc(tz) - start);
            out.push(build(ov, start, dur, ov.start.is_date(), rec_id, tz));
        }
    }

    out.sort_by(|a, b| a.starts_at.cmp(&b.starts_at).then(a.uid.cmp(&b.uid)));
    out
}

fn build(
    ev: &RawEvent,
    start: DateTime<Utc>,
    duration: Option<Duration>,
    all_day: bool,
    recurrence_id: Option<String>,
    tz: Tz,
) -> CalendarEvent {
    let end = duration.map(|d| start + d);

    CalendarEvent {
        uid: ev.uid.clone(),
        recurrence_id,
        summary: if ev.summary.is_empty() {
            "(untitled)".into()
        } else {
            ev.summary.clone()
        },
        description: ev.description.clone(),
        location: ev.location.clone(),
        starts_at: start.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ends_at: end.map(|e| e.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        all_day,
        // The day the event belongs to for a human reading a calendar, which is
        // the local date of its start — not the UTC date, and not Retain's 4am
        // study day. A 9pm class is on the day it starts.
        local_date: start.with_timezone(&tz).date_naive().format("%Y-%m-%d").to_string(),
    }
}

// ---------------------------------------------------------------------------
// Fetch and store
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub enabled: bool,
    pub url: String,
    pub last_sync_at: Option<String>,
    pub last_error: Option<String>,
    pub event_count: i64,
}

pub fn status(conn: &Connection) -> Result<SyncStatus> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM calendar_events", [], |r| r.get(0))?;

    Ok(SyncStatus {
        enabled: crate::settings::get_bool(conn, "ics_enabled", false)?,
        url: crate::settings::get(conn, "ics_url")?.unwrap_or_default(),
        last_sync_at: crate::settings::get(conn, "ics_last_sync")?,
        last_error: crate::settings::get(conn, "ics_last_error")?,
        event_count: count,
    })
}

/// Normalise and check a feed URL before anything is requested.
///
/// Pure, so the scheme rules are testable without a network. Refusing anything
/// that isn't HTTP(S) here is what keeps a `file://` path or a pasted Compass
/// login page out of the fetcher entirely.
pub fn normalise_url(url: &str) -> Result<String> {
    let trimmed = url.trim();

    // webcal:// is the same thing over https; the subscribe link Compass gives
    // you uses that scheme, and pasting it in should just work.
    let normalised = trimmed
        .strip_prefix("webcal://")
        .map(|rest| format!("https://{rest}"))
        .unwrap_or_else(|| trimmed.to_string());

    if !normalised.starts_with("https://") && !normalised.starts_with("http://") {
        return Err(anyhow!("The calendar address should start with https://"));
    }

    Ok(normalised)
}

/// Download the feed. Separated from parsing so both halves are testable.
pub async fn fetch(url: &str) -> Result<String> {
    let normalised = normalise_url(url)?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
        .build()?;

    let response = client
        .get(&normalised)
        .header("accept", "text/calendar, text/plain")
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                anyhow!("The calendar server didn't respond in time.")
            } else {
                anyhow!("Couldn't reach the calendar. Are you online?")
            }
        })?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "The calendar server returned {}. Check the URL is still valid — these links expire.",
            response.status().as_u16()
        ));
    }

    let bytes = response.bytes().await.context("reading the calendar")?;
    if bytes.len() > MAX_BYTES {
        return Err(anyhow!("That calendar is unexpectedly large; not importing it."));
    }

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Replace the stored calendar with what the feed now says.
///
/// Deliberately a full replace inside one transaction rather than a merge.
/// A merge has to guess whether a missing event was deleted upstream or just
/// outside the window, and guessing wrong leaves a cancelled SAC on your Today
/// screen. The events have no local state attached to them, so replacing costs
/// nothing.
pub fn store(conn: &mut Connection, events: &[CalendarEvent]) -> Result<usize> {
    let now = util::rfc3339(Utc::now());
    let tx = conn.transaction()?;

    tx.execute("DELETE FROM calendar_events", [])?;

    let mut written = 0usize;
    {
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO calendar_events
               (uid, recurrence_id, summary, description, location, starts_at, ends_at,
                all_day, local_date, fetched_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        )?;

        for e in events {
            stmt.execute(rusqlite::params![
                e.uid,
                e.recurrence_id,
                e.summary,
                e.description,
                e.location,
                e.starts_at,
                e.ends_at,
                e.all_day as i64,
                e.local_date,
                now,
            ])?;
            written += 1;
        }
    }

    tx.commit()?;
    Ok(written)
}

/// Events on or after today, soonest first.
///
/// Uses the **actual local calendar date**, not Retain's 4am study day. That
/// matches how `local_date` was computed when the events were stored, and the
/// two must agree: at 1am, Retain's study day is still yesterday, so filtering
/// on it would surface classes that finished twelve hours ago as "upcoming".
/// A school calendar rolls over at midnight like everyone else's.
///
/// Returns nothing when the integration is switched off. The toggle has to mean
/// something: leaving previously-synced events on the Today screen after you
/// turned the calendar off would make it a switch that doesn't switch anything.
/// The rows are kept rather than deleted, so turning it back on is instant.
pub fn upcoming(conn: &Connection, days: i64, limit: i64) -> Result<Vec<CalendarEvent>> {
    if !crate::settings::get_bool(conn, "ics_enabled", false)? {
        return Ok(Vec::new());
    }

    let from = Utc::now().with_timezone(&local_tz()).date_naive();
    let to = from + Duration::days(days);

    let mut stmt = conn.prepare(
        "SELECT uid, recurrence_id, summary, description, starts_at, ends_at, all_day, local_date,
                location
           FROM calendar_events
          WHERE local_date BETWEEN ?1 AND ?2
          ORDER BY starts_at, id
          LIMIT ?3",
    )?;

    let rows = stmt
        .query_map(
            rusqlite::params![
                from.format("%Y-%m-%d").to_string(),
                to.format("%Y-%m-%d").to_string(),
                limit
            ],
            |r| {
                Ok(CalendarEvent {
                    uid: r.get(0)?,
                    recurrence_id: r.get(1)?,
                    summary: r.get(2)?,
                    description: r.get(3)?,
                    location: r.get(8)?,
                    starts_at: r.get(4)?,
                    ends_at: r.get(5)?,
                    all_day: r.get::<_, i64>(6)? == 1,
                    local_date: r.get(7)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

#[cfg(test)]
mod tests;

/// The machine's own timezone, for interpreting floating times and all-day
/// events.
///
/// Falls back to Melbourne rather than UTC when the system won't say: this is a
/// VCE app, and being an hour or ten out is far more visible than being in the
/// wrong hemisphere. The fallback only bites on a misconfigured machine.
pub fn local_tz() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|name| name.parse::<Tz>().ok())
        .unwrap_or(chrono_tz::Australia::Melbourne)
}

// ---------------------------------------------------------------------------
// Making a Compass class readable
// ---------------------------------------------------------------------------

/// A class, decoded from what Compass actually sends.
///
/// Compass splits a class across three ICS properties and none of them is a
/// sentence: SUMMARY is a class code (`11CHEU2`), LOCATION is a room (`T3`),
/// DESCRIPTION is `Attending Staff : BGY`. Shown raw, Today said "11CHEU2" and
/// nothing else — which is the one thing you already know at 8:25am.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassDetail {
    /// The raw code, kept because it's what's printed on your timetable.
    pub code: String,
    /// The subject, matched against your own subject list. `None` when the code
    /// belongs to something that isn't one of your subjects — an assembly, a
    /// formal — and inventing a subject for those would be worse than silence.
    pub subject_name: Option<String>,
    pub colour: Option<String>,
    pub room: Option<String>,
    pub teacher: Option<String>,
}

/// Pull the teacher out of Compass's description line.
///
/// The format is `Attending Staff : BGY`, occasionally with several initials.
/// Anything that doesn't look like that is left alone rather than guessed at.
pub fn teacher_from_description(description: Option<&str>) -> Option<String> {
    let d = description?.trim();
    let rest = d
        .strip_prefix("Attending Staff :")
        .or_else(|| d.strip_prefix("Attending Staff:"))?;
    let name = rest.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// The letters at the heart of a class code.
///
/// `11CHEU2` → `CHEU`, `12BIOS` → `BIOS`, `11MMEP2` → `MMEP`. Compass codes are
/// a year prefix, a subject stem, and a group suffix; the stem is the only part
/// that identifies the subject.
fn code_stem(code: &str) -> String {
    code.chars()
        .skip_while(|c| c.is_ascii_digit())
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_uppercase()
}

/// Match a class code to one of the student's subjects.
///
/// Matched on the code's stem against the subject name's initials and its first
/// letters, which covers the shapes Compass uses: `CHEU`→Chemistry,
/// `BIOS`→Biology, `MMEP`→Mathematical Methods, `SMAR`→Specialist Mathematics.
///
/// A code with no match returns `None`. That is deliberate: `ASMEDA` is a year
/// level assembly, and filing it under whichever subject shares two letters
/// would put a fake Biology class on your timetable.
pub fn match_subject<'a>(code: &str, subjects: &'a [(String, String)]) -> Option<&'a (String, String)> {
    let stem = code_stem(code);
    if stem.len() < 3 {
        return None;
    }

    subjects.iter().find(|(name, _)| {
        let upper = name.to_uppercase();

        // "Mathematical Methods" → "MM", matching the MMEP stem.
        let initials: String = upper
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .collect();

        // The first letters of the first word: "CHEMISTRY" → "CHE" for CHEU.
        let first_word = upper.split_whitespace().next().unwrap_or("");

        (initials.len() >= 2 && stem.starts_with(&initials))
            || (first_word.len() >= 3 && stem.starts_with(&first_word[..3.min(first_word.len())]))
    })
}

/// Decode one event into something worth putting on a screen.
pub fn describe(
    summary: &str,
    description: Option<&str>,
    location: Option<&str>,
    subjects: &[(String, String)],
) -> ClassDetail {
    let code = summary.trim().to_string();
    let matched = match_subject(&code, subjects);

    ClassDetail {
        subject_name: matched.map(|(n, _)| n.clone()),
        colour: matched.map(|(_, c)| c.clone()),
        room: location.map(str::trim).filter(|l| !l.is_empty()).map(str::to_string),
        teacher: teacher_from_description(description),
        code,
    }
}

/// One day's classes as prose, for the assistant's context.
///
/// Rooms and teachers included: "what have I got before Chemistry" and "where
/// am I at 11" are questions about a real timetable, and a list of bare codes
/// can't answer either.
pub fn day_summary(conn: &Connection, local_date: &str) -> Result<String> {
    let subjects: Vec<(String, String)> = {
        let mut stmt =
            conn.prepare("SELECT name, colour FROM subjects WHERE archived = 0 ORDER BY sort_order")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };

    let mut stmt = conn.prepare(
        "SELECT summary, description, location, starts_at, all_day
           FROM calendar_events WHERE local_date = ?1 ORDER BY all_day DESC, starts_at",
    )?;
    // summary, description, location, starts_at, all_day
    type Row = (String, Option<String>, Option<String>, String, bool);
    let rows: Vec<Row> = stmt
        .query_map([local_date], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, i64>(4)? == 1))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    if rows.is_empty() {
        return Ok(String::new());
    }

    let mut out = String::from("Classes today:\n");
    for (summary, description, location, starts_at, all_day) in rows {
        let d = describe(&summary, description.as_deref(), location.as_deref(), &subjects);
        let name = d.subject_name.unwrap_or_else(|| d.code.clone());

        let when = if all_day {
            "all day".to_string()
        } else {
            DateTime::parse_from_rfc3339(&starts_at)
                .map(|t| t.with_timezone(&chrono::Local).format("%-I:%M%P").to_string())
                .unwrap_or_else(|_| "?".into())
        };

        let mut line = format!("- {when} {name}");
        if let Some(room) = d.room {
            line.push_str(&format!(" in {room}"));
        }
        if let Some(teacher) = d.teacher {
            line.push_str(&format!(" with {teacher}"));
        }
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}
