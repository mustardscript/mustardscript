use super::*;
#[cfg(test)]
use std::sync::{Mutex, MutexGuard};
use std::sync::{
    OnceLock,
    atomic::{AtomicU8, Ordering},
};

const MAX_TIME_MS: f64 = 8_640_000_000_000_000.0;
const MS_PER_SECOND: i64 = 1_000;
const MS_PER_MINUTE: i64 = 60 * MS_PER_SECOND;
const MS_PER_HOUR: i64 = 60 * MS_PER_MINUTE;
const MS_PER_DAY: i64 = 24 * MS_PER_HOUR;

const ASCII_STRING_FAST_PATH_OVERRIDE_UNSET: u8 = 0;
const ASCII_STRING_FAST_PATH_OVERRIDE_DISABLED: u8 = 1;
const ASCII_STRING_FAST_PATH_OVERRIDE_ENABLED: u8 = 2;

static ASCII_STRING_FAST_PATH_OVERRIDE: AtomicU8 =
    AtomicU8::new(ASCII_STRING_FAST_PATH_OVERRIDE_UNSET);
#[cfg(test)]
static ASCII_STRING_FAST_PATH_TEST_LOCK: Mutex<()> = Mutex::new(());

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
    })
}

pub(super) fn ascii_string_fast_paths_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();

    match ASCII_STRING_FAST_PATH_OVERRIDE.load(Ordering::Relaxed) {
        ASCII_STRING_FAST_PATH_OVERRIDE_DISABLED => false,
        ASCII_STRING_FAST_PATH_OVERRIDE_ENABLED => true,
        _ => *ENABLED.get_or_init(|| env_flag_enabled("MUSTARD_ENABLE_ASCII_STRING_FAST_PATHS")),
    }
}

#[cfg(test)]
pub(super) struct AsciiStringFastPathOverrideGuard {
    previous: u8,
    _lock: MutexGuard<'static, ()>,
}

#[cfg(test)]
impl Drop for AsciiStringFastPathOverrideGuard {
    fn drop(&mut self) {
        ASCII_STRING_FAST_PATH_OVERRIDE.store(self.previous, Ordering::Relaxed);
    }
}

#[cfg(test)]
pub(super) fn override_ascii_string_fast_paths_for_tests(
    enabled: bool,
) -> AsciiStringFastPathOverrideGuard {
    let lock = ASCII_STRING_FAST_PATH_TEST_LOCK
        .lock()
        .expect("ascii string fast path test override lock should not be poisoned");
    let next = if enabled {
        ASCII_STRING_FAST_PATH_OVERRIDE_ENABLED
    } else {
        ASCII_STRING_FAST_PATH_OVERRIDE_DISABLED
    };
    let previous = ASCII_STRING_FAST_PATH_OVERRIDE.swap(next, Ordering::Relaxed);
    AsciiStringFastPathOverrideGuard {
        previous,
        _lock: lock,
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RegExpFlagsState {
    pub(super) global: bool,
    pub(super) ignore_case: bool,
    pub(super) multiline: bool,
    pub(super) dot_all: bool,
    pub(super) unicode: bool,
    pub(super) sticky: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct RegExpMatchData {
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
    pub(crate) start_index: usize,
    pub(crate) end_index: usize,
    pub(crate) captures: Vec<Option<String>>,
    pub(crate) named_groups: IndexMap<String, Option<String>>,
}

#[derive(Debug, Clone)]
pub(super) enum StringSearchPattern {
    Literal(String),
    RegExp {
        object: ObjectKey,
        regex: RegExpObject,
    },
}

#[derive(Debug, Clone, Copy)]
pub(super) struct DateTimeFields {
    pub(super) year: i64,
    pub(super) month: u8,
    pub(super) day: u8,
    pub(super) hour: u8,
    pub(super) minute: u8,
    pub(super) second: u8,
    pub(super) millisecond: u16,
}

#[cfg(target_arch = "wasm32")]
unsafe extern "C" {
    fn mustard_now_millis() -> f64;
}

pub(super) fn current_time_millis() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        unsafe { mustard_now_millis() }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as f64)
            .unwrap_or(0.0)
    }
}

pub(super) fn parse_date_timestamp_ms(value: &str) -> f64 {
    parse_iso_date_timestamp_ms(value).unwrap_or(f64::NAN)
}

pub(super) fn time_clip(timestamp_ms: f64) -> f64 {
    if !timestamp_ms.is_finite() || timestamp_ms.abs() > MAX_TIME_MS {
        f64::NAN
    } else {
        let clipped = timestamp_ms.trunc();
        if clipped == 0.0 { 0.0 } else { clipped }
    }
}

fn parse_iso_date_timestamp_ms(value: &str) -> Option<f64> {
    let (year, mut index) = parse_iso_year(value)?;
    index += 1;
    let month = parse_two_digits(value, &mut index)?;
    require_byte(value, &mut index, b'-')?;
    let day = parse_two_digits(value, &mut index)?;
    if !is_valid_date(year, month, day) {
        return None;
    }

    let days = days_from_civil(year, month, day);
    if index == value.len() {
        return Some(days as f64 * MS_PER_DAY as f64);
    }

    require_byte(value, &mut index, b'T')?;
    let hour = parse_two_digits(value, &mut index)?;
    require_byte(value, &mut index, b':')?;
    let minute = parse_two_digits(value, &mut index)?;
    require_byte(value, &mut index, b':')?;
    let second = parse_two_digits(value, &mut index)?;
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let mut millisecond = 0i64;
    if matches!(value.as_bytes().get(index), Some(b'.')) {
        index += 1;
        let start = index;
        while value.as_bytes().get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return None;
        }
        let digits = &value.as_bytes()[start..index];
        let mut parsed = 0i64;
        for digit in digits.iter().take(3) {
            parsed = parsed * 10 + i64::from(digit - b'0');
        }
        for _ in digits.len().min(3)..3 {
            parsed *= 10;
        }
        millisecond = parsed;
    }

    let offset_ms = match value.as_bytes().get(index).copied() {
        Some(b'Z') if index + 1 == value.len() => 0i64,
        Some(sign @ (b'+' | b'-')) => {
            index += 1;
            let offset_hours = parse_two_digits(value, &mut index)?;
            require_byte(value, &mut index, b':')?;
            let offset_minutes = parse_two_digits(value, &mut index)?;
            if offset_hours > 23 || offset_minutes > 59 || index != value.len() {
                return None;
            }
            let magnitude =
                i64::from(offset_hours) * MS_PER_HOUR + i64::from(offset_minutes) * MS_PER_MINUTE;
            if sign == b'+' { magnitude } else { -magnitude }
        }
        _ => return None,
    };

    let time_ms = i64::from(hour) * MS_PER_HOUR
        + i64::from(minute) * MS_PER_MINUTE
        + i64::from(second) * MS_PER_SECOND
        + millisecond;
    let timestamp_ms =
        i128::from(days) * i128::from(MS_PER_DAY) + i128::from(time_ms) - i128::from(offset_ms);
    if !(i128::from(i64::MIN)..=i128::from(i64::MAX)).contains(&timestamp_ms) {
        return None;
    }
    Some(timestamp_ms as f64)
}

pub(super) fn date_time_fields_from_timestamp_ms(timestamp_ms: f64) -> Option<DateTimeFields> {
    if !timestamp_ms.is_finite() {
        return None;
    }
    let timestamp_ms = if timestamp_ms.trunc() == 0.0 {
        0.0
    } else {
        timestamp_ms.trunc()
    };
    if timestamp_ms < i64::MIN as f64 || timestamp_ms > i64::MAX as f64 {
        return None;
    }
    let timestamp_ms = timestamp_ms as i64;
    let days = timestamp_ms.div_euclid(MS_PER_DAY);
    let day_ms = timestamp_ms.rem_euclid(MS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    Some(DateTimeFields {
        year,
        month,
        day,
        hour: (day_ms / MS_PER_HOUR) as u8,
        minute: ((day_ms % MS_PER_HOUR) / MS_PER_MINUTE) as u8,
        second: ((day_ms % MS_PER_MINUTE) / MS_PER_SECOND) as u8,
        millisecond: (day_ms % MS_PER_SECOND) as u16,
    })
}

pub(super) fn format_iso_datetime(timestamp_ms: f64) -> Option<String> {
    let datetime = date_time_fields_from_timestamp_ms(timestamp_ms)?;
    Some(format!(
        "{}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        format_iso_year(datetime.year),
        datetime.month,
        datetime.day,
        datetime.hour,
        datetime.minute,
        datetime.second,
        datetime.millisecond,
    ))
}

fn parse_iso_year(value: &str) -> Option<(i64, usize)> {
    let bytes = value.as_bytes();
    let mut index = 0usize;
    let signed = matches!(bytes.first(), Some(b'+' | b'-'));
    if signed {
        index += 1;
    }
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    let digits = if signed {
        index.saturating_sub(1)
    } else {
        index
    };
    if digits == 0 || bytes.get(index) != Some(&b'-') {
        return None;
    }
    if (!signed && digits != 4) || (signed && digits != 6) {
        return None;
    }
    Some((value[..index].parse::<i64>().ok()?, index))
}

fn parse_two_digits(value: &str, index: &mut usize) -> Option<u8> {
    let bytes = value.as_bytes();
    let tens = *bytes.get(*index)?;
    let ones = *bytes.get(*index + 1)?;
    if !tens.is_ascii_digit() || !ones.is_ascii_digit() {
        return None;
    }
    *index += 2;
    Some((tens - b'0') * 10 + (ones - b'0'))
}

fn require_byte(value: &str, index: &mut usize, expected: u8) -> Option<()> {
    if value.as_bytes().get(*index) == Some(&expected) {
        *index += 1;
        Some(())
    } else {
        None
    }
}

fn is_valid_date(year: i64, month: u8, day: u8) -> bool {
    matches!(month, 1..=12) && (1..=days_in_month(year, month)).contains(&day)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn days_from_civil(year: i64, month: u8, day: u8) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month as u8, day as u8)
}

fn format_iso_year(year: i64) -> String {
    if (0..=9_999).contains(&year) {
        format!("{year:04}")
    } else if year < 0 {
        format!("-{:06}", year.unsigned_abs())
    } else {
        format!("+{:06}", year as u64)
    }
}

pub(super) fn format_en_us_number_grouped(integer: &str) -> String {
    let mut chars = integer.chars().collect::<Vec<_>>();
    let negative = matches!(chars.first(), Some('-'));
    if negative {
        chars.remove(0);
    }
    let mut grouped = String::new();
    for (index, ch) in chars.iter().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(*ch);
    }
    let grouped = grouped.chars().rev().collect::<String>();
    if negative {
        format!("-{grouped}")
    } else {
        grouped
    }
}

pub(super) fn clamp_index(index: i64, len: usize) -> usize {
    if index < 0 {
        0
    } else {
        (index as usize).min(len)
    }
}

pub(super) fn normalize_relative_bound(index: i64, len: usize) -> usize {
    let len = len as i64;
    if index < 0 {
        (len + index).max(0) as usize
    } else {
        index.min(len) as usize
    }
}

pub(super) fn normalize_search_index(index: i64, len: usize) -> usize {
    if index < 0 {
        normalize_relative_bound(index, len)
    } else {
        clamp_index(index, len)
    }
}

pub(super) fn ascii_to_lowercase(value: &str) -> Option<String> {
    if !value.is_ascii() {
        return None;
    }
    let mut bytes = value.as_bytes().to_vec();
    for byte in &mut bytes {
        *byte = byte.to_ascii_lowercase();
    }
    Some(String::from_utf8(bytes).expect("ASCII lowercase should stay UTF-8"))
}

pub(super) fn ascii_to_uppercase(value: &str) -> Option<String> {
    if !value.is_ascii() {
        return None;
    }
    let mut bytes = value.as_bytes().to_vec();
    for byte in &mut bytes {
        *byte = byte.to_ascii_uppercase();
    }
    Some(String::from_utf8(bytes).expect("ASCII uppercase should stay UTF-8"))
}

fn is_ascii_js_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

fn replace_ascii_matching_runs<F>(value: &str, replacement: &str, mut matches: F) -> String
where
    F: FnMut(u8) -> bool,
{
    let bytes = value.as_bytes();
    let mut result = String::with_capacity(value.len().max(replacement.len()));
    let mut index = 0usize;
    while index < bytes.len() {
        if matches(bytes[index]) {
            result.push_str(replacement);
            index += 1;
            while index < bytes.len() && matches(bytes[index]) {
                index += 1;
            }
            continue;
        }
        result.push(bytes[index] as char);
        index += 1;
    }
    result
}

pub(super) fn try_ascii_cleanup_replace_all(
    value: &str,
    pattern: &str,
    flags: &str,
    replacement: &str,
) -> Option<String> {
    if !value.is_ascii() || flags != "g" || replacement.contains('$') {
        return None;
    }
    match pattern {
        r"\s+" => Some(replace_ascii_matching_runs(
            value,
            replacement,
            is_ascii_js_space,
        )),
        r"[^\w ]+" => Some(replace_ascii_matching_runs(value, replacement, |byte| {
            !(byte.is_ascii_alphanumeric() || byte == b'_' || byte == b' ')
        })),
        r"[^a-z0-9 ]+" => Some(replace_ascii_matching_runs(
            value,
            replacement,
            |byte| !matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b' '),
        )),
        _ => None,
    }
}

fn is_ascii_literal_regex_byte(byte: u8) -> bool {
    !matches!(
        byte,
        b'\\' | b'.' | b'^' | b'$' | b'*' | b'+' | b'?' | b'(' | b')' | b'[' | b']' | b'{' | b'}'
    )
}

fn is_ascii_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

struct AsciiLiteralAlternationPattern<'a> {
    alternatives: Vec<&'a str>,
    requires_word_boundaries: bool,
    capture_full_match: bool,
}

fn parse_ascii_literal_alternation_pattern<'a>(
    pattern: &'a str,
    flags: &str,
) -> Option<AsciiLiteralAlternationPattern<'a>> {
    if flags != "g" || !pattern.is_ascii() || !pattern.contains('|') {
        return None;
    }

    let (requires_word_boundaries, bounded) = if let Some(stripped) = pattern
        .strip_prefix(r"\b")
        .and_then(|value| value.strip_suffix(r"\b"))
    {
        (true, stripped)
    } else {
        (false, pattern)
    };

    let (capture_full_match, alternation_source) = if let Some(stripped) = bounded
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
    {
        (true, stripped)
    } else {
        (false, bounded)
    };

    let alternatives = alternation_source.split('|').collect::<Vec<_>>();
    if alternatives.is_empty() {
        return None;
    }

    let valid_alternatives = alternatives.iter().all(|alternative| {
        if alternative.is_empty() {
            return false;
        }
        if requires_word_boundaries {
            alternative.bytes().all(is_ascii_word_byte)
        } else {
            alternative.bytes().all(is_ascii_literal_regex_byte)
        }
    });
    if !valid_alternatives {
        return None;
    }

    Some(AsciiLiteralAlternationPattern {
        alternatives,
        requires_word_boundaries,
        capture_full_match,
    })
}

pub(super) fn is_ascii_literal_alternation_regex(pattern: &str, flags: &str) -> bool {
    parse_ascii_literal_alternation_pattern(pattern, flags).is_some()
}

pub(super) fn collect_ascii_literal_alternation_matches(
    value: &str,
    pattern: &str,
    flags: &str,
    all: bool,
) -> Option<Vec<RegExpMatchData>> {
    let parsed = parse_ascii_literal_alternation_pattern(pattern, flags)?;
    if !value.is_ascii() {
        return None;
    }

    let bytes = value.as_bytes();
    let mut matches = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        let matched = parsed
            .alternatives
            .iter()
            .find(|alternative| bytes[index..].starts_with(alternative.as_bytes()))
            .copied();
        if let Some(matched) = matched {
            let end = index + matched.len();
            if parsed.requires_word_boundaries
                && (index > 0 && is_ascii_word_byte(bytes[index - 1])
                    || end < bytes.len() && is_ascii_word_byte(bytes[end]))
            {
                index += 1;
                continue;
            }
            matches.push(RegExpMatchData {
                start_byte: index,
                end_byte: end,
                start_index: index,
                end_index: end,
                captures: if parsed.capture_full_match {
                    vec![Some(matched.to_string())]
                } else {
                    Vec::new()
                },
                named_groups: IndexMap::new(),
            });
            if !all {
                break;
            }
            index = end;
        } else {
            index += 1;
        }
    }

    Some(matches)
}

pub(super) fn collect_literal_matches(value: &str, needle: &str) -> Vec<RegExpMatchData> {
    if needle.is_empty() {
        let total = value.chars().count();
        return (0..=total)
            .map(|index| {
                let byte = char_index_to_byte_index(value, index);
                RegExpMatchData {
                    start_byte: byte,
                    end_byte: byte,
                    start_index: index,
                    end_index: index,
                    captures: Vec::new(),
                    named_groups: IndexMap::new(),
                }
            })
            .collect();
    }

    let mut matches = Vec::new();
    let mut start_index = 0usize;
    while let Some(matched) = find_string_pattern(value, needle, start_index).map(|index| {
        let start_byte = char_index_to_byte_index(value, index);
        let end_index = index + needle.chars().count();
        let end_byte = char_index_to_byte_index(value, end_index);
        RegExpMatchData {
            start_byte,
            end_byte,
            start_index: index,
            end_index,
            captures: Vec::new(),
            named_groups: IndexMap::new(),
        }
    }) {
        start_index = matched.end_index;
        matches.push(matched);
    }
    matches
}

pub(super) fn char_index_to_byte_index(value: &str, index: usize) -> usize {
    if value.is_ascii() {
        return index.min(value.len());
    }
    if index == 0 {
        return 0;
    }
    value
        .char_indices()
        .nth(index)
        .map(|(byte, _)| byte)
        .unwrap_or_else(|| value.len())
}

pub(super) fn byte_index_to_char_index(value: &str, byte_index: usize) -> usize {
    if value.is_ascii() {
        return byte_index.min(value.len());
    }
    value[..byte_index].chars().count()
}

pub(super) fn advance_char_index(value: &str, index: usize) -> usize {
    if value.is_ascii() {
        return (index + 1).min(value.len());
    }
    let total = value.chars().count();
    (index + 1).min(total)
}

pub(super) fn find_string_pattern(value: &str, needle: &str, start: usize) -> Option<usize> {
    if value.is_ascii() && needle.is_ascii() {
        let start = start.min(value.len());
        return value[start..]
            .find(needle)
            .map(|byte_index| start + byte_index);
    }
    let start_byte = char_index_to_byte_index(value, start);
    value[start_byte..]
        .find(needle)
        .map(|byte_index| byte_index_to_char_index(value, start_byte + byte_index))
}

pub(super) fn split_string_by_pattern(
    value: &str,
    separator: Option<&str>,
    limit: usize,
) -> Vec<String> {
    let mut parts = Vec::new();
    match separator {
        None => {
            parts.push(value.to_string());
        }
        Some("") => {
            if limit == 0 {
                return Vec::new();
            }
            if value.is_empty() {
                return parts;
            }
            for ch in value.chars() {
                if parts.len() == limit {
                    break;
                }
                parts.push(ch.to_string());
            }
            return parts;
        }
        Some(separator) => {
            let mut remaining = value;
            while let Some(index) = remaining.find(separator) {
                if parts.len() + 1 == limit {
                    parts.push(remaining[..index].to_string());
                    return parts;
                }
                parts.push(remaining[..index].to_string());
                remaining = &remaining[index + separator.len()..];
            }
            if parts.len() < limit {
                parts.push(remaining.to_string());
            }
        }
    }
    parts
}

pub(super) fn replace_all_string_matches(value: &str, search: &str, replacement: &str) -> String {
    if search.is_empty() {
        let mut result = String::new();
        for ch in value.chars() {
            result.push_str(replacement);
            result.push(ch);
        }
        result.push_str(replacement);
        return result;
    }

    let mut result = String::new();
    let mut start_index = 0usize;
    while let Some(index) = find_string_pattern(value, search, start_index) {
        let start_byte = char_index_to_byte_index(value, index);
        let end_index = index + search.chars().count();
        let end_byte = char_index_to_byte_index(value, end_index);
        result.push_str(&value[char_index_to_byte_index(value, start_index)..start_byte]);
        result.push_str(replacement);
        start_index = end_index;
        if start_byte == end_byte {
            start_index = advance_char_index(value, start_index);
        }
    }
    result.push_str(&value[char_index_to_byte_index(value, start_index)..]);
    result
}

pub(super) fn replace_first_string_match(value: &str, search: &str, replacement: &str) -> String {
    if search.is_empty() {
        let mut result = String::new();
        result.push_str(replacement);
        result.push_str(value);
        return result;
    }
    let Some(index) = find_string_pattern(value, search, 0) else {
        return value.to_string();
    };
    let start_byte = char_index_to_byte_index(value, index);
    let end_index = index + search.chars().count();
    let end_byte = char_index_to_byte_index(value, end_index);
    let mut result = String::new();
    result.push_str(&value[..start_byte]);
    result.push_str(replacement);
    result.push_str(&value[end_byte..]);
    result
}

pub(super) fn expand_regexp_replacement_template(
    replacement: &str,
    input: &str,
    matched: &RegExpMatchData,
) -> String {
    let mut result = String::new();
    let mut chars = replacement.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            result.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('$') => {
                chars.next();
                result.push('$');
            }
            Some('&') => {
                chars.next();
                result.push_str(&input[matched.start_byte..matched.end_byte]);
            }
            Some('`') => {
                chars.next();
                result.push_str(&input[..matched.start_byte]);
            }
            Some('\'') => {
                chars.next();
                result.push_str(&input[matched.end_byte..]);
            }
            Some('<') => {
                chars.next();
                let mut name = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == '>' {
                        closed = true;
                        break;
                    }
                    name.push(next);
                }
                if closed {
                    if let Some(Some(value)) = matched.named_groups.get(&name) {
                        result.push_str(value);
                    }
                } else {
                    result.push('$');
                    result.push('<');
                    result.push_str(&name);
                    break;
                }
            }
            Some(digit @ '1'..='9') => {
                let mut index = digit.to_digit(10).unwrap() as usize;
                chars.next();
                if let Some(next_digit @ '0'..='9') = chars.peek().copied() {
                    let candidate = index * 10 + next_digit.to_digit(10).unwrap() as usize;
                    if candidate <= matched.captures.len() {
                        index = candidate;
                        chars.next();
                    }
                }
                if index > 0
                    && let Some(Some(value)) = matched.captures.get(index - 1)
                {
                    result.push_str(value);
                }
            }
            _ => result.push('$'),
        }
    }
    result
}
