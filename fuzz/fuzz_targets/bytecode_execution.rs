#![no_main]

use indexmap::IndexMap;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(program) = mustard::load_program(data) {
        let _ = mustard::start_bytecode(
            &program,
            mustard::ExecutionOptions {
                inputs: IndexMap::new(),
                capabilities: Vec::new(),
                limits: mustard::RuntimeLimits {
                    instruction_budget: 2_048,
                    heap_limit_bytes: 64 * 1024,
                    allocation_budget: 1_024,
                    ..mustard::RuntimeLimits::default()
                },
                cancellation_token: None,
            },
        );
    }

    let source = String::from_utf8_lossy(data);
    if let Ok(program) = mustard::compile(&source) {
        let _ = mustard::start(
            &program,
            mustard::ExecutionOptions {
                inputs: IndexMap::new(),
                capabilities: Vec::new(),
                limits: mustard::RuntimeLimits {
                    instruction_budget: 2_048,
                    heap_limit_bytes: 64 * 1024,
                    allocation_budget: 1_024,
                    ..mustard::RuntimeLimits::default()
                },
                cancellation_token: None,
            },
        );
    }
});
