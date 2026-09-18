use mustard::{ExecutionOptions, RuntimeLimits, StructuredValue, compile, runtime::execute};
use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const HELPER: &str = "MUSTARD_NATIVE_STACK_HELPER";

#[test]
fn native_recursion_is_bounded_on_small_host_stacks() {
    if std::env::var_os(HELPER).is_some() {
        let cases = [
            (
                "reviver",
                r#"
                const text = '['.repeat(120) + '0' + ']'.repeat(120);
                function visit(k, v) { if (v === 0) JSON.parse(text, visit); return v; }
                JSON.parse(text, visit);
            "#,
            ),
            (
                "replacer",
                r#"
                const root = JSON.parse('['.repeat(120) + '0' + ']'.repeat(120));
                function visit(k, v) { if (v === 0) JSON.stringify(root, visit); return v; }
                JSON.stringify(root, visit);
            "#,
            ),
            (
                "toJSON",
                r#"
                let root = { toJSON() { return JSON.stringify(root); } };
                for (let i = 0; i < 120; i++) root = [root];
                JSON.stringify(root);
            "#,
            ),
            ("map", "function f() { return [1].map(f); } f();"),
            ("sort", "function f() { return [2,1].sort(f); } f();"),
            (
                "toSorted",
                "function f() { return [2,1].toSorted(f); } f();",
            ),
            (
                "groupBy",
                "function f() { return Object.groupBy([1], f); } f();",
            ),
            (
                "Map.groupBy",
                "function f() { return Map.groupBy([1], f); } f();",
            ),
            (
                "Array.from",
                "function f() { return Array.from([1], f); } f();",
            ),
            (
                "set-like has",
                "function f() { return new Set([1]).isSubsetOf({size:1, has:f, keys:() => [1].values()}); } f();",
            ),
        ];
        for stack_size in [1024 * 1024, 8 * 1024 * 1024] {
            for (label, source) in cases {
                eprintln!("{label} on {stack_size} byte stack");
                let program = compile(source).unwrap();
                thread::Builder::new()
                    .stack_size(stack_size)
                    .spawn(move || {
                        let error = execute(
                            &program,
                            ExecutionOptions {
                                limits: RuntimeLimits {
                                    instruction_budget: 10_000_000,
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                        )
                        .expect_err("recursive native work must reach a limit");
                        assert!(
                            error
                                .to_string()
                                .contains("native recursion depth limit exceeded"),
                            "{error}"
                        );
                    })
                    .unwrap()
                    .join()
                    .unwrap();
            }
            // Deep documents without recursive callbacks and repeated ordinary
            // callback use must still work; successful/error exits refund depth.
            let program = compile(
                r#"
                const text = '['.repeat(120) + '0' + ']'.repeat(120);
                for (let i = 0; i < 100; i++) {
                    try { [1].map(() => { throw 'expected'; }); } catch (e) {}
                    JSON.parse('1', (k, v) => JSON.parse('2'));
                }
                JSON.stringify(JSON.parse(text, (k, v) => v)) === text;
            "#,
            )
            .unwrap();
            thread::Builder::new()
                .stack_size(stack_size)
                .spawn(move || {
                    assert_eq!(
                        execute(&program, ExecutionOptions::default()).unwrap(),
                        StructuredValue::Bool(true)
                    );
                })
                .unwrap()
                .join()
                .unwrap();
        }
        return;
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "native_recursion_is_bounded_on_small_host_stacks",
            "--nocapture",
        ])
        .env(HELPER, "1")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().unwrap().is_some() {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "native stack regression: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            break;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!(
                "native recursion helper exceeded deadline: {:?}",
                child.wait_with_output().unwrap()
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}
