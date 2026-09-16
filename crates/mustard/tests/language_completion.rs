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

#[test]
fn deletion_removes_own_properties_and_preserves_array_holes() {
    assert_eq!(run(r#"
        const o = { a: 1, b: 2, c: 3 };
        function read() { return o.b; }
        read(); read();
        const first = delete o.b;
        const absent = read() === undefined && !Object.hasOwn(o, "b") && !("b" in o);
        o.b = 4;
        const a = [1, 2, 3];
        const removed = delete a[1];
        a.extra = 1; delete a.extra;
        JSON.stringify([first, absent, read(), Object.keys(o), removed, a.length,
            1 in a, Object.keys(a), a, [...a], a.map(x => x * 2), delete a[99]]);
    "#), r#"[true,true,4,["a","c","b"],true,3,false,["0","2"],[1,null,3],[1,null,3],[2,null,6],true]"#.into());
}

#[test]
fn deletion_evaluates_references_once_and_preserves_strict_failures() {
    assert_eq!(
        run(r#"
        let calls = 0;
        const object = { x: 1 };
        function base() { calls++; return object; }
        function key() { calls++; return "x"; }
        const removed = delete base()[key()];
        const skipped = delete null?.[key()];
        const ignored = delete (calls++);
        JSON.stringify([removed, skipped, ignored, calls, delete object.x, delete (1).x]);
    "#),
        "[true,true,true,3,true,true]".into()
    );
    for source in [
        "delete [1].length",
        "delete 'x'[0]",
        "delete null.x",
        "delete globalThis.Math",
        "delete Math.PI",
    ] {
        let error = execute(&compile(source).unwrap(), ExecutionOptions::default()).unwrap_err();
        assert!(error.to_string().contains("TypeError"), "{source}: {error}");
    }
}
