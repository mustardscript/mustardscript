# Benchmark Findings

This document summarizes the latest local benchmark run from
`benchmarks/workloads.ts`, comparing:

- `mustard` addon mode
- `mustard` sidecar mode
- a V8 isolate baseline via `isolated-vm`

Reference report:

- `benchmarks/results/2026-04-13T09-59-36-458Z-workloads.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA: `bd4d52b`
- Fixture version: `4`

## Headline Results

### 1. V8 isolates win decisively on raw execution throughput

For cold start, warm execution, code-mode search, and the synthetic
programmatic tool-calling workflow, the `isolated-vm` baseline is much faster
than either `mustard` mode.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Cold start, small script | 9.87 ms | 21.83 ms | 0.51 ms |
| Warm run, small script | 9.94 ms | 10.18 ms | 0.18 ms |
| Cold start, code-mode search | 34.62 ms | 39.74 ms | 0.62 ms |
| Warm run, code-mode search | 35.11 ms | 34.78 ms | 0.21 ms |
| Programmatic tool workflow | 36.23 ms | 39.42 ms | 0.34 ms |

The practical read is that `mustard` is not competitive with a V8 isolate on
pure execution speed for these local fixtures.

### 2. Addon mode is consistently faster than sidecar mode on compute and host-call-heavy paths

The sidecar transport adds measurable overhead relative to in-process addon
execution.

Representative medians:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 9.87 ms | 21.83 ms | 2.21x |
| Programmatic tool workflow | 36.23 ms | 39.42 ms | 1.09x |
| Host fanout, 10 calls | 0.60 ms | 0.81 ms | 1.35x |
| Host fanout, 100 calls | 5.02 ms | 7.32 ms | 1.46x |

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
| Suspend/resume, 1 boundary | 0.19 ms | 0.11 ms | 0.92 ms |
| Suspend/resume, 5 boundaries | 0.70 ms | 0.37 ms | 2.75 ms |
| Suspend/resume, 20 boundaries | 2.79 ms | 1.39 ms | 9.90 ms |

On this workload shape, `mustard` addon is about 3.4x to 4.8x faster than the
isolate baseline, and sidecar is faster still.

## Host Call Findings

For very small host-call counts, addon and isolate are in the same order of
magnitude. As call counts increase, the isolate baseline is much faster in this
harness.

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Host fanout, 1 call | 0.07 ms | 0.13 ms | 0.15 ms |
| Host fanout, 10 calls | 0.60 ms | 0.81 ms | 0.15 ms |
| Host fanout, 50 calls | 2.66 ms | 3.83 ms | 0.19 ms |
| Host fanout, 100 calls | 5.02 ms | 7.32 ms | 0.24 ms |

The read is:

- addon stays better than sidecar
- addon now beats the isolate baseline at one call in this harness
- isolate pulls far ahead once the benchmark becomes many synchronous host
  crossings

## Failure Cleanup Findings

The benchmark includes two failure-and-recovery cases:

- runtime-limit failure followed by a known-good run
- host-failure path followed by a known-good run

Median recovery timings:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Limit failure then recover | 10.13 ms | 10.24 ms | 2.52 ms |
| Host failure then recover | 10.31 ms | 10.22 ms | 0.69 ms |

The isolate baseline recovers much faster in this harness. For addon and
sidecar, failure recovery is close to the cost of a normal rerun.

## Retained Memory Findings

The benchmark captures post-GC deltas after 20 workflow runs.

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +33,776 B | +425,984 B |
| Sidecar | +25,832 B | +344,064 B |
| V8 isolate | -3,104 B | -9,912,320 B |

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
| `execution_only_small` | `12.66 ms` |
| `suspend_only` | `0.05 ms` |
| `snapshot_dump_only` | `0.00 ms` |
| `apply_snapshot_policy_only` | `0.01 ms` |
| `snapshot_load_only` | `0.02 ms` |
| `Progress.load_only` | `0.09 ms` |

The practical read is still that the addon is not dominated by snapshot policy
or `Progress.load(...)` overhead on this fixture. The large fixed cost is still
core execution itself, but the latest wrapper changes have now removed the
native `inspectSnapshot(...)` hop from `Progress.load(...)` for current dumps
and collapsed single-use enforcement onto one shared filesystem claim path
instead of the older JS-plus-native double registry.

The clearest direct signal is in the Rust-core `runtime_core` bench: relative
to the previous Criterion baseline, `snapshot_dump_suspended` improved by about
`58%`, `encode_nested_suspend_args` improved by about `3%`, and `vm_hot_loop`
improved by about `2%`, while `snapshot_load_suspended` stayed effectively
flat.

Relative to `2026-04-13T09-45-36-897Z`, the second Milestone 2 cleanup chunk
produced another small but targeted win on the public load path. Addon
`Progress.load_only` improved from `0.097 ms` to `0.092 ms` (`-5.9%`) and
`apply_snapshot_policy_only` improved from `0.015 ms` to `0.013 ms`
(`-13.7%`), which matches the removal of the extra native single-use registry
checks. Raw native `snapshot_load_only` stayed flat at about `0.02 ms`.
Broader throughput stayed roughly in family: `programmatic_tool_workflow`
regressed slightly (`36.02 ms -> 36.23 ms`, `+0.6%`), `host_fanout_100` stayed
flat (`5.01 ms -> 5.02 ms`, `+0.2%`), and `suspend_resume_20` regressed
slightly (`2.73 ms -> 2.79 ms`, `+2.0%`).

The new artifact now also records `addon.suspendState`, which gives the first
checked-in size signal for suspend-heavy fixtures. In the current benchmark
shape, the serialized program is `457 B` and the dumped snapshot is `3,195 B`
for each `suspend_resume_*` fixture, which confirms that stable program bytes
are already a meaningful fraction of the snapshot payload. Holding `20` live
suspended `Progress` objects retains about `22 KB` of Node heap for the
`suspend_resume_5` and `suspend_resume_20` fixtures, while the `1`-boundary
heap sample is still slightly negative after GC and should be treated as
allocator noise at this scale.

The release smoke suite remained within budget versus `2026-04-13T09-45-59-333Z`.
Startup improved (`0.130 ms -> 0.119 ms`, `-8.7%`), compute regressed slightly
(`3.15 ms -> 3.21 ms`, `+2.1%`), the host-call median ratio regressed slightly
(`3.66x -> 3.75x`, `+2.4%`), and the snapshot median ratio improved again
(`1.28x -> 1.22x`, `-4.8%`), which is directionally consistent with the
lighter same-process resume/load bookkeeping.

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
