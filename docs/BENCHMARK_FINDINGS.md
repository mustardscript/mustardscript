# Benchmark Findings

This document summarizes the latest local benchmark run from
`benchmarks/workloads.ts`, comparing:

- `mustard` addon mode
- `mustard` sidecar mode
- a V8 isolate baseline via `isolated-vm`

Reference report:

- `benchmarks/results/2026-04-13T09-17-14-652Z-workloads.json`

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
| Cold start, small script | 10.41 ms | 21.84 ms | 0.50 ms |
| Warm run, small script | 10.47 ms | 10.20 ms | 0.19 ms |
| Cold start, code-mode search | 36.01 ms | 39.60 ms | 0.63 ms |
| Warm run, code-mode search | 35.16 ms | 35.76 ms | 0.22 ms |
| Programmatic tool workflow | 35.20 ms | 39.52 ms | 0.35 ms |

The practical read is that `mustard` is not competitive with a V8 isolate on
pure execution speed for these local fixtures.

### 2. Addon mode is consistently faster than sidecar mode on compute and host-call-heavy paths

The sidecar transport adds measurable overhead relative to in-process addon
execution.

Representative medians:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 10.41 ms | 21.84 ms | 2.10x |
| Programmatic tool workflow | 35.20 ms | 39.52 ms | 1.12x |
| Host fanout, 10 calls | 0.53 ms | 0.81 ms | 1.53x |
| Host fanout, 100 calls | 4.77 ms | 7.27 ms | 1.52x |

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
| Suspend/resume, 1 boundary | 0.20 ms | 0.10 ms | 0.96 ms |
| Suspend/resume, 5 boundaries | 0.78 ms | 0.37 ms | 2.85 ms |
| Suspend/resume, 20 boundaries | 3.06 ms | 1.41 ms | 10.35 ms |

On this workload shape, `mustard` addon is about 3.4x to 4.8x faster than the
isolate baseline, and sidecar is faster still.

## Host Call Findings

For very small host-call counts, addon and isolate are in the same order of
magnitude. As call counts increase, the isolate baseline is much faster in this
harness.

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Host fanout, 1 call | 0.07 ms | 0.13 ms | 0.15 ms |
| Host fanout, 10 calls | 0.53 ms | 0.81 ms | 0.16 ms |
| Host fanout, 50 calls | 2.50 ms | 3.77 ms | 0.20 ms |
| Host fanout, 100 calls | 4.77 ms | 7.27 ms | 0.24 ms |

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
| Limit failure then recover | 10.19 ms | 10.32 ms | 2.45 ms |
| Host failure then recover | 10.40 ms | 10.59 ms | 0.66 ms |

The isolate baseline recovers much faster in this harness. For addon and
sidecar, failure recovery is close to the cost of a normal rerun.

## Retained Memory Findings

The benchmark captures post-GC deltas after 20 workflow runs.

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +4,544 B | +344,064 B |
| Sidecar | +18,928 B | +344,064 B |
| V8 isolate | -3,608 B | -10,272,768 B |

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
| `runtime_init_only` | `0.05 ms` |
| `execution_only_small` | `13.69 ms` |
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

The broader release workload suite stayed mixed, but the suspend-heavy and
host-heavy paths did move in the right direction versus
`2026-04-13T09-02-17-086Z`: `host_fanout_100` improved from `5.23 ms` to
`4.77 ms` (`-8.9%`), `programmatic_tool_workflow` improved from `36.95 ms` to
`35.20 ms` (`-4.7%`), `cold_start_code_mode_search` improved from `36.51 ms`
to `36.01 ms` (`-1.4%`), and `suspend_resume_20` improved from `3.10 ms` to
`3.06 ms` (`-1.3%`). Some metrics moved the wrong way: `warm_run_small`
regressed slightly (`+0.6%`) and `execution_only_small` regressed modestly
(`+1.8%`), which suggests the runtime-clone removal is a narrow suspend-path
win rather than a broad execution-throughput improvement.

The release smoke suite improved on the ratios that include snapshot handling:
startup median improved from `0.133 ms` to `0.120 ms` (`-9.6%`), the host-call
median ratio improved from `3.99x` to `3.71x` (`-6.9%`), the snapshot median
ratio improved from `1.51x` to `1.45x` (`-3.7%`), and retained heap delta
improved from `10,880 B` to `10,360 B` (`-4.8%`).

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
