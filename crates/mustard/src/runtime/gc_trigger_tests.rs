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
