# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T15-31-34-682Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T15-31-19-504Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T15-32-43-544Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `b1342b4`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. Promise and lexical accounting deltas removed nearly all hot-path full-refresh bookkeeping

The latest change set replaces promise-state, promise-driver, env-binding, cell,
and per-frame `this` accounting refreshes with exact byte deltas wherever the
runtime already knows the old and new payload sizes. `Promise.all`,
`Promise.any`, and `Promise.allSettled` also now move their driver buffers out
on terminal completion instead of cloning the buffered values back into new
vectors.

Relative to the tracked addon baseline
`benchmarks/results/2026-04-13T15-02-00-153Z-workloads.json`, the latest
checked-in workload artifact shows the main effect on execution-heavy and
fanout-heavy slices:

| Workload | Baseline | Current | Delta |
| --- | ---: | ---: | ---: |
| Warm run, small script | 0.95 ms | 0.97 ms | `+2.1%` |
| Warm run, code-mode search | 0.52 ms | 0.53 ms | `+0.9%` |
| Programmatic tool workflow | 1.51 ms | 1.48 ms | `-2.1%` |
| Host fanout, 100 calls | 0.41 ms | 0.37 ms | `-11.0%` |
| Suspend/resume, 20 boundaries | 2.32 ms | 2.33 ms | `+0.4%` |

The clearest signal is in the phase-level breakdown:

| Phase | Baseline | Current | Delta |
| --- | ---: | ---: | ---: |
| `execution_only_small` | 2.08 ms | 1.99 ms | `-4.2%` |
| `Progress.load_only` | 0.22 ms | 0.21 ms | `-5.9%` |
| `snapshot_load_only` | 0.06 ms | 0.05 ms | `-6.9%` |
| `suspend_only` | 0.06 ms | 0.06 ms | `-6.1%` |
| `runtime_init_only` | 0.03 ms | 0.03 ms | `+1.0%` |

### 2. The hottest addon bookkeeping counters collapsed from thousands of refreshes to almost none

The runtime-counter section of the new workload artifact shows that the new
delta path is actually getting used on the benchmarked code paths:

| Surface | Baseline `accounting_refreshes` | Current | Delta |
| --- | ---: | ---: | ---: |
| `warm_run_small` | 3808 | 0 | `-100.0%` |
| `programmatic_tool_workflow` | 47 | 3 | `-93.6%` |
| `host_fanout_100` | 207 | 1 | `-99.5%` |
| `execution_only_small` | 3810 | 1 | `-100.0%` |
| `suspend_resume_20` | 46 | 1 | `-97.8%` |

GC collection counts stayed flat on those same surfaces, so the change is
removing accounting churn rather than simply shifting work into collection.

### 3. Boundary-heavy restore paths mostly improved, but one tiny resume-error surface still trips the tracked regression gate

Current addon boundary medians:

| Surface | Small | Medium | Large |
| --- | ---: | ---: | ---: |
| `startInputs` | `0.15 ms` | `0.35 ms` | `1.07 ms` |
| `suspendedArgs` | `0.27 ms` | `0.69 ms` | `2.23 ms` |
| `resumeValues` | `0.14 ms` | `0.31 ms` | `0.97 ms` |
| `resumeErrors` | `0.13 ms` | `0.32 ms` | `0.91 ms` |

Relative to the tracked addon baseline:

- `startInputs.small` improved by `3.3%`
- `resumeErrors.medium` improved by `6.3%`
- `resumeErrors.large` improved by `1.7%`
- `suspendedArgs.medium` stayed effectively flat while p95 improved by `3.9%`

The addon-only tracked regression gate still exits nonzero on one small-payload
surface:

- `addon.boundary.resumeErrors.small`: median `0.11 ms -> 0.13 ms` (`+10.2%`)
- p95 `0.12 ms -> 0.13 ms` (`+11.2%`)

Everything else in the tracked addon regression report is either improved or
within the configured `10%` threshold, so the remaining gate failure is now a
single low-latency surface rather than a broad execution-path regression.

### 4. Smoke gates still pass comfortably

Current release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.05 ms` |
| Compute | `0.38 ms` |
| Host-call median ratio | `4.53x` |
| Host-call p95 ratio | `5.09x` |
| Snapshot median ratio | `7.37x` |
| Snapshot p95 ratio | `4.61x` |

Relative to the previous checked-in release smoke artifact
`benchmarks/results/2026-04-13T13-55-18-321Z-smoke-release.json`:

- compute median improved by `12.7%`
- compute p95 improved by `6.0%`
- startup moved slightly slower but remained far inside the `1 ms` / `2 ms`
  budgets
- host-call and snapshot ratio budgets still passed without any budget rebase

Current dev smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.28 ms` |
| Compute | `2.48 ms` |
| Host-call median ratio | `3.46x` |
| Host-call p95 ratio | `3.70x` |
| Snapshot median ratio | `8.64x` |
| Snapshot p95 ratio | `8.57x` |

The dev smoke gate also still passes with the previously rebased snapshot ratio
budget.

## Sidecar And Microbench Notes

Current sidecar/addon ratios from the new workload artifact:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Warm run, small script | 0.97 ms | 1.46 ms | `1.51x` |
| Programmatic tool workflow | 1.48 ms | 18.38 ms | `12.40x` |
| Host fanout, 100 calls | 0.37 ms | 6.58 ms | `17.92x` |

The shared Rust core got cheaper to execute, but sidecar mode is still mostly a
transport and session-state problem.

The local `npm run bench:rust` rerun was mixed rather than uniformly better:

- `env_lookup_hot` improved by about `4.6%`
- `local_load_store_hot` improved in the low-single-digit range, within noise
- `compile_pipeline` parse/deserialize benches regressed by about `3%` to `5%`
- `builtin_method_hot`, `array_callback_hot`, and `collection_callback_hot`
  regressed by roughly `3%` to `9%`

That mixed signal suggests the latest work removed real runtime bookkeeping
churn, but the microbench suite still needs more targeted coverage for the new
promise/accounting path before it can cleanly separate wins from harness noise.

## Conclusions

1. Incremental accounting now covers the runtime's hot promise, env, and cell
   mutation paths, and the addon workload counters confirm that the old
   full-refresh bookkeeping was almost entirely removed from those surfaces.
2. The latest checked-in workload artifact improved `host_fanout_100`,
   `programmatic_tool_workflow`, `execution_only_small`, and `Progress.load`
   while keeping suspend/resume essentially flat.
3. Release and dev smoke gates still pass. The remaining tracked workload gate
   failure is now isolated to `addon.boundary.resumeErrors.small`, which stayed
   just above the configured `10%` regression threshold.
4. The next Milestone 4 work should stay focused on incremental accounting and
   async promise-path cleanup, especially the remaining boundary-only resume
   surfaces and the still-unoptimized `Map` / `Set` accounting paths.
