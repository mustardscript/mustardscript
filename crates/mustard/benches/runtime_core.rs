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
    env_lookup_shared: Arc<BytecodeProgram>,
    property_access_shared: Arc<BytecodeProgram>,
    map_set_shared: Arc<BytecodeProgram>,
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
        let env_lookup_shared = Arc::new(compile_to_bytecode(env_lookup_source()));
        let property_access_shared = Arc::new(compile_to_bytecode(property_access_source()));
        let map_set_shared = Arc::new(compile_to_bytecode(map_set_source()));
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
            env_lookup_shared,
            property_access_shared,
            map_set_shared,
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
        structured_boundary_benches,
        snapshot_benches
);
criterion_main!(runtime_core);
