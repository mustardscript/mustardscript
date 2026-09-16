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

#[test]
fn array_from_supports_array_likes_and_live_mappers() {
    assert_eq!(
        run(r#"JSON.stringify(Array.from({length: 4}, (_, index) => index * 2));"#),
        "[0,2,4,6]".into()
    );
    assert_eq!(
        run(r#"
        const source = {0: 1, 2: 3, length: 3.9};
        const result = Array.from(source, function(value, index) {
            if (index === 0) { source[1] = 4; source.length = 1; }
            return (value ?? 0) + this.offset;
        }, {offset: 10});
        JSON.stringify(result);
    "#),
        "[11,14,13]".into()
    );
    assert_eq!(
        run(
            r#"JSON.stringify([Array.from({length:-3}), Array.from(1), Array.from({length:2}), Array.from(new String("ab"))]);"#
        ),
        r#"[[],[],[null,null],["a","b"]]"#.into()
    );
    for source in [
        "Array.from({length:1e9})",
        "new Array(1e9)",
        "const a=[]; a.length=1e9",
        "const a=[]; a[1e9]=1",
    ] {
        let error = execute(&compile(source).unwrap(), ExecutionOptions::default()).unwrap_err();
        assert!(
            error.to_string().contains("heap limit exceeded"),
            "{source}: {error}"
        );
    }
}

#[test]
fn array_queue_copy_and_overlap_operations_preserve_presence() {
    assert_eq!(run(r#"
        const a = [, 2, 3];
        const shifted = a.shift(); const length = a.unshift(0, 1);
        const b = [, undefined, 3, 1];
        const sorted = b.toSorted((a,b) => a-b);
        const reversed = b.toReversed(); const spliced = b.toSpliced(1);
        const replaced = b.with(-1, 4);
        b.copyWithin(1, 0, 3);
        JSON.stringify([shifted, length, a, sorted, Object.keys(sorted), reversed, spliced, Object.keys(spliced), replaced, b, Object.keys(b)]);
    "#), r#"[null,4,[0,1,2,3],[1,3,null,null],["0","1","2","3"],[1,3,null,null],[null],["0"],[null,null,3,4],[null,null,null,3],["2","3"]]"#.into());
    assert_eq!(
        run(r#"
        const values = [{n:2}, {n:1}, {n:1}];
        const sorted = values.toSorted((a,b) => a.n-b.n);
        sorted[0] === values[1] && sorted[1] === values[2] && sorted[2] === values[0];
    "#),
        true.into()
    );
    assert_eq!(
        run(
            r#"const a=[3,1,2]; let done=false; a.sort((a,b)=> { if (!done) { done=true; } return a-b; }); JSON.stringify(a);"#
        ),
        "[1,2,3]".into()
    );
    assert_eq!(
        run(r#"const a=[1,2,3]; JSON.stringify([a.splice(1),a]);"#),
        "[[2,3],[1]]".into()
    );
    assert!(
        execute(
            &compile("[1].with(1, 2)").unwrap(),
            ExecutionOptions::default()
        )
        .unwrap_err()
        .to_string()
        .contains("RangeError")
    );
}

#[test]
fn grouping_preserves_order_key_identity_and_null_prototype_absence() {
    assert_eq!(
        run(r#"
        const groups = Object.groupBy([1,2,3,4], (value,index) => index % 2);
        const empty = Object.groupBy([], () => "x");
        JSON.stringify([groups, empty.constructor === undefined, !("toString" in empty), empty instanceof Object]);
    "#),
        r#"[{"0":[1,3],"1":[2,4]},true,true,false]"#.into()
    );
    assert_eq!(
        run(r#"
        const groups = Object.groupBy(["__proto__", "constructor", "__proto__"], value => value);
        const copied = {...groups}; delete groups.constructor;
        JSON.stringify([Object.keys(groups), groups.__proto__, copied.constructor]);
    "#),
        r#"[["__proto__"],["__proto__","__proto__"],["constructor"]]"#.into()
    );
    assert_eq!(
        run(r#"
        const a = {}, b = {};
        const groups = Map.groupBy([a,b,a], value => value);
        const keys = [...groups.keys()];
        const numeric = Map.groupBy([NaN, -0, NaN, 0], value => value);
        JSON.stringify([groups.size, keys[0] === a, keys[1] === b, groups.get(a).length,
          numeric.size, numeric.get(NaN).length, numeric.get(0).length, 1 / [...numeric.keys()][1] === Infinity]);
    "#),
        "[2,true,true,2,2,2,2,true]".into()
    );
    for source in [
        "Object.groupBy({}, value=>value)",
        "Object.groupBy([], 1)",
        "Map.groupBy(null, value=>value)",
        "String(Object.groupBy([], value=>value))",
    ] {
        assert!(
            execute(&compile(source).unwrap(), ExecutionOptions::default())
                .unwrap_err()
                .to_string()
                .contains("TypeError")
        );
    }
}

#[test]
fn object_compatibility_and_array_stringification_preserve_identity_and_absence() {
    assert_eq!(run(r#"
        const own = Object.prototype.hasOwnProperty;
        const tag = Object.prototype.toString;
        const a = [1, , null, [2, 3]]; a.push(a);
        const o = {x: 1};
        function read() { return o.hasOwnProperty('x'); }
        for (let i = 0; i < 40; i++) read();
        const inherited = read();
        o.hasOwnProperty = undefined;
        const same = {};
        JSON.stringify([Object.is(NaN, NaN), Object.is(0, -0), Object.is(same, same),
          Object.is({}, {}), inherited, o.hasOwnProperty === undefined,
          own.call(o, 'x'), own.call(o, 'toString'), own.call('abc', '1'),
          Object.hasOwn('abc', 'length'), own.call(new Map(), 'size'),
          own.call(Object.prototype, 'hasOwnProperty'), own.call(Array.prototype, 'toString'),
          own.call(Array.prototype, 'hasOwnProperty'), tag.call(null), tag.call(undefined),
          tag.call(a), tag.call(new Map()), tag.call(new Error('x')),
          a.toString(), a.join(undefined), ({x: 1}).toString(),
          'hasOwnProperty' in [], 'toString' in {},
          Array.prototype.toString.call({join() { return this.x; }, x: 42}),
          Array.prototype.toString.call({join: 0})]);
    "#), r#"[true,false,true,false,true,true,true,false,true,true,false,true,true,false,"[object Null]","[object Undefined]","[object Array]","[object Map]","[object Error]","1,,,2,3,","1,,,2,3,","[object Object]",true,true,42,"[object Object]"]"#.into());
    for source in [
        "Object.hasOwn(null, 'x');",
        "Object.prototype.hasOwnProperty.call(undefined, 'x');",
        "Array.prototype.toString.call(null);",
    ] {
        let error = execute(&compile(source).unwrap(), ExecutionOptions::default()).unwrap_err();
        assert!(error.to_string().contains("TypeError"), "{source}: {error}");
    }
    let error = execute(
        &compile("let a = []; for (let i = 0; i < 140; i++) a = [a]; a.toString();").unwrap(),
        ExecutionOptions::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("nesting limit"));
}

#[test]
fn uri_codecs_round_trip_unicode_and_reject_malformed_utf8() {
    assert_eq!(run(r#"
        const input = "https://x.test/a b?q=é🙂&v=+%#片";
        JSON.stringify([encodeURI(input), decodeURI(encodeURI(input)),
            encodeURIComponent(input), decodeURIComponent(encodeURIComponent(input)),
            decodeURI('%2f%3F%23%2b%20%25'), decodeURIComponent('%2f%3F%23%2b%20%25'),
            encodeURIComponent(), encodeURIComponent(Infinity), encodeURIComponent(1e21)]);
    "#), r#"["https://x.test/a%20b?q=%C3%A9%F0%9F%99%82&v=+%25#%E7%89%87","https://x.test/a b?q=é🙂&v=+%#片","https%3A%2F%2Fx.test%2Fa%20b%3Fq%3D%C3%A9%F0%9F%99%82%26v%3D%2B%25%23%E7%89%87","https://x.test/a b?q=é🙂&v=+%#片","%2f%3F%23%2b %","/?#+ %","undefined","Infinity","1e%2B21"]"#.into());
    assert_eq!(
        run(r#"
        ['%', '%0', '%GG', '%FF', '%80', '%C0%AF', '%E0%80%AF', '%ED%A0%80',
         '%F4%90%80%80', '%F0%9F%99', '%C2x', '%E2%28%A1'].every(input => {
          try { decodeURIComponent(input); return false; }
          catch (error) { return error instanceof URIError; }
        });
    "#),
        true.into()
    );
}
