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
