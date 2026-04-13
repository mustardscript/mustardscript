# Benchmark Findings

This document summarizes the latest local benchmark run from
`benchmarks/workloads.ts`, comparing:

- `mustard` addon mode
- `mustard` sidecar mode
- a V8 isolate baseline via `isolated-vm`

Reference report:

- `benchmarks/results/2026-04-13T09-02-17-086Z-workloads.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA: `4814c3e`
- Fixture version: `4`

## Headline Results

### 1. V8 isolates win decisively on raw execution throughput

For cold start, warm execution, code-mode search, and the synthetic
programmatic tool-calling workflow, the `isolated-vm` baseline is much faster
than either `mustard` mode.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Cold start, small script | 10.35 ms | 22.26 ms | 0.51 ms |
| Warm run, small script | 10.41 ms | 10.22 ms | 0.17 ms |
| Cold start, code-mode search | 36.51 ms | 38.90 ms | 0.63 ms |
| Warm run, code-mode search | 35.34 ms | 35.60 ms | 0.20 ms |
| Programmatic tool workflow | 36.95 ms | 42.84 ms | 0.33 ms |

The practical read is that `mustard` is not competitive with a V8 isolate on
pure execution speed for these local fixtures.

### 2. Addon mode is consistently faster than sidecar mode on compute and host-call-heavy paths

The sidecar transport adds measurable overhead relative to in-process addon
execution.

Representative medians:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 10.35 ms | 22.26 ms | 2.15x |
| Programmatic tool workflow | 36.95 ms | 42.84 ms | 1.16x |
| Host fanout, 10 calls | 0.58 ms | 1.09 ms | 1.88x |
| Host fanout, 100 calls | 5.23 ms | 9.17 ms | 1.75x |

If low latency matters and the deployment model allows it, addon mode remains
the better `mustard` path.

### 3. `mustard` wins on resumable execution

The benchmark’s suspend/resume workload favors `mustard` because `mustard`
supports explicit suspended execution and snapshot reload, while the isolate
baseline in this harness must reconstruct progress by re-entering a fresh
isolate with host-carried state.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Suspend/resume, 1 boundary | 0.21 ms | 0.11 ms | 0.93 ms |
| Suspend/resume, 5 boundaries | 0.80 ms | 0.44 ms | 2.77 ms |
| Suspend/resume, 20 boundaries | 3.10 ms | 1.70 ms | 9.96 ms |

On this workload shape, `mustard` addon is about 3.2x to 4.5x faster than the
isolate baseline, and sidecar is faster still.

## Host Call Findings

For very small host-call counts, addon and isolate are in the same order of
magnitude. As call counts increase, the isolate baseline is much faster in this
harness.

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Host fanout, 1 call | 0.08 ms | 0.13 ms | 0.15 ms |
| Host fanout, 10 calls | 0.58 ms | 1.09 ms | 0.15 ms |
| Host fanout, 50 calls | 2.82 ms | 4.79 ms | 0.19 ms |
| Host fanout, 100 calls | 5.23 ms | 9.17 ms | 0.25 ms |

The read is:

- addon stays better than sidecar
- isolate is near parity at one call
- isolate pulls far ahead once the benchmark becomes many synchronous host
  crossings

## Failure Cleanup Findings

The benchmark includes two failure-and-recovery cases:

- runtime-limit failure followed by a known-good run
- host-failure path followed by a known-good run

Median recovery timings:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Limit failure then recover | 10.37 ms | 10.26 ms | 2.47 ms |
| Host failure then recover | 10.58 ms | 10.55 ms | 0.69 ms |

The isolate baseline recovers much faster in this harness. For addon and
sidecar, failure recovery is close to the cost of a normal rerun.

## Retained Memory Findings

The benchmark captures post-GC deltas after 20 workflow runs.

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +4,432 B | +360,448 B |
| Sidecar | +18,880 B | +442,368 B |
| V8 isolate | -3,608 B | -10,289,152 B |

These numbers should be treated cautiously:

- they are retained-memory deltas, not peak-memory measurements
- small RSS changes are noisy
- the isolate RSS delta can go negative because allocator reuse and OS page
  reclamation are happening during the sampling window
- sidecar RSS includes both the parent Node process and the live child process

The memory section is useful as a rough regression signal, not as a precise
capacity-planning number.

## Addon Phase Split Findings

The new addon-only phase metrics make the current bottleneck distribution much
clearer:

| Phase | Median |
| --- | ---: |
| `runtime_init_only` | `0.04 ms` |
| `execution_only_small` | `13.45 ms` |
| `suspend_only` | `0.06 ms` |
| `snapshot_dump_only` | `0.00 ms` |
| `apply_snapshot_policy_only` | `0.01 ms` |
| `snapshot_load_only` | `0.02 ms` |
| `Progress.load_only` | `0.13 ms` |

The practical read is still that the addon is not dominated by snapshot policy
or `Progress.load(...)` overhead on this fixture. The large fixed cost is still
core execution itself, but the new reusable `ExecutionContext` handle did
remove a measurable amount of repeated JS-side policy setup from the addon
start path.

The latest Rust-core `runtime_core` bench is mostly unchanged because
`ExecutionContext` is a Node-wrapper optimization rather than a Rust hot-path
change. The newest run stayed flat-to-mixed: `runtime_init_empty` improved
(`~ -3%`) and `encode_nested_suspend_args` improved (`~ -2.3%`), while
`deserialize_and_validate_small_program` and `snapshot_load_suspended` both
regressed modestly (`~ +7%` and `~ +2.7%`).

The more relevant signal for this cut is the addon workload suite itself, which
now reuses a real `ExecutionContext` for repeated benchmark loops. Relative to
`2026-04-13T08-36-39-534Z`, the clearest wins were `runtime_init_only`
(`0.07 ms -> 0.04 ms`, about `-43%`), `execution_only_small`
(`13.60 ms -> 13.45 ms`, about `-1%`), `programmatic_tool_workflow`
(`37.98 ms -> 36.95 ms`, about `-2.7%`), and `suspend_resume_20`
(`3.19 ms -> 3.10 ms`, about `-2.9%`). The rest of the suite stayed mixed:
`warm_run_small` and `host_fanout_100` regressed slightly (`+1.4%` and
`+1.5%`), which suggests repeated policy setup was only one contributor to the
remaining addon overhead.

## Conclusions

For the measured local workloads:

1. Choose a V8 isolate when raw execution speed is the primary goal and
   resumable continuation state is not required.
2. Choose `mustard` addon mode when you want `mustard` semantics with the lowest
   latency available inside the current process.
3. Choose `mustard` sidecar mode when you want stronger process isolation and
   can afford the added transport overhead.
4. Choose `mustard` over the isolate baseline when suspend/resume behavior is a
   first-class requirement rather than an implementation detail.

## Important Caveats

- This is a single-machine local benchmark, not a published cross-machine
  study.
- Sample counts are intentionally small to keep the benchmark runnable during
  development.
- The isolate suspend/resume path is only a best-effort comparison because this
  harness does not provide equivalent continuation snapshotting for V8 isolates.
- The programmatic tool workflow is synthetic. It is more realistic than the
  simple fanout microbenchmark, but it is still not a production trace.
