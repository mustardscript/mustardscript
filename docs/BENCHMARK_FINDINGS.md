# Benchmark Findings

This document summarizes the latest local benchmark run from
`benchmarks/workloads.ts`, comparing:

- `mustard` addon mode
- `mustard` sidecar mode
- a V8 isolate baseline via `isolated-vm`

Reference report:

- `benchmarks/results/2026-04-13T12-17-33-931Z-workloads.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA: `64619e5`
- Fixture version: `5`

## Headline Results

### 1. V8 isolates still win decisively on raw execution throughput

For cold start, warm execution, code-mode search, and the synthetic
programmatic tool-calling workflow, the `isolated-vm` baseline is still much
faster than either `mustard` mode.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Cold start, small script | 4.76 ms | 16.87 ms | 0.50 ms |
| Warm run, small script | 4.83 ms | 4.78 ms | 0.17 ms |
| Cold start, code-mode search | 34.26 ms | 35.19 ms | 0.64 ms |
| Warm run, code-mode search | 33.82 ms | 32.78 ms | 0.20 ms |
| Programmatic tool workflow | 16.77 ms | 33.78 ms | 0.35 ms |

The practical read is unchanged: `mustard` is still not competitive with a V8
isolate on pure execution speed for these local fixtures, even after the latest
interpreter-path improvements.

### 2. Addon mode remains the lowest-latency `mustard` path

The latest lexical-slot work materially reduced addon execution cost. Sidecar is
now roughly tied on the tiny `warm_run_small` fixture, but addon still wins
clearly on the broader workflow and host-call-heavy paths that matter more to
the real product shape.

Representative medians:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 4.76 ms | 16.87 ms | 3.54x |
| Programmatic tool workflow | 16.77 ms | 33.78 ms | 2.01x |
| Host fanout, 10 calls | 0.09 ms | 0.78 ms | 8.36x |
| Host fanout, 100 calls | 0.72 ms | 6.96 ms | 9.64x |

If low latency matters and the deployment model allows it, addon mode remains
the better `mustard` path.

### 3. `mustard` still wins on resumable execution

The suspend/resume workload still favors `mustard` because `mustard` supports
explicit suspended execution and snapshot reload, while the isolate baseline in
this harness must reconstruct progress by re-entering a fresh isolate with
host-carried state.

Representative medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Suspend/resume, 1 boundary | 0.16 ms | 0.09 ms | 0.94 ms |
| Suspend/resume, 5 boundaries | 0.63 ms | 0.36 ms | 2.80 ms |
| Suspend/resume, 20 boundaries | 2.31 ms | 1.31 ms | 10.12 ms |

On this workload shape, `mustard` addon is still about 4x faster than the
isolate baseline, and sidecar is faster still.

## Host Call Findings

Addon and isolate are still in the same order of magnitude for very small
host-call counts. Addon now clearly beats the isolate baseline through 10
crossings in this harness, but the isolate still pulls ahead by 50 to 100
synchronous calls.

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Host fanout, 1 call | 0.03 ms | 0.13 ms | 0.15 ms |
| Host fanout, 10 calls | 0.09 ms | 0.78 ms | 0.15 ms |
| Host fanout, 50 calls | 0.38 ms | 3.77 ms | 0.19 ms |
| Host fanout, 100 calls | 0.72 ms | 6.96 ms | 0.26 ms |

Relative to `2026-04-13T11-51-16-063Z`, addon `host_fanout_100` improved by
about `25.4%` (`0.97 ms -> 0.72 ms`), which is one of the clearest end-to-end
signals from the lexical-slot change set.

## Failure Cleanup Findings

The benchmark includes two failure-and-recovery cases:

- runtime-limit failure followed by a known-good run
- host-failure path followed by a known-good run

Median recovery timings:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Limit failure then recover | 4.70 ms | 4.87 ms | 2.44 ms |
| Host failure then recover | 4.75 ms | 5.05 ms | 0.70 ms |

The isolate baseline still recovers faster, but addon and sidecar roughly
halved their recovery cost relative to the previous checked-in workload report.

## Retained Memory Findings

The benchmark captures post-GC deltas after 20 workflow runs.

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +15,976 B | +278,528 B |
| Sidecar | +12,544 B | +7,766,016 B |
| V8 isolate | -2,384 B | -10,338,304 B |

These numbers should still be treated cautiously:

- they are retained-memory deltas, not peak-memory measurements
- small RSS changes are noisy
- the isolate RSS delta can go negative because allocator reuse and OS page
  reclamation are happening during the sampling window
- sidecar RSS includes both the parent Node process and the live child process

The memory section is useful as a rough regression signal, not as a precise
capacity-planning number.

## Addon Phase Split Findings

The addon-only phase metrics now show a real interpreter-path step change:

| Phase | Median |
| --- | ---: |
| `runtime_init_only` | `0.04 ms` |
| `execution_only_small` | `8.72 ms` |
| `suspend_only` | `0.04 ms` |
| `snapshot_dump_only` | `0.04 ms` |
| `apply_snapshot_policy_only` | `0.02 ms` |
| `snapshot_load_only` | `0.03 ms` |
| `Progress.load_only` | `0.16 ms` |

Relative to `2026-04-13T11-51-16-063Z`, the latest release artifact improved
addon medians on:

- `warm_run_small` by about `52.3%` (`10.13 ms -> 4.83 ms`)
- `programmatic_tool_workflow` by about `22.5%` (`21.63 ms -> 16.77 ms`)
- `host_fanout_100` by about `25.4%` (`0.97 ms -> 0.72 ms`)
- `execution_only_small` by about `35.0%` (`13.41 ms -> 8.72 ms`)

At the same time, `warm_run_code_mode_search` and `suspend_resume_20` stayed
mostly flat (`-2.6%` and `-1.2%`), so the current read is "major win on local
execution and ordinary callback-heavy work, but not yet on the search-heavy
fixture."

The Rust-core microbench suite shows the same pattern more directly:

- `local_load_store_hot` improved by about `77%`
- `env_lookup_hot` improved by about `68%`
- `vm_hot_loop` improved by about `58%`
- `closure_access_hot` improved by about `50%`
- `property_access_hot` improved by about `50%`
- `array_callback_hot` improved by about `28%`
- `collection_callback_hot` improved by about `18%`

Not every measured surface improved. The tracked workload regression gate still
fails because several tiny boundary and phase-only medians moved the wrong way,
including `Progress.load_only` (`0.11 ms -> 0.16 ms`, `+39.2%`) and multiple
small/medium structured-boundary metrics. Those regressions are real, but they
are much smaller in absolute latency than the execution-path gains above.

## Boundary-Only Findings

The `addon.boundary` section isolates structured host-boundary work across
small, medium, and large nested payloads:

| Surface | Small | Medium | Large |
| --- | ---: | ---: | ---: |
| `startInputs` | `0.11 ms` | `0.26 ms` | `0.78 ms` |
| `suspendedArgs` | `0.47 ms` | `2.02 ms` | `14.79 ms` |
| `resumeValues` | `0.10 ms` | `0.21 ms` | `0.71 ms` |
| `resumeErrors` | `0.10 ms` | `0.24 ms` | `0.77 ms` |

The practical read is:

- large `suspendedArgs` is still the obvious boundary hotspot at about `14.8 ms`
- the lexical-slot work did not improve boundary encode/decode directly
- several small and medium boundary medians regressed by about `18%` to `35%`
  versus the previous report, which is why `npm run bench:regress:workloads`
  still exits nonzero

The artifact also still records `addon.suspendState`. In the current benchmark
shape, the serialized program is `494 B`, the dumped snapshot is `2,774 B`, and
the retained live heap for handle-backed suspended state is about `23 KB` for
`suspend_resume_1`, about `20 KB` for `suspend_resume_5`, and about `14 KB` for
`suspend_resume_20`.

The release smoke suite still passed its intended budgets: startup median
`0.11 ms`, compute median `2.77 ms`, host-call median ratio `0.90x`, and
snapshot round-trip median ratio `6.00x`.

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
5. The lexical-slot work clears two local Milestone 3 latency targets
   (`warm_run_small` at `4.83 ms` and `programmatic_tool_workflow` at
   `16.77 ms`), but `warm_run_code_mode_search` at `33.82 ms` remains the
   obvious next execution-path bottleneck.

## Important Caveats

- This is a single-machine local benchmark, not a published cross-machine
  study.
- Sample counts are intentionally small to keep the benchmark runnable during
  development.
- The isolate suspend/resume path is only a best-effort comparison because this
  harness does not provide equivalent continuation snapshotting for V8 isolates.
- The programmatic tool workflow is synthetic. It is more realistic than the
  simple fanout microbenchmark, but it is still not a production trace.
