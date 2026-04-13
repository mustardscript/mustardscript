# Benchmark Findings

This document summarizes the latest local benchmark run from
`benchmarks/workloads.ts`, comparing:

- `mustard` addon mode
- `mustard` sidecar mode
- a V8 isolate baseline via `isolated-vm`

Reference report:

- `benchmarks/results/2026-04-13T11-03-11-141Z-workloads.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA: `86b4956`
- Fixture version: `5`

## Headline Results

### 1. V8 isolates win decisively on raw execution throughput

For cold start, warm execution, code-mode search, and the synthetic
programmatic tool-calling workflow, the `isolated-vm` baseline is much faster
than either `mustard` mode.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Cold start, small script | 9.95 ms | 21.91 ms | 0.51 ms |
| Warm run, small script | 9.98 ms | 10.00 ms | 0.18 ms |
| Cold start, code-mode search | 34.51 ms | 36.83 ms | 0.63 ms |
| Warm run, code-mode search | 34.62 ms | 34.27 ms | 0.21 ms |
| Programmatic tool workflow | 22.45 ms | 38.89 ms | 0.35 ms |

The practical read is that `mustard` is not competitive with a V8 isolate on
pure execution speed for these local fixtures.

### 2. Addon mode is consistently faster than sidecar mode on compute and host-call-heavy paths

The sidecar transport adds measurable overhead relative to in-process addon
execution.

Representative medians:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 9.95 ms | 21.91 ms | 2.20x |
| Programmatic tool workflow | 22.45 ms | 38.89 ms | 1.73x |
| Host fanout, 10 calls | 0.11 ms | 0.82 ms | 7.45x |
| Host fanout, 100 calls | 1.01 ms | 7.22 ms | 7.15x |

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
| Suspend/resume, 5 boundaries | 0.65 ms | 0.37 ms | 2.85 ms |
| Suspend/resume, 20 boundaries | 2.40 ms | 1.38 ms | 10.28 ms |

On this workload shape, `mustard` addon is about 3.4x to 4.8x faster than the
isolate baseline, and sidecar is faster still.

## Host Call Findings

For very small host-call counts, addon and isolate are in the same order of
magnitude. As call counts increase, the isolate baseline is much faster in this
harness.

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Host fanout, 1 call | 0.03 ms | 0.13 ms | 0.15 ms |
| Host fanout, 10 calls | 0.11 ms | 0.82 ms | 0.16 ms |
| Host fanout, 50 calls | 0.52 ms | 3.89 ms | 0.19 ms |
| Host fanout, 100 calls | 1.01 ms | 7.22 ms | 0.25 ms |

The read is:

- addon stays better than sidecar
- addon now beats the isolate baseline through 10 calls in this harness
- the new same-process snapshot-handle path cut addon host fanout sharply, but
  isolates still pull ahead by 50 to 100 synchronous crossings

## Failure Cleanup Findings

The benchmark includes two failure-and-recovery cases:

- runtime-limit failure followed by a known-good run
- host-failure path followed by a known-good run

Median recovery timings:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Limit failure then recover | 10.01 ms | 9.98 ms | 2.42 ms |
| Host failure then recover | 10.09 ms | 10.33 ms | 0.69 ms |

The isolate baseline recovers much faster in this harness. For addon and
sidecar, failure recovery is close to the cost of a normal rerun.

## Retained Memory Findings

The benchmark captures post-GC deltas after 20 workflow runs.

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +19,296 B | +344,064 B |
| Sidecar | +12,192 B | +7,798,784 B |
| V8 isolate | -2,408 B | -10,354,688 B |

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
| `execution_only_small` | `13.61 ms` |
| `suspend_only` | `0.03 ms` |
| `snapshot_dump_only` | `0.03 ms` |
| `apply_snapshot_policy_only` | `0.02 ms` |
| `snapshot_load_only` | `0.02 ms` |
| `Progress.load_only` | `0.12 ms` |

The practical read is still that the addon is not dominated by snapshot policy
or `Progress.load(...)` overhead on this fixture. The large fixed cost remains
core execution itself, but the new opaque snapshot-handle path did make the
direct suspend/resume segment materially cheaper: `suspend_only` dropped to
`0.03 ms`, while `snapshot_dump_only` and `Progress.load_only` still capture the
explicit byte-materialization work separately.

Relative to `2026-04-13T10-41-58-990Z`, the snapshot-handle iteration produced
the clearest addon win of the current milestone. Representative medians moved by
`programmatic_tool_workflow -36.3%` (`35.23 ms -> 22.45 ms`),
`host_fanout_10 -79.2%` (`0.53 ms -> 0.11 ms`),
`host_fanout_100 -78.9%` (`4.79 ms -> 1.01 ms`),
`suspend_resume_20 -9.2%` (`2.64 ms -> 2.40 ms`), and
`cold_start_small -2.3%` (`10.18 ms -> 9.95 ms`), while snapshot size stayed
flat at `2,774 B` because `Progress.dump()` still emits the same detached byte
format.

## Boundary-Only Findings

The new `addon.boundary` section isolates structured host-boundary work across
small, medium, and large nested payloads:

| Surface | Small | Medium | Large |
| --- | ---: | ---: | ---: |
| `startInputs` | `0.09 ms` | `0.22 ms` | `0.68 ms` |
| `suspendedArgs` | `0.34 ms` | `1.58 ms` | `14.49 ms` |
| `resumeValues` | `0.09 ms` | `0.17 ms` | `0.70 ms` |
| `resumeErrors` | `0.08 ms` | `0.21 ms` | `0.78 ms` |

The practical read is that host-to-guest decode for start inputs, resume
values, and resume errors still scales reasonably on this fixture, staying
sub-millisecond even for the large payload. The obvious outlier remains
guest-to-host encoding of large suspended capability arguments at about
`14.5 ms`, which is still an order of magnitude slower than the other
large-payload surfaces and now gives Milestone 5 a precise boundary hotspot to
optimize.

The new artifact still records `addon.suspendState`. In the current benchmark
shape, the serialized program is still `457 B` and the dumped snapshot remains
`2,774 B` for each `suspend_resume_*` fixture, but the retained live heap for
the handle-backed suspended state dropped further to about `14 KB` for
`suspend_resume_20`, about `20 KB` for the `5`-boundary case, and about
`23 KB` for the `1`-boundary case.

The release smoke suite still passed its intended budgets with the new addon
path after rebaselining the snapshot ratio gate for handle-backed direct
resumes: startup median `0.12 ms`, compute median `3.16 ms`, host-call median
ratio `0.91x`, and snapshot round-trip median ratio `6.05x`. That higher
snapshot ratio is expected because the direct path no longer serializes raw
bytes, while `Progress.dump()` still measures full detached snapshot
materialization.

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
