use mustard::{ExecutionOptions, StructuredValue, compile, execute};

fn run(source: &str) -> StructuredValue {
    execute(
        &compile(source).expect("compile"),
        ExecutionOptions::default(),
    )
    .expect("execute")
}

#[test]
fn typeof_unresolvable_names_preserves_lexical_and_global_resolution() {
    assert_eq!(run("typeof missingName;"), "undefined".into());
    assert_eq!(run("typeof (missingName);"), "undefined".into());
    assert_eq!(
        run("const value = 1; (() => typeof value)();"),
        "number".into()
    );
    assert_eq!(
        run("globalThis.extra = 'yes'; typeof extra;"),
        "string".into()
    );
    assert_eq!(
        run("function f(x) { return typeof x; } f(false);"),
        "boolean".into()
    );
    let error = execute(
        &compile("{ typeof value; let value = 1; }").unwrap(),
        ExecutionOptions::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("before initialization"));
    let error = execute(
        &compile("typeof missingName.value;").unwrap(),
        ExecutionOptions::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("not defined"));
    assert!(compile("typeof process;").is_err());
}
