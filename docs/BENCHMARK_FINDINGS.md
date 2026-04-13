# Benchmark Findings

This document summarizes the latest local benchmark run from
`benchmarks/workloads.ts`, comparing:

- `mustard` addon mode
- `mustard` sidecar mode
- a V8 isolate baseline via `isolated-vm`

Reference report:

- `benchmarks/results/2026-04-13T09-28-45-001Z-workloads.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA: `aa543d5`
- Fixture version: `4`

## Headline Results

### 1. V8 isolates win decisively on raw execution throughput

For cold start, warm execution, code-mode search, and the synthetic
programmatic tool-calling workflow, the `isolated-vm` baseline is much faster
than either `mustard` mode.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Cold start, small script | 9.73 ms | 22.41 ms | 0.51 ms |
| Warm run, small script | 9.75 ms | 10.24 ms | 0.18 ms |
| Cold start, code-mode search | 33.91 ms | 38.08 ms | 0.62 ms |
| Warm run, code-mode search | 35.30 ms | 35.51 ms | 0.21 ms |
| Programmatic tool workflow | 36.00 ms | 39.38 ms | 0.34 ms |

The practical read is that `mustard` is not competitive with a V8 isolate on
pure execution speed for these local fixtures.

### 2. Addon mode is consistently faster than sidecar mode on compute and host-call-heavy paths

The sidecar transport adds measurable overhead relative to in-process addon
execution.

Representative medians:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 9.73 ms | 22.41 ms | 2.30x |
| Programmatic tool workflow | 36.00 ms | 39.38 ms | 1.09x |
| Host fanout, 10 calls | 0.57 ms | 0.83 ms | 1.46x |
| Host fanout, 100 calls | 4.90 ms | 7.36 ms | 1.50x |

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
| Suspend/resume, 1 boundary | 0.21 ms | 0.10 ms | 0.94 ms |
| Suspend/resume, 5 boundaries | 0.83 ms | 0.37 ms | 2.80 ms |
| Suspend/resume, 20 boundaries | 3.06 ms | 1.39 ms | 10.11 ms |

On this workload shape, `mustard` addon is about 3.4x to 4.8x faster than the
isolate baseline, and sidecar is faster still.

## Host Call Findings

For very small host-call counts, addon and isolate are in the same order of
magnitude. As call counts increase, the isolate baseline is much faster in this
harness.

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Host fanout, 1 call | 0.07 ms | 0.13 ms | 0.15 ms |
| Host fanout, 10 calls | 0.57 ms | 0.83 ms | 0.16 ms |
| Host fanout, 50 calls | 2.63 ms | 3.83 ms | 0.19 ms |
| Host fanout, 100 calls | 4.90 ms | 7.36 ms | 0.23 ms |

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
| Limit failure then recover | 9.78 ms | 10.08 ms | 2.46 ms |
| Host failure then recover | 9.96 ms | 10.20 ms | 0.68 ms |

The isolate baseline recovers much faster in this harness. For addon and
sidecar, failure recovery is close to the cost of a normal rerun.

## Retained Memory Findings

The benchmark captures post-GC deltas after 20 workflow runs.

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +44,096 B | +327,680 B |
| Sidecar | +25,320 B | +327,680 B |
| V8 isolate | -2,672 B | -10,354,688 B |

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
| `execution_only_small` | `13.52 ms` |
| `suspend_only` | `0.05 ms` |
| `snapshot_dump_only` | `0.00 ms` |
| `apply_snapshot_policy_only` | `0.01 ms` |
| `snapshot_load_only` | `0.02 ms` |
| `Progress.load_only` | `0.12 ms` |

The practical read is still that the addon is not dominated by snapshot policy
or `Progress.load(...)` overhead on this fixture. The large fixed cost is still
core execution itself, but the latest runtime change did remove the extra
full-`Runtime` clone from live suspension capture and from `dump_snapshot()`.

The clearest direct signal is in the Rust-core `runtime_core` bench: relative
to the previous Criterion baseline, `snapshot_dump_suspended` improved by about
`58%`, `encode_nested_suspend_args` improved by about `3%`, and `vm_hot_loop`
improved by about `2%`, while `snapshot_load_suspended` stayed effectively
flat.

The latest rerun was primarily about measurement coverage rather than a new
runtime optimization, so the release workload suite is best read as "still in
family" rather than as a fresh speedup claim. Relative to
`2026-04-13T09-17-14-652Z`, addon `cold_start_small` improved from `10.41 ms`
to `9.73 ms` (`-6.5%`), `warm_run_small` improved from `10.47 ms` to `9.75 ms`
(`-6.9%`), and `cold_start_code_mode_search` improved from `36.01 ms` to
`33.91 ms` (`-5.8%`), while `programmatic_tool_workflow` regressed from
`35.20 ms` to `36.00 ms` (`+2.3%`) and `host_fanout_100` regressed from
`4.77 ms` to `4.90 ms` (`+2.9%`). `suspend_resume_20` stayed effectively flat
at `3.06 ms`.

The new artifact now also records `addon.suspendState`, which gives the first
checked-in size signal for suspend-heavy fixtures. In the current benchmark
shape, the serialized program is `457 B` and the dumped snapshot is `3,195 B`
for each `suspend_resume_*` fixture, which confirms that stable program bytes
are already a meaningful fraction of the snapshot payload. Holding `20` live
suspended `Progress` objects retains about `22 KB` of Node heap for the
`suspend_resume_5` and `suspend_resume_20` fixtures, while the `1`-boundary
heap sample is still slightly negative after GC and should be treated as
allocator noise at this scale.

The release smoke suite remained within budget and broadly flat-to-slightly
better versus `2026-04-13T09-17-27-082Z`: startup median improved from
`0.120 ms` to `0.119 ms` (`-0.6%`), compute median improved from `3.19 ms` to
`3.11 ms` (`-2.8%`), the host-call median ratio regressed slightly
(`3.71x -> 3.73x`), and the snapshot median ratio regressed slightly
(`1.45x -> 1.49x`).

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
