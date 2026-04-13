use super::*;

#[test]
fn suspends_and_resumes_host_capability_calls() {
    let suspension = suspend(
        r#"
            const value = fetch_data(41);
            value + 1;
            "#,
        &["fetch_data"],
    );
    assert_eq!(suspension.capability, "fetch_data");
    assert_eq!(
        suspension.args,
        vec![number(41.0)]
    );

    let resumed = resume(
        suspension.snapshot,
        ResumePayload::Value(number(41.0)),
    )
    .expect("resume should succeed");

    match resumed {
        ExecutionStep::Completed(value) => {
            assert_eq!(value, number(42.0));
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn console_callbacks_resume_with_undefined_guest_results() {
    let suspension = suspend(
        r#"
            const logged = console.log(41);
            logged === undefined ? 2 : 0;
            "#,
        &["console.log"],
    );
    assert_eq!(suspension.capability, "console.log");
    assert_eq!(
        suspension.args,
        vec![number(41.0)]
    );

    let resumed = resume(
        suspension.snapshot,
        ResumePayload::Value(StructuredValue::String("ignored".to_string())),
    )
    .expect("resume should ignore host return values for console callbacks");

    match resumed {
        ExecutionStep::Completed(value) => {
            assert_eq!(value, number(2.0));
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn catches_host_errors_after_resume() {
    let suspension = suspend(
        r#"
            let captured;
            try {
              fetch_data(1);
            } catch (error) {
              captured = [error.name, error.message, error.code, error.details.status];
            }
            captured;
            "#,
        &["fetch_data"],
    );

    let resumed = resume(
        suspension.snapshot,
        ResumePayload::Error(HostError {
            name: "CapabilityError".to_string(),
            message: "upstream failed".to_string(),
            code: Some("E_UPSTREAM".to_string()),
            details: Some(StructuredValue::Object(IndexMap::from([(
                "status".to_string(),
                number(503.0),
            )]))),
        }),
    )
    .expect("guest catch should handle resumed host errors");

    match resumed {
        ExecutionStep::Completed(value) => {
            assert_eq!(
                value,
                StructuredValue::Array(vec![
                    StructuredValue::String("CapabilityError".to_string()),
                    StructuredValue::String("upstream failed".to_string()),
                    StructuredValue::String("E_UPSTREAM".to_string()),
                    number(503.0),
                ])
            );
        }
        other => panic!("expected completion, got {other:?}"),
    }
}
