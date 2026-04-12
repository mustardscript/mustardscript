use jslite::ir::{AssignTarget, Expr, LogicalOp, MemberProperty, Stmt};
use jslite::structured::StructuredNumber;
use jslite::{
    ExecutionOptions, ExecutionStep, ResumeOptions, ResumePayload, RuntimeLimits, SnapshotPolicy,
    StructuredValue, compile, dump_program, dump_snapshot, execute, load_program, load_snapshot,
    lower_to_bytecode, resume_with_options, start_bytecode,
};

fn snapshot_policy(capabilities: &[&str], limits: RuntimeLimits) -> SnapshotPolicy {
    SnapshotPolicy {
        capabilities: capabilities
            .iter()
            .map(|name| (*name).to_string())
            .collect(),
        limits,
    }
}

fn stmt_contains_expr(stmt: &Stmt, predicate: &impl Fn(&Expr) -> bool) -> bool {
    match stmt {
        Stmt::Block { body, .. } => body
            .iter()
            .any(|entry| stmt_contains_expr(entry, predicate)),
        Stmt::VariableDecl { declarators, .. } => declarators
            .iter()
            .filter_map(|declarator| declarator.initializer.as_ref())
            .any(|initializer| expr_contains(initializer, predicate)),
        Stmt::FunctionDecl { function, .. } => function
            .body
            .iter()
            .any(|entry| stmt_contains_expr(entry, predicate)),
        Stmt::Expression { expression, .. } => expr_contains(expression, predicate),
        Stmt::If {
            test,
            consequent,
            alternate,
            ..
        } => {
            expr_contains(test, predicate)
                || stmt_contains_expr(consequent, predicate)
                || alternate
                    .as_deref()
                    .is_some_and(|entry| stmt_contains_expr(entry, predicate))
        }
        Stmt::While { test, body, .. } | Stmt::DoWhile { body, test, .. } => {
            expr_contains(test, predicate) || stmt_contains_expr(body, predicate)
        }
        Stmt::For {
            init,
            test,
            update,
            body,
            ..
        } => {
            init.as_ref().is_some_and(|init| match init {
                jslite::ir::ForInit::VariableDecl { declarators, .. } => declarators
                    .iter()
                    .filter_map(|declarator| declarator.initializer.as_ref())
                    .any(|initializer| expr_contains(initializer, predicate)),
                jslite::ir::ForInit::Expression(expression) => expr_contains(expression, predicate),
            }) || test
                .as_ref()
                .is_some_and(|expression| expr_contains(expression, predicate))
                || update
                    .as_ref()
                    .is_some_and(|expression| expr_contains(expression, predicate))
                || stmt_contains_expr(body, predicate)
        }
        Stmt::ForOf { iterable, body, .. } => {
            expr_contains(iterable, predicate) || stmt_contains_expr(body, predicate)
        }
        Stmt::Return { value, .. } => value
            .as_ref()
            .is_some_and(|expression| expr_contains(expression, predicate)),
        Stmt::Throw { value, .. } => expr_contains(value, predicate),
        Stmt::Try {
            body,
            catch,
            finally,
            ..
        } => {
            stmt_contains_expr(body, predicate)
                || catch
                    .as_ref()
                    .is_some_and(|entry| stmt_contains_expr(&entry.body, predicate))
                || finally
                    .as_deref()
                    .is_some_and(|entry| stmt_contains_expr(entry, predicate))
        }
        Stmt::Switch {
            discriminant,
            cases,
            ..
        } => {
            expr_contains(discriminant, predicate)
                || cases.iter().any(|case| {
                    case.test
                        .as_ref()
                        .is_some_and(|test| expr_contains(test, predicate))
                        || case
                            .consequent
                            .iter()
                            .any(|entry| stmt_contains_expr(entry, predicate))
                })
        }
        Stmt::Break { .. } | Stmt::Continue { .. } | Stmt::Empty { .. } => false,
    }
}

fn expr_contains(expr: &Expr, predicate: &impl Fn(&Expr) -> bool) -> bool {
    if predicate(expr) {
        return true;
    }

    match expr {
        Expr::Undefined { .. }
        | Expr::Null { .. }
        | Expr::Bool { .. }
        | Expr::Number { .. }
        | Expr::BigInt { .. }
        | Expr::String { .. }
        | Expr::RegExp { .. }
        | Expr::Identifier { .. }
        | Expr::This { .. } => false,
        Expr::Array { elements, .. } => {
            elements.iter().any(|entry| expr_contains(entry, predicate))
        }
        Expr::Object { properties, .. } => properties
            .iter()
            .any(|property| expr_contains(&property.value, predicate)),
        Expr::Function(function) => function
            .body
            .iter()
            .any(|entry| stmt_contains_expr(entry, predicate)),
        Expr::Unary { argument, .. } => expr_contains(argument, predicate),
        Expr::Binary { left, right, .. } | Expr::Logical { left, right, .. } => {
            expr_contains(left, predicate) || expr_contains(right, predicate)
        }
        Expr::Conditional {
            test,
            consequent,
            alternate,
            ..
        } => {
            expr_contains(test, predicate)
                || expr_contains(consequent, predicate)
                || expr_contains(alternate, predicate)
        }
        Expr::Assignment { target, value, .. } => {
            assign_target_contains_expr(target, predicate) || expr_contains(value, predicate)
        }
        Expr::Member {
            object, property, ..
        } => expr_contains(object, predicate) || member_property_contains_expr(property, predicate),
        Expr::Call {
            callee, arguments, ..
        }
        | Expr::New {
            callee, arguments, ..
        } => {
            expr_contains(callee, predicate)
                || arguments
                    .iter()
                    .any(|entry| expr_contains(entry, predicate))
        }
        Expr::Template { expressions, .. } => expressions
            .iter()
            .any(|entry| expr_contains(entry, predicate)),
        Expr::Await { value, .. } => expr_contains(value, predicate),
    }
}

fn assign_target_contains_expr(target: &AssignTarget, predicate: &impl Fn(&Expr) -> bool) -> bool {
    match target {
        AssignTarget::Identifier { .. } => false,
        AssignTarget::Member {
            object, property, ..
        } => expr_contains(object, predicate) || member_property_contains_expr(property, predicate),
    }
}

fn member_property_contains_expr(
    property: &MemberProperty,
    predicate: &impl Fn(&Expr) -> bool,
) -> bool {
    match property {
        MemberProperty::Static(_) => false,
        MemberProperty::Computed(expression) => expr_contains(expression, predicate),
    }
}

#[test]
fn ir_covers_supported_destructuring_and_short_circuit_forms() {
    let program = compile(
        r#"
        const { nested } = { nested: { value: 3, invoke: (value) => value + 1 } };
        const [first] = [nested];
        (nested?.value ?? 0) + (nested?.invoke?.(first.value) ?? 0);
        "#,
    )
    .expect("source should compile");

    assert!(matches!(
        &program.script.body[0],
        Stmt::VariableDecl { declarators, .. }
            if matches!(declarators[0].pattern, jslite::ir::Pattern::Object { .. })
    ));
    assert!(matches!(
        &program.script.body[1],
        Stmt::VariableDecl { declarators, .. }
            if matches!(declarators[0].pattern, jslite::ir::Pattern::Array { .. })
    ));
    assert!(stmt_contains_expr(
        &program.script.body[2],
        &|expr| matches!(
            expr,
            Expr::Logical {
                operator: LogicalOp::NullishCoalesce,
                ..
            }
        )
    ));
    assert!(stmt_contains_expr(
        &program.script.body[2],
        &|expr| matches!(expr, Expr::Member { optional: true, .. })
    ));
    assert!(stmt_contains_expr(
        &program.script.body[2],
        &|expr| matches!(expr, Expr::Call { optional: true, .. })
    ));
}

#[test]
fn ir_covers_regexp_literals() {
    let program = compile(r#"/(?<word>[a-z]+)\d+/gi;"#).expect("source should compile");

    assert!(stmt_contains_expr(
        &program.script.body[0],
        &|expr| matches!(
            expr,
            Expr::RegExp {
                pattern,
                flags,
                ..
            } if pattern == "(?<word>[a-z]+)\\d+" && flags == "gi"
        )
    ));
}

#[test]
fn bytecode_and_snapshot_round_trips_preserve_resume_behavior() {
    let source = compile("const value = fetch_data(4); value + 2;").expect("source should compile");
    let bytecode = lower_to_bytecode(&source).expect("lowering should succeed");
    let encoded_program = dump_program(&bytecode).expect("program should serialize");
    let loaded_program = load_program(&encoded_program).expect("program should deserialize");

    let step = start_bytecode(
        &loaded_program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("bytecode execution should start");

    let suspension = match step {
        ExecutionStep::Completed(value) => panic!("expected suspension, got completion {value:?}"),
        ExecutionStep::Suspended(suspension) => suspension,
    };
    assert_eq!(suspension.capability, "fetch_data");
    assert_eq!(
        suspension.args,
        vec![StructuredValue::Number(StructuredNumber::Finite(4.0))]
    );

    let encoded_snapshot = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
    let loaded_snapshot = load_snapshot(&encoded_snapshot).expect("snapshot should deserialize");
    let resumed = resume_with_options(
        loaded_snapshot,
        ResumePayload::Value(StructuredValue::Number(StructuredNumber::Finite(4.0))),
        ResumeOptions {
            cancellation_token: None,
            snapshot_policy: Some(snapshot_policy(&["fetch_data"], RuntimeLimits::default())),
        },
    )
    .expect("resume should succeed");

    match resumed {
        ExecutionStep::Completed(value) => assert_eq!(
            value,
            StructuredValue::Number(StructuredNumber::Finite(6.0))
        ),
        ExecutionStep::Suspended(other) => panic!("expected completion, got suspension {other:?}"),
    }
}

#[test]
fn public_execution_api_reclaims_cyclic_garbage_under_tight_limits() {
    let program = compile(
        r#"
        let total = 0;
        for (let index = 0; index < 120; index += 1) {
          const left = {};
          const right = {};
          left.peer = right;
          right.peer = left;
          total += index;
        }
        total;
        "#,
    )
    .expect("source should compile");

    let value = execute(
        &program,
        ExecutionOptions {
            limits: RuntimeLimits {
                heap_limit_bytes: 24 * 1024,
                allocation_budget: 256,
                ..RuntimeLimits::default()
            },
            ..ExecutionOptions::default()
        },
    )
    .expect("cyclic garbage should be reclaimed");

    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(7140.0))
    );
}
