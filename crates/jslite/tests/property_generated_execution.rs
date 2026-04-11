mod property_support;

use jslite::{ExecutionOptions, RuntimeLimits, compile, execute, start};
use proptest::prelude::*;

use property_support::{assert_host_safe_message, completed_value, supported_program_strategy};

fn is_limit_error(message: &str, expected: &[&str]) -> bool {
    message.starts_with("Limit:") && expected.iter().any(|needle| message.contains(needle))
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        .. ProptestConfig::default()
    })]

    #[test]
    fn generated_supported_programs_compile_execute_and_start_consistently(source in supported_program_strategy()) {
        let program = compile(&source).unwrap_or_else(|error| panic!("generated program should compile:\n{source}\n{error}"));

        let executed = execute(&program, ExecutionOptions::default())
            .unwrap_or_else(|error| panic!("generated program should execute:\n{source}\n{error}"));

        let started = completed_value(
            start(&program, ExecutionOptions::default())
                .unwrap_or_else(|error| panic!("generated program should start cleanly:\n{source}\n{error}"))
        );

        prop_assert_eq!(executed, started, "source: {}", source);
    }

    #[test]
    fn generated_loop_stress_programs_finish_or_hit_instruction_limits(
        iterations in 0usize..80,
        budget in 1usize..200,
    ) {
        let source = format!(
            "let total = 0; for (let i = 0; i < {iterations}; i += 1) {{ total += i; }} total;"
        );
        let program = compile(&source).unwrap_or_else(|error| panic!("loop stress program should compile:\n{source}\n{error}"));
        let result = execute(
            &program,
            ExecutionOptions {
                limits: RuntimeLimits {
                    instruction_budget: budget,
                    ..RuntimeLimits::default()
                },
                ..ExecutionOptions::default()
            },
        );

        if let Err(error) = result {
            let message = error.to_string();
            assert_host_safe_message(&message);
            prop_assert!(
                is_limit_error(&message, &["instruction budget exhausted"]),
                "unexpected execution error for source `{source}`: {message}"
            );
        }
    }

    #[test]
    fn generated_allocation_stress_programs_finish_or_hit_heap_limits(
        count in 0usize..64,
        allocation_budget in 8usize..128,
        heap_limit_bytes in 128usize..2048,
    ) {
        let source = format!(
            "let values = []; for (let i = 0; i < {count}; i += 1) {{ values.push([i, i + 1, i + 2]); }} values.length;"
        );
        let program = compile(&source).unwrap_or_else(|error| panic!("allocation stress program should compile:\n{source}\n{error}"));
        let result = execute(
            &program,
            ExecutionOptions {
                limits: RuntimeLimits {
                    allocation_budget,
                    heap_limit_bytes,
                    ..RuntimeLimits::default()
                },
                ..ExecutionOptions::default()
            },
        );

        if let Err(error) = result {
            let message = error.to_string();
            assert_host_safe_message(&message);
            prop_assert!(
                is_limit_error(&message, &["allocation budget exhausted", "heap limit exceeded"]),
                "unexpected execution error for source `{source}`: {message}"
            );
        }
    }
}
