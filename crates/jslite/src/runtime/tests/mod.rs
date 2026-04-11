use super::*;
use crate::compile;

fn test_function(code: Vec<Instruction>) -> FunctionPrototype {
    FunctionPrototype {
        name: None,
        params: Vec::new(),
        rest: None,
        code,
        is_async: false,
        is_arrow: false,
        span: SourceSpan::new(0, 0),
    }
}

fn invalid_program(code: Vec<Instruction>) -> BytecodeProgram {
    BytecodeProgram {
        functions: vec![test_function(code)],
        root: 0,
    }
}

fn run(source: &str) -> StructuredValue {
    let program = compile(source).expect("source should compile");
    execute(&program, ExecutionOptions::default()).expect("program should run")
}

#[test]
fn runs_arithmetic_and_locals() {
    let value = run(r#"
            const a = 4;
            const b = 3;
            a * b + 2;
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(14.0))
    );
}

#[test]
fn runs_functions_and_closures() {
    let value = run(r#"
            function makeAdder(x) {
              return (y) => x + y;
            }
            const add2 = makeAdder(2);
            add2(5);
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(7.0))
    );
}

#[test]
fn runs_arrays_objects_and_member_access() {
    let value = run(r#"
            const values = [1, 2];
            const record = { total: values[0] + values[1] };
            record.total;
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(3.0))
    );
}

#[test]
fn runs_branching_loops_and_switch() {
    let value = run(r#"
            let total = 0;
            let i = 0;
            while (i < 4) {
              total += i;
              i += 1;
            }
            do {
              total += 1;
            } while (false);
            for (let j = 0; j < 2; j += 1) {
              if (j === 0) {
                continue;
              }
              total += j;
            }
            switch (total) {
              case 8:
                total += 1;
                break;
              default:
                total = 0;
            }
            total;
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(9.0))
    );
}

#[test]
fn runs_math_and_json_builtins() {
    let value = run(r#"
            const encoded = JSON.stringify({ value: Math.max(1, 9, 4) });
            JSON.parse(encoded).value;
            "#);
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(9.0))
    );
}

#[test]
fn preserves_supported_enumeration_order_for_json_stringify() {
    let value = run(r#"
            const record = {};
            record.beta = "b";
            record.alpha = "a";
            const values = ["c", "d"];
            values.extra = "ignored";
            JSON.stringify({ record, values });
            "#);
    assert_eq!(
        value,
        StructuredValue::String(
            r#"{"record":{"alpha":"a","beta":"b"},"values":["c","d"]}"#.to_string()
        )
    );
}

#[test]
fn enforces_instruction_budget() {
    let program = compile("while (true) {}").expect("source should compile");
    let error = execute(
        &program,
        ExecutionOptions {
            inputs: IndexMap::new(),
            capabilities: Vec::new(),
            limits: RuntimeLimits {
                instruction_budget: 100,
                ..RuntimeLimits::default()
            },
            cancellation_token: None,
        },
    )
    .expect_err("infinite loop should exhaust budget");
    assert!(error.to_string().contains("instruction budget exhausted"));
}

#[test]
fn tracks_heap_growth_and_enforces_heap_limits() {
    let program = lower_to_bytecode(&compile("1;").expect("source should compile"))
        .expect("lowering should succeed");
    let mut runtime = Runtime::new(program.clone(), ExecutionOptions::default())
        .expect("runtime should initialize");

    let baseline_heap = runtime.heap_bytes_used;
    let array = runtime
        .insert_array(vec![Value::String("payload".to_string())], IndexMap::new())
        .expect("array allocation should succeed");
    assert!(runtime.heap_bytes_used > baseline_heap);

    let array_heap = runtime.heap_bytes_used;
    runtime
        .set_property(
            Value::Array(array),
            Value::String("extra".to_string()),
            Value::String("more payload".to_string()),
        )
        .expect("array growth should succeed");
    assert!(runtime.heap_bytes_used > array_heap);

    let mut heap_limited = Runtime::new(program.clone(), ExecutionOptions::default())
        .expect("runtime should initialize");
    heap_limited.limits.heap_limit_bytes = heap_limited.heap_bytes_used;
    let error = heap_limited
        .insert_array(vec![Value::String("payload".to_string())], IndexMap::new())
        .expect_err("next allocation should exceed the heap limit");
    assert!(error.to_string().contains("heap limit exceeded"));

    let mut allocation_limited =
        Runtime::new(program, ExecutionOptions::default()).expect("runtime should initialize");
    allocation_limited.limits.allocation_budget = allocation_limited.allocation_count;
    let error = allocation_limited
        .insert_object(IndexMap::new(), ObjectKind::Plain)
        .expect_err("next allocation should exhaust the allocation budget");
    assert!(error.to_string().contains("allocation budget exhausted"));
}

#[test]
fn iterators_participate_in_heap_accounting_and_gc() {
    let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
        .expect("lowering should succeed");
    let mut runtime = Runtime::new(program, ExecutionOptions::default()).expect("runtime init");

    let baseline_heap = runtime.heap_bytes_used;
    let kept_array = runtime
        .insert_array(
            vec![Value::Number(1.0), Value::Number(2.0)],
            IndexMap::new(),
        )
        .expect("kept array should allocate");
    let kept_iterator = runtime
        .insert_iterator(IteratorState::Array(ArrayIteratorState {
            array: kept_array,
            next_index: 1,
        }))
        .expect("kept iterator should allocate");
    assert!(runtime.heap_bytes_used > baseline_heap);

    let frame_env = runtime
        .new_env(Some(runtime.globals))
        .expect("frame env should allocate");
    let iterator_cell = runtime
        .insert_cell(Value::Iterator(kept_iterator), true, true)
        .expect("iterator cell should allocate");
    runtime
        .envs
        .get_mut(frame_env)
        .expect("frame env should exist")
        .bindings
        .insert("\0kept_iter".to_string(), iterator_cell);
    runtime
        .refresh_env_accounting(frame_env)
        .expect("frame env accounting should refresh");
    runtime.frames.push(Frame {
        function_id: 0,
        ip: 0,
        env: frame_env,
        scope_stack: Vec::new(),
        stack: Vec::new(),
        handlers: Vec::new(),
        pending_exception: None,
        pending_completions: Vec::new(),
        active_finally: Vec::new(),
        async_promise: None,
    });

    let garbage_array = runtime
        .insert_array(vec![Value::Number(9.0)], IndexMap::new())
        .expect("garbage array should allocate");
    let garbage_iterator = runtime
        .insert_iterator(IteratorState::Array(ArrayIteratorState {
            array: garbage_array,
            next_index: 0,
        }))
        .expect("garbage iterator should allocate");

    runtime.collect_garbage().expect("gc should succeed");
    assert!(runtime.arrays.contains_key(kept_array));
    assert!(runtime.iterators.contains_key(kept_iterator));
    assert!(!runtime.arrays.contains_key(garbage_array));
    assert!(!runtime.iterators.contains_key(garbage_iterator));

    runtime.frames.clear();
    runtime.collect_garbage().expect("gc should succeed");
    assert!(!runtime.arrays.contains_key(kept_array));
    assert!(!runtime.iterators.contains_key(kept_iterator));
}

#[test]
fn map_and_set_iterators_keep_keyed_collections_alive_for_gc() {
    let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
        .expect("lowering should succeed");
    let mut runtime = Runtime::new(program, ExecutionOptions::default()).expect("runtime init");

    let kept_map = runtime.insert_map(Vec::new()).expect("map should allocate");
    runtime
        .map_set(
            kept_map,
            Value::String("alpha".to_string()),
            Value::Number(1.0),
        )
        .expect("map entry should insert");
    let kept_set = runtime.insert_set(Vec::new()).expect("set should allocate");
    runtime
        .set_add(kept_set, Value::String("beta".to_string()))
        .expect("set entry should insert");
    let kept_map_iterator = runtime
        .insert_iterator(IteratorState::MapEntries(MapIteratorState {
            map: kept_map,
            next_index: 0,
        }))
        .expect("map iterator should allocate");
    let kept_set_iterator = runtime
        .insert_iterator(IteratorState::SetValues(SetIteratorState {
            set: kept_set,
            next_index: 0,
        }))
        .expect("set iterator should allocate");

    let frame_env = runtime
        .new_env(Some(runtime.globals))
        .expect("frame env should allocate");
    let map_iterator_cell = runtime
        .insert_cell(Value::Iterator(kept_map_iterator), true, true)
        .expect("map iterator cell should allocate");
    let set_iterator_cell = runtime
        .insert_cell(Value::Iterator(kept_set_iterator), true, true)
        .expect("set iterator cell should allocate");
    let env = runtime
        .envs
        .get_mut(frame_env)
        .expect("frame env should exist");
    env.bindings
        .insert("\0kept_map_iter".to_string(), map_iterator_cell);
    env.bindings
        .insert("\0kept_set_iter".to_string(), set_iterator_cell);
    runtime
        .refresh_env_accounting(frame_env)
        .expect("frame env accounting should refresh");
    runtime.frames.push(Frame {
        function_id: 0,
        ip: 0,
        env: frame_env,
        scope_stack: Vec::new(),
        stack: Vec::new(),
        handlers: Vec::new(),
        pending_exception: None,
        pending_completions: Vec::new(),
        active_finally: Vec::new(),
        async_promise: None,
    });

    let garbage_map = runtime.insert_map(Vec::new()).expect("map should allocate");
    runtime
        .map_set(
            garbage_map,
            Value::String("gamma".to_string()),
            Value::Number(2.0),
        )
        .expect("garbage map entry should insert");
    let garbage_set = runtime.insert_set(Vec::new()).expect("set should allocate");
    runtime
        .set_add(garbage_set, Value::String("delta".to_string()))
        .expect("garbage set entry should insert");
    let garbage_map_iterator = runtime
        .insert_iterator(IteratorState::MapEntries(MapIteratorState {
            map: garbage_map,
            next_index: 0,
        }))
        .expect("garbage map iterator should allocate");
    let garbage_set_iterator = runtime
        .insert_iterator(IteratorState::SetValues(SetIteratorState {
            set: garbage_set,
            next_index: 0,
        }))
        .expect("garbage set iterator should allocate");

    runtime.collect_garbage().expect("gc should succeed");
    assert!(runtime.maps.contains_key(kept_map));
    assert!(runtime.sets.contains_key(kept_set));
    assert!(runtime.iterators.contains_key(kept_map_iterator));
    assert!(runtime.iterators.contains_key(kept_set_iterator));
    assert!(!runtime.maps.contains_key(garbage_map));
    assert!(!runtime.sets.contains_key(garbage_set));
    assert!(!runtime.iterators.contains_key(garbage_map_iterator));
    assert!(!runtime.iterators.contains_key(garbage_set_iterator));
}

#[test]
fn promise_reactions_keep_target_promises_alive_for_gc() {
    let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
        .expect("lowering should succeed");
    let mut runtime = Runtime::new(program, ExecutionOptions::default()).expect("runtime init");

    let kept_source = runtime
        .insert_promise(PromiseState::Pending)
        .expect("source promise should allocate");
    let kept_target = runtime
        .insert_promise(PromiseState::Pending)
        .expect("target promise should allocate");
    runtime
        .attach_promise_reaction(
            kept_source,
            PromiseReaction::Then {
                target: kept_target,
                on_fulfilled: None,
                on_rejected: None,
            },
        )
        .expect("reaction should attach");

    let frame_env = runtime
        .new_env(Some(runtime.globals))
        .expect("frame env should allocate");
    let source_cell = runtime
        .insert_cell(Value::Promise(kept_source), true, true)
        .expect("promise cell should allocate");
    runtime
        .envs
        .get_mut(frame_env)
        .expect("frame env should exist")
        .bindings
        .insert("\0kept_promise".to_string(), source_cell);
    runtime
        .refresh_env_accounting(frame_env)
        .expect("frame env accounting should refresh");
    runtime.frames.push(Frame {
        function_id: 0,
        ip: 0,
        env: frame_env,
        scope_stack: Vec::new(),
        stack: Vec::new(),
        handlers: Vec::new(),
        pending_exception: None,
        pending_completions: Vec::new(),
        active_finally: Vec::new(),
        async_promise: None,
    });

    let garbage_source = runtime
        .insert_promise(PromiseState::Pending)
        .expect("garbage promise should allocate");
    let garbage_target = runtime
        .insert_promise(PromiseState::Pending)
        .expect("garbage target should allocate");
    runtime
        .attach_promise_reaction(
            garbage_source,
            PromiseReaction::Then {
                target: garbage_target,
                on_fulfilled: None,
                on_rejected: None,
            },
        )
        .expect("garbage reaction should attach");

    runtime.collect_garbage().expect("gc should succeed");
    assert!(runtime.promises.contains_key(kept_source));
    assert!(runtime.promises.contains_key(kept_target));
    assert!(!runtime.promises.contains_key(garbage_source));
    assert!(!runtime.promises.contains_key(garbage_target));
}

#[test]
fn maps_preserve_insertion_order_and_same_value_zero_updates() {
    let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
        .expect("lowering should succeed");
    let mut runtime = Runtime::new(program, ExecutionOptions::default()).expect("runtime init");
    let map = runtime.insert_map(Vec::new()).expect("map should allocate");
    let object = runtime
        .insert_object(IndexMap::new(), ObjectKind::Plain)
        .expect("object should allocate");

    runtime
        .map_set(map, Value::String("alpha".to_string()), Value::Number(1.0))
        .expect("alpha insert should succeed");
    runtime
        .map_set(
            map,
            Value::Number(f64::NAN),
            Value::String("nan".to_string()),
        )
        .expect("nan insert should succeed");
    runtime
        .map_set(map, Value::Number(-0.0), Value::String("zero".to_string()))
        .expect("negative zero insert should succeed");
    runtime
        .map_set(map, Value::Object(object), Value::Bool(true))
        .expect("object key insert should succeed");
    runtime
        .map_set(map, Value::String("alpha".to_string()), Value::Number(2.0))
        .expect("alpha update should keep insertion order");
    runtime
        .map_set(
            map,
            Value::Number(0.0),
            Value::String("zero-updated".to_string()),
        )
        .expect("positive zero update should reuse the existing entry");

    let entries = &runtime.maps.get(map).expect("map should exist").entries;
    assert_eq!(entries.len(), 4);
    assert!(matches!(entries[0].key, Value::String(ref value) if value == "alpha"));
    assert!(matches!(entries[0].value, Value::Number(value) if value == 2.0));
    assert!(matches!(entries[1].key, Value::Number(value) if value.is_nan()));
    assert!(matches!(entries[1].value, Value::String(ref value) if value == "nan"));
    assert!(matches!(entries[2].key, Value::Number(value) if value == 0.0));
    assert!(matches!(entries[2].value, Value::String(ref value) if value == "zero-updated"));
    assert!(matches!(entries[3].key, Value::Object(key) if key == object));
    assert!(matches!(entries[3].value, Value::Bool(true)));
}

#[test]
fn keyed_collections_participate_in_heap_accounting_and_gc() {
    let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
        .expect("lowering should succeed");
    let mut runtime = Runtime::new(program, ExecutionOptions::default()).expect("runtime init");

    let baseline_heap = runtime.heap_bytes_used;
    let kept_map = runtime.insert_map(Vec::new()).expect("map should allocate");
    let kept_set = runtime.insert_set(Vec::new()).expect("set should allocate");
    runtime
        .map_set(
            kept_map,
            Value::String("set".to_string()),
            Value::Set(kept_set),
        )
        .expect("map should store the set");
    runtime
        .set_add(kept_set, Value::Map(kept_map))
        .expect("set should store the map");
    assert!(runtime.heap_bytes_used > baseline_heap);

    let frame_env = runtime
        .new_env(Some(runtime.globals))
        .expect("frame env should allocate");
    let map_cell = runtime
        .insert_cell(Value::Map(kept_map), true, true)
        .expect("map cell should allocate");
    runtime
        .envs
        .get_mut(frame_env)
        .expect("frame env should exist")
        .bindings
        .insert("\0kept_map".to_string(), map_cell);
    runtime
        .refresh_env_accounting(frame_env)
        .expect("frame env accounting should refresh");
    runtime.frames.push(Frame {
        function_id: 0,
        ip: 0,
        env: frame_env,
        scope_stack: Vec::new(),
        stack: Vec::new(),
        handlers: Vec::new(),
        pending_exception: None,
        pending_completions: Vec::new(),
        active_finally: Vec::new(),
        async_promise: None,
    });

    let garbage_map = runtime
        .insert_map(Vec::new())
        .expect("garbage map should allocate");
    let garbage_set = runtime
        .insert_set(Vec::new())
        .expect("garbage set should allocate");
    runtime
        .map_set(
            garbage_map,
            Value::String("set".to_string()),
            Value::Set(garbage_set),
        )
        .expect("garbage map should store the set");
    runtime
        .set_add(garbage_set, Value::Map(garbage_map))
        .expect("garbage set should store the map");

    runtime.collect_garbage().expect("gc should succeed");
    assert!(runtime.maps.contains_key(kept_map));
    assert!(runtime.sets.contains_key(kept_set));
    assert!(!runtime.maps.contains_key(garbage_map));
    assert!(!runtime.sets.contains_key(garbage_set));

    runtime.frames.clear();
    runtime.collect_garbage().expect("gc should succeed");
    assert!(!runtime.maps.contains_key(kept_map));
    assert!(!runtime.sets.contains_key(kept_set));
}

#[test]
fn garbage_collection_marks_runtime_roots_and_collects_cycles() {
    let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
        .expect("lowering should succeed");
    let mut runtime =
        Runtime::new(program.clone(), ExecutionOptions::default()).expect("runtime init");

    let closure_env = runtime
        .new_env(Some(runtime.globals))
        .expect("closure env should allocate");
    let rooted_closure = runtime
        .insert_closure(program.root, closure_env)
        .expect("closure should allocate");
    let rooted_object = runtime
        .insert_object(
            IndexMap::from([("closure".to_string(), Value::Closure(rooted_closure))]),
            ObjectKind::Plain,
        )
        .expect("object should allocate");
    let rooted_array = runtime
        .insert_array(vec![Value::Object(rooted_object)], IndexMap::new())
        .expect("array should allocate");

    let frame_env = runtime
        .new_env(Some(runtime.globals))
        .expect("frame env should allocate");
    let rooted_cell = runtime
        .insert_cell(Value::Array(rooted_array), true, true)
        .expect("cell should allocate");
    runtime
        .envs
        .get_mut(frame_env)
        .expect("frame env should exist")
        .bindings
        .insert("kept".to_string(), rooted_cell);
    runtime
        .refresh_env_accounting(frame_env)
        .expect("frame env accounting should refresh");
    runtime.frames.push(Frame {
        function_id: program.root,
        ip: 0,
        env: frame_env,
        scope_stack: vec![closure_env],
        stack: vec![Value::Closure(rooted_closure)],
        handlers: vec![ExceptionHandler {
            catch: None,
            finally: None,
            env: closure_env,
            scope_stack_len: 0,
            stack_len: 0,
        }],
        pending_exception: Some(Value::Object(rooted_object)),
        pending_completions: vec![
            CompletionRecord::Return(Value::Array(rooted_array)),
            CompletionRecord::Throw(Value::Closure(rooted_closure)),
        ],
        active_finally: Vec::new(),
        async_promise: None,
    });

    let garbage_env = runtime.new_env(None).expect("garbage env should allocate");
    let garbage_left = runtime
        .insert_object(IndexMap::new(), ObjectKind::Plain)
        .expect("garbage object should allocate");
    let garbage_right = runtime
        .insert_object(IndexMap::new(), ObjectKind::Plain)
        .expect("garbage object should allocate");
    let garbage_array = runtime
        .insert_array(vec![Value::Object(garbage_left)], IndexMap::new())
        .expect("garbage array should allocate");
    let garbage_closure = runtime
        .insert_closure(program.root, garbage_env)
        .expect("garbage closure should allocate");
    runtime
        .set_property(
            Value::Object(garbage_left),
            Value::String("peer".to_string()),
            Value::Object(garbage_right),
        )
        .expect("left cycle should update");
    runtime
        .set_property(
            Value::Object(garbage_right),
            Value::String("peer".to_string()),
            Value::Object(garbage_left),
        )
        .expect("right cycle should update");
    runtime
        .set_property(
            Value::Object(garbage_right),
            Value::String("items".to_string()),
            Value::Array(garbage_array),
        )
        .expect("array cycle should update");
    runtime
        .set_property(
            Value::Object(garbage_left),
            Value::String("closure".to_string()),
            Value::Closure(garbage_closure),
        )
        .expect("closure cycle should update");
    let garbage_cell = runtime
        .insert_cell(Value::Object(garbage_left), true, true)
        .expect("garbage cell should allocate");
    runtime
        .envs
        .get_mut(garbage_env)
        .expect("garbage env should exist")
        .bindings
        .insert("garbage".to_string(), garbage_cell);
    runtime
        .refresh_env_accounting(garbage_env)
        .expect("garbage env accounting should refresh");

    let stats = runtime.collect_garbage().expect("gc should succeed");

    assert!(stats.reclaimed_allocations >= 5);
    assert!(stats.reclaimed_bytes > 0);
    assert!(runtime.envs.contains_key(frame_env));
    assert!(runtime.envs.contains_key(closure_env));
    assert!(runtime.cells.contains_key(rooted_cell));
    assert!(runtime.objects.contains_key(rooted_object));
    assert!(runtime.arrays.contains_key(rooted_array));
    assert!(runtime.closures.contains_key(rooted_closure));

    assert!(!runtime.envs.contains_key(garbage_env));
    assert!(!runtime.cells.contains_key(garbage_cell));
    assert!(!runtime.objects.contains_key(garbage_left));
    assert!(!runtime.objects.contains_key(garbage_right));
    assert!(!runtime.arrays.contains_key(garbage_array));
    assert!(!runtime.closures.contains_key(garbage_closure));
}

#[test]
fn garbage_collection_reclaims_cyclic_garbage_under_execution_pressure() {
    let program = compile(
        r#"
            let total = 0;
            for (let i = 0; i < 120; i += 1) {
              let left = {};
              let right = {};
              left.peer = right;
              right.peer = left;
              total += i;
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
            cancellation_token: None,
            ..ExecutionOptions::default()
        },
    )
    .expect("cyclic garbage should be reclaimed");
    assert_eq!(
        value,
        StructuredValue::Number(StructuredNumber::Finite(7140.0))
    );
}

#[test]
fn lowering_errors_preserve_source_spans() {
    let program = compile("continue;").expect("source should compile");
    let error =
        lower_to_bytecode(&program).expect_err("continue outside a loop should fail lowering");
    let rendered = error.to_string();
    assert!(rendered.contains("`continue` used outside of a loop"));
    assert!(rendered.contains("[0..9]"));
}

#[test]
fn runtime_errors_include_guest_tracebacks() {
    let program = compile(
        r#"
            function outer() {
              return inner();
            }
            function inner() {
              const value = null;
              return value.answer;
            }
            outer();
            "#,
    )
    .expect("source should compile");
    let error = execute(&program, ExecutionOptions::default())
        .expect_err("nullish property access should fail");
    let rendered = error.to_string();
    assert!(rendered.contains("TypeError: cannot read properties of nullish value"));
    assert!(rendered.contains("at inner ["));
    assert!(rendered.contains("at outer ["));
    assert!(rendered.contains("at <script> ["));
    assert!(!rendered.contains(".rs"));
}

#[test]
fn suspends_and_resumes_host_capability_calls() {
    let program = compile(
        r#"
            const value = fetch_data(41);
            value + 1;
            "#,
    )
    .expect("source should compile");

    let step = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("execution should start");

    let suspension = match step {
        ExecutionStep::Suspended(suspension) => suspension,
        other => panic!("expected suspension, got {other:?}"),
    };
    assert_eq!(suspension.capability, "fetch_data");
    assert_eq!(
        suspension.args,
        vec![StructuredValue::Number(StructuredNumber::Finite(41.0))]
    );

    let resumed = resume(
        suspension.snapshot,
        ResumePayload::Value(StructuredValue::Number(StructuredNumber::Finite(41.0))),
    )
    .expect("resume should succeed");

    match resumed {
        ExecutionStep::Completed(value) => {
            assert_eq!(
                value,
                StructuredValue::Number(StructuredNumber::Finite(42.0))
            );
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn console_callbacks_resume_with_undefined_guest_results() {
    let program = compile(
        r#"
            const logged = console.log(41);
            logged === undefined ? 2 : 0;
            "#,
    )
    .expect("source should compile");

    let step = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["console.log".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("execution should suspend on console.log");

    let suspension = match step {
        ExecutionStep::Suspended(suspension) => suspension,
        other => panic!("expected suspension, got {other:?}"),
    };
    assert_eq!(suspension.capability, "console.log");
    assert_eq!(
        suspension.args,
        vec![StructuredValue::Number(StructuredNumber::Finite(41.0))]
    );

    let resumed = resume(
        suspension.snapshot,
        ResumePayload::Value(StructuredValue::String("ignored".to_string())),
    )
    .expect("resume should ignore host return values for console callbacks");

    match resumed {
        ExecutionStep::Completed(value) => {
            assert_eq!(
                value,
                StructuredValue::Number(StructuredNumber::Finite(2.0))
            );
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn runs_throw_try_catch_and_finally() {
    let value = run(r#"
            let log = [];
            try {
              log[log.length] = "body";
              throw new Error("boom");
            } catch (error) {
              log[log.length] = error.name;
              log[log.length] = error.message;
            } finally {
              log[log.length] = "finally";
            }
            log;
            "#);
    assert_eq!(
        value,
        StructuredValue::Array(vec![
            StructuredValue::String("body".to_string()),
            StructuredValue::String("Error".to_string()),
            StructuredValue::String("boom".to_string()),
            StructuredValue::String("finally".to_string()),
        ])
    );
}

#[test]
fn catches_runtime_type_errors_as_guest_errors() {
    let value = run(r#"
            let captured;
            try {
              const value = null;
              value.answer;
            } catch (error) {
              captured = [error.name, error.message];
            }
            captured;
            "#);
    assert_eq!(
        value,
        StructuredValue::Array(vec![
            StructuredValue::String("TypeError".to_string()),
            StructuredValue::String("cannot read properties of nullish value".to_string()),
        ])
    );
}

#[test]
fn finally_runs_for_return_break_and_continue() {
    let value = run(r#"
            let events = [];
            function earlyReturn() {
              try {
                return "body";
              } finally {
                events[events.length] = "return";
              }
            }
            let index = 0;
            while (index < 2) {
              index += 1;
              try {
                if (index === 1) {
                  continue;
                }
                break;
              } finally {
                events[events.length] = index;
              }
            }
            [earlyReturn(), events];
            "#);
    assert_eq!(
        value,
        StructuredValue::Array(vec![
            StructuredValue::String("body".to_string()),
            StructuredValue::Array(vec![
                StructuredValue::Number(StructuredNumber::Finite(1.0)),
                StructuredValue::Number(StructuredNumber::Finite(2.0)),
                StructuredValue::String("return".to_string()),
            ]),
        ])
    );
}

#[test]
fn nested_exception_unwind_preserves_finally_order() {
    let value = run(r#"
            let events = [];
            function nested() {
              try {
                try {
                  events[events.length] = "inner-body";
                  throw new Error("boom");
                } catch (error) {
                  events[events.length] = error.message;
                  throw new TypeError("wrapped");
                } finally {
                  events[events.length] = "inner-finally";
                }
              } catch (error) {
                events[events.length] = error.name;
              } finally {
                events[events.length] = "outer-finally";
              }
              return events;
            }
            nested();
            "#);
    assert_eq!(
        value,
        StructuredValue::Array(vec![
            StructuredValue::String("inner-body".to_string()),
            StructuredValue::String("boom".to_string()),
            StructuredValue::String("inner-finally".to_string()),
            StructuredValue::String("TypeError".to_string()),
            StructuredValue::String("outer-finally".to_string()),
        ])
    );
}

#[test]
fn catches_host_errors_after_resume() {
    let program = compile(
        r#"
            let captured;
            try {
              fetch_data(1);
            } catch (error) {
              captured = [error.name, error.message, error.code, error.details.status];
            }
            captured;
            "#,
    )
    .expect("source should compile");

    let step = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("execution should suspend");

    let suspension = match step {
        ExecutionStep::Suspended(suspension) => suspension,
        other => panic!("expected suspension, got {other:?}"),
    };

    let resumed = resume(
        suspension.snapshot,
        ResumePayload::Error(HostError {
            name: "CapabilityError".to_string(),
            message: "upstream failed".to_string(),
            code: Some("E_UPSTREAM".to_string()),
            details: Some(StructuredValue::Object(IndexMap::from([(
                "status".to_string(),
                StructuredValue::Number(StructuredNumber::Finite(503.0)),
            )]))),
        }),
    )
    .expect("guest catch should handle resumed host errors");

    match resumed {
        ExecutionStep::Completed(value) => {
            assert_eq!(
                value,
                StructuredValue::Array(vec![
                    StructuredValue::String("CapabilityError".to_string()),
                    StructuredValue::String("upstream failed".to_string()),
                    StructuredValue::String("E_UPSTREAM".to_string()),
                    StructuredValue::Number(StructuredNumber::Finite(503.0)),
                ])
            );
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn round_trips_program_and_snapshot() {
    let program =
        compile("const value = fetch_data(1); value + 2;").expect("compile should succeed");
    let bytecode = lower_to_bytecode(&program).expect("lowering should succeed");
    let program_bytes = dump_program(&bytecode).expect("program dump should succeed");
    let loaded_program = load_program(&program_bytes).expect("program load should succeed");
    assert_eq!(loaded_program.root, bytecode.root);
    assert_eq!(loaded_program.functions.len(), bytecode.functions.len());

    let step = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("execution should suspend");
    let suspension = match step {
        ExecutionStep::Suspended(suspension) => suspension,
        other => panic!("expected suspension, got {other:?}"),
    };
    let snapshot_bytes = dump_snapshot(&suspension.snapshot).expect("snapshot dump should succeed");
    let loaded_snapshot = load_snapshot(&snapshot_bytes).expect("snapshot load should succeed");
    let resumed = resume(
        loaded_snapshot,
        ResumePayload::Value(StructuredValue::Number(StructuredNumber::Finite(1.0))),
    )
    .expect("resume should succeed");
    match resumed {
        ExecutionStep::Completed(value) => {
            assert_eq!(
                value,
                StructuredValue::Number(StructuredNumber::Finite(3.0))
            );
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn rejects_invalid_jump_targets_before_execution() {
    let program = invalid_program(vec![Instruction::Jump(99), Instruction::Return]);
    let error = start_bytecode(&program, ExecutionOptions::default())
        .expect_err("invalid jump target should fail validation");
    assert!(error.to_string().contains("jumps to invalid target 99"));
}

#[test]
fn rejects_inconsistent_stack_depth_in_serialized_programs() {
    let program = invalid_program(vec![
        Instruction::PushNumber(1.0),
        Instruction::JumpIfTrue(3),
        Instruction::Pop,
        Instruction::Return,
    ]);
    let bytes = dump_program(&program).expect("invalid program still serializes");
    let error =
        load_program(&bytes).expect_err("invalid serialized program should fail validation");
    assert!(
        error
            .to_string()
            .contains("has inconsistent validation state")
    );
}

#[test]
fn rejects_cross_version_serialized_programs() {
    let program = lower_to_bytecode(&compile("1;").expect("compile should succeed"))
        .expect("lowering should succeed");
    let mut bytes = dump_program(&program).expect("program should serialize");
    bytes[0] = bytes[0].saturating_add(1);
    let error = load_program(&bytes).expect_err("cross-version program should be rejected");
    assert!(
        error
            .to_string()
            .contains("serialized program version mismatch")
    );
}

#[test]
fn rejects_invalid_snapshot_frame_state() {
    let program =
        compile("const value = fetch_data(1); value + 2;").expect("compile should succeed");
    let step = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("execution should suspend");
    let mut suspension = match step {
        ExecutionStep::Suspended(suspension) => *suspension,
        other => panic!("expected suspension, got {other:?}"),
    };
    suspension.snapshot.runtime.frames[0].ip = 999;
    let bytes = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
    let error = load_snapshot(&bytes).expect_err("invalid snapshot should fail validation");
    assert!(
        error
            .to_string()
            .contains("frame instruction pointer 999 is out of range")
    );
}

#[test]
fn rejects_cross_version_snapshots() {
    let program =
        compile("const value = fetch_data(1); value + 2;").expect("compile should succeed");
    let step = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .expect("execution should suspend");
    let suspension = match step {
        ExecutionStep::Suspended(suspension) => suspension,
        other => panic!("expected suspension, got {other:?}"),
    };
    let mut bytes = dump_snapshot(&suspension.snapshot).expect("snapshot should serialize");
    bytes[0] = bytes[0].saturating_add(1);
    let error = load_snapshot(&bytes).expect_err("cross-version snapshot should be rejected");
    assert!(
        error
            .to_string()
            .contains("serialized snapshot version mismatch")
    );
}
