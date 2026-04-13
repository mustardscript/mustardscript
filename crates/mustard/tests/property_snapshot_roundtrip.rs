mod property_support;

use mustard::{ExecutionOptions, compile, start};
use proptest::prelude::*;

use property_support::{drive_with_echo, suspending_program_strategy};

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 48,
        .. ProptestConfig::default()
    })]

    #[test]
    fn generated_suspending_programs_round_trip_snapshots(source in suspending_program_strategy()) {
        let program = compile(&source)
            .unwrap_or_else(|error| panic!("generated suspending program should compile:\n{source}\n{error}"));

        let direct = drive_with_echo(
            start(
                &program,
                ExecutionOptions {
                    capabilities: vec!["fetch_data".to_string()],
                    ..ExecutionOptions::default()
                },
            )
            .unwrap_or_else(|error| panic!("generated suspending program should start:\n{source}\n{error}")),
            false,
        )
        .unwrap_or_else(|error| panic!("direct resume path should succeed:\n{source}\n{error}"));

        let serialized = drive_with_echo(
            start(
                &program,
                ExecutionOptions {
                    capabilities: vec!["fetch_data".to_string()],
                    ..ExecutionOptions::default()
                },
            )
            .unwrap_or_else(|error| panic!("generated suspending program should restart:\n{source}\n{error}")),
            true,
        )
        .unwrap_or_else(|error| panic!("serialized resume path should succeed:\n{source}\n{error}"));

        prop_assert_eq!(direct, serialized, "source: {}", source);
    }
}
