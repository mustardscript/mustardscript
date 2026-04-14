use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use indexmap::IndexMap;
use mustard::structured::StructuredNumber;
use mustard::{
    BytecodeProgram, ExecutionOptions, ExecutionSnapshot, ExecutionStep, RuntimeLimits,
    StructuredValue, compile, dump_program, dump_snapshot, load_program, load_snapshot,
    lower_to_bytecode, start_shared_bytecode, start_validated_bytecode,
};

struct BenchFixtures {
    runtime_init_program: BytecodeProgram,
    small_compute_program: BytecodeProgram,
    small_compute_shared: Arc<BytecodeProgram>,
    serialized_small_compute_program: Vec<u8>,
    vm_hot_loop_shared: Arc<BytecodeProgram>,
    local_load_store_shared: Arc<BytecodeProgram>,
    closure_access_shared: Arc<BytecodeProgram>,
    env_lookup_shared: Arc<BytecodeProgram>,
    global_lookup_shared: Arc<BytecodeProgram>,
    property_access_shared: Arc<BytecodeProgram>,
    builtin_method_shared: Arc<BytecodeProgram>,
    array_from_shared: Arc<BytecodeProgram>,
    object_from_entries_shared: Arc<BytecodeProgram>,
    array_callback_shared: Arc<BytecodeProgram>,
    collection_callback_shared: Arc<BytecodeProgram>,
    promise_all_immediate_shared: Arc<BytecodeProgram>,
    promise_all_settled_immediate_shared: Arc<BytecodeProgram>,
    promise_all_derived_ids_shared: Arc<BytecodeProgram>,
    promise_all_map_set_reduction_shared: Arc<BytecodeProgram>,
    map_set_shared: Arc<BytecodeProgram>,
    map_ctor_large_shared: Arc<BytecodeProgram>,
    set_ctor_large_shared: Arc<BytecodeProgram>,
    map_get_large_shared: Arc<BytecodeProgram>,
    map_set_large_shared: Arc<BytecodeProgram>,
    map_has_large_shared: Arc<BytecodeProgram>,
    set_add_large_shared: Arc<BytecodeProgram>,
    set_has_large_shared: Arc<BytecodeProgram>,
    set_delete_large_shared: Arc<BytecodeProgram>,
    collection_iterator_large_shared: Arc<BytecodeProgram>,
    boundary_decode_shared: Arc<BytecodeProgram>,
    boundary_encode_shared: Arc<BytecodeProgram>,
    boundary_payload: StructuredValue,
    boundary_inputs: IndexMap<String, StructuredValue>,
    suspended_snapshot: ExecutionSnapshot,
    suspended_snapshot_bytes: Vec<u8>,
}

impl BenchFixtures {
    fn new() -> Self {
        let runtime_init_program = compile_to_bytecode("0;");
        let small_compute_program = compile_to_bytecode(small_compute_source());
        let serialized_small_compute_program =
            dump_program(&small_compute_program).expect("small compute program should serialize");
        let small_compute_shared = Arc::new(small_compute_program.clone());
        let vm_hot_loop_shared = Arc::new(compile_to_bytecode(vm_hot_loop_source()));
        let local_load_store_shared = Arc::new(compile_to_bytecode(local_load_store_source()));
        let closure_access_shared = Arc::new(compile_to_bytecode(closure_access_source()));
        let env_lookup_shared = Arc::new(compile_to_bytecode(env_lookup_source()));
        let global_lookup_shared = Arc::new(compile_to_bytecode(global_lookup_source()));
        let property_access_shared = Arc::new(compile_to_bytecode(property_access_source()));
        let builtin_method_shared = Arc::new(compile_to_bytecode(builtin_method_source()));
        let array_from_shared = Arc::new(compile_to_bytecode(array_from_source()));
        let object_from_entries_shared =
            Arc::new(compile_to_bytecode(object_from_entries_source()));
        let array_callback_shared = Arc::new(compile_to_bytecode(array_callback_source()));
        let collection_callback_shared =
            Arc::new(compile_to_bytecode(collection_callback_source()));
        let promise_all_immediate_shared =
            Arc::new(compile_to_bytecode(promise_all_immediate_source()));
        let promise_all_settled_immediate_shared =
            Arc::new(compile_to_bytecode(promise_all_settled_immediate_source()));
        let promise_all_derived_ids_shared =
            Arc::new(compile_to_bytecode(promise_all_derived_ids_source()));
        let promise_all_map_set_reduction_shared =
            Arc::new(compile_to_bytecode(promise_all_map_set_reduction_source()));
        let map_set_shared = Arc::new(compile_to_bytecode(map_set_source()));
        let map_ctor_large_shared = Arc::new(compile_to_bytecode(map_ctor_large_source()));
        let set_ctor_large_shared = Arc::new(compile_to_bytecode(set_ctor_large_source()));
        let map_get_large_shared = Arc::new(compile_to_bytecode(map_get_large_source()));
        let map_set_large_shared = Arc::new(compile_to_bytecode(map_set_large_source()));
        let map_has_large_shared = Arc::new(compile_to_bytecode(map_has_large_source()));
        let set_add_large_shared = Arc::new(compile_to_bytecode(set_add_large_source()));
        let set_has_large_shared = Arc::new(compile_to_bytecode(set_has_large_source()));
        let set_delete_large_shared = Arc::new(compile_to_bytecode(set_delete_large_source()));
        let collection_iterator_large_shared =
            Arc::new(compile_to_bytecode(collection_iterator_large_source()));
        let boundary_decode_shared = Arc::new(compile_to_bytecode(boundary_decode_source()));
        let boundary_encode_shared = Arc::new(compile_to_bytecode(boundary_encode_source()));
        let boundary_payload = nested_boundary_payload();
        let boundary_inputs = IndexMap::from([("payload".to_string(), boundary_payload.clone())]);
        let suspended_snapshot = create_suspended_snapshot(
            &boundary_encode_shared,
            ExecutionOptions {
                capabilities: vec!["emit".to_string()],
                ..ExecutionOptions::default()
            },
        );
        let suspended_snapshot_bytes =
            dump_snapshot(&suspended_snapshot).expect("snapshot should serialize");

        Self {
            runtime_init_program,
            small_compute_program,
            small_compute_shared,
            serialized_small_compute_program,
            vm_hot_loop_shared,
            local_load_store_shared,
            closure_access_shared,
            env_lookup_shared,
            global_lookup_shared,
            property_access_shared,
            builtin_method_shared,
            array_from_shared,
            object_from_entries_shared,
            array_callback_shared,
            collection_callback_shared,
            promise_all_immediate_shared,
            promise_all_settled_immediate_shared,
            promise_all_derived_ids_shared,
            promise_all_map_set_reduction_shared,
            map_set_shared,
            map_ctor_large_shared,
            set_ctor_large_shared,
            map_get_large_shared,
            map_set_large_shared,
            map_has_large_shared,
            set_add_large_shared,
            set_has_large_shared,
            set_delete_large_shared,
            collection_iterator_large_shared,
            boundary_decode_shared,
            boundary_encode_shared,
            boundary_payload,
            boundary_inputs,
            suspended_snapshot,
            suspended_snapshot_bytes,
        }
    }
}

fn fixtures() -> &'static BenchFixtures {
    static FIXTURES: OnceLock<BenchFixtures> = OnceLock::new();
    FIXTURES.get_or_init(BenchFixtures::new)
}

fn bench_config() -> Criterion {
    Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_millis(200))
        .measurement_time(Duration::from_millis(600))
}

fn compile_to_bytecode(source: &str) -> BytecodeProgram {
    let program = compile(source).expect("benchmark source should compile");
    lower_to_bytecode(&program).expect("benchmark source should lower")
}

fn create_suspended_snapshot(
    program: &Arc<BytecodeProgram>,
    options: ExecutionOptions,
) -> ExecutionSnapshot {
    match start_shared_bytecode(Arc::clone(program), options).expect("execution should suspend") {
        ExecutionStep::Suspended(suspension) => suspension.snapshot,
        ExecutionStep::Completed(_) => panic!("benchmark fixture expected suspension"),
    }
}

fn consume_completed(step: ExecutionStep) {
    match step {
        ExecutionStep::Completed(value) => {
            criterion::black_box(value);
        }
        ExecutionStep::Suspended(_) => panic!("benchmark expected completed execution"),
    }
}

fn consume_suspended(step: ExecutionStep) {
    match step {
        ExecutionStep::Suspended(suspension) => {
            criterion::black_box(suspension.capability);
            criterion::black_box(suspension.args);
            criterion::black_box(suspension.snapshot);
        }
        ExecutionStep::Completed(_) => panic!("benchmark expected suspended execution"),
    }
}

fn hot_runtime_options() -> ExecutionOptions {
    ExecutionOptions {
        limits: RuntimeLimits {
            instruction_budget: 10_000_000,
            ..RuntimeLimits::default()
        },
        ..ExecutionOptions::default()
    }
}

fn global_lookup_runtime_options() -> ExecutionOptions {
    ExecutionOptions {
        inputs: IndexMap::from([(
            "config".to_string(),
            StructuredValue::Object(IndexMap::from([(
                "seed".to_string(),
                StructuredValue::Number(StructuredNumber::Finite(7.0)),
            )])),
        )]),
        capabilities: vec!["fetch_data".to_string()],
        limits: RuntimeLimits {
            instruction_budget: 10_000_000,
            ..RuntimeLimits::default()
        },
        ..ExecutionOptions::default()
    }
}

fn parse_and_lower_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("compile_pipeline");
    let small_source = small_compute_source();
    let compiled_small = compile(small_source).expect("small source should compile");
    let serialized_small = fixtures().serialized_small_compute_program.as_slice();

    group.bench_function("parse_and_validate_small_source", |b| {
        b.iter(|| {
            let program = compile(criterion::black_box(small_source))
                .expect("small source should compile during bench");
            criterion::black_box(program);
        });
    });
    group.bench_function("lower_bytecode_small_source", |b| {
        b.iter(|| {
            let bytecode = lower_to_bytecode(criterion::black_box(&compiled_small))
                .expect("small program should lower during bench");
            criterion::black_box(bytecode);
        });
    });
    group.bench_function("deserialize_and_validate_small_program", |b| {
        b.iter(|| {
            let program = load_program(criterion::black_box(serialized_small))
                .expect("serialized program should load");
            criterion::black_box(program);
        });
    });
    group.finish();
}

fn startup_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("startup");
    let fixtures = fixtures();
    let capability_names = vec![
        "fetch_data".to_string(),
        "persist".to_string(),
        "search".to_string(),
        "schedule".to_string(),
        "send_email".to_string(),
        "console.log".to_string(),
    ];
    let input_options = IndexMap::from([
        (
            "budget".to_string(),
            StructuredValue::Number(StructuredNumber::Finite(1250.0)),
        ),
        ("payload".to_string(), fixtures.boundary_payload.clone()),
    ]);

    group.bench_function("runtime_init_empty", |b| {
        b.iter_batched(
            ExecutionOptions::default,
            |options| {
                let step = start_validated_bytecode(
                    criterion::black_box(&fixtures.runtime_init_program),
                    options,
                )
                .expect("runtime init bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("runtime_init_with_capabilities", |b| {
        b.iter_batched(
            || ExecutionOptions {
                capabilities: capability_names.clone(),
                ..ExecutionOptions::default()
            },
            |options| {
                let step = start_validated_bytecode(
                    criterion::black_box(&fixtures.runtime_init_program),
                    options,
                )
                .expect("runtime init with capabilities should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("runtime_init_with_inputs", |b| {
        b.iter_batched(
            || ExecutionOptions {
                inputs: input_options.clone(),
                ..ExecutionOptions::default()
            },
            |options| {
                let step = start_validated_bytecode(
                    criterion::black_box(&fixtures.runtime_init_program),
                    options,
                )
                .expect("runtime init with inputs should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("start_validated_bytecode_small_compute", |b| {
        b.iter_batched(
            ExecutionOptions::default,
            |options| {
                let step = start_validated_bytecode(
                    criterion::black_box(&fixtures.small_compute_program),
                    options,
                )
                .expect("validated small compute should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("execute_shared_small_compute", |b| {
        b.iter_batched(
            ExecutionOptions::default,
            |options| {
                let step = start_shared_bytecode(
                    Arc::clone(criterion::black_box(&fixtures.small_compute_shared)),
                    options,
                )
                .expect("shared small compute should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

fn runtime_execution_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_execution");
    let fixtures = fixtures();

    group.bench_function("vm_hot_loop", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step = start_shared_bytecode(Arc::clone(&fixtures.vm_hot_loop_shared), options)
                    .expect("hot loop should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("local_load_store_hot", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.local_load_store_shared), options)
                        .expect("local load/store hot path should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("closure_access_hot", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.closure_access_shared), options)
                        .expect("closure access hot path should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("env_lookup_hot", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step = start_shared_bytecode(Arc::clone(&fixtures.env_lookup_shared), options)
                    .expect("env lookup hot path should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("global_lookup_hot", |b| {
        b.iter_batched(
            global_lookup_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.global_lookup_shared), options)
                        .expect("global lookup hot path should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("property_access_hot", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.property_access_shared), options)
                        .expect("property access hot path should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("builtin_method_hot", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.builtin_method_shared), options)
                        .expect("builtin method hot path should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("array_from_hot", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step = start_shared_bytecode(Arc::clone(&fixtures.array_from_shared), options)
                    .expect("Array.from hot path should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("object_from_entries_hot", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step = start_shared_bytecode(
                    Arc::clone(&fixtures.object_from_entries_shared),
                    options,
                )
                .expect("Object.fromEntries hot path should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("array_callback_hot", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.array_callback_shared), options)
                        .expect("array callback hot path should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("collection_callback_hot", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step = start_shared_bytecode(
                    Arc::clone(&fixtures.collection_callback_shared),
                    options,
                )
                .expect("collection callback hot path should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("promise_all_immediate_fanout", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step = start_shared_bytecode(
                    Arc::clone(&fixtures.promise_all_immediate_shared),
                    options,
                )
                .expect("Promise.all immediate fanout should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("promise_all_settled_immediate", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step = start_shared_bytecode(
                    Arc::clone(&fixtures.promise_all_settled_immediate_shared),
                    options,
                )
                .expect("Promise.allSettled immediate bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("promise_all_derived_ids_fanout", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step = start_shared_bytecode(
                    Arc::clone(&fixtures.promise_all_derived_ids_shared),
                    options,
                )
                .expect("Promise.all derived-ID fanout should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("promise_all_map_set_reduction", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step = start_shared_bytecode(
                    Arc::clone(&fixtures.promise_all_map_set_reduction_shared),
                    options,
                )
                .expect("Promise.all map/set reduction bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("map_set_hot", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step = start_shared_bytecode(Arc::clone(&fixtures.map_set_shared), options)
                    .expect("map/set hot path should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

fn keyed_collection_large_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("keyed_collection_large");
    let fixtures = fixtures();

    group.bench_function("map_get_large", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.map_get_large_shared), options)
                        .expect("large Map.get bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("map_ctor_large", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.map_ctor_large_shared), options)
                        .expect("large Map constructor bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("set_ctor_large", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.set_ctor_large_shared), options)
                        .expect("large Set constructor bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("map_set_large", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.map_set_large_shared), options)
                        .expect("large Map.set bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("map_has_large", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.map_has_large_shared), options)
                        .expect("large Map.has bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("set_add_large", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.set_add_large_shared), options)
                        .expect("large Set.add bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("set_has_large", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.set_has_large_shared), options)
                        .expect("large Set.has bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("set_delete_large", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.set_delete_large_shared), options)
                        .expect("large Set.delete bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("iterator_throughput_large", |b| {
        b.iter_batched(
            hot_runtime_options,
            |options| {
                let step = start_shared_bytecode(
                    Arc::clone(&fixtures.collection_iterator_large_shared),
                    options,
                )
                .expect("large iterator throughput bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

fn structured_boundary_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("structured_boundary");
    let fixtures = fixtures();

    group.bench_function("decode_nested_input", |b| {
        b.iter_batched(
            || ExecutionOptions {
                inputs: fixtures.boundary_inputs.clone(),
                ..ExecutionOptions::default()
            },
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.boundary_decode_shared), options)
                        .expect("boundary decode bench should execute");
                consume_completed(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.bench_function("encode_nested_suspend_args", |b| {
        b.iter_batched(
            || ExecutionOptions {
                capabilities: vec!["emit".to_string()],
                ..ExecutionOptions::default()
            },
            |options| {
                let step =
                    start_shared_bytecode(Arc::clone(&fixtures.boundary_encode_shared), options)
                        .expect("boundary encode bench should suspend");
                consume_suspended(step);
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

fn snapshot_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot");
    let fixtures = fixtures();

    group.bench_function("snapshot_dump_suspended", |b| {
        b.iter(|| {
            let bytes = dump_snapshot(criterion::black_box(&fixtures.suspended_snapshot))
                .expect("snapshot dump should succeed");
            criterion::black_box(bytes);
        });
    });
    group.bench_function("snapshot_load_suspended", |b| {
        b.iter(|| {
            let snapshot = load_snapshot(criterion::black_box(
                fixtures.suspended_snapshot_bytes.as_slice(),
            ))
            .expect("snapshot load should succeed");
            criterion::black_box(snapshot);
        });
    });
    group.finish();
}

fn small_compute_source() -> &'static str {
    r#"
    const values = [1, 2, 3, 4, 5, 6, 7, 8];
    let total = 0;
    for (let round = 0; round < 200; round += 1) {
      for (let index = 0; index < values.length; index += 1) {
        total += values[index] * (round + 1);
      }
    }
    total;
    "#
}

fn vm_hot_loop_source() -> &'static str {
    r#"
    let total = 0;
    for (let outer = 0; outer < 240; outer += 1) {
      for (let inner = 0; inner < 32; inner += 1) {
        total += (outer * inner) % 17;
      }
    }
    total;
    "#
}

fn local_load_store_source() -> &'static str {
    r#"
    let alpha = 1;
    let beta = 2;
    let gamma = 3;
    let delta = 4;
    let total = 0;
    for (let outer = 0; outer < 240; outer += 1) {
      for (let inner = 0; inner < 48; inner += 1) {
        alpha = alpha + 1;
        beta = beta + alpha + inner;
        gamma = gamma + beta - outer;
        delta = delta + gamma + alpha;
        total += alpha + beta + gamma + delta;
      }
    }
    total;
    "#
}

fn closure_access_source() -> &'static str {
    r#"
    function makeCounter(seed) {
      let alpha = 1;
      let beta = 2;
      let gamma = 3;
      return function(step) {
        alpha = alpha + 1;
        beta = beta + alpha + step;
        gamma = gamma + beta + seed;
        return alpha + beta + gamma + seed;
      };
    }
    const counter = makeCounter(5);
    let total = 0;
    for (let round = 0; round < 2400; round += 1) {
      total += counter(round % 11);
    }
    total;
    "#
}

fn env_lookup_source() -> &'static str {
    r#"
    const seed = 7;
    function makeStepper(base) {
      let offset = 3;
      return function(step) {
        let total = 0;
        for (let i = 0; i < 120; i += 1) {
          total += base + offset + seed + step + i;
          offset += 1;
        }
        return total;
      };
    }
    const stepper = makeStepper(4);
    let result = 0;
    for (let round = 0; round < 100; round += 1) {
      result += stepper(round);
    }
    result;
    "#
}

fn global_lookup_source() -> &'static str {
    r#"
    let total = 0;
    for (let round = 0; round < 2400; round += 1) {
      total += config.seed;
      total += Array.length;
      total += Math.PI > 3 ? 1 : 0;
      total += fetch_data.name.length;
    }
    total;
    "#
}

fn property_access_source() -> &'static str {
    r#"
    const record = { alpha: 1, beta: 2, gamma: 3, delta: 4 };
    const values = [1, 2, 3, 4, 5, 6, 7, 8];
    let total = 0;
    for (let round = 0; round < 1000; round += 1) {
      total += record.alpha + record.beta + record.gamma;
      record.delta = record.delta + values[round % values.length];
      total += record.delta;
    }
    total;
    "#
}

fn builtin_method_source() -> &'static str {
    r#"
    const values = [1, 2, 3, 4, 5, 6, 7, 8];
    const text = "mustardscript";
    let total = 0;
    for (let round = 0; round < 2400; round += 1) {
      const map = values.map;
      const slice = values.slice;
      const startsWith = text.startsWith;
      const toUpperCase = text.toUpperCase;
      if (map && slice && startsWith && toUpperCase) {
        total += 1;
      }
    }
    total;
    "#
}

fn array_from_source() -> &'static str {
    r#"
    const seed = [];
    for (let i = 0; i < 256; i += 1) {
      seed.push(i);
    }
    let total = 0;
    for (let round = 0; round < 128; round += 1) {
      const mapped = Array.from(seed, (value, index) => value + index + round);
      total += mapped[round % seed.length];
    }
    total;
    "#
}

fn object_from_entries_source() -> &'static str {
    r#"
    const entries = [];
    for (let i = 0; i < 256; i += 1) {
      entries.push(["k" + i, i]);
    }
    let total = 0;
    for (let round = 0; round < 128; round += 1) {
      const object = Object.fromEntries(entries);
      total += object["k" + (round % entries.length)];
    }
    total;
    "#
}

fn array_callback_source() -> &'static str {
    r#"
    const values = [1, 2, 3, 4, 5, 6, 7, 8];
    let total = 0;
    let bias = 3;
    for (let round = 0; round < 240; round += 1) {
      values.forEach((value, index) => {
        bias += 1;
        total += (value * bias) + index + round;
      });
    }
    total;
    "#
}

fn collection_callback_source() -> &'static str {
    r#"
    const map = new Map([
      ["alpha", 1],
      ["beta", 2],
      ["gamma", 3],
      ["delta", 4],
    ]);
    const set = new Set(["alpha", "beta", "gamma", "delta"]);
    let total = 0;
    for (let round = 0; round < 240; round += 1) {
      map.forEach((value, key) => {
        total += value + key.length + round;
      });
      set.forEach((value) => {
        total += value.length + round;
      });
    }
    total;
    "#
}

fn promise_all_immediate_source() -> &'static str {
    r#"
    async function main() {
      const seed = [];
      for (let i = 0; i < 128; i += 1) {
        seed.push(i);
      }
      let total = 0;
      for (let round = 0; round < 64; round += 1) {
        const values = await Promise.all(seed);
        total += values[round % seed.length];
      }
      return total;
    }
    main();
    "#
}

fn promise_all_settled_immediate_source() -> &'static str {
    r#"
    async function main() {
      const seed = [];
      for (let i = 0; i < 96; i += 1) {
        seed.push(i % 4 === 0 ? Promise.reject("boom:" + i) : i);
      }
      let total = 0;
      for (let round = 0; round < 48; round += 1) {
        const settled = await Promise.allSettled(seed);
        for (let index = 0; index < settled.length; index += 1) {
          if (settled[index].status === "fulfilled") {
            total += settled[index].value;
          }
        }
      }
      return total;
    }
    main();
    "#
}

fn promise_all_derived_ids_source() -> &'static str {
    r#"
    async function main() {
      const base = [];
      for (let i = 0; i < 96; i += 1) {
        base.push({
          accountId: "acct_" + (i % 24),
          entityId: "ent_" + (i % 18),
          score: (i % 11) + 1,
        });
      }
      let total = 0;
      for (let round = 0; round < 24; round += 1) {
        const firstStage = await Promise.all(base.map((record, index) => ({
          accountId: record.accountId,
          entityId: record.entityId,
          signalId: "sig_" + ((index + round) % 32),
          score: record.score + (round % 5),
        })));
        const accountIds = new Set();
        const entityIds = new Set();
        for (let index = 0; index < firstStage.length; index += 1) {
          accountIds.add(firstStage[index].accountId);
          entityIds.add(firstStage[index].entityId);
        }
        const derivedIds = Array.from(accountIds).concat(Array.from(entityIds));
        const secondStage = await Promise.all(derivedIds.map((id, index) => ({
          id,
          weight: (index % 7) + round + id.length,
        })));
        total += firstStage.length + secondStage.length + accountIds.size + entityIds.size;
      }
      return total;
    }
    main();
    "#
}

fn promise_all_map_set_reduction_source() -> &'static str {
    r#"
    async function main() {
      const seed = [];
      for (let i = 0; i < 128; i += 1) {
        seed.push(i);
      }
      let total = 0;
      for (let round = 0; round < 24; round += 1) {
        const records = await Promise.all(seed.map((value) => ({
          accountId: "acct_" + (value % 32),
          entityId: "ent_" + (value % 24),
          score: ((value + round) % 13) + 1,
          tag: value % 5 === 0 ? "priority" : "routine",
        })));
        const dedupedAccounts = new Set();
        const totalsByEntity = new Map();
        for (let index = 0; index < records.length; index += 1) {
          const record = records[index];
          dedupedAccounts.add(record.accountId);
          const current = totalsByEntity.get(record.entityId);
          if (current === undefined) {
            totalsByEntity.set(record.entityId, {
              score: record.score,
              priorityHits: record.tag === "priority" ? 1 : 0,
            });
          } else {
            current.score += record.score;
            if (record.tag === "priority") {
              current.priorityHits += 1;
            }
          }
        }
        total += dedupedAccounts.size + totalsByEntity.size;
        for (const [entityId, bucket] of totalsByEntity) {
          total += bucket.score + bucket.priorityHits + entityId.length;
        }
      }
      return total;
    }
    main();
    "#
}

fn map_set_source() -> &'static str {
    r#"
    const keys = [
      "k0", "k1", "k2", "k3", "k4", "k5", "k6", "k7",
      "k8", "k9", "k10", "k11", "k12", "k13", "k14", "k15",
      "k16", "k17", "k18", "k19", "k20", "k21", "k22", "k23",
      "k24", "k25", "k26", "k27", "k28", "k29", "k30", "k31",
      "k32", "k33", "k34", "k35", "k36", "k37", "k38", "k39",
      "k40", "k41", "k42", "k43", "k44", "k45", "k46", "k47",
      "k48", "k49", "k50", "k51", "k52", "k53", "k54", "k55",
      "k56", "k57", "k58", "k59", "k60", "k61", "k62", "k63"
    ];
    const map = new Map();
    const set = new Set();
    for (let i = 0; i < keys.length; i += 1) {
      const key = keys[i];
      map.set(key, i);
      set.add(key);
    }
    let total = 0;
    for (let round = 0; round < 400; round += 1) {
      const key = keys[round % keys.length];
      if (set.has(key)) {
        total += map.get(key);
      }
      map.set(key, total);
      if ((round % 9) === 0) {
        set.delete(key);
        set.add(key);
      }
    }
    total;
    "#
}

fn map_get_large_source() -> &'static str {
    r#"
    const size = 1024;
    const keys = [];
    for (let i = 0; i < size; i += 1) {
      keys.push("k" + i);
    }
    const map = new Map();
    for (let i = 0; i < keys.length; i += 1) {
      map.set(keys[i], i);
    }
    let total = 0;
    for (let round = 0; round < 8192; round += 1) {
      total += map.get(keys[round % size]);
    }
    total;
    "#
}

fn map_ctor_large_source() -> &'static str {
    r#"
    const size = 1024;
    const entries = [];
    for (let i = 0; i < size; i += 1) {
      entries.push(["k" + i, i]);
    }
    let total = 0;
    for (let round = 0; round < 256; round += 1) {
      const map = new Map(entries);
      total += map.get("k" + (round % size));
    }
    total;
    "#
}

fn set_ctor_large_source() -> &'static str {
    r#"
    const size = 1024;
    const values = [];
    for (let i = 0; i < size; i += 1) {
      values.push("k" + i);
    }
    let total = 0;
    for (let round = 0; round < 256; round += 1) {
      const set = new Set(values);
      if (set.has("k" + (round % size))) {
        total += 1;
      }
    }
    total;
    "#
}

fn map_set_large_source() -> &'static str {
    r#"
    const size = 1024;
    const keys = [];
    for (let i = 0; i < size; i += 1) {
      keys.push("k" + i);
    }
    const map = new Map();
    for (let i = 0; i < keys.length; i += 1) {
      map.set(keys[i], i);
    }
    let total = 0;
    for (let round = 0; round < 4096; round += 1) {
      const key = keys[round % size];
      map.set(key, round);
      total += round;
    }
    total + map.get(keys[0]);
    "#
}

fn map_has_large_source() -> &'static str {
    r#"
    const size = 1024;
    const keys = [];
    for (let i = 0; i < size; i += 1) {
      keys.push("k" + i);
    }
    const map = new Map();
    for (let i = 0; i < keys.length; i += 1) {
      map.set(keys[i], i);
    }
    let total = 0;
    for (let round = 0; round < 8192; round += 1) {
      if (map.has(keys[round % size])) {
        total += 1;
      }
    }
    total;
    "#
}

fn set_add_large_source() -> &'static str {
    r#"
    const size = 1024;
    const keys = [];
    const extra = [];
    for (let i = 0; i < size; i += 1) {
      keys.push("k" + i);
      extra.push("extra" + i);
    }
    const set = new Set(keys);
    let total = 0;
    for (let round = 0; round < extra.length; round += 1) {
      set.add(extra[round]);
      total += round;
    }
    total + (set.has(extra[extra.length - 1]) ? 1 : 0);
    "#
}

fn set_has_large_source() -> &'static str {
    r#"
    const size = 1024;
    const keys = [];
    for (let i = 0; i < size; i += 1) {
      keys.push("k" + i);
    }
    const set = new Set(keys);
    let total = 0;
    for (let round = 0; round < 8192; round += 1) {
      if (set.has(keys[round % size])) {
        total += 1;
      }
    }
    total;
    "#
}

fn set_delete_large_source() -> &'static str {
    r#"
    const size = 2048;
    const keys = [];
    for (let i = 0; i < size; i += 1) {
      keys.push("k" + i);
    }
    const set = new Set(keys);
    let total = 0;
    for (let round = 0; round < 1024; round += 1) {
      if (set.delete(keys[round])) {
        total += 1;
      }
    }
    total;
    "#
}

fn collection_iterator_large_source() -> &'static str {
    r#"
    const size = 512;
    const keys = [];
    for (let i = 0; i < size; i += 1) {
      keys.push("k" + i);
    }
    const map = new Map();
    const set = new Set();
    for (let i = 0; i < keys.length; i += 1) {
      map.set(keys[i], i);
      set.add(keys[i]);
    }
    let total = 0;
    for (let round = 0; round < 64; round += 1) {
      for (const [key, value] of map) {
        total += value + key.length + round;
      }
      for (const value of set) {
        total += value.length + round;
      }
    }
    total;
    "#
}

fn boundary_decode_source() -> &'static str {
    r#"
    let total = 0;
    for (let index = 0; index < payload.items.length; index += 1) {
      const item = payload.items[index];
      total += item.metrics.score;
      if (item.flags.active) {
        total += payload.meta.weights[index % payload.meta.weights.length];
      }
    }
    total + payload.meta.region.length;
    "#
}

fn boundary_encode_source() -> &'static str {
    r#"
    emit({
      meta: {
        region: "us-west-2",
        owner: "runtime-core-bench",
        weights: [3, 5, 8, 13, 21, 34],
      },
      items: [
        { id: "item-0", metrics: { score: 11, cost: 101 }, flags: { active: true, stale: false } },
        { id: "item-1", metrics: { score: 17, cost: 202 }, flags: { active: false, stale: true } },
        { id: "item-2", metrics: { score: 23, cost: 303 }, flags: { active: true, stale: false } },
        { id: "item-3", metrics: { score: 29, cost: 404 }, flags: { active: true, stale: true } },
        { id: "item-4", metrics: { score: 31, cost: 505 }, flags: { active: false, stale: false } },
        { id: "item-5", metrics: { score: 37, cost: 606 }, flags: { active: true, stale: false } }
      ],
    });
    "#
}

fn nested_boundary_payload() -> StructuredValue {
    let items = (0..6)
        .map(|index| {
            StructuredValue::Object(IndexMap::from([
                (
                    "id".to_string(),
                    StructuredValue::String(format!("item-{index}")),
                ),
                (
                    "metrics".to_string(),
                    StructuredValue::Object(IndexMap::from([
                        (
                            "score".to_string(),
                            StructuredValue::Number(StructuredNumber::Finite(
                                11.0 + (index as f64 * 6.0),
                            )),
                        ),
                        (
                            "cost".to_string(),
                            StructuredValue::Number(StructuredNumber::Finite(
                                101.0 + (index as f64 * 101.0),
                            )),
                        ),
                    ])),
                ),
                (
                    "flags".to_string(),
                    StructuredValue::Object(IndexMap::from([
                        ("active".to_string(), StructuredValue::Bool(index % 2 == 0)),
                        ("stale".to_string(), StructuredValue::Bool(index % 3 == 1)),
                    ])),
                ),
            ]))
        })
        .collect();

    StructuredValue::Object(IndexMap::from([
        (
            "meta".to_string(),
            StructuredValue::Object(IndexMap::from([
                (
                    "region".to_string(),
                    StructuredValue::String("us-west-2".to_string()),
                ),
                (
                    "owner".to_string(),
                    StructuredValue::String("runtime-core-bench".to_string()),
                ),
                (
                    "weights".to_string(),
                    StructuredValue::Array(vec![
                        StructuredValue::Number(StructuredNumber::Finite(3.0)),
                        StructuredValue::Number(StructuredNumber::Finite(5.0)),
                        StructuredValue::Number(StructuredNumber::Finite(8.0)),
                        StructuredValue::Number(StructuredNumber::Finite(13.0)),
                        StructuredValue::Number(StructuredNumber::Finite(21.0)),
                        StructuredValue::Number(StructuredNumber::Finite(34.0)),
                    ]),
                ),
            ])),
        ),
        ("items".to_string(), StructuredValue::Array(items)),
    ]))
}

criterion_group!(
    name = runtime_core;
    config = bench_config();
    targets =
        parse_and_lower_benches,
        startup_benches,
        runtime_execution_benches,
        keyed_collection_large_benches,
        structured_boundary_benches,
        snapshot_benches
);
criterion_main!(runtime_core);
