#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let source = String::from_utf8_lossy(data);
    if let Ok(program) = mustard::compile(&source) {
        let _ = serde_json::to_vec(&program.script);
    }
});
