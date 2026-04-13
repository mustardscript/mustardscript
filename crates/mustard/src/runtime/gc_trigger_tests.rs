use std::sync::Arc;

use indexmap::IndexMap;

use super::*;
use crate::compile;

fn test_runtime() -> Runtime {
    let program = lower_to_bytecode(&compile("0;").expect("source should compile"))
        .expect("lowering should succeed");
    Runtime::new(Arc::new(program), ExecutionOptions::default()).expect("runtime init")
}

#[test]
fn garbage_collection_preflight_skips_low_pressure_cycles() {
    let mut runtime = test_runtime();
    let garbage_object = runtime
        .insert_object(IndexMap::new(), ObjectKind::Plain)
        .expect("garbage object should allocate");

    runtime
        .collect_garbage_before_instruction(&Instruction::MakeObject { keys: Vec::new() })
        .expect("gc preflight should succeed");

    assert!(
        runtime.objects.contains_key(garbage_object),
        "low-pressure allocation boundaries should not force a full collection",
    );
}

#[test]
fn allocation_pressure_collects_garbage_before_limit_failures() {
    let mut runtime = test_runtime();
    let baseline_heap = runtime.heap_bytes_used;
    let baseline_allocations = runtime.allocation_count;
    let garbage_array = runtime
        .insert_array(vec![Value::String("payload".repeat(256))], IndexMap::new())
        .expect("garbage array should allocate");

    runtime.limits.heap_limit_bytes = baseline_heap + 512;
    runtime.limits.allocation_budget = baseline_allocations + 1;

    let kept_object = runtime
        .insert_object(IndexMap::new(), ObjectKind::Plain)
        .expect("allocation pressure should trigger gc before failing");

    assert!(runtime.objects.contains_key(kept_object));
    assert!(
        !runtime.arrays.contains_key(garbage_array),
        "pressure-triggered collection should reclaim unreachable garbage",
    );
    assert!(runtime.heap_bytes_used <= runtime.limits.heap_limit_bytes);
    assert!(runtime.allocation_count <= runtime.limits.allocation_budget);
}

#[test]
fn garbage_collection_updates_cached_totals_from_reclaimed_items() {
    let mut runtime = test_runtime();
    let kept_object = runtime
        .insert_object(IndexMap::new(), ObjectKind::Plain)
        .expect("kept object should allocate");
    runtime.root_result = Some(Value::Object(kept_object));

    let garbage_object = runtime
        .insert_object(IndexMap::new(), ObjectKind::Plain)
        .expect("garbage object should allocate");
    let garbage_array = runtime
        .insert_array(vec![Value::String("payload".repeat(128))], IndexMap::new())
        .expect("garbage array should allocate");

    let baseline_heap = runtime.heap_bytes_used;
    let baseline_allocations = runtime.allocation_count;
    let stats = runtime.collect_garbage().expect("gc should succeed");

    assert!(runtime.objects.contains_key(kept_object));
    assert!(!runtime.objects.contains_key(garbage_object));
    assert!(!runtime.arrays.contains_key(garbage_array));
    assert!(stats.reclaimed_bytes > 0);
    assert!(stats.reclaimed_allocations >= 2);
    assert_eq!(
        runtime.heap_bytes_used,
        baseline_heap - stats.reclaimed_bytes
    );
    assert_eq!(
        runtime.allocation_count,
        baseline_allocations - stats.reclaimed_allocations
    );

    let after_gc_totals = (runtime.heap_bytes_used, runtime.allocation_count);
    let second_stats = runtime.collect_garbage().expect("second gc should succeed");
    assert_eq!(second_stats.reclaimed_bytes, 0);
    assert_eq!(second_stats.reclaimed_allocations, 0);
    assert_eq!(
        after_gc_totals,
        (runtime.heap_bytes_used, runtime.allocation_count)
    );
}

#[test]
fn runtime_debug_metrics_track_gc_and_accounting_refreshes() {
    let mut runtime = test_runtime();
    let object = runtime
        .insert_object(IndexMap::new(), ObjectKind::Plain)
        .expect("object should allocate");

    let baseline_metrics = runtime.debug_metrics();
    runtime
        .objects
        .get_mut(object)
        .expect("object should exist")
        .properties
        .insert("label".to_string(), Value::String("payload".to_string()));
    runtime
        .refresh_object_accounting(object)
        .expect("object accounting refresh should succeed");

    let refreshed_metrics = runtime.debug_metrics();
    assert_eq!(
        refreshed_metrics.accounting_refreshes,
        baseline_metrics.accounting_refreshes + 1
    );

    let gc_metrics_before = runtime.debug_metrics();
    let stats = runtime.collect_garbage().expect("gc should succeed");
    let gc_metrics_after = runtime.debug_metrics();
    assert_eq!(
        gc_metrics_after.gc_collections,
        gc_metrics_before.gc_collections + 1
    );
    assert!(gc_metrics_after.gc_total_time_ns > gc_metrics_before.gc_total_time_ns);
    assert_eq!(
        gc_metrics_after.gc_reclaimed_bytes,
        gc_metrics_before.gc_reclaimed_bytes + stats.reclaimed_bytes as u64
    );
    assert_eq!(
        gc_metrics_after.gc_reclaimed_allocations,
        gc_metrics_before.gc_reclaimed_allocations + stats.reclaimed_allocations as u64
    );
}

#[test]
fn lexical_binding_deltas_preserve_cached_totals_without_full_refreshes() {
    let mut runtime = test_runtime();
    let frame_env = runtime
        .new_env(Some(runtime.globals))
        .expect("frame env should allocate");
    let baseline_metrics = runtime.debug_metrics();

    runtime
        .push_frame(0, frame_env, &[], Value::String("self".to_string()), None)
        .expect("frame push should succeed");
    runtime
        .declare_name(frame_env, "value".to_string(), true)
        .expect("binding should declare");
    runtime
        .initialize_name_in_env(frame_env, "value", Value::String("seed".to_string()))
        .expect("binding should initialize");
    runtime
        .assign_name(frame_env, "value", Value::String("payload".to_string()))
        .expect("binding should assign");

    let final_metrics = runtime.debug_metrics();
    assert_eq!(
        final_metrics.accounting_refreshes,
        baseline_metrics.accounting_refreshes
    );
    #[cfg(debug_assertions)]
    runtime.debug_assert_cached_accounting_matches_full_walk();
}

#[test]
fn array_length_and_object_from_entries_deltas_preserve_cached_totals() {
    let mut runtime = test_runtime();
    let array = runtime
        .insert_array(
            vec![
                Value::String("alpha".repeat(16)),
                Value::String("beta".repeat(16)),
                Value::String("gamma".repeat(16)),
            ],
            IndexMap::from([("length".to_string(), Value::Number(99.0))]),
        )
        .expect("array should allocate");
    let pair_alpha = runtime
        .insert_array(
            vec![Value::String("alpha".to_string()), Value::Number(1.0)],
            IndexMap::new(),
        )
        .expect("pair should allocate");
    let pair_alpha_update = runtime
        .insert_array(
            vec![
                Value::String("alpha".to_string()),
                Value::String("payload".repeat(16)),
            ],
            IndexMap::new(),
        )
        .expect("pair should allocate");
    let pair_beta = runtime
        .insert_array(
            vec![Value::String("beta".to_string()), Value::Bool(true)],
            IndexMap::new(),
        )
        .expect("pair should allocate");
    let entries = runtime
        .insert_array(
            vec![
                Value::Array(pair_alpha),
                Value::Array(pair_alpha_update),
                Value::Array(pair_beta),
            ],
            IndexMap::new(),
        )
        .expect("entries array should allocate");
    let frame_env = runtime
        .new_env(Some(runtime.globals))
        .expect("frame env should allocate");
    runtime
        .push_frame(0, frame_env, &[], Value::Undefined, None)
        .expect("frame push should succeed");
    let baseline_metrics = runtime.debug_metrics();

    runtime
        .set_array_length(array, Value::Number(1.0))
        .expect("array shrink should succeed");
    runtime
        .set_array_length(array, Value::Number(4.0))
        .expect("array growth should succeed");
    let after_array_metrics = runtime.debug_metrics();
    assert_eq!(
        after_array_metrics.accounting_refreshes,
        baseline_metrics.accounting_refreshes
    );
    let built = runtime
        .call_object_from_entries(&[Value::Array(entries)])
        .expect("Object.fromEntries should succeed");

    let Value::Object(object) = built else {
        panic!("Object.fromEntries should return an object");
    };
    let object = runtime.objects.get(object).expect("object should exist");
    assert!(matches!(
        object.properties.get("alpha"),
        Some(Value::String(value)) if value == &"payload".repeat(16)
    ));
    assert!(matches!(
        object.properties.get("beta"),
        Some(Value::Bool(true))
    ));

    let array = runtime.arrays.get(array).expect("array should exist");
    assert_eq!(array.elements.len(), 4);
    assert!(matches!(
        array.elements[0].as_ref(),
        Some(Value::String(value)) if value == &"alpha".repeat(16)
    ));
    assert!(array.elements[1].is_none());
    assert!(array.elements[2].is_none());
    assert!(array.elements[3].is_none());
    assert!(
        !array.properties.contains_key("length"),
        "array writes should not leave a custom length property behind",
    );

    let final_metrics = runtime.debug_metrics();
    assert_eq!(
        final_metrics.accounting_refreshes, baseline_metrics.accounting_refreshes,
        "Array length updates and Object.fromEntries should stay on the incremental accounting path"
    );
    #[cfg(debug_assertions)]
    runtime.debug_assert_cached_accounting_matches_full_walk();
}

#[test]
fn hot_path_mutations_preserve_cached_totals_without_full_refreshes() {
    let mut runtime = test_runtime();
    let array = runtime
        .insert_array(
            vec![
                Value::String("alpha".to_string()),
                Value::String("b".to_string()),
                Value::String("ccc".to_string()),
            ],
            IndexMap::new(),
        )
        .expect("array should allocate");
    let closure = runtime
        .insert_closure(runtime.program.root, runtime.globals, Value::Undefined)
        .expect("closure should allocate");
    let regex = match runtime
        .make_regexp_value("a".to_string(), "g".to_string())
        .expect("regexp should allocate")
    {
        Value::Object(object) => object,
        other => panic!("expected regexp object, got {other:?}"),
    };
    let string_iterator = match runtime
        .create_iterator(Value::String("ab".to_string()))
        .expect("string iterator should allocate")
    {
        Value::Iterator(iterator) => iterator,
        other => panic!("expected iterator, got {other:?}"),
    };
    let frame_env = runtime
        .new_env(Some(runtime.globals))
        .expect("frame env should allocate");
    runtime
        .push_frame(0, frame_env, &[], Value::Undefined, None)
        .expect("frame push should succeed");

    let baseline_metrics = runtime.debug_metrics();

    runtime
        .call_array_fill(
            Value::Array(array),
            &[Value::String("payload".repeat(4)), Value::Number(1.0)],
        )
        .expect("array fill should succeed");
    runtime
        .call_array_splice(
            Value::Array(array),
            &[
                Value::Number(0.0),
                Value::Number(1.0),
                Value::String("zz".to_string()),
            ],
        )
        .expect("array splice should succeed");
    runtime
        .call_array_sort(Value::Array(array), &[])
        .expect("array sort should succeed");

    runtime
        .define_global("named".to_string(), Value::Closure(closure), true)
        .expect("define_global should succeed");
    runtime
        .set_property_static(
            Value::Closure(closure),
            "meta",
            Value::String("payload".repeat(8)),
        )
        .expect("closure custom property should succeed");
    runtime
        .set_property_static(
            Value::BuiltinFunction(BuiltinFunction::ArrayMap),
            "meta",
            Value::Bool(true),
        )
        .expect("builtin custom property should succeed");
    runtime
        .set_property_static(
            Value::HostFunction("fetch_data".to_string()),
            "meta",
            Value::Number(1.0),
        )
        .expect("host custom property should succeed");

    runtime
        .iterator_next(Value::Iterator(string_iterator))
        .expect("first string iterator step should succeed");
    runtime
        .iterator_next(Value::Iterator(string_iterator))
        .expect("second string iterator step should succeed");
    runtime
        .call_regexp_exec(Value::Object(regex), &[Value::String("aba".to_string())])
        .expect("regexp exec should succeed");
    runtime
        .call_string_match(Value::String("aba".to_string()), &[Value::Object(regex)])
        .expect("string match should succeed");
    runtime
        .call_string_match_all(Value::String("aba".to_string()), &[Value::Object(regex)])
        .expect("string matchAll should succeed");

    let final_metrics = runtime.debug_metrics();
    assert_eq!(
        final_metrics.accounting_refreshes,
        baseline_metrics.accounting_refreshes
    );
    #[cfg(debug_assertions)]
    runtime.debug_assert_cached_accounting_matches_full_walk();

    let array_ref = runtime.arrays.get(array).expect("array should exist");
    let sorted = array_ref
        .elements
        .iter()
        .map(|value| value.clone().unwrap_or(Value::Undefined))
        .collect::<Vec<_>>();
    assert!(matches!(
        sorted.as_slice(),
        [Value::String(first), Value::String(second), Value::String(third)]
            if first == "payloadpayloadpayloadpayload"
                && second == "payloadpayloadpayloadpayload"
                && third == "zz"
    ));
    assert_eq!(
        runtime
            .closures
            .get(closure)
            .and_then(|closure| closure.name.as_deref()),
        Some("named")
    );
    assert!(matches!(
        runtime
            .builtin_function_custom_property(BuiltinFunction::ArrayMap, "meta")
            .expect("builtin custom property should exist"),
        Some(Value::Bool(true))
    ));
    assert!(matches!(
        runtime
            .host_function_custom_property("fetch_data", "meta")
            .expect("host custom property should exist"),
        Some(Value::Number(value)) if value == 1.0
    ));
    assert_eq!(
        runtime
            .regexp_object(regex)
            .expect("regexp should exist")
            .last_index,
        0
    );
}

#[test]
fn keyed_collection_constructors_trim_unused_builder_slots_after_duplicates() {
    let mut runtime = test_runtime();

    let pair_alpha = runtime
        .insert_array(
            vec![Value::String("alpha".to_string()), Value::Number(1.0)],
            IndexMap::new(),
        )
        .expect("pair should allocate");
    let pair_beta = runtime
        .insert_array(
            vec![Value::String("beta".to_string()), Value::Number(2.0)],
            IndexMap::new(),
        )
        .expect("pair should allocate");
    let pair_alpha_update = runtime
        .insert_array(
            vec![Value::String("alpha".to_string()), Value::Number(3.0)],
            IndexMap::new(),
        )
        .expect("pair should allocate");
    let map_entries = runtime
        .insert_array(
            vec![
                Value::Array(pair_alpha),
                Value::Array(pair_beta),
                Value::Array(pair_alpha_update),
            ],
            IndexMap::new(),
        )
        .expect("outer entries should allocate");

    let Value::Map(map) = runtime
        .construct_map(&[Value::Array(map_entries)])
        .expect("Map constructor should succeed")
    else {
        panic!("Map constructor should produce a map");
    };
    let map_ref = runtime.maps.get(map).expect("map should exist");
    assert_eq!(map_ref.live_len, 2);
    assert_eq!(
        map_ref.entries.len(),
        2,
        "duplicate-heavy constructors should trim unused builder slots"
    );
    let entries: Vec<_> = map_ref.entries.iter().flatten().collect();
    assert!(matches!(entries[0].key, Value::String(ref value) if value == "alpha"));
    assert!(matches!(entries[0].value, Value::Number(value) if value == 3.0));
    assert!(matches!(entries[1].key, Value::String(ref value) if value == "beta"));

    let set_values = runtime
        .insert_array(
            vec![
                Value::String("alpha".to_string()),
                Value::String("beta".to_string()),
                Value::String("alpha".to_string()),
            ],
            IndexMap::new(),
        )
        .expect("set values should allocate");
    let Value::Set(set) = runtime
        .construct_set(&[Value::Array(set_values)])
        .expect("Set constructor should succeed")
    else {
        panic!("Set constructor should produce a set");
    };
    let set_ref = runtime.sets.get(set).expect("set should exist");
    assert_eq!(set_ref.live_len, 2);
    assert_eq!(
        set_ref.entries.len(),
        2,
        "duplicate-heavy constructors should trim unused builder slots"
    );
    let values: Vec<_> = set_ref.entries.iter().flatten().collect();
    assert!(matches!(values[0], Value::String(value) if value == "alpha"));
    assert!(matches!(values[1], Value::String(value) if value == "beta"));
    runtime.debug_assert_cached_accounting_matches_full_walk();
}

#[test]
fn map_and_set_deltas_preserve_cached_totals_without_full_refreshes() {
    let mut runtime = test_runtime();
    let map = runtime.insert_map(Vec::new()).expect("map should allocate");
    let set = runtime.insert_set(Vec::new()).expect("set should allocate");
    let baseline_metrics = runtime.debug_metrics();

    runtime
        .call_map_set(
            Value::Map(map),
            &[
                Value::String("alpha".to_string()),
                Value::String("seed".to_string()),
            ],
        )
        .expect("map insert should succeed");
    runtime
        .call_map_set(
            Value::Map(map),
            &[
                Value::String("alpha".to_string()),
                Value::String("payload".repeat(32)),
            ],
        )
        .expect("map update should succeed");
    runtime
        .call_map_set(Value::Map(map), &[Value::Number(1.0), Value::Bool(true)])
        .expect("second map insert should succeed");
    runtime
        .call_map_delete(Value::Map(map), &[Value::Number(1.0)])
        .expect("map delete should succeed");
    runtime
        .call_map_clear(Value::Map(map))
        .expect("map clear should succeed");

    runtime
        .call_set_add(Value::Set(set), &[Value::String("beta".repeat(16))])
        .expect("set insert should succeed");
    runtime
        .call_set_add(Value::Set(set), &[Value::String("beta".repeat(16))])
        .expect("duplicate set insert should be a no-op");
    runtime
        .call_set_add(Value::Set(set), &[Value::Number(2.0)])
        .expect("second set insert should succeed");
    runtime
        .call_set_delete(Value::Set(set), &[Value::Number(2.0)])
        .expect("set delete should succeed");
    runtime
        .call_set_clear(Value::Set(set))
        .expect("set clear should succeed");

    let final_metrics = runtime.debug_metrics();
    assert_eq!(
        final_metrics.accounting_refreshes,
        baseline_metrics.accounting_refreshes
    );
    #[cfg(debug_assertions)]
    runtime.debug_assert_cached_accounting_matches_full_walk();
}

#[test]
fn promise_accounting_deltas_preserve_cached_totals_without_full_refreshes() {
    let mut runtime = test_runtime();
    let source = runtime
        .insert_promise(PromiseState::Pending)
        .expect("source promise should allocate");
    let dependent = runtime
        .insert_promise(PromiseState::Pending)
        .expect("dependent promise should allocate");
    let reaction_target = runtime
        .insert_promise(PromiseState::Pending)
        .expect("reaction target should allocate");

    let baseline_metrics = runtime.debug_metrics();
    runtime
        .attach_awaiter(source, AsyncContinuation { frames: Vec::new() })
        .expect("awaiter should attach");
    runtime
        .attach_dependent(source, dependent)
        .expect("dependent should attach");
    runtime
        .attach_promise_reaction(
            source,
            PromiseReaction::Then {
                target: reaction_target,
                on_fulfilled: None,
                on_rejected: None,
            },
        )
        .expect("reaction should attach");
    runtime
        .replace_promise_driver(
            source,
            Some(PromiseDriver::Thenable {
                value: Value::String("thenable".to_string()),
            }),
        )
        .expect("thenable driver should attach");

    let attached_metrics = runtime.debug_metrics();
    assert_eq!(
        attached_metrics.accounting_refreshes,
        baseline_metrics.accounting_refreshes
    );
    #[cfg(debug_assertions)]
    runtime.debug_assert_cached_accounting_matches_full_walk();

    runtime
        .settle_promise_with_outcome(
            source,
            PromiseOutcome::Fulfilled(Value::String("done".to_string())),
        )
        .expect("promise should settle");

    let settled_metrics = runtime.debug_metrics();
    assert_eq!(
        settled_metrics.accounting_refreshes,
        baseline_metrics.accounting_refreshes
    );
    #[cfg(debug_assertions)]
    runtime.debug_assert_cached_accounting_matches_full_walk();

    assert_eq!(runtime.microtasks.len(), 2);
    assert!(matches!(
        runtime.promise_outcome(source).expect("source outcome should exist"),
        Some(PromiseOutcome::Fulfilled(Value::String(ref value))) if value == "done"
    ));
    assert!(matches!(
        runtime.promise_outcome(dependent).expect("dependent outcome should exist"),
        Some(PromiseOutcome::Fulfilled(Value::String(ref value))) if value == "done"
    ));
}

#[test]
fn promise_combinator_completion_moves_driver_buffers_without_full_refreshes() {
    let mut runtime = test_runtime();
    let all_target = runtime
        .insert_promise(PromiseState::Pending)
        .expect("Promise.all target should allocate");
    let all_settled_target = runtime
        .insert_promise(PromiseState::Pending)
        .expect("Promise.allSettled target should allocate");
    let any_target = runtime
        .insert_promise(PromiseState::Pending)
        .expect("Promise.any target should allocate");

    runtime
        .replace_promise_driver(
            all_target,
            Some(PromiseDriver::All {
                remaining: 1,
                values: vec![Some(Value::String("alpha".to_string())), None],
            }),
        )
        .expect("Promise.all driver should attach");
    runtime
        .replace_promise_driver(
            all_settled_target,
            Some(PromiseDriver::AllSettled {
                remaining: 1,
                results: vec![
                    Some(PromiseSettledResult::Fulfilled(Value::String(
                        "ok".to_string(),
                    ))),
                    None,
                ],
            }),
        )
        .expect("Promise.allSettled driver should attach");
    runtime
        .replace_promise_driver(
            any_target,
            Some(PromiseDriver::Any {
                remaining: 1,
                reasons: vec![Some(Value::String("first".to_string())), None],
            }),
        )
        .expect("Promise.any driver should attach");

    let baseline_metrics = runtime.debug_metrics();
    runtime
        .activate_promise_combinator(
            all_target,
            1,
            PromiseCombinatorKind::All,
            PromiseOutcome::Fulfilled(Value::String("beta".to_string())),
        )
        .expect("Promise.all completion should succeed");
    #[cfg(debug_assertions)]
    runtime.debug_assert_cached_accounting_matches_full_walk();
    runtime
        .activate_promise_combinator(
            all_settled_target,
            1,
            PromiseCombinatorKind::AllSettled,
            PromiseOutcome::Rejected(PromiseRejection {
                value: Value::String("boom".to_string()),
                span: None,
                traceback: Vec::new(),
            }),
        )
        .expect("Promise.allSettled completion should succeed");
    #[cfg(debug_assertions)]
    runtime.debug_assert_cached_accounting_matches_full_walk();
    runtime
        .activate_promise_combinator(
            any_target,
            1,
            PromiseCombinatorKind::Any,
            PromiseOutcome::Rejected(PromiseRejection {
                value: Value::String("second".to_string()),
                span: None,
                traceback: Vec::new(),
            }),
        )
        .expect("Promise.any completion should succeed");

    let final_metrics = runtime.debug_metrics();
    assert_eq!(
        final_metrics.accounting_refreshes,
        baseline_metrics.accounting_refreshes
    );
    #[cfg(debug_assertions)]
    runtime.debug_assert_cached_accounting_matches_full_walk();

    let all_result = match runtime
        .promise_outcome(all_target)
        .expect("Promise.all outcome should exist")
    {
        Some(PromiseOutcome::Fulfilled(Value::Array(array))) => array,
        other => panic!("expected Promise.all to fulfill to an array, got {other:?}"),
    };
    let all_values = runtime
        .arrays
        .get(all_result)
        .expect("Promise.all array should exist")
        .elements
        .iter()
        .map(|value| value.clone().unwrap_or(Value::Undefined))
        .collect::<Vec<_>>();
    assert!(matches!(
        all_values.as_slice(),
        [Value::String(first), Value::String(second)] if first == "alpha" && second == "beta"
    ));

    let all_settled_result = match runtime
        .promise_outcome(all_settled_target)
        .expect("Promise.allSettled outcome should exist")
    {
        Some(PromiseOutcome::Fulfilled(Value::Array(array))) => array,
        other => panic!("expected Promise.allSettled to fulfill to an array, got {other:?}"),
    };
    assert_eq!(
        runtime
            .arrays
            .get(all_settled_result)
            .expect("Promise.allSettled array should exist")
            .elements
            .len(),
        2
    );

    assert!(matches!(
        runtime
            .promise_outcome(any_target)
            .expect("Promise.any outcome should exist"),
        Some(PromiseOutcome::Rejected(PromiseRejection {
            value: Value::Object(_),
            ..
        }))
    ));
}
