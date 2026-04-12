use super::*;

#[test]
fn runs_throw_try_catch_and_finally() {
    let value = run(r#"
            let log = [];
            try {
              log[log.length] = "body";
              throw new Error("boom");
            } catch (error) {
              log[log.length] = error.name;
              log[log.length] = error.message;
            } finally {
              log[log.length] = "finally";
            }
            log;
            "#);
    assert_eq!(
        value,
        StructuredValue::Array(vec![
            StructuredValue::String("body".to_string()),
            StructuredValue::String("Error".to_string()),
            StructuredValue::String("boom".to_string()),
            StructuredValue::String("finally".to_string()),
        ])
    );
}

#[test]
fn catches_runtime_type_errors_as_guest_errors() {
    let value = run(r#"
            let captured;
            try {
              const value = null;
              value.answer;
            } catch (error) {
              captured = [error.name, error.message];
            }
            captured;
            "#);
    assert_eq!(
        value,
        StructuredValue::Array(vec![
            StructuredValue::String("TypeError".to_string()),
            StructuredValue::String("cannot read properties of nullish value".to_string()),
        ])
    );
}

#[test]
fn finally_runs_for_return_break_and_continue() {
    let value = run(r#"
            let events = [];
            function earlyReturn() {
              try {
                return "body";
              } finally {
                events[events.length] = "return";
              }
            }
            let index = 0;
            while (index < 2) {
              index += 1;
              try {
                if (index === 1) {
                  continue;
                }
                break;
              } finally {
                events[events.length] = index;
              }
            }
            [earlyReturn(), events];
            "#);
    assert_eq!(
        value,
        StructuredValue::Array(vec![
            StructuredValue::String("body".to_string()),
            StructuredValue::Array(vec![
                StructuredValue::Number(StructuredNumber::Finite(1.0)),
                StructuredValue::Number(StructuredNumber::Finite(2.0)),
                StructuredValue::String("return".to_string()),
            ]),
        ])
    );
}

#[test]
fn nested_exception_unwind_preserves_finally_order() {
    let value = run(r#"
            let events = [];
            function nested() {
              try {
                try {
                  events[events.length] = "inner-body";
                  throw new Error("boom");
                } catch (error) {
                  events[events.length] = error.message;
                  throw new TypeError("wrapped");
                } finally {
                  events[events.length] = "inner-finally";
                }
              } catch (error) {
                events[events.length] = error.name;
              } finally {
                events[events.length] = "outer-finally";
              }
              return events;
            }
            nested();
            "#);
    assert_eq!(
        value,
        StructuredValue::Array(vec![
            StructuredValue::String("inner-body".to_string()),
            StructuredValue::String("boom".to_string()),
            StructuredValue::String("inner-finally".to_string()),
            StructuredValue::String("TypeError".to_string()),
            StructuredValue::String("outer-finally".to_string()),
        ])
    );
}
