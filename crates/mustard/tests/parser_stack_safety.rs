use mustard::compile;
use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
fn delimiter_free_chains_are_bounded_on_small_stacks() {
    const HELPER: &str = "MUSTARD_PARSER_STACK_HELPER";
    if std::env::var_os(HELPER).is_some() {
        let chains = [
            format!("{}0;", "a=>".repeat(3000)),
            format!("{};", "if(1);else ".repeat(3000)),
            format!("{};", "if(1) a,b;else ".repeat(3000)),
            format!("{};", "if(1){}else ".repeat(3000)),
            format!(
                "{}0;",
                (0..3000).map(|i| format!("label{i}:")).collect::<String>()
            ),
            format!("{}true;", "!".repeat(3000)),
            format!("{}0;", "x=".repeat(3000)),
            format!("{}0;", "1+".repeat(3000)),
            format!("x{};", ".x".repeat(3000)),
            format!("{}0;", "true?0:".repeat(3000)),
        ];
        for stack_size in [1024 * 1024, 8 * 1024 * 1024] {
            for source in &chains {
                let source = source.clone();
                thread::Builder::new()
                    .stack_size(stack_size)
                    .spawn(move || {
                        let error = compile(&source)
                            .expect_err("chain must reject before recursive parsing");
                        assert!(
                            error.to_string().contains("source nesting limit exceeded"),
                            "{error}"
                        );
                    })
                    .unwrap()
                    .join()
                    .unwrap();
            }
        }
        return;
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "delimiter_free_chains_are_bounded_on_small_stacks",
            "--nocapture",
        ])
        .env(HELPER, "1")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "parser stack regression: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            break;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!(
                "parser helper timed out: {:?}",
                child.wait_with_output().unwrap()
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn flat_scripts_and_precise_lexical_diagnostics_do_not_depend_on_opener_byte_counts() {
    let prefix = (0..1000)
        .map(|i| format!("function f{i}() {{ return {i}; }}\n"))
        .collect::<String>();
    compile(&prefix).unwrap();
    for source in [
        "const x = `unterminated",
        r#"const x = '\u{zz}';"#,
        "const x = /unterminated",
        "const x = '\\xGG';",
    ] {
        let short = compile(source).unwrap_err().to_string();
        let long = compile(&format!("{prefix}{source}"))
            .unwrap_err()
            .to_string();
        assert_eq!(short, long, "lexical diagnostic changed for {source}");
        assert!(!long.contains("source tokenization failed"));
    }
    for source in [
        format!("const items = [{}];", "1,".repeat(3000)),
        format!("{}42;", "if (true) {} if(false){}\n".repeat(1000)),
        format!("{}42;", "if(true) a,b; else c,d;\n".repeat(1000)),
        format!("const text = '{}';", "a=>!if(1);else label:".repeat(3000)),
    ] {
        compile(&source).unwrap();
    }
}
