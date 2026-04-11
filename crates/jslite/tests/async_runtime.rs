use indexmap::IndexMap;

use jslite::{
    ExecutionOptions, ExecutionStep, HostError, ResumePayload, RuntimeLimits, StructuredValue,
    compile, execute, resume, start,
};

fn number(value: f64) -> StructuredValue {
    StructuredValue::from(value)
}

#[test]
fn executes_async_functions_and_microtasks() {
    let program = compile(
        r#"
        let events = [];
        async function tick(label, value) {
          events[events.length] = label + ":start";
          const resolved = await Promise.resolve(value);
          events[events.length] = label + ":end:" + resolved;
          return resolved;
        }
        async function main() {
          const first = tick("a", 1);
          const second = tick("b", 2);
          events[events.length] = "sync";
          return [await first, await second, events];
        }
        main();
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            number(1.0),
            number(2.0),
            StructuredValue::Array(vec![
                "a:start".into(),
                "b:start".into(),
                "sync".into(),
                "a:end:1".into(),
                "b:end:2".into(),
            ]),
        ])
    );
}

#[test]
fn suspends_and_resumes_async_host_calls() {
    let program = compile(
        r#"
        async function load(value) {
          const resolved = await fetch_data(value);
          return resolved * 2;
        }
        load(21);
        "#,
    )
    .expect("source should compile");

    let suspended = match start(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("start should succeed")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected suspension, got {value:?}"),
    };

    assert_eq!(suspended.capability, "fetch_data");
    assert_eq!(suspended.args, vec![number(21.0)]);

    let resumed = resume(suspended.snapshot, ResumePayload::Value(number(21.0)))
        .expect("resume should succeed");
    match resumed {
        ExecutionStep::Completed(value) => assert_eq!(value, number(42.0)),
        ExecutionStep::Suspended(_) => panic!("expected completion after resume"),
    }
}

#[test]
fn async_await_catches_rejections() {
    let program = compile(
        r#"
        async function load() {
          try {
            await fetch_data(1);
          } catch (error) {
            return [error.name, error.message, error.code, error.details.reason];
          }
        }
        load();
        "#,
    )
    .expect("source should compile");

    let suspended = match start(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("start should succeed")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected suspension, got {value:?}"),
    };

    let resumed = resume(
        suspended.snapshot,
        ResumePayload::Error(HostError {
            name: "CapabilityError".to_string(),
            message: "upstream failed".to_string(),
            code: Some("E_UPSTREAM".to_string()),
            details: Some(StructuredValue::Object(IndexMap::from([(
                "reason".to_string(),
                "timeout".into(),
            )]))),
        }),
    )
    .expect("resume should succeed");

    match resumed {
        ExecutionStep::Completed(value) => {
            assert_eq!(
                value,
                StructuredValue::Array(vec![
                    "CapabilityError".into(),
                    "upstream failed".into(),
                    "E_UPSTREAM".into(),
                    "timeout".into(),
                ])
            );
        }
        ExecutionStep::Suspended(_) => panic!("expected completion after resume"),
    }
}

#[test]
fn enforces_outstanding_host_call_limits_for_async_guest_code() {
    let program = compile(
        r#"
        async function fanOut() {
          const first = fetch_data(1);
          const second = fetch_data(2);
          return (await first) + (await second);
        }
        fanOut();
        "#,
    )
    .expect("source should compile");

    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits {
                max_outstanding_host_calls: 1,
                ..RuntimeLimits::default()
            },
            cancellation_token: None,
        },
    )
    .expect_err("execution should fail closed");

    assert!(
        error
            .to_string()
            .contains("outstanding host-call limit exhausted")
    );
}
