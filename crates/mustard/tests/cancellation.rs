use std::{thread, time::Duration};

use indexmap::IndexMap;
use mustard::{
    CancellationToken, DiagnosticKind, ExecutionOptions, ExecutionStep, ResumePayload,
    RuntimeLimits, StructuredValue, compile, execute, resume, start,
};

fn assert_cancelled_limit(error: &mustard::MustardError) {
    match error {
        mustard::MustardError::Message { kind, message, .. } => {
            assert_eq!(*kind, DiagnosticKind::Limit);
            assert!(message.contains("execution cancelled"));
        }
        other => panic!("expected limit error, got {other:?}"),
    }
}

fn descending_numbers(len: usize) -> StructuredValue {
    StructuredValue::Array(
        (0..len)
            .map(|index| StructuredValue::from((len - index) as f64))
            .collect(),
    )
}

fn object_with_keys(len: usize) -> StructuredValue {
    StructuredValue::Object(
        (0..len)
            .map(|index| {
                (
                    format!("key_{index:05}"),
                    StructuredValue::from(index as f64),
                )
            })
            .collect::<IndexMap<_, _>>(),
    )
}

fn json_number_array(len: usize) -> String {
    format!("[{}0]", "0,".repeat(len))
}

fn json_string_literal(len: usize) -> String {
    format!("\"{}\"", "x".repeat(len))
}

fn repeated_digits(len: usize) -> String {
    "9".repeat(len)
}

fn large_number_array(len: usize) -> StructuredValue {
    StructuredValue::Array(
        (0..len)
            .map(|index| StructuredValue::from(index as f64))
            .collect(),
    )
}

#[test]
fn cooperative_cancellation_interrupts_running_guest_code() {
    let program = compile(
        r#"
        try {
          while (true) {}
        } catch (error) {
          "guest-caught";
        }
        "#,
    )
    .expect("source should compile");

    let token = CancellationToken::new();
    let canceller = token.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(1));
        canceller.cancel();
    });

    let error = execute(
        &program,
        ExecutionOptions {
            limits: RuntimeLimits {
                instruction_budget: usize::MAX,
                ..RuntimeLimits::default()
            },
            cancellation_token: Some(token),
            ..ExecutionOptions::default()
        },
    )
    .expect_err("running guest code should observe cancellation");

    worker.join().expect("canceller thread should finish");
    assert_cancelled_limit(&error);
}

#[test]
fn cooperative_cancellation_wins_under_gc_pressure() {
    let program = compile(
        r#"
        let total = 0;
        while (true) {
          const left = {};
          const right = {};
          left.peer = right;
          right.peer = left;
          total = total + 1;
        }
        "#,
    )
    .expect("source should compile");

    let token = CancellationToken::new();
    let canceller = token.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(1));
        canceller.cancel();
    });

    let error = execute(
        &program,
        ExecutionOptions {
            limits: RuntimeLimits {
                instruction_budget: usize::MAX,
                heap_limit_bytes: 24 * 1024,
                allocation_budget: 256,
                ..RuntimeLimits::default()
            },
            cancellation_token: Some(token),
            ..ExecutionOptions::default()
        },
    )
    .expect_err("running guest code under allocation pressure should still observe cancellation");

    worker.join().expect("canceller thread should finish");
    assert_cancelled_limit(&error);
}

#[test]
fn cancelling_a_suspended_async_host_wait_fails_top_level() {
    let program = compile(
        r#"
        async function main() {
          try {
            await fetch_data(1);
            return "done";
          } catch (error) {
            return "guest-caught";
          }
        }
        main();
        "#,
    )
    .expect("source should compile");

    let step = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("program should suspend");

    let suspension = match step {
        ExecutionStep::Suspended(suspension) => suspension,
        other => panic!("expected suspension, got {other:?}"),
    };

    let error = resume(suspension.snapshot, ResumePayload::Cancelled)
        .expect_err("cancelling a suspended host wait should fail the execution");
    let rendered = error.to_string();

    assert_cancelled_limit(&error);
    assert!(rendered.contains("at main ["));
    assert!(rendered.contains("at <script> ["));
}

#[test]
fn native_helper_array_sort_observes_instruction_budget() {
    let program = compile("big.sort(); 1;").expect("source should compile");
    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([("big".to_string(), descending_numbers(512))]),
            limits: RuntimeLimits {
                instruction_budget: 20,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect_err("native helper loop should consume instruction budget");

    match error {
        mustard::MustardError::Message { kind, message, .. } => {
            assert_eq!(kind, DiagnosticKind::Limit);
            assert!(message.contains("instruction budget exhausted"));
        }
        other => panic!("expected limit error, got {other:?}"),
    }
}

#[test]
fn native_helper_object_keys_observes_instruction_budget() {
    let program = compile("Object.keys(big).length;").expect("source should compile");
    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([("big".to_string(), object_with_keys(2_048))]),
            limits: RuntimeLimits {
                instruction_budget: 20,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect_err("native object helper should consume instruction budget");

    match error {
        mustard::MustardError::Message { kind, message, .. } => {
            assert_eq!(kind, DiagnosticKind::Limit);
            assert!(message.contains("instruction budget exhausted"));
        }
        other => panic!("expected limit error, got {other:?}"),
    }
}

#[test]
fn native_helper_array_sort_observes_cancellation() {
    let program = compile("big.sort(); 1;").expect("source should compile");

    let token = CancellationToken::new();
    let canceller = token.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(1));
        canceller.cancel();
    });

    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([("big".to_string(), descending_numbers(20_000))]),
            limits: RuntimeLimits {
                instruction_budget: usize::MAX,
                ..RuntimeLimits::default()
            },
            cancellation_token: Some(token),
            ..ExecutionOptions::default()
        },
    )
    .expect_err("native array helper should observe cancellation");

    worker.join().expect("canceller thread should finish");
    assert_cancelled_limit(&error);
}

#[test]
fn native_helper_object_keys_observes_cancellation() {
    let program = compile("Object.keys(big).length;").expect("source should compile");

    let token = CancellationToken::new();
    let canceller = token.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(1));
        canceller.cancel();
    });

    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([("big".to_string(), object_with_keys(40_000))]),
            limits: RuntimeLimits {
                instruction_budget: usize::MAX,
                ..RuntimeLimits::default()
            },
            cancellation_token: Some(token),
            ..ExecutionOptions::default()
        },
    )
    .expect_err("native object helper should observe cancellation");

    worker.join().expect("canceller thread should finish");
    assert_cancelled_limit(&error);
}

#[test]
fn json_parse_helper_observes_instruction_budget() {
    let program = compile("JSON.parse(text).length;").expect("source should compile");
    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([(
                "text".to_string(),
                StructuredValue::String(json_number_array(20_000)),
            )]),
            limits: RuntimeLimits {
                instruction_budget: 8,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect_err("JSON.parse should consume instruction budget");

    match error {
        mustard::MustardError::Message { kind, message, .. } => {
            assert_eq!(kind, DiagnosticKind::Limit);
            assert!(message.contains("instruction budget exhausted"));
        }
        other => panic!("expected limit error, got {other:?}"),
    }
}

#[test]
fn json_stringify_helper_observes_instruction_budget() {
    let program = compile("JSON.stringify(values).length;").expect("source should compile");
    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([("values".to_string(), descending_numbers(20_000))]),
            limits: RuntimeLimits {
                instruction_budget: 8,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect_err("JSON.stringify should consume instruction budget");

    match error {
        mustard::MustardError::Message { kind, message, .. } => {
            assert_eq!(kind, DiagnosticKind::Limit);
            assert!(message.contains("instruction budget exhausted"));
        }
        other => panic!("expected limit error, got {other:?}"),
    }
}

#[test]
fn json_parse_bare_string_respects_heap_limit() {
    let program = compile("JSON.parse(text);").expect("source should compile");
    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([(
                "text".to_string(),
                StructuredValue::String(json_string_literal(10_000)),
            )]),
            limits: RuntimeLimits {
                heap_limit_bytes: 15_000,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect_err("JSON.parse bare strings should respect the heap limit");

    match error {
        mustard::MustardError::Message { kind, message, .. } => {
            assert_eq!(kind, DiagnosticKind::Limit);
            assert!(message.contains("heap limit exceeded"));
        }
        other => panic!("expected limit error, got {other:?}"),
    }
}

#[test]
fn number_parse_int_observes_instruction_budget() {
    let program = compile("Number.parseInt(text, 10);").expect("source should compile");
    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([(
                "text".to_string(),
                StructuredValue::String(repeated_digits(20_000)),
            )]),
            limits: RuntimeLimits {
                instruction_budget: 8,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect_err("Number.parseInt should consume instruction budget");

    match error {
        mustard::MustardError::Message { kind, message, .. } => {
            assert_eq!(kind, DiagnosticKind::Limit);
            assert!(message.contains("instruction budget exhausted"));
        }
        other => panic!("expected limit error, got {other:?}"),
    }
}

#[test]
fn number_parse_float_observes_instruction_budget() {
    let program = compile("Number.parseFloat(text);").expect("source should compile");
    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([(
                "text".to_string(),
                StructuredValue::String(format!("{}.5", repeated_digits(20_000))),
            )]),
            limits: RuntimeLimits {
                instruction_budget: 8,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect_err("Number.parseFloat should consume instruction budget");

    match error {
        mustard::MustardError::Message { kind, message, .. } => {
            assert_eq!(kind, DiagnosticKind::Limit);
            assert!(message.contains("instruction budget exhausted"));
        }
        other => panic!("expected limit error, got {other:?}"),
    }
}

#[test]
fn json_parse_helper_observes_cancellation() {
    let program = compile("JSON.parse(text).length;").expect("source should compile");

    let token = CancellationToken::new();
    let canceller = token.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(1));
        canceller.cancel();
    });

    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([(
                "text".to_string(),
                StructuredValue::String(json_number_array(200_000)),
            )]),
            limits: RuntimeLimits {
                instruction_budget: usize::MAX,
                ..RuntimeLimits::default()
            },
            cancellation_token: Some(token),
            ..ExecutionOptions::default()
        },
    )
    .expect_err("JSON.parse should observe cancellation");

    worker.join().expect("canceller thread should finish");
    assert_cancelled_limit(&error);
}

#[test]
fn number_parse_int_observes_cancellation() {
    let program = compile("Number.parseInt(text, 10);").expect("source should compile");

    let token = CancellationToken::new();
    let canceller = token.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(1));
        canceller.cancel();
    });

    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([(
                "text".to_string(),
                StructuredValue::String(repeated_digits(2_000_000)),
            )]),
            limits: RuntimeLimits {
                instruction_budget: usize::MAX,
                ..RuntimeLimits::default()
            },
            cancellation_token: Some(token),
            ..ExecutionOptions::default()
        },
    )
    .expect_err("Number.parseInt should observe cancellation");

    worker.join().expect("canceller thread should finish");
    assert_cancelled_limit(&error);
}

#[test]
fn number_parse_float_observes_cancellation() {
    let program = compile("Number.parseFloat(text);").expect("source should compile");

    let token = CancellationToken::new();
    let canceller = token.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(1));
        canceller.cancel();
    });

    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([(
                "text".to_string(),
                StructuredValue::String(format!("{}.5", repeated_digits(2_000_000))),
            )]),
            limits: RuntimeLimits {
                instruction_budget: usize::MAX,
                ..RuntimeLimits::default()
            },
            cancellation_token: Some(token),
            ..ExecutionOptions::default()
        },
    )
    .expect_err("Number.parseFloat should observe cancellation");

    worker.join().expect("canceller thread should finish");
    assert_cancelled_limit(&error);
}

#[test]
fn json_stringify_helper_observes_cancellation() {
    let program = compile("JSON.stringify(values).length;").expect("source should compile");

    let token = CancellationToken::new();
    let canceller = token.clone();
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(1));
        canceller.cancel();
    });

    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::from([("values".to_string(), descending_numbers(200_000))]),
            limits: RuntimeLimits {
                instruction_budget: usize::MAX,
                ..RuntimeLimits::default()
            },
            cancellation_token: Some(token),
            ..ExecutionOptions::default()
        },
    )
    .expect_err("JSON.stringify should observe cancellation");

    worker.join().expect("canceller thread should finish");
    assert_cancelled_limit(&error);
}

#[test]
fn resume_payload_boundary_conversion_observes_instruction_budget() {
    let program = compile("fetch_data(); 0;").expect("source should compile");
    let suspension = match start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits {
                instruction_budget: 5,
                heap_limit_bytes: 1_000_000_000,
                allocation_budget: 2_000_000,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect("program should suspend")
    {
        ExecutionStep::Suspended(suspension) => *suspension,
        other => panic!("expected suspension, got {other:?}"),
    };

    let error = resume(
        suspension.snapshot,
        ResumePayload::Value(large_number_array(20_000)),
    )
    .expect_err("structured resume payload should consume instruction budget");
    match error {
        mustard::MustardError::Message { kind, message, .. } => {
            assert_eq!(kind, DiagnosticKind::Limit);
            assert!(message.contains("instruction budget exhausted"));
        }
        other => panic!("expected limit error, got {other:?}"),
    }
}

#[test]
fn host_argument_boundary_conversion_observes_instruction_budget() {
    let program = compile(
        r#"
        let values = [];
        for (let index = 0; index < 20000; index = index + 1) {
          values.push(index);
        }
        fetch_data(values);
        "#,
    )
    .expect("source should compile");
    let error = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits {
                instruction_budget: 5,
                heap_limit_bytes: 1_000_000_000,
                allocation_budget: 2_000_000,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect_err("structured host-call arguments should consume instruction budget");

    match error {
        mustard::MustardError::Message { kind, message, .. } => {
            assert_eq!(kind, DiagnosticKind::Limit);
            assert!(message.contains("instruction budget exhausted"));
        }
        other => panic!("expected limit error, got {other:?}"),
    }
}
