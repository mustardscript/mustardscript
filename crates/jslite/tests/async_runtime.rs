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

#[test]
fn promise_instance_methods_and_combinators_run_for_supported_cases() {
    let program = compile(
        r#"
        async function main() {
          let events = [];
          const chained = await Promise.resolve(3)
            .then((value) => {
              events[events.length] = "then:" + value;
              return value + 4;
            })
            .finally(() => {
              events[events.length] = "finally";
            });
          const recovered = await Promise.reject("boom").catch((reason) => {
            events[events.length] = "catch:" + reason;
            return reason + ":handled";
          });
          const all = await Promise.all([1, Promise.resolve(2), chained]);
          const race = await Promise.race([Promise.resolve("fast"), Promise.resolve("slow")]);
          const any = await Promise.any([Promise.reject("x"), Promise.resolve("winner")]);
          const settled = await Promise.allSettled([Promise.resolve(1), Promise.reject("nope")]);
          return [chained, recovered, all, race, any, settled, events];
        }
        main();
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            number(7.0),
            "boom:handled".into(),
            StructuredValue::Array(vec![number(1.0), number(2.0), number(7.0)]),
            "fast".into(),
            "winner".into(),
            StructuredValue::Array(vec![
                StructuredValue::Object(IndexMap::from([
                    ("status".to_string(), "fulfilled".into()),
                    ("value".to_string(), number(1.0)),
                ])),
                StructuredValue::Object(IndexMap::from([
                    ("status".to_string(), "rejected".into()),
                    ("reason".to_string(), "nope".into()),
                ])),
            ]),
            StructuredValue::Array(vec!["then:3".into(), "finally".into(), "catch:boom".into(),]),
        ])
    );
}

#[test]
fn promise_any_rejects_with_aggregate_error_details() {
    let program = compile(
        r#"
        async function main() {
          try {
            await Promise.any([Promise.reject("alpha"), Promise.reject("beta")]);
            return "unreachable";
          } catch (error) {
            return [error.name, error.message, error.errors];
          }
        }
        main();
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            "AggregateError".into(),
            "All promises were rejected".into(),
            StructuredValue::Array(vec!["alpha".into(), "beta".into()]),
        ])
    );
}

#[test]
fn promise_callbacks_can_suspend_through_host_capabilities() {
    let program = compile(
        r#"
        async function main() {
          return await Promise.resolve(7).then(fetch_data);
        }
        main();
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
    assert_eq!(suspended.args, vec![number(7.0)]);

    let resumed = resume(suspended.snapshot, ResumePayload::Value(number(21.0)))
        .expect("resume should succeed");
    match resumed {
        ExecutionStep::Completed(value) => assert_eq!(value, number(21.0)),
        ExecutionStep::Suspended(_) => panic!("expected completion after resume"),
    }
}

#[test]
fn promise_constructors_bridge_async_host_calls_and_thenable_adoption() {
    let program = compile(
        r#"
        function wrapDouble(value) {
          return new Promise((resolve, reject) => {
            Promise.resolve(value)
              .then((resolved) => resolve(resolved * 2))
              .catch(reject);
          });
        }
        async function waitForApproval(ticketId) {
          return await new Promise((resolve, reject) => {
            fetch_decision(ticketId)
              .then((decision) => {
                if (decision.approved) {
                  resolve(decision.ticketId);
                } else {
                  reject(decision.reason);
                }
              })
              .catch(reject);
          });
        }
        async function main() {
          const thenable = {};
          thenable.then = function(resolve) {
            resolve(wrapDouble(5));
          };
          return [await Promise.resolve(thenable), await waitForApproval("A-9")];
        }
        main();
        "#,
    )
    .expect("source should compile");

    let suspended = match start(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_decision".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("start should succeed")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected suspension, got {value:?}"),
    };

    assert_eq!(suspended.capability, "fetch_decision");
    assert_eq!(suspended.args, vec!["A-9".into()]);

    let resumed = resume(
        suspended.snapshot,
        ResumePayload::Value(StructuredValue::Object(IndexMap::from([
            ("approved".to_string(), true.into()),
            ("ticketId".to_string(), "A-9:approved".into()),
        ]))),
    )
    .expect("resume should succeed");
    match resumed {
        ExecutionStep::Completed(value) => assert_eq!(
            value,
            StructuredValue::Array(vec![number(10.0), "A-9:approved".into(),])
        ),
        ExecutionStep::Suspended(other) => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn promise_constructors_preserve_rejection_propagation_and_cleanup() {
    let program = compile(
        r#"
        async function main() {
          let events = [];
          const denied = await new Promise((resolve, reject) => {
            events[events.length] = "executor:start";
            reject("manual-review");
            resolve("ignored");
            events[events.length] = "executor:cleanup";
            throw new Error("ignored");
          }).catch((reason) => {
            events[events.length] = "catch:" + reason;
            return reason;
          });
          const thenable = {};
          thenable.then = function(resolve, reject) {
            events[events.length] = "thenable:start";
            reject("thenable:no");
            resolve("ignored");
            events[events.length] = "thenable:cleanup";
            throw new Error("ignored");
          };

          const adopted = await Promise.resolve(thenable).catch((reason) => {
            events[events.length] = "adopted:" + reason;
            return reason;
          });

          return [denied, adopted, events];
        }
        main();
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec![
            "manual-review".into(),
            "thenable:no".into(),
            StructuredValue::Array(vec![
                "executor:start".into(),
                "executor:cleanup".into(),
                "catch:manual-review".into(),
                "thenable:start".into(),
                "thenable:cleanup".into(),
                "adopted:thenable:no".into(),
            ]),
        ])
    );
}

#[test]
fn promise_constructors_preserve_thrown_values_before_settlement() {
    let program = compile(
        r#"
        async function main() {
          const thrown = await new Promise((resolve, reject) => {
            throw "boom";
          }).catch((reason) => reason);

          const thenable = {};
          thenable.then = function(resolve, reject) {
            throw "thenable:explode";
          };
          const adopted = await Promise.resolve(thenable).catch((reason) => reason);

          return [thrown, adopted];
        }
        main();
        "#,
    )
    .expect("source should compile");

    let result = execute(&program, ExecutionOptions::default()).expect("program should run");
    assert_eq!(
        result,
        StructuredValue::Array(vec!["boom".into(), "thenable:explode".into(),])
    );
}

#[test]
fn array_map_callbacks_can_feed_promise_all_from_async_guest_flows() {
    let program = compile(
        r#"
        async function main() {
          const values = await Promise.all([1, 2].map((value) => fetch_data(value)));
          return values;
        }
        main();
        "#,
    )
    .expect("source should compile");

    let first = match start(
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

    assert_eq!(first.capability, "fetch_data");
    assert_eq!(first.args, vec![number(1.0)]);

    let second = match resume(first.snapshot, ResumePayload::Value(number(10.0)))
        .expect("resume should succeed")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected a second suspension, got {value:?}"),
    };

    assert_eq!(second.capability, "fetch_data");
    assert_eq!(second.args, vec![number(2.0)]);

    let completed =
        resume(second.snapshot, ResumePayload::Value(number(20.0))).expect("resume should succeed");
    match completed {
        ExecutionStep::Completed(value) => assert_eq!(
            value,
            StructuredValue::Array(vec![number(10.0), number(20.0)])
        ),
        ExecutionStep::Suspended(other) => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn array_from_mapping_can_feed_promise_all_from_async_guest_flows() {
    let program = compile(
        r#"
        async function main() {
          const values = await Promise.all(
            Array.from(new Set([1, 2]), (value) => fetch_data(value))
          );
          return values;
        }
        main();
        "#,
    )
    .expect("source should compile");

    let first = match start(
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

    assert_eq!(first.capability, "fetch_data");
    assert_eq!(first.args, vec![number(1.0)]);

    let second = match resume(first.snapshot, ResumePayload::Value(number(100.0)))
        .expect("resume should succeed")
    {
        ExecutionStep::Suspended(suspension) => suspension,
        ExecutionStep::Completed(value) => panic!("expected a second suspension, got {value:?}"),
    };

    assert_eq!(second.capability, "fetch_data");
    assert_eq!(second.args, vec![number(2.0)]);

    let completed = resume(second.snapshot, ResumePayload::Value(number(200.0)))
        .expect("resume should succeed");
    match completed {
        ExecutionStep::Completed(value) => assert_eq!(
            value,
            StructuredValue::Array(vec![number(100.0), number(200.0)])
        ),
        ExecutionStep::Suspended(other) => panic!("expected completion, got {other:?}"),
    }
}
