# Benchmark Findings

This document summarizes the latest local benchmark run from
`benchmarks/workloads.ts`, comparing:

- `mustard` addon mode
- `mustard` sidecar mode
- a V8 isolate baseline via `isolated-vm`

Reference report:

- `benchmarks/results/2026-04-13T04-35-04-948Z-workloads.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Fixture version: `3`

## Headline Results

### 1. V8 isolates win decisively on raw execution throughput

For cold start, warm execution, code-mode search, and the synthetic
programmatic tool-calling workflow, the `isolated-vm` baseline is much faster
than either `mustard` mode.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Cold start, small script | 17.81 ms | 24.15 ms | 0.49 ms |
| Warm run, small script | 16.89 ms | 16.49 ms | 0.17 ms |
| Cold start, code-mode search | 39.00 ms | 43.18 ms | 0.62 ms |
| Warm run, code-mode search | 38.65 ms | 39.22 ms | 0.20 ms |
| Programmatic tool workflow | 42.47 ms | 47.76 ms | 0.34 ms |

The practical read is that `mustard` is not competitive with a V8 isolate on
pure execution speed for these local fixtures.

### 2. Addon mode is consistently faster than sidecar mode on compute and host-call-heavy paths

The sidecar transport adds measurable overhead relative to in-process addon
execution.

Representative medians:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 17.81 ms | 24.15 ms | 1.36x |
| Programmatic tool workflow | 42.47 ms | 47.76 ms | 1.12x |
| Host fanout, 10 calls | 0.70 ms | 1.01 ms | 1.44x |
| Host fanout, 100 calls | 6.67 ms | 9.44 ms | 1.44x |

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
| Suspend/resume, 1 boundary | 0.24 ms | 0.17 ms | 0.92 ms |
| Suspend/resume, 5 boundaries | 1.02 ms | 0.51 ms | 2.78 ms |
| Suspend/resume, 20 boundaries | 3.64 ms | 1.90 ms | 9.94 ms |

On this workload shape, `mustard` addon is about 2.5x to 3.6x faster than the
isolate baseline, and sidecar is faster still.

## Host Call Findings

For very small host-call counts, addon and isolate are in the same order of
magnitude. As call counts increase, the isolate baseline is much faster in this
harness.

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Host fanout, 1 call | 0.11 ms | 0.17 ms | 0.14 ms |
| Host fanout, 10 calls | 0.70 ms | 1.01 ms | 0.17 ms |
| Host fanout, 50 calls | 3.49 ms | 4.93 ms | 0.19 ms |
| Host fanout, 100 calls | 6.67 ms | 9.44 ms | 0.24 ms |

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
| Limit failure then recover | 16.76 ms | 16.85 ms | 2.45 ms |
| Host failure then recover | 16.71 ms | 16.71 ms | 0.67 ms |

The isolate baseline recovers much faster in this harness. For addon and
sidecar, failure recovery is close to the cost of a normal rerun.

## Retained Memory Findings

The benchmark captures post-GC deltas after 20 workflow runs.

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +7,464 B | +360,448 B |
| Sidecar | +16,256 B | +540,672 B |
| V8 isolate | -5,944 B | -10,223,616 B |

These numbers should be treated cautiously:

- they are retained-memory deltas, not peak-memory measurements
- small RSS changes are noisy
- the isolate RSS delta can go negative because allocator reuse and OS page
  reclamation are happening during the sampling window
- sidecar RSS includes both the parent Node process and the live child process

The memory section is useful as a rough regression signal, not as a precise
capacity-planning number.

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
