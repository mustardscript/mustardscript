#![allow(dead_code)]

use std::fmt::Write;

use jslite::{
    ExecutionStep, JsliteResult, ResumeOptions, ResumePayload, RuntimeLimits, SnapshotPolicy,
    StructuredValue, dump_snapshot, load_snapshot, resume_with_options,
};
use proptest::prelude::*;

const SAFE_MESSAGE_PATH_FRAGMENTS: &[&str] = &["/Users/", "\\Users\\", "C:\\", "/home/"];
const OBJECT_KEYS: &[&str] = &["alpha", "beta", "gamma", "delta", "epsilon"];

fn snapshot_policy(capabilities: &[&str], limits: RuntimeLimits) -> SnapshotPolicy {
    SnapshotPolicy {
        capabilities: capabilities
            .iter()
            .map(|name| (*name).to_string())
            .collect(),
        limits,
    }
}

pub fn assert_host_safe_message(message: &str) {
    for fragment in SAFE_MESSAGE_PATH_FRAGMENTS {
        assert!(
            !message.contains(fragment),
            "message leaked host path fragment `{fragment}`: {message}"
        );
    }
}

pub fn structured_literal_strategy() -> BoxedStrategy<String> {
    let leaf = prop_oneof![
        Just("undefined".to_string()),
        Just("null".to_string()),
        any::<bool>().prop_map(|value| value.to_string()),
        (-20i16..=20).prop_map(|value| value.to_string()),
        string_literal_strategy(),
    ]
    .boxed();

    leaf.prop_recursive(3, 48, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..=3)
                .prop_map(|items| format!("[{}]", items.join(", "))),
            prop::collection::btree_map(
                prop::sample::select(
                    OBJECT_KEYS
                        .iter()
                        .map(|key| key.to_string())
                        .collect::<Vec<_>>()
                ),
                inner,
                0..=3,
            )
            .prop_map(render_object),
        ]
    })
    .boxed()
}

pub fn supported_program_strategy() -> BoxedStrategy<String> {
    let literal = structured_literal_strategy();
    let small_number = (-12i16..=12).prop_map(|value| value.to_string()).boxed();
    let bool_literal = any::<bool>()
        .prop_map(|value| {
            if value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        })
        .boxed();
    let numeric_array = prop::collection::vec(-8i16..=8, 0..=5).boxed();

    prop_oneof![
        (literal.clone(), literal.clone()).prop_map(|(left, right)| format!(
            "const left = {left}; const right = {right}; [left, right, {{ left: left, right: right }}];"
        )),
        numeric_array.clone().prop_map(|values| {
            let values = render_numeric_array(&values);
            format!(
                "const values = {values}; let total = 0; for (const value of values) {{ total += value; }} ({{
                  values: values,
                  total: total
                }});"
            )
        }),
        (small_number.clone(), small_number.clone()).prop_map(|(input, offset)| format!(
            "function wrap(value) {{ return [value, value + {offset}]; }} const input = {input}; wrap(input);"
        )),
        (literal.clone(), literal.clone(), bool_literal.clone()).prop_map(
            |(thrown, fallback, flag)| format!(
                "function run(flag) {{ try {{ if (flag) {{ throw {thrown}; }} return {fallback}; }} catch (error) {{ return [error, \"caught\"]; }} finally {{ }} }} run({flag});"
            ),
        ),
        ((-5i16..=5), (-5i16..=5), (-5i16..=5)).prop_map(|(first_key, second_key, second_value)| format!(
            "const map = new Map([[{first_key}, 1], [{second_key}, {second_value}], [{first_key}, 9]]);
             const set = new Set([{first_key}, {second_key}, {first_key}]);
             ({{
               size: map.size,
               first: map.get({first_key}),
               seen: set.size
             }});"
        )),
        prop::collection::vec(-6i16..=6, 0..=4).prop_map(|values| {
            let values = render_numeric_array(&values);
            format!(
                "const values = {values};
                 const mapped = values.map((value, index) => value + index);
                 ({{
                   mapped: mapped,
                   filtered: values.filter((value) => value >= 0),
                   some: values.some((value) => value === 0),
                   reduced: values.reduce((acc, value) => acc + value, 0)
                 }});"
            )
        }),
        (small_number.clone(), small_number.clone()).prop_map(|(present, fallback)| format!(
            "const present = {{ nested: {{ value: {present} }} }}; const missing = null; [present?.nested?.value ?? {fallback}, missing?.nested?.value ?? {fallback}];"
        )),
        prop::array::uniform2(-8i16..=8).prop_map(|[left, right]| {
            format!(
                "const pair = [{left}, {right}];
                 let [first, second] = pair;
                 let {{ total }} = {{ total: first + second }};
                 ({{
                   first: first,
                   second: second,
                   total: total
                 }});"
            )
        }),
    ]
    .boxed()
}

pub fn suspending_program_strategy() -> BoxedStrategy<String> {
    let numeric_array = prop::collection::vec(0i16..=6, 1..=4).boxed();

    prop_oneof![
        ((0i16..=9), (0i16..=9)).prop_map(|(value, offset)| format!(
            "const value = fetch_data({value}); value + {offset};"
        )),
        numeric_array.clone().prop_map(|values| {
            let values = render_numeric_array(&values);
            format!(
                "let total = 0; for (const value of {values}) {{ total += fetch_data(value); }} total;"
            )
        }),
        ((0i16..=9), (0i16..=9)).prop_map(|(value, offset)| format!(
            "async function main() {{ const loaded = await fetch_data({value}); return loaded + {offset}; }} main();"
        )),
        (0i16..=9).prop_map(|value| format!(
            "async function main() {{ return Promise.resolve({value}).then(fetch_data); }} main();"
        )),
        ((0i16..=9), (0i16..=9)).prop_map(|(left, right)| format!(
            "const key = {{ label: \"shared\" }}; const map = new Map([[key, {left}]]); const set = new Set([key]); const value = fetch_data(map.get(key) + set.size); [value, map.size, set.size, {right}];"
        )),
    ]
    .boxed()
}

pub fn completed_value(step: ExecutionStep) -> StructuredValue {
    match step {
        ExecutionStep::Completed(value) => value,
        ExecutionStep::Suspended(suspension) => panic!(
            "expected completion but execution suspended on {}",
            suspension.capability
        ),
    }
}

pub fn drive_with_echo(
    mut step: ExecutionStep,
    serialize_each_snapshot: bool,
) -> JsliteResult<StructuredValue> {
    loop {
        match step {
            ExecutionStep::Completed(value) => return Ok(value),
            ExecutionStep::Suspended(suspension) => {
                let payload = ResumePayload::Value(
                    suspension
                        .args
                        .first()
                        .cloned()
                        .unwrap_or(StructuredValue::Undefined),
                );
                let snapshot = if serialize_each_snapshot {
                    load_snapshot(&dump_snapshot(&suspension.snapshot)?)?
                } else {
                    suspension.snapshot
                };
                let options = if serialize_each_snapshot {
                    ResumeOptions {
                        cancellation_token: None,
                        snapshot_policy: Some(snapshot_policy(
                            &[suspension.capability.as_str()],
                            RuntimeLimits::default(),
                        )),
                    }
                } else {
                    ResumeOptions::default()
                };
                step = resume_with_options(snapshot, payload, options)?;
            }
        }
    }
}

fn string_literal_strategy() -> BoxedStrategy<String> {
    prop::collection::vec(
        prop_oneof![
            Just('a'),
            Just('b'),
            Just('c'),
            Just('x'),
            Just('y'),
            Just('z'),
            Just('0'),
            Just('1'),
            Just('2'),
            Just(' '),
            Just('-'),
        ],
        0..=6,
    )
    .prop_map(|chars| {
        let value = chars.into_iter().collect::<String>();
        serde_json::to_string(&value).expect("string literal should encode")
    })
    .boxed()
}

fn render_numeric_array(values: &[i16]) -> String {
    let mut rendered = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            rendered.push_str(", ");
        }
        write!(&mut rendered, "{value}").expect("array literal should render");
    }
    rendered.push(']');
    rendered
}

fn render_object(entries: std::collections::BTreeMap<String, String>) -> String {
    let mut rendered = String::from("{");
    for (index, (key, value)) in entries.into_iter().enumerate() {
        if index > 0 {
            rendered.push_str(", ");
        }
        let encoded_key = serde_json::to_string(&key).expect("object key should encode");
        write!(&mut rendered, "{encoded_key}: {value}").expect("object property should render");
    }
    rendered.push('}');
    rendered
}
