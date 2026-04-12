use std::{thread, time::Duration};

use indexmap::IndexMap;
use jslite::{
    CancellationToken, DiagnosticKind, ExecutionOptions, ExecutionStep, ResumePayload,
    RuntimeLimits, StructuredValue, compile, execute, resume, start,
};

fn assert_cancelled_limit(error: &jslite::JsliteError) {
    match error {
        jslite::JsliteError::Message { kind, message, .. } => {
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
        jslite::JsliteError::Message { kind, message, .. } => {
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
        jslite::JsliteError::Message { kind, message, .. } => {
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
