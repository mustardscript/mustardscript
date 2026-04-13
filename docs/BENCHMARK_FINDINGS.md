# Benchmark Findings

This document summarizes the latest local benchmark run from
`benchmarks/workloads.ts`, comparing:

- `mustard` addon mode
- `mustard` sidecar mode
- a V8 isolate baseline via `isolated-vm`

Reference report:

- `benchmarks/results/2026-04-13T06-59-51-780Z-workloads.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA: `a222015`
- Fixture version: `4`

## Headline Results

### 1. V8 isolates win decisively on raw execution throughput

For cold start, warm execution, code-mode search, and the synthetic
programmatic tool-calling workflow, the `isolated-vm` baseline is much faster
than either `mustard` mode.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Cold start, small script | 16.27 ms | 27.09 ms | 0.53 ms |
| Warm run, small script | 16.25 ms | 16.51 ms | 0.21 ms |
| Cold start, code-mode search | 37.68 ms | 41.92 ms | 0.64 ms |
| Warm run, code-mode search | 37.71 ms | 39.89 ms | 0.21 ms |
| Programmatic tool workflow | 42.30 ms | 47.36 ms | 0.35 ms |

The practical read is that `mustard` is not competitive with a V8 isolate on
pure execution speed for these local fixtures.

### 2. Addon mode is consistently faster than sidecar mode on compute and host-call-heavy paths

The sidecar transport adds measurable overhead relative to in-process addon
execution.

Representative medians:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 16.27 ms | 27.09 ms | 1.67x |
| Programmatic tool workflow | 42.30 ms | 47.36 ms | 1.12x |
| Host fanout, 10 calls | 0.74 ms | 1.07 ms | 1.45x |
| Host fanout, 100 calls | 6.61 ms | 9.38 ms | 1.42x |

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
| Suspend/resume, 1 boundary | 0.23 ms | 0.14 ms | 0.93 ms |
| Suspend/resume, 5 boundaries | 0.95 ms | 0.49 ms | 2.75 ms |
| Suspend/resume, 20 boundaries | 3.65 ms | 1.80 ms | 9.88 ms |

On this workload shape, `mustard` addon is about 2.5x to 3.6x faster than the
isolate baseline, and sidecar is faster still.

## Host Call Findings

For very small host-call counts, addon and isolate are in the same order of
magnitude. As call counts increase, the isolate baseline is much faster in this
harness.

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Host fanout, 1 call | 0.10 ms | 0.16 ms | 0.15 ms |
| Host fanout, 10 calls | 0.74 ms | 1.07 ms | 0.15 ms |
| Host fanout, 50 calls | 3.44 ms | 4.92 ms | 0.19 ms |
| Host fanout, 100 calls | 6.61 ms | 9.38 ms | 0.24 ms |

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
| Limit failure then recover | 16.12 ms | 16.56 ms | 2.43 ms |
| Host failure then recover | 16.16 ms | 16.47 ms | 0.68 ms |

The isolate baseline recovers much faster in this harness. For addon and
sidecar, failure recovery is close to the cost of a normal rerun.

## Retained Memory Findings

The benchmark captures post-GC deltas after 20 workflow runs.

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +1,408 B | +425,984 B |
| Sidecar | +17,784 B | +360,448 B |
| V8 isolate | -3,608 B | -9,781,248 B |

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
| `runtime_init_only` | `0.09 ms` |
| `execution_only_small` | `17.63 ms` |
| `suspend_only` | `0.07 ms` |
| `snapshot_dump_only` | `0.00 ms` |
| `apply_snapshot_policy_only` | `0.02 ms` |
| `snapshot_load_only` | `0.03 ms` |
| `Progress.load_only` | `0.12 ms` |

The practical read is that the addon is not currently dominated by snapshot
policy or `Progress.load(...)` overhead on this fixture. The large fixed cost is
still in core execution itself, which supports prioritizing Milestone 1 runtime
start/run work before deeper snapshot or bridge surgery.

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
