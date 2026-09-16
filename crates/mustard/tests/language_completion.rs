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

#[test]
fn json_options_transform_values_in_specified_traversal_order() {
    assert_eq!(
        run(r#"JSON.stringify({b: [1, {a: 2}], a: 3}, ["a", "b", "a"], 2);"#),
        "{\n  \"a\": 3,\n  \"b\": [\n    1,\n    {\n      \"a\": 2\n    }\n  ]\n}".into()
    );
    assert_eq!(
        run(r#"
        const seen = [];
        const object = { a: 1, b: 2, c: 3 };
        const text = JSON.stringify(object, function(key, value) {
            seen.push(key);
            if (key === "a") { delete this.b; this.c = 4; }
            if (typeof value === "number") return value * 2;
            return value;
        });
        JSON.stringify([text, seen]);
    "#),
        r#"["{\"a\":2,\"c\":8}",["","a","b","c"]]"#.into()
    );
    assert_eq!(
        run(r#"
        const seen = [];
        const value = JSON.parse('{"a":[1,2],"b":3}', function(key, value) {
            seen.push(key);
            if (key === "0" || key === "b") return undefined;
            return typeof value === "number" ? value * 3 : value;
        });
        JSON.stringify([value, seen, 0 in value.a, Object.keys(value)]);
    "#),
        r#"[{"a":[null,6]},["0","1","a","b",""],false,["a"]]"#.into()
    );
    assert_eq!(
        run("JSON.parse('1', () => undefined);"),
        StructuredValue::Undefined
    );
    assert_eq!(
        run(
            "(() => { try { JSON.parse('{'); } catch (error) { return error instanceof SyntaxError; } })()"
        ),
        true.into()
    );
}

#[test]
fn json_preserves_unicode_chunks_to_json_and_boxed_primitives() {
    assert_eq!(
        run(r#"
        const text = "🙂a".repeat(200);
        JSON.parse(JSON.stringify(text)) === text;
    "#),
        true.into()
    );
    assert_eq!(
        run(r#"
        JSON.stringify({a: {toJSON(key) { return key; }}, b: new Number(2), c: new String("x"), d: new Boolean(false)});
    "#),
        r#"{"a":"a","b":2,"c":"x","d":false}"#.into()
    );
    let error = execute(&compile("let value = {}; for (let i = 0; i < 150; i++) value = {value}; JSON.stringify(value, null, 2);").unwrap(), ExecutionOptions::default()).unwrap_err();
    assert!(error.to_string().contains("nesting depth limit"));
}

#[test]
fn json_callback_host_suspensions_fail_before_transferring_vm_state() {
    for source in [
        "JSON.stringify({x:1}, () => checkpoint());",
        "JSON.parse('1', () => checkpoint());",
    ] {
        let error = execute(
            &compile(source).unwrap(),
            ExecutionOptions {
                capabilities: vec!["checkpoint".into()],
                ..ExecutionOptions::default()
            },
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("JSON callbacks do not support synchronous host suspensions"),
            "{error}"
        );
    }
}

#[test]
fn error_family_has_correct_constructors_metadata_and_guest_stacks() {
    assert_eq!(
        run(r#"
        const rows = [Error, TypeError, ReferenceError, RangeError, SyntaxError, EvalError, URIError].map(C => {
            const error = new C("message", {cause: 1});
            return [error instanceof C, error instanceof Error, error.constructor === C,
                error.name === C.name, error.cause, error.toString(), Object.keys(error).length,
                typeof error.stack, C.prototype.name, C.length];
        });
        rows.every(row => row[0] && row[1] && row[2] && row[3] && row[4] === 1 && row[6] === 0 && row[7] === "string" && row[9] === 1);
    "#),
        true.into()
    );
    assert_eq!(
        run(r#"
        function guestFailure() { return new URIError("bad URI"); }
        const error = guestFailure();
        error.stack.startsWith("URIError: bad URI") && error.stack.includes("guestFailure") && !error.stack.includes("crates/mustard") && !error.stack.includes(".rs:");
    "#),
        true.into()
    );
    assert_eq!(
        run(r#"
        const errors = [1, "two"];
        const error = AggregateError(errors, "failed", {cause: 3});
        errors.push(4);
        JSON.stringify([error instanceof AggregateError, error instanceof Error, error.constructor === AggregateError, error.name, error.errors, error.cause, Object.keys(error), error.toString(), AggregateError.length]);
    "#),
        r#"[true,true,true,"AggregateError",[1,"two"],3,[],"AggregateError: failed",2]"#.into()
    );
    assert_eq!(
        run(r#"
        (async () => { try { await Promise.any([Promise.reject(1), Promise.reject(2)]); } catch(error) { return error instanceof AggregateError && error instanceof Error && typeof error.stack === "string"; } })();
    "#),
        true.into()
    );
    assert_eq!(
        run(r#"Error.prototype.toString.call({name: "", message: "just text"});"#),
        "just text".into()
    );
    assert!(
        execute(
            &compile("new AggregateError({length: 1});").unwrap(),
            ExecutionOptions::default()
        )
        .unwrap_err()
        .to_string()
        .contains("not iterable")
    );
}
