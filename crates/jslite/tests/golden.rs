use std::{
    fs,
    path::{Path, PathBuf},
};

use jslite::{compile, lower_to_bytecode};

const GOLDEN_SOURCE: &str = r#"
const makeTotal = (left, right = 1) => {
  const pair = [left, right];
  return { total: pair[0] + pair[1] };
};
makeTotal(2).total;
"#;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden_path(category: &str, name: &str) -> PathBuf {
    workspace_root()
        .join("tests")
        .join("golden")
        .join(category)
        .join(name)
}

fn read_golden(category: &str, name: &str) -> String {
    fs::read_to_string(golden_path(category, name))
        .unwrap_or_else(|error| panic!("failed to read golden {category}/{name}: {error}"))
}

fn normalize(value: &str) -> String {
    value.replace("\r\n", "\n").trim_end().to_string()
}

fn assert_golden(category: &str, name: &str, actual: &str) {
    let expected = read_golden(category, name);
    assert_eq!(normalize(&expected), normalize(actual));
}

#[test]
fn diagnostics_match_golden_files() {
    let eval_error = compile("eval('1 + 1');").expect_err("free eval should be rejected");
    assert_golden("diagnostics", "free-eval.txt", &eval_error.to_string());

    let function_error =
        compile("new Function('return 1;');").expect_err("free Function should be rejected");
    assert_golden(
        "diagnostics",
        "free-function-constructor.txt",
        &function_error.to_string(),
    );
}

#[test]
fn ir_lowering_matches_golden() {
    let program = compile(GOLDEN_SOURCE).expect("golden source should compile");
    let actual = serde_json::to_string_pretty(&program.script).expect("ir should serialize");
    assert_golden("ir", "make-total.json", &actual);
}

#[test]
fn bytecode_lowering_matches_golden() {
    let program = compile(GOLDEN_SOURCE).expect("golden source should compile");
    let bytecode = lower_to_bytecode(&program).expect("bytecode lowering should succeed");
    let actual = serde_json::to_string_pretty(&bytecode).expect("bytecode should serialize");
    assert_golden("bytecode", "make-total.json", &actual);
}
