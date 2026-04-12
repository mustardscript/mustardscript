use std::time::{SystemTime, UNIX_EPOCH};

use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use super::*;

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

pub(super) fn current_time_millis() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

pub(super) fn parse_date_timestamp_ms(value: &str) -> f64 {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|datetime| datetime.unix_timestamp_nanos() as f64 / 1_000_000.0)
        .unwrap_or(f64::NAN)
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
    value[..byte_index].chars().count()
}

pub(super) fn advance_char_index(value: &str, index: usize) -> usize {
    let total = value.chars().count();
    (index + 1).min(total)
}

pub(super) fn find_string_pattern(value: &str, needle: &str, start: usize) -> Option<usize> {
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
