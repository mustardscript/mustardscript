# Benchmark Findings

This document summarizes the latest local benchmark run from
`benchmarks/workloads.ts`, comparing:

- `mustard` addon mode
- `mustard` sidecar mode
- a V8 isolate baseline via `isolated-vm`

Reference report:

- `benchmarks/results/2026-04-13T10-10-36-682Z-workloads.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA: `e3714ee`
- Fixture version: `5`

## Headline Results

### 1. V8 isolates win decisively on raw execution throughput

For cold start, warm execution, code-mode search, and the synthetic
programmatic tool-calling workflow, the `isolated-vm` baseline is much faster
than either `mustard` mode.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Cold start, small script | 9.96 ms | 23.30 ms | 0.50 ms |
| Warm run, small script | 9.90 ms | 10.49 ms | 0.19 ms |
| Cold start, code-mode search | 34.36 ms | 37.20 ms | 0.62 ms |
| Warm run, code-mode search | 34.13 ms | 35.15 ms | 0.21 ms |
| Programmatic tool workflow | 34.34 ms | 39.32 ms | 0.36 ms |

The practical read is that `mustard` is not competitive with a V8 isolate on
pure execution speed for these local fixtures.

### 2. Addon mode is consistently faster than sidecar mode on compute and host-call-heavy paths

The sidecar transport adds measurable overhead relative to in-process addon
execution.

Representative medians:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 9.96 ms | 23.30 ms | 2.34x |
| Programmatic tool workflow | 34.34 ms | 39.32 ms | 1.14x |
| Host fanout, 10 calls | 0.52 ms | 0.79 ms | 1.51x |
| Host fanout, 100 calls | 4.70 ms | 7.25 ms | 1.54x |

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
| Suspend/resume, 1 boundary | 0.16 ms | 0.10 ms | 0.94 ms |
| Suspend/resume, 5 boundaries | 0.69 ms | 0.37 ms | 2.80 ms |
| Suspend/resume, 20 boundaries | 2.58 ms | 1.38 ms | 10.00 ms |

On this workload shape, `mustard` addon is about 3.4x to 4.8x faster than the
isolate baseline, and sidecar is faster still.

## Host Call Findings

For very small host-call counts, addon and isolate are in the same order of
magnitude. As call counts increase, the isolate baseline is much faster in this
harness.

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Host fanout, 1 call | 0.07 ms | 0.13 ms | 0.16 ms |
| Host fanout, 10 calls | 0.52 ms | 0.79 ms | 0.16 ms |
| Host fanout, 50 calls | 2.41 ms | 3.86 ms | 0.20 ms |
| Host fanout, 100 calls | 4.70 ms | 7.25 ms | 0.24 ms |

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
| Limit failure then recover | 9.86 ms | 10.04 ms | 2.43 ms |
| Host failure then recover | 9.94 ms | 10.33 ms | 0.68 ms |

The isolate baseline recovers much faster in this harness. For addon and
sidecar, failure recovery is close to the cost of a normal rerun.

## Retained Memory Findings

The benchmark captures post-GC deltas after 20 workflow runs.

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +13,888 B | +98,304 B |
| Sidecar | +28,880 B | +7,225,344 B |
| V8 isolate | -7,240 B | -10,223,616 B |

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
| `execution_only_small` | `12.76 ms` |
| `suspend_only` | `0.05 ms` |
| `snapshot_dump_only` | `0.00 ms` |
| `apply_snapshot_policy_only` | `0.01 ms` |
| `snapshot_load_only` | `0.02 ms` |
| `Progress.load_only` | `0.10 ms` |

The practical read is still that the addon is not dominated by snapshot policy
or `Progress.load(...)` overhead on this fixture. The large fixed cost remains
core execution itself, and the current suspend/resume bookkeeping overhead is
still far below the execution-only slice.

Relative to `2026-04-13T09-59-36-458Z`, the older phase metrics stayed roughly
in family while this iteration added the missing boundary-only coverage.
Representative addon medians moved by `warm_run_small -0.4%`, `warm_run_code_mode_search -2.8%`,
`programmatic_tool_workflow -5.2%`, `host_fanout_100 -6.4%`, and
`suspend_resume_20 -7.3%`, while `execution_only_small` was effectively flat
at this sample size (`12.66 ms -> 12.76 ms`, `+0.8%`).

## Boundary-Only Findings

The new `addon.boundary` section isolates structured host-boundary work across
small, medium, and large nested payloads:

| Surface | Small | Medium | Large |
| --- | ---: | ---: | ---: |
| `startInputs` | `0.13 ms` | `0.25 ms` | `0.86 ms` |
| `suspendedArgs` | `0.38 ms` | `1.79 ms` | `15.30 ms` |
| `resumeValues` | `0.16 ms` | `0.24 ms` | `0.81 ms` |
| `resumeErrors` | `0.15 ms` | `0.26 ms` | `0.89 ms` |

The practical read is that host-to-guest decode for start inputs, resume
values, and resume errors still scales reasonably on this fixture, staying
sub-millisecond even for the large payload. The obvious outlier is guest-to-host
encoding of large suspended capability arguments at about `15.30 ms`, which is
an order of magnitude slower than the other large-payload surfaces and now
gives Milestone 5 a precise boundary hotspot to optimize.

The new artifact now also records `addon.suspendState`, which gives the first
checked-in size signal for suspend-heavy fixtures. In the current benchmark
shape, the serialized program is `457 B` and the dumped snapshot is `3,195 B`
for each `suspend_resume_*` fixture, which confirms that stable program bytes
are already a meaningful fraction of the snapshot payload. Holding `20` live
suspended `Progress` objects retains about `22 KB` of Node heap for the
`suspend_resume_5` and `suspend_resume_20` fixtures, while the `1`-boundary
heap sample is still slightly negative after GC and should be treated as
allocator noise at this scale.

The release smoke suite still passed its intended budgets versus
`2026-04-13T09-59-48-548Z-smoke-release.json`, but this rerun was noisier:
startup regressed from `0.12 ms` to `0.15 ms` (`+30.7%`), compute regressed
from `3.21 ms` to `3.65 ms` (`+13.7%`), the host-call median ratio improved
slightly from about `3.75x` to `3.63x`, and the snapshot median ratio stayed
close to flat (`1.25x -> 1.26x`, `+1.0%`). The useful signal for this
iteration came from the new workload boundary section rather than smoke.

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
