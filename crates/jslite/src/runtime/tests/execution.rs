use super::*;

#[test]
fn runs_arithmetic_and_locals() {
    let value = run(r#"
            const a = 4;
            const b = 3;
            a * b + 2;
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(14.0))
    );
}

#[test]
fn runs_functions_and_closures() {
    let value = run(r#"
            function makeAdder(x) {
              return (y) => x + y;
            }
            const add2 = makeAdder(2);
            add2(5);
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(7.0))
    );
}

#[test]
fn runs_arrays_objects_and_member_access() {
    let value = run(r#"
            const values = [1, 2];
            const record = { total: values[0] + values[1] };
            record.total;
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(3.0))
    );
}

#[test]
fn runs_branching_loops_and_switch() {
    let value = run(r#"
            let total = 0;
            let i = 0;
            while (i < 4) {
              total += i;
              i += 1;
            }
            do {
              total += 1;
            } while (false);
            for (let j = 0; j < 2; j += 1) {
              if (j === 0) {
                continue;
              }
              total += j;
            }
            switch (total) {
              case 8:
                total += 1;
                break;
              default:
                total = 0;
            }
            total;
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(9.0))
    );
}

#[test]
fn runs_math_and_json_builtins() {
    let value = run(r#"
            const encoded = JSON.stringify({ value: Math.max(1, 9, 4) });
            JSON.parse(encoded).value;
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(9.0))
    );
}

#[test]
fn preserves_supported_enumeration_order_for_json_stringify() {
    let value = run(r#"
            const record = {};
            record.beta = "b";
            record.alpha = "a";
            const values = ["c", "d"];
            values.extra = "ignored";
            JSON.stringify({ record, values });
            "#);
    assert_eq!(
        value,
        StructuredValue::String(
            r#"{"record":{"alpha":"a","beta":"b"},"values":["c","d"]}"#.to_string()
        )
    );
}

#[test]
fn enforces_instruction_budget() {
    let program = compile("while (true) {}").expect("source should compile");
    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: Vec::new(),
            limits: RuntimeLimits {
                instruction_budget: 100,
                ..RuntimeLimits::default()
            },
            cancellation_token: None,
        },
    )
    .expect_err("infinite loop should exhaust budget");
    assert!(error.to_string().contains("instruction budget exhausted"));
}
