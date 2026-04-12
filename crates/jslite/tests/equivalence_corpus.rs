use std::{fs, path::PathBuf};

use jslite::{
    ExecutionOptions, ExecutionStep, HostError, ResumeOptions, ResumePayload, RuntimeLimits,
    SnapshotPolicy, StructuredValue, compile, dump_program, dump_snapshot, execute, load_program,
    load_snapshot, lower_to_bytecode, resume, resume_with_options, start, start_bytecode,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct CorpusCase {
    id: String,
    source: String,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    steps: Vec<CorpusStep>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum CorpusStep {
    Value {
        value: Value,
    },
    Error {
        name: String,
        message: String,
        #[serde(default)]
        code: Option<String>,
        #[serde(default)]
        details: Option<Value>,
    },
}

fn snapshot_policy(capabilities: &[String]) -> SnapshotPolicy {
    SnapshotPolicy {
        capabilities: capabilities.to_vec(),
        limits: RuntimeLimits::default(),
    }
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/shared/equivalence-corpus.json")
}

fn load_corpus() -> Vec<CorpusCase> {
    let path = corpus_path();
    let body = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read corpus {}: {error}", path.display()));
    serde_json::from_str(&body)
        .unwrap_or_else(|error| panic!("failed to parse corpus {}: {error}", path.display()))
}

fn structured_from_json(value: Value) -> StructuredValue {
    match value {
        Value::Null => StructuredValue::Null,
        Value::Bool(value) => StructuredValue::Bool(value),
        Value::Number(value) => StructuredValue::from(
            value
                .as_f64()
                .unwrap_or_else(|| panic!("corpus numbers must fit f64: {value}")),
        ),
        Value::String(value) => StructuredValue::String(value),
        Value::Array(values) => {
            StructuredValue::Array(values.into_iter().map(structured_from_json).collect())
        }
        Value::Object(values) => StructuredValue::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, structured_from_json(value)))
                .collect(),
        ),
    }
}

fn payload_from_corpus_step(step: CorpusStep) -> ResumePayload {
    match step {
        CorpusStep::Value { value } => ResumePayload::Value(structured_from_json(value)),
        CorpusStep::Error {
            name,
            message,
            code,
            details,
        } => ResumePayload::Error(HostError {
            name,
            message,
            code,
            details: details.map(structured_from_json),
        }),
    }
}

fn drive_case(
    step: ExecutionStep,
    case: &CorpusCase,
    serialize_each_snapshot: bool,
) -> StructuredValue {
    let mut current = step;
    let mut index = 0usize;

    loop {
        match current {
            ExecutionStep::Completed(value) => {
                assert_eq!(
                    index,
                    case.steps.len(),
                    "case `{}` completed after {index} host resumes but corpus defines {}",
                    case.id,
                    case.steps.len(),
                );
                return value;
            }
            ExecutionStep::Suspended(suspension) => {
                let corpus_step = case.steps.get(index).unwrap_or_else(|| {
                    panic!(
                        "case `{}` suspended on `{}` but corpus defines only {} steps",
                        case.id,
                        suspension.capability,
                        case.steps.len(),
                    )
                });
                assert!(
                    case.capabilities.contains(&suspension.capability),
                    "case `{}` suspended on unexpected capability `{}`",
                    case.id,
                    suspension.capability,
                );
                let payload = payload_from_corpus_step(corpus_step.clone());
                index += 1;

                current = if serialize_each_snapshot {
                    let snapshot =
                        load_snapshot(&dump_snapshot(&suspension.snapshot).unwrap_or_else(
                            |error| panic!("case `{}` snapshot dump failed: {error}", case.id),
                        ))
                        .unwrap_or_else(|error| {
                            panic!("case `{}` snapshot load failed: {error}", case.id)
                        });
                    resume_with_options(
                        snapshot,
                        payload,
                        ResumeOptions {
                            cancellation_token: None,
                            snapshot_policy: Some(snapshot_policy(&case.capabilities)),
                        },
                    )
                    .unwrap_or_else(|error| {
                        panic!("case `{}` serialized resume failed: {error}", case.id)
                    })
                } else {
                    resume(suspension.snapshot, payload).unwrap_or_else(|error| {
                        panic!("case `{}` direct resume failed: {error}", case.id)
                    })
                };
            }
        }
    }
}

#[test]
fn shared_equivalence_corpus_agrees_across_core_execution_paths() {
    for case in load_corpus() {
        let options = ExecutionOptions {
            capabilities: case.capabilities.clone(),
            ..ExecutionOptions::default()
        };
        let program = compile(&case.source).unwrap_or_else(|error| {
            panic!(
                "case `{}` should compile:\n{}\n{error}",
                case.id, case.source
            )
        });

        let bytecode = lower_to_bytecode(&program).unwrap_or_else(|error| {
            panic!("case `{}` should lower:\n{}\n{error}", case.id, case.source)
        });
        let loaded_program = load_program(
            &dump_program(&bytecode)
                .unwrap_or_else(|error| panic!("case `{}` program dump failed: {error}", case.id)),
        )
        .unwrap_or_else(|error| panic!("case `{}` program load failed: {error}", case.id));

        let canonical = drive_case(
            start(&program, options.clone())
                .unwrap_or_else(|error| panic!("case `{}` direct start failed: {error}", case.id)),
            &case,
            false,
        );

        let loaded_outcome = drive_case(
            start_bytecode(&loaded_program, options.clone())
                .unwrap_or_else(|error| panic!("case `{}` loaded start failed: {error}", case.id)),
            &case,
            false,
        );
        assert_eq!(
            canonical, loaded_outcome,
            "case `{}` loaded-program drifted",
            case.id
        );

        if case.steps.is_empty() {
            let executed = execute(&program, options.clone())
                .unwrap_or_else(|error| panic!("case `{}` execute failed: {error}", case.id));
            assert_eq!(
                canonical, executed,
                "case `{}` execute drifted from start()",
                case.id
            );
        } else {
            let serialized = drive_case(
                start(&program, options.clone()).unwrap_or_else(|error| {
                    panic!("case `{}` serialized start failed: {error}", case.id)
                }),
                &case,
                true,
            );
            assert_eq!(
                canonical, serialized,
                "case `{}` dump/load snapshot path drifted",
                case.id
            );

            let loaded_serialized = drive_case(
                start_bytecode(&loaded_program, options.clone()).unwrap_or_else(|error| {
                    panic!("case `{}` loaded serialized start failed: {error}", case.id)
                }),
                &case,
                true,
            );
            assert_eq!(
                canonical, loaded_serialized,
                "case `{}` loaded program plus snapshot round-trip drifted",
                case.id
            );
        }
    }
}
