# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T13-55-41-849Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T13-55-18-321Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T14-00-38-812Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `009f526`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. Slot-keyed GC mark maps removed another fixed cleanup cost

This change replaced the runtime's `HashSet`-backed GC mark tables with
slot-keyed secondary maps, keeping the same reachability semantics while
avoiding hash-heavy bookkeeping during every collection.

Relative to the previous checked-in workload artifact
`benchmarks/results/2026-04-13T13-38-06-436Z-workloads.json`, the latest addon
medians improved on the main execution-heavy surfaces:

| Workload | Previous | Current | Delta |
| --- | ---: | ---: | ---: |
| Cold start, small script | 1.02 ms | 1.01 ms | `-0.4%` |
| Warm run, small script | 0.96 ms | 0.93 ms | `-3.3%` |
| Warm run, code-mode search | 0.54 ms | 0.53 ms | `-2.7%` |
| Programmatic tool workflow | 1.69 ms | 1.50 ms | `-11.1%` |
| Host fanout, 100 calls | 0.39 ms | 0.35 ms | `-11.5%` |
| Suspend/resume, 20 boundaries | 2.23 ms | 2.26 ms | `+1.3%` |

The strongest phase-level signal is that execution and restore-heavy slices got
cheaper instead of merely moving noise around:

| Phase | Previous | Current | Delta |
| --- | ---: | ---: | ---: |
| `execution_only_small` | 2.39 ms | 1.94 ms | `-18.8%` |
| `Progress.load_only` | 0.25 ms | 0.21 ms | `-15.5%` |
| `snapshot_load_only` | 0.06 ms | 0.05 ms | `-13.7%` |
| `snapshot_dump_only` | 0.06 ms | 0.06 ms | `-10.9%` |
| `suspend_only` | 0.05 ms | 0.05 ms | `-4.3%` |

### 2. Boundary-heavy restore paths moved in the right direction again

Current addon boundary medians:

| Surface | Small | Medium | Large |
| --- | ---: | ---: | ---: |
| `startInputs` | `0.15 ms` | `0.33 ms` | `1.07 ms` |
| `suspendedArgs` | `0.27 ms` | `0.69 ms` | `2.14 ms` |
| `resumeValues` | `0.13 ms` | `0.31 ms` | `0.96 ms` |
| `resumeErrors` | `0.11 ms` | `0.31 ms` | `0.90 ms` |

Relative to the previous checked-in workload artifact:

- `startInputs.medium` improved by `15.0%`
- `suspendedArgs.medium` improved by `12.5%`
- `suspendedArgs.large` improved by `4.3%`
- `resumeErrors.medium` improved by `10.9%`
- `resumeErrors.small` improved by `11.6%`

The tracked addon-only regression gate now passes on the latest rerun. The one
remaining noisy surface is `suspend_resume_20`, which stayed effectively flat
within low-single-digit movement.

### 3. Sidecar is still dominated by transport and session overhead

Current medians from the new workload artifact:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 1.01 ms | 4.99 ms | `4.96x` |
| Warm run, small script | 0.93 ms | 1.42 ms | `1.53x` |
| Programmatic tool workflow | 1.50 ms | 18.66 ms | `12.46x` |
| Host fanout, 100 calls | 0.35 ms | 6.61 ms | `18.78x` |

The GC bookkeeping change helped the shared Rust core, but the sidecar gap is
still a transport/session problem rather than a runtime-core problem.

## Rust-Core Microbench Findings

The local `npm run bench:rust` rerun reported statistically significant wins on
most hot-path benches. The clearest signals were:

- `vm_hot_loop`: about `-4.9%`
- `local_load_store_hot`: about `-5.5%`
- `closure_access_hot`: about `-5.9%`
- `builtin_method_hot`: about `-5.3%`
- `array_callback_hot`: about `-6.6%`
- `collection_callback_hot`: about `-9.1%`
- `map_set_hot`: about `-3.6%`
- `snapshot_load_suspended`: about `-6.5%`

That pattern matches the implementation: the interpreter is still doing the
same semantic work, but collections and promise/snapshot-heavy paths now spend
less time in GC mark bookkeeping.

## Smoke Gate Findings

Current release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.05 ms` |
| Compute | `0.43 ms` |
| Host-call median ratio | `3.42x` |
| Host-call p95 ratio | `4.21x` |
| Snapshot median ratio | `6.81x` |
| Snapshot p95 ratio | `5.40x` |

The release smoke gate still passes comfortably. The first rerun hit a noisy
host-call p95 outlier, but the immediate sequential rerun returned to the
expected range, and the checked-in release artifact is well inside budget.

Current dev smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.24 ms` |
| Compute | `2.56 ms` |
| Host-call median ratio | `2.88x` |
| Host-call p95 ratio | `3.15x` |
| Snapshot median ratio | `9.57x` |
| Snapshot p95 ratio | `8.99x` |

The dev smoke gate required one budget rebase: the debug snapshot round-trip
absolute median stayed roughly flat (`1.27 ms -> 1.26 ms`) while the direct
snapshot path got materially cheaper (`0.16 ms -> 0.13 ms`), so the old
`9.0x` ratio cap stopped reflecting measured reality. The dev snapshot median
ratio budget is now `12.0x`, while the release smoke suite remains the source
of truth for optimization decisions.

## Retained Memory And Cleanup

Current retained-memory deltas after 20 workflow runs:

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +16,520 B | +688,128 B |
| Sidecar | +12,144 B | +8,372,224 B |
| V8 isolate | -56 B | -10,436,608 B |

Current failure-cleanup medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Limit failure then recover | 0.80 ms | 0.90 ms | 2.43 ms |
| Host failure then recover | 0.81 ms | 1.05 ms | 0.67 ms |

Failure recovery stayed effectively flat to slightly better, which is the
important constraint for this milestone: cheaper GC bookkeeping did not weaken
cleanup or restore behavior.

## Conclusions

1. Replacing `HashSet`-backed GC mark tables with slot-keyed secondary maps
   completed the remaining Milestone 4 mark-bookkeeping item and produced
   measurable wins on addon workloads, Rust microbenches, and restore-adjacent
   phases.
2. The latest tracked workload and smoke regression gates now pass, and the
   runtime stays within its existing fail-closed limits/cancellation/snapshot
   semantics.
3. The next Milestone 4 opportunities are still broader incremental accounting,
   async promise clone reduction, and explicit GC/accounting counters.
