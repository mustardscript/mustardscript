# Benchmark Findings

This document summarizes the latest local benchmark run from
`benchmarks/workloads.ts`, comparing:

- `mustard` addon mode
- `mustard` sidecar mode
- a V8 isolate baseline via `isolated-vm`

Reference report:

- `benchmarks/results/2026-04-13T10-41-58-990Z-workloads.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA: `8df3ec9`
- Fixture version: `5`

## Headline Results

### 1. V8 isolates win decisively on raw execution throughput

For cold start, warm execution, code-mode search, and the synthetic
programmatic tool-calling workflow, the `isolated-vm` baseline is much faster
than either `mustard` mode.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Cold start, small script | 10.18 ms | 22.30 ms | 0.52 ms |
| Warm run, small script | 9.69 ms | 10.12 ms | 0.17 ms |
| Cold start, code-mode search | 34.23 ms | 36.35 ms | 0.62 ms |
| Warm run, code-mode search | 34.46 ms | 33.96 ms | 0.20 ms |
| Programmatic tool workflow | 35.23 ms | 38.44 ms | 0.34 ms |

The practical read is that `mustard` is not competitive with a V8 isolate on
pure execution speed for these local fixtures.

### 2. Addon mode is consistently faster than sidecar mode on compute and host-call-heavy paths

The sidecar transport adds measurable overhead relative to in-process addon
execution.

Representative medians:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 10.18 ms | 22.30 ms | 2.19x |
| Programmatic tool workflow | 35.23 ms | 38.44 ms | 1.09x |
| Host fanout, 10 calls | 0.53 ms | 0.80 ms | 1.51x |
| Host fanout, 100 calls | 4.79 ms | 7.18 ms | 1.50x |

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
| Suspend/resume, 5 boundaries | 0.71 ms | 0.37 ms | 2.80 ms |
| Suspend/resume, 20 boundaries | 2.64 ms | 1.38 ms | 10.05 ms |

On this workload shape, `mustard` addon is about 3.4x to 4.8x faster than the
isolate baseline, and sidecar is faster still.

## Host Call Findings

For very small host-call counts, addon and isolate are in the same order of
magnitude. As call counts increase, the isolate baseline is much faster in this
harness.

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Host fanout, 1 call | 0.07 ms | 0.13 ms | 0.15 ms |
| Host fanout, 10 calls | 0.53 ms | 0.80 ms | 0.15 ms |
| Host fanout, 50 calls | 2.40 ms | 3.85 ms | 0.19 ms |
| Host fanout, 100 calls | 4.79 ms | 7.18 ms | 0.24 ms |

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
| Limit failure then recover | 10.51 ms | 9.85 ms | 2.45 ms |
| Host failure then recover | 10.70 ms | 10.17 ms | 0.69 ms |

The isolate baseline recovers much faster in this harness. For addon and
sidecar, failure recovery is close to the cost of a normal rerun.

## Retained Memory Findings

The benchmark captures post-GC deltas after 20 workflow runs.

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +19,000 B | +1,212,416 B |
| Sidecar | +32,728 B | +7,782,400 B |
| V8 isolate | -9,568 B | -10,403,840 B |

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
| `execution_only_small` | `13.43 ms` |
| `suspend_only` | `0.06 ms` |
| `snapshot_dump_only` | `0.01 ms` |
| `apply_snapshot_policy_only` | `0.01 ms` |
| `snapshot_load_only` | `0.02 ms` |
| `Progress.load_only` | `0.10 ms` |

The practical read is still that the addon is not dominated by snapshot policy
or `Progress.load(...)` overhead on this fixture. The large fixed cost remains
core execution itself, and the current suspend/resume bookkeeping overhead is
still far below the execution-only slice.

Relative to `2026-04-13T10-10-36-682Z`, the detached-snapshot iteration was
mixed on broad latency but clear on snapshot size. Representative addon medians
moved by `warm_run_small -2.1%` (`9.90 ms -> 9.69 ms`),
`warm_run_code_mode_search +1.0%` (`34.13 ms -> 34.46 ms`),
`programmatic_tool_workflow +2.6%` (`34.34 ms -> 35.23 ms`),
`host_fanout_100 +1.9%` (`4.70 ms -> 4.79 ms`), and
`suspend_resume_20 +2.3%` (`2.58 ms -> 2.64 ms`), while the dumped snapshot
payload for the suspend fixtures dropped from `3,195 B` to `2,774 B`
(`-13.2%`).

## Boundary-Only Findings

The new `addon.boundary` section isolates structured host-boundary work across
small, medium, and large nested payloads:

| Surface | Small | Medium | Large |
| --- | ---: | ---: | ---: |
| `startInputs` | `0.12 ms` | `0.26 ms` | `0.90 ms` |
| `suspendedArgs` | `0.41 ms` | `1.77 ms` | `14.98 ms` |
| `resumeValues` | `0.16 ms` | `0.26 ms` | `0.76 ms` |
| `resumeErrors` | `0.17 ms` | `0.26 ms` | `0.90 ms` |

The practical read is that host-to-guest decode for start inputs, resume
values, and resume errors still scales reasonably on this fixture, staying
sub-millisecond even for the large payload. The obvious outlier is guest-to-host
encoding of large suspended capability arguments at about `15.30 ms`, which is
an order of magnitude slower than the other large-payload surfaces and now
gives Milestone 5 a precise boundary hotspot to optimize.

The new artifact still records `addon.suspendState`, and it now shows the
detached-program change directly. In the current benchmark shape, the
serialized program is still `457 B`, but the dumped snapshot fell to `2,774 B`
for each `suspend_resume_*` fixture. That `421 B` drop means the current addon
snapshot now carries only a small program-identity overhead instead of the full
stable program payload. Holding `20` live suspended `Progress` objects retains
about `30 KB` of Node heap for the `suspend_resume_20` fixture and about
`20 KB` for the `5`-boundary case, while the `1`-boundary heap sample remains
too noisy to over-interpret at this scale.

The release smoke suite still passed its intended budgets with the new addon
path: startup median `0.14 ms`, compute median `3.37 ms`, host-call median
ratio `3.60x`, and snapshot round-trip median ratio `1.33x`.

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
