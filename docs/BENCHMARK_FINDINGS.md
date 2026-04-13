# Benchmark Findings

This document summarizes the latest local benchmark run from
`benchmarks/workloads.ts`, comparing:

- `mustard` addon mode
- `mustard` sidecar mode
- a V8 isolate baseline via `isolated-vm`

Reference report:

- `benchmarks/results/2026-04-13T12-54-09-301Z-workloads.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA: `ce7d018`
- Fixture version: `5`

## Headline Results

### 1. V8 isolates still win decisively on raw execution throughput

The latest runtime work trimmed more addon overhead, but the isolate baseline is
still far ahead on pure execution speed for cold start, warm execution, and the
broader workflow fixture.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Cold start, small script | 4.52 ms | 16.51 ms | 0.49 ms |
| Warm run, small script | 4.59 ms | 4.79 ms | 0.18 ms |
| Cold start, code-mode search | 32.93 ms | 34.86 ms | 0.62 ms |
| Warm run, code-mode search | 32.68 ms | 33.15 ms | 0.20 ms |
| Programmatic tool workflow | 16.59 ms | 33.43 ms | 0.34 ms |

The practical read is unchanged: `mustard` is still not competitive with a V8
isolate on raw local execution throughput, even after the latest global/static
fast paths and mutation-accounting work.

### 2. Addon mode remains the lowest-latency `mustard` path

Addon still clearly beats sidecar on the workflow and host-call-heavy cases, and
it is now slightly ahead again on `warm_run_small`.

Representative medians:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 4.52 ms | 16.51 ms | 3.65x |
| Warm run, small script | 4.59 ms | 4.79 ms | 1.04x |
| Programmatic tool workflow | 16.59 ms | 33.43 ms | 2.01x |
| Host fanout, 100 calls | 0.71 ms | 7.02 ms | 9.92x |

If low latency matters and the deployment model allows it, addon mode remains
the better `mustard` path.

### 3. `mustard` still wins on resumable execution

The suspend/resume workload still favors `mustard` because the runtime can keep
and re-enter explicit suspended execution state, while the isolate baseline in
this harness must reconstruct progress in a fresh isolate.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Suspend/resume, 1 boundary | 0.16 ms | 0.10 ms | 0.92 ms |
| Suspend/resume, 5 boundaries | 0.66 ms | 0.36 ms | 2.76 ms |
| Suspend/resume, 20 boundaries | 2.32 ms | 1.33 ms | 9.84 ms |

On this workload shape, addon still holds about a 4x advantage over the
isolate baseline, and sidecar is faster still.

## Host Call Findings

Addon and isolate remain in the same order of magnitude for tiny host-call
counts. Addon still wins through 10 crossings in this harness, while the
isolate pulls ahead by 50 to 100 synchronous calls.

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Host fanout, 1 call | 0.03 ms | 0.12 ms | 0.14 ms |
| Host fanout, 10 calls | 0.09 ms | 0.80 ms | 0.15 ms |
| Host fanout, 50 calls | 0.37 ms | 3.74 ms | 0.19 ms |
| Host fanout, 100 calls | 0.71 ms | 7.02 ms | 0.26 ms |

Relative to `2026-04-13T12-17-33-931Z`, addon `host_fanout_100` improved by
about `1.7%` (`0.72 ms -> 0.71 ms`), so the latest chunk helped this surface a
bit, but not dramatically.

## Failure Cleanup Findings

The benchmark includes two failure-and-recovery cases:

- runtime-limit failure followed by a known-good run
- host-failure path followed by a known-good run

Median recovery timings:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Limit failure then recover | 4.56 ms | 4.73 ms | 2.46 ms |
| Host failure then recover | 4.66 ms | 4.88 ms | 0.71 ms |

Relative to the previous checked-in workload report, both addon cleanup paths
improved modestly.

## Retained Memory Findings

The benchmark captures post-GC deltas after 20 workflow runs.

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +15,976 B | +311,296 B |
| Sidecar | +12,368 B | +8,175,616 B |
| V8 isolate | -504 B | -10,240,000 B |

These numbers should still be treated cautiously:

- they are retained-memory deltas, not peak-memory measurements
- small RSS changes are noisy
- the isolate RSS delta can go negative because allocator reuse and OS page
  reclamation are happening during the sampling window
- sidecar RSS includes both the parent Node process and the live child process

The memory section is useful as a rough regression signal, not as a precise
capacity-planning number.

## Addon Phase Split Findings

The addon-only phase metrics still show the same overall shape:

| Phase | Median |
| --- | ---: |
| `runtime_init_only` | `0.05 ms` |
| `execution_only_small` | `8.81 ms` |
| `suspend_only` | `0.04 ms` |
| `snapshot_dump_only` | `0.04 ms` |
| `apply_snapshot_policy_only` | `0.02 ms` |
| `snapshot_load_only` | `0.04 ms` |
| `Progress.load_only` | `0.15 ms` |

Relative to `2026-04-13T12-17-33-931Z`, the latest release artifact improved
addon medians on:

- `cold_start_small` by about `5.0%` (`4.76 ms -> 4.52 ms`)
- `warm_run_small` by about `5.0%` (`4.83 ms -> 4.59 ms`)
- `cold_start_code_mode_search` by about `3.9%` (`34.26 ms -> 32.93 ms`)
- `warm_run_code_mode_search` by about `3.4%` (`33.82 ms -> 32.68 ms`)
- `programmatic_tool_workflow` by about `1.1%` (`16.77 ms -> 16.59 ms`)

At the same time, `execution_only_small` regressed slightly (`8.72 ms -> 8.81
ms`, about `+1.1%`), `suspend_resume_20` stayed effectively flat (`+0.6%`),
and `runtime_init_only` moved from `0.04 ms` to `0.05 ms`. The current read is
"real but modest improvement on ordinary execution and mutation-heavy code,
without another major step change."

The Rust-core microbench suite shows the direct hot-path signal more clearly:

- `global_lookup_hot` improved by about `3.7%`
- `property_access_hot` improved by about `4.1%`
- `builtin_method_hot` improved by about `3.4%`
- `vm_hot_loop` improved by about `4.0%`
- `local_load_store_hot` improved by about `3.2%`
- `closure_access_hot` improved by about `4.9%`
- `map_set_hot` improved by about `3.2%`

`array_callback_hot` and `collection_callback_hot` moved in the right direction
but stayed within noise on this run.

The tracked workload regression gate still exits nonzero, but the remaining
issue is now a tiny phase-only threshold: addon `runtime_init_only` moved from
`0.04 ms` to `0.05 ms`, which is over the current relative `10%` limit despite
being only about `0.01 ms` in absolute terms.

## Boundary-Only Findings

The `addon.boundary` section isolates structured host-boundary work across
small, medium, and large nested payloads:

| Surface | Small | Medium | Large |
| --- | ---: | ---: | ---: |
| `startInputs` | `0.11 ms` | `0.26 ms` | `0.78 ms` |
| `suspendedArgs` | `0.45 ms` | `2.04 ms` | `14.63 ms` |
| `resumeValues` | `0.10 ms` | `0.21 ms` | `0.73 ms` |
| `resumeErrors` | `0.10 ms` | `0.23 ms` | `0.76 ms` |

The practical read is:

- large `suspendedArgs` is still the obvious boundary hotspot at about `14.6 ms`
- the latest runtime work did not materially change boundary encode/decode
  behavior
- the small and medium boundary surfaces are now mostly flat or slightly
  improved versus the previous checked-in report

The artifact also still records `addon.suspendState`. In the current benchmark
shape, the serialized program is `494 B`, the dumped snapshot is `2,774 B`, and
the retained live heap for handle-backed suspended state is about `23 KB` for
`suspend_resume_1`, about `20 KB` for `suspend_resume_5`, and about `14 KB` for
`suspend_resume_20`.

The release smoke suite still passed its intended budgets: startup median
`0.09 ms`, compute median `2.73 ms`, host-call median ratio `0.88x`, and
snapshot round-trip median ratio `6.27x`.

## Conclusions

For the measured local workloads:

1. Choose a V8 isolate when raw execution speed is the primary goal and
   resumable continuation state is not required.
2. Choose `mustard` addon mode when you want `mustard` semantics with the
   lowest latency available inside the current process.
3. Choose `mustard` sidecar mode when you want stronger process isolation and
   can afford the added transport overhead.
4. Choose `mustard` over the isolate baseline when suspend/resume behavior is a
   first-class requirement rather than an implementation detail.
5. The latest global/static fast paths plus local mutation-accounting changes
   deliver a modest follow-on win after the lexical-slot milestone, but
   `warm_run_code_mode_search` at `32.68 ms` remains the clearest execution-path
   bottleneck.

## Important Caveats

- This is a single-machine local benchmark, not a published cross-machine
  study.
- Sample counts are intentionally small to keep the benchmark runnable during
  development.
- The isolate suspend/resume path is only a best-effort comparison because this
  harness does not provide equivalent continuation snapshotting for V8 isolates.
- The programmatic tool workflow is synthetic. It is more realistic than the
  simple fanout microbenchmark, but it is still not a production trace.
