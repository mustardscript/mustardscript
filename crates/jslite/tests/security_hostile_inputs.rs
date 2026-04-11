use indexmap::IndexMap;
use jslite::runtime::{FunctionPrototype, Instruction};
use jslite::{
    BytecodeProgram, ExecutionOptions, HostError, ResumePayload, RuntimeLimits, StructuredValue,
    compile, dump_program, dump_snapshot, load_program, load_snapshot, lower_to_bytecode, start,
    start_bytecode,
};
use proptest::prelude::*;

const SAFE_MESSAGE_PATH_FRAGMENTS: &[&str] = &["/Users/", "\\Users\\", "C:\\", "/home/"];

fn assert_host_safe_message(message: &str) {
    for fragment in SAFE_MESSAGE_PATH_FRAGMENTS {
        assert!(
            !message.contains(fragment),
            "message leaked host path fragment `{fragment}`: {message}"
        );
    }
}

fn simple_function(code: Vec<Instruction>) -> FunctionPrototype {
    FunctionPrototype {
        name: None,
        params: Vec::new(),
        code,
        is_async: false,
        span: jslite::span::SourceSpan::new(0, 0),
    }
}

fn suspended_snapshot_bytes() -> Vec<u8> {
    let program = compile("const value = fetch_data(1); value + 2;").expect("compile should work");
    let step = start(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("program should start");

    let snapshot = match step {
        jslite::ExecutionStep::Completed(_) => panic!("program should suspend"),
        jslite::ExecutionStep::Suspended(suspension) => suspension.snapshot,
    };
    dump_snapshot(&snapshot).expect("snapshot should serialize")
}

fn iterating_snapshot_bytes() -> Vec<u8> {
    let program = compile(
        r#"
        let total = 0;
        for (const value of [1, 2, 3]) {
          total += fetch_data(value);
        }
        total;
        "#,
    )
    .expect("compile should work");
    let step = start(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("program should start");

    let snapshot = match step {
        jslite::ExecutionStep::Completed(_) => panic!("program should suspend"),
        jslite::ExecutionStep::Suspended(suspension) => suspension.snapshot,
    };
    dump_snapshot(&snapshot).expect("snapshot should serialize")
}

fn keyed_collection_snapshot_bytes() -> Vec<u8> {
    let program = compile(
        r#"
        const key = { label: 'shared' };
        const map = new Map();
        const set = new Set();
        map.set(key, set);
        set.add(map);
        fetch_data(1);
        "#,
    )
    .expect("compile should work");
    let step = start(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("program should start");

    let snapshot = match step {
        jslite::ExecutionStep::Completed(_) => panic!("program should suspend"),
        jslite::ExecutionStep::Suspended(suspension) => suspension.snapshot,
    };
    dump_snapshot(&snapshot).expect("snapshot should serialize")
}

fn byte_mutations(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut cases = vec![
        Vec::new(),
        vec![0],
        vec![0xff],
        bytes.iter().rev().copied().collect(),
    ];

    for index in 0..bytes.len().min(64) {
        let mut flipped = bytes.to_vec();
        flipped[index] ^= 0xa5;
        cases.push(flipped);
    }

    for cut in 0..=bytes.len().min(64) {
        cases.push(bytes[..cut].to_vec());
    }

    let mut appended = bytes.to_vec();
    appended.extend_from_slice(b"hostile-trailer");
    cases.push(appended);

    cases
}

#[test]
fn hostile_sources_fail_closed_without_host_leaks() {
    std::thread::Builder::new()
        .name("hostile-source-suite".to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let mut cases = vec![
                String::new(),
                "\0".repeat(16),
                "(".repeat(256),
                "{".repeat(256),
                "const value = `unterminated".to_string(),
                "function x(".repeat(32),
                "while (true) {".repeat(32),
                "eval(".repeat(256),
                "import('fs')".to_string(),
                "export const value = 1;".to_string(),
                "delete target.value;".to_string(),
            ];
            cases.push(format!("{}1{}", "(".repeat(256), ")".repeat(256)));

            for source in cases {
                if let Err(error) = compile(&source) {
                    assert_host_safe_message(&error.to_string());
                }
            }
        })
        .expect("hostile source thread should spawn")
        .join()
        .expect("hostile source thread should finish");
}

#[test]
fn crafted_bytecode_inputs_fail_validation_before_execution() {
    let invalid_programs = vec![
        BytecodeProgram {
            functions: Vec::new(),
            root: 0,
        },
        BytecodeProgram {
            functions: vec![simple_function(vec![
                Instruction::Jump(99),
                Instruction::Return,
            ])],
            root: 0,
        },
        BytecodeProgram {
            functions: vec![simple_function(vec![
                Instruction::PopEnv,
                Instruction::Return,
            ])],
            root: 0,
        },
        BytecodeProgram {
            functions: vec![simple_function(vec![
                Instruction::PushPendingJump {
                    target: 1,
                    target_handler_depth: 1,
                    target_scope_depth: 0,
                },
                Instruction::Return,
            ])],
            root: 0,
        },
    ];

    for program in invalid_programs {
        let error = start_bytecode(&program, ExecutionOptions::default())
            .expect_err("invalid bytecode should fail validation");
        assert_host_safe_message(&error.to_string());
    }
}

#[test]
fn mutated_serialized_programs_fail_safely() {
    let source = compile("const value = 41; value + 1;").expect("compile should succeed");
    let program = lower_to_bytecode(&source).expect("lowering should succeed");
    let bytes = dump_program(&program).expect("program should serialize");

    for mutated in byte_mutations(&bytes) {
        if let Err(error) = load_program(&mutated) {
            assert_host_safe_message(&error.to_string());
        }
    }
}

#[test]
fn mutated_snapshots_fail_safely() {
    let bytes = suspended_snapshot_bytes();

    for mutated in byte_mutations(&bytes) {
        if let Err(error) = load_snapshot(&mutated) {
            assert_host_safe_message(&error.to_string());
        }
    }
}

#[test]
fn mutated_iteration_snapshots_fail_safely() {
    let bytes = iterating_snapshot_bytes();

    for mutated in byte_mutations(&bytes) {
        if let Err(error) = load_snapshot(&mutated) {
            assert_host_safe_message(&error.to_string());
        }
    }
}

#[test]
fn mutated_keyed_collection_snapshots_fail_safely() {
    let bytes = keyed_collection_snapshot_bytes();

    for mutated in byte_mutations(&bytes) {
        if let Err(error) = load_snapshot(&mutated) {
            assert_host_safe_message(&error.to_string());
        }
    }
}

#[test]
fn compound_limit_failures_remain_guest_safe() {
    let compute = compile("while (true) {}").expect("compile should succeed");
    let compute_error = start(
        &compute,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: Vec::new(),
            limits: RuntimeLimits {
                instruction_budget: 64,
                ..RuntimeLimits::default()
            },
            cancellation_token: None,
        },
    )
    .expect_err("instruction limit should trigger");
    assert_host_safe_message(&compute_error.to_string());

    let heap = compile(
        "const values = []; while (true) { values[values.length] = { payload: 'xxxxxxxx' }; }",
    )
    .expect("compile should succeed");
    let heap_error = start(
        &heap,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: Vec::new(),
            limits: RuntimeLimits {
                instruction_budget: 10_000,
                heap_limit_bytes: 4_096,
                allocation_budget: 128,
                ..RuntimeLimits::default()
            },
            cancellation_token: None,
        },
    )
    .expect_err("heap or allocation limit should trigger");
    assert_host_safe_message(&heap_error.to_string());
}

#[test]
fn sanitized_resume_errors_preserve_safe_shape() {
    let program = compile(
        "let output = 'ok'; try { const value = fetch_data(1); value + 1; } catch (error) { output = error.name + ':' + error.message; } output;",
    )
    .expect("compile should work");
    let step = start(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: vec!["fetch_data".to_string()],
            limits: RuntimeLimits::default(),
            cancellation_token: None,
        },
    )
    .expect("program should suspend");

    let snapshot = match step {
        jslite::ExecutionStep::Completed(_) => panic!("program should suspend"),
        jslite::ExecutionStep::Suspended(suspension) => suspension.snapshot,
    };

    let resumed = jslite::resume(
        snapshot,
        ResumePayload::Error(HostError {
            name: "CapabilityError".to_string(),
            message: "host capability failed".to_string(),
            code: Some("E_HOST".to_string()),
            details: Some(StructuredValue::String("safe details".to_string())),
        }),
    )
    .expect("resume should succeed");

    let rendered = match resumed {
        jslite::ExecutionStep::Completed(value) => format!("{value:?}"),
        jslite::ExecutionStep::Suspended(_) => panic!("program should complete"),
    };
    assert!(rendered.contains("CapabilityError"));
    assert!(rendered.contains("host capability failed"));
    assert_host_safe_message(&rendered);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn parser_and_ir_lowering_handle_arbitrary_source(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let source = String::from_utf8_lossy(&bytes);
        if let Err(error) = compile(&source) {
            assert_host_safe_message(&error.to_string());
        }
    }

    #[test]
    fn bytecode_execution_handles_arbitrary_compilable_source(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let source = String::from_utf8_lossy(&bytes);
        if let Ok(program) = compile(&source)
            && let Err(error) = start(
                &program,
                ExecutionOptions {
                    inputs: IndexMap::new(),
                    capabilities: Vec::new(),
                    limits: RuntimeLimits {
                        instruction_budget: 2_048,
                        heap_limit_bytes: 64 * 1024,
                        allocation_budget: 1_024,
                        ..RuntimeLimits::default()
                    },
                    cancellation_token: None,
                },
            ) {
            assert_host_safe_message(&error.to_string());
        }
    }

    #[test]
    fn compiled_program_loader_handles_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        if let Err(error) = load_program(&bytes) {
            assert_host_safe_message(&error.to_string());
        }
    }

    #[test]
    fn snapshot_loader_handles_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        if let Err(error) = load_snapshot(&bytes) {
            assert_host_safe_message(&error.to_string());
        }
    }
}
