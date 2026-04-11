#![no_main]

use indexmap::IndexMap;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(program) = jslite::load_program(data) {
        let _ = jslite::start_bytecode(
            &program,
            jslite::ExecutionOptions {
                inputs: IndexMap::new(),
                capabilities: Vec::new(),
                limits: jslite::RuntimeLimits {
                    instruction_budget: 2_048,
                    heap_limit_bytes: 64 * 1024,
                    allocation_budget: 1_024,
                    ..jslite::RuntimeLimits::default()
                },
            },
        );
    }

    let source = String::from_utf8_lossy(data);
    if let Ok(program) = jslite::compile(&source) {
        let _ = jslite::start(
            &program,
            jslite::ExecutionOptions {
                inputs: IndexMap::new(),
                capabilities: Vec::new(),
                limits: jslite::RuntimeLimits {
                    instruction_budget: 2_048,
                    heap_limit_bytes: 64 * 1024,
                    allocation_budget: 1_024,
                    ..jslite::RuntimeLimits::default()
                },
            },
        );
    }
});
