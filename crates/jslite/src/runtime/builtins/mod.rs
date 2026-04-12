use super::*;

use indexmap::IndexMap;

mod arrays;
mod collections;
mod install;
mod intl;
mod objects;
mod primitives;
mod promises;
mod regexp;
mod strings;
mod support;

use self::support::{
    RegExpFlagsState, StringSearchPattern, advance_char_index, byte_index_to_char_index,
    char_index_to_byte_index, clamp_index, collect_literal_matches, current_time_millis,
    date_time_from_timestamp_ms, expand_regexp_replacement_template, find_string_pattern,
    format_en_us_number_grouped, format_iso_datetime, normalize_relative_bound,
    normalize_search_index, parse_date_timestamp_ms, replace_all_string_matches,
    replace_first_string_match, split_string_by_pattern,
};
pub(crate) use promises::PromiseSetupPolicy;
pub(crate) use support::RegExpMatchData;
