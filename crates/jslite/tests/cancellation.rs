use std::{thread, time::Duration};

use jslite::{
    CancellationToken, DiagnosticKind, ExecutionOptions, ExecutionStep, ResumePayload,
    RuntimeLimits, compile, execute, resume, start,
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
