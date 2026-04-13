use std::sync::Arc;

use super::*;

#[test]
fn tracks_heap_growth_and_enforces_heap_limits() {
    let program = lower_to_bytecode(&compile("1;").expect("source should compile"))
        .expect("lowering should succeed");
    let mut runtime = Runtime::new(Arc::new(program.clone()), ExecutionOptions::default())
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

    let mut heap_limited = Runtime::new(Arc::new(program.clone()), ExecutionOptions::default())
        .expect("runtime should initialize");
    heap_limited.limits.heap_limit_bytes = heap_limited.heap_bytes_used;
    let error = heap_limited
        .insert_array(vec![Value::String("payload".to_string())], IndexMap::new())
        .expect_err("next allocation should exceed the heap limit");
    assert!(error.to_string().contains("heap limit exceeded"));

    let mut allocation_limited = Runtime::new(Arc::new(program), ExecutionOptions::default())
        .expect("runtime should initialize");
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
    let mut runtime =
        Runtime::new(Arc::new(program), ExecutionOptions::default()).expect("runtime init");

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
    let mut runtime =
        Runtime::new(Arc::new(program), ExecutionOptions::default()).expect("runtime init");

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
    let mut runtime =
        Runtime::new(Arc::new(program), ExecutionOptions::default()).expect("runtime init");

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
fn keyed_collections_participate_in_heap_accounting_and_gc() {
    let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
        .expect("lowering should succeed");
    let mut runtime =
        Runtime::new(Arc::new(program), ExecutionOptions::default()).expect("runtime init");

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
        Runtime::new(Arc::new(program.clone()), ExecutionOptions::default()).expect("runtime init");

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
