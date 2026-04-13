# Benchmark Findings

This document summarizes the latest local benchmark run from
`benchmarks/workloads.ts`, comparing:

- `mustard` addon mode
- `mustard` sidecar mode
- a V8 isolate baseline via `isolated-vm`

Reference report:

- `benchmarks/results/2026-04-13T09-45-36-897Z-workloads.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA: `2e754a3`
- Fixture version: `4`

## Headline Results

### 1. V8 isolates win decisively on raw execution throughput

For cold start, warm execution, code-mode search, and the synthetic
programmatic tool-calling workflow, the `isolated-vm` baseline is much faster
than either `mustard` mode.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Cold start, small script | 9.85 ms | 21.74 ms | 0.49 ms |
| Warm run, small script | 9.76 ms | 10.28 ms | 0.17 ms |
| Cold start, code-mode search | 33.62 ms | 37.02 ms | 0.63 ms |
| Warm run, code-mode search | 34.99 ms | 35.60 ms | 0.20 ms |
| Programmatic tool workflow | 36.02 ms | 39.27 ms | 0.33 ms |

The practical read is that `mustard` is not competitive with a V8 isolate on
pure execution speed for these local fixtures.

### 2. Addon mode is consistently faster than sidecar mode on compute and host-call-heavy paths

The sidecar transport adds measurable overhead relative to in-process addon
execution.

Representative medians:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 9.85 ms | 21.74 ms | 2.21x |
| Programmatic tool workflow | 36.02 ms | 39.27 ms | 1.09x |
| Host fanout, 10 calls | 0.58 ms | 0.82 ms | 1.41x |
| Host fanout, 100 calls | 5.01 ms | 7.29 ms | 1.46x |

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
| Suspend/resume, 1 boundary | 0.18 ms | 0.11 ms | 0.92 ms |
| Suspend/resume, 5 boundaries | 0.72 ms | 0.38 ms | 2.75 ms |
| Suspend/resume, 20 boundaries | 2.73 ms | 1.38 ms | 9.82 ms |

On this workload shape, `mustard` addon is about 3.4x to 4.8x faster than the
isolate baseline, and sidecar is faster still.

## Host Call Findings

For very small host-call counts, addon and isolate are in the same order of
magnitude. As call counts increase, the isolate baseline is much faster in this
harness.

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Host fanout, 1 call | 0.09 ms | 0.12 ms | 0.15 ms |
| Host fanout, 10 calls | 0.58 ms | 0.82 ms | 0.15 ms |
| Host fanout, 50 calls | 2.63 ms | 3.82 ms | 0.19 ms |
| Host fanout, 100 calls | 5.01 ms | 7.29 ms | 0.24 ms |

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
| Limit failure then recover | 9.80 ms | 10.06 ms | 2.47 ms |
| Host failure then recover | 9.86 ms | 10.27 ms | 0.68 ms |

The isolate baseline recovers much faster in this harness. For addon and
sidecar, failure recovery is close to the cost of a normal rerun.

## Retained Memory Findings

The benchmark captures post-GC deltas after 20 workflow runs.

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +39,496 B | +344,064 B |
| Sidecar | +26,024 B | +327,680 B |
| V8 isolate | -3,472 B | -9,797,632 B |

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
| `execution_only_small` | `13.33 ms` |
| `suspend_only` | `0.06 ms` |
| `snapshot_dump_only` | `0.00 ms` |
| `apply_snapshot_policy_only` | `0.02 ms` |
| `snapshot_load_only` | `0.02 ms` |
| `Progress.load_only` | `0.10 ms` |

The practical read is still that the addon is not dominated by snapshot policy
or `Progress.load(...)` overhead on this fixture. The large fixed cost is still
core execution itself, but the latest wrapper change did remove the extra
native `inspectSnapshot(...)` hop from `Progress.load(...)` for current dumps by
verifying an authenticated suspended manifest in JS and only falling back to
native inspection for legacy dumps.

The clearest direct signal is in the Rust-core `runtime_core` bench: relative
to the previous Criterion baseline, `snapshot_dump_suspended` improved by about
`58%`, `encode_nested_suspend_args` improved by about `3%`, and `vm_hot_loop`
improved by about `2%`, while `snapshot_load_suspended` stayed effectively
flat.

Relative to `2026-04-13T09-28-45-001Z`, the release workload suite stayed
mostly flat on the large throughput workloads, but the targeted suspend/load
path moved in the right direction. Addon `Progress.load_only` improved from
`0.123 ms` to `0.097 ms` (`-21.1%`) while raw native `snapshot_load_only`
stayed flat at about `0.02 ms`, which is the expected shape for a JS-wrapper
fast path. Addon `suspend_resume_20` improved from `3.06 ms` to `2.73 ms`
(`-10.7%`), while `programmatic_tool_workflow` stayed effectively flat
(`36.00 ms -> 36.02 ms`, `+0.1%`) and `host_fanout_100` regressed slightly
(`4.90 ms -> 5.01 ms`, `+2.2%`).

The new artifact now also records `addon.suspendState`, which gives the first
checked-in size signal for suspend-heavy fixtures. In the current benchmark
shape, the serialized program is `457 B` and the dumped snapshot is `3,195 B`
for each `suspend_resume_*` fixture, which confirms that stable program bytes
are already a meaningful fraction of the snapshot payload. Holding `20` live
suspended `Progress` objects retains about `22 KB` of Node heap for the
`suspend_resume_5` and `suspend_resume_20` fixtures, while the `1`-boundary
heap sample is still slightly negative after GC and should be treated as
allocator noise at this scale.

The release smoke suite remained within budget versus `2026-04-13T09-29-05-358Z`.
Startup and compute medians regressed slightly (`0.119 ms -> 0.130 ms`,
`+8.8%`; `3.11 ms -> 3.15 ms`, `+1.3%`), the host-call median ratio improved
slightly (`3.73x -> 3.66x`, `-2.0%`), and the snapshot median ratio improved
materially (`1.49x -> 1.28x`, `-14.3%`), which matches the targeted
suspend/load-path optimization.

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
