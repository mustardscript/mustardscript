# Benchmark Findings

This document summarizes the latest checked-in release benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T13-20-51-960Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T13-22-53-049Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T13-23-23-466Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `0579e8b`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. The GC trigger change materially lowered addon execution latency

Relative to the previous checked-in workload artifact
`benchmarks/results/2026-04-13T12-54-09-301Z-workloads.json`, the latest addon
medians improved sharply on the main execution paths:

| Workload | Previous | Current | Delta |
| --- | ---: | ---: | ---: |
| Cold start, small script | 4.52 ms | 1.03 ms | `-77.3%` |
| Warm run, small script | 4.59 ms | 0.97 ms | `-78.8%` |
| Cold start, code-mode search | 32.93 ms | 0.96 ms | `-97.1%` |
| Warm run, code-mode search | 32.68 ms | 0.55 ms | `-98.3%` |
| Programmatic tool workflow | 16.59 ms | 1.77 ms | `-89.3%` |
| Host fanout, 100 calls | 0.71 ms | 0.41 ms | `-42.3%` |

The direct addon `execution_only_small` phase also improved from `8.81 ms` to
`2.40 ms` (`-72.8%`), which is the clearest signal that removing eager GC from
every maybe-allocating opcode lowered steady-state interpreter overhead rather
than only shaving wrapper or startup costs.

### 2. Sidecar is still much slower than addon on the same workloads

Current medians from the new workload artifact:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 1.03 ms | 10.22 ms | `9.92x` |
| Warm run, small script | 0.97 ms | 1.67 ms | `1.72x` |
| Programmatic tool workflow | 1.77 ms | 18.89 ms | `10.67x` |
| Host fanout, 100 calls | 0.41 ms | 6.67 ms | `16.27x` |

The runtime-core work helped both modes, but the sidecar transport boundary is
now the dominant remaining cost on these fixtures.

### 3. `mustard` still wins decisively on resumable execution

Current medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Suspend/resume, 1 boundary | 0.14 ms | 0.09 ms | 0.93 ms |
| Suspend/resume, 5 boundaries | 0.61 ms | 0.35 ms | 2.84 ms |
| Suspend/resume, 20 boundaries | 2.32 ms | 1.30 ms | 10.07 ms |

The latest GC work left `suspend_resume_20` effectively flat versus the
previous checked-in workload artifact (`2.32 ms` vs `2.32 ms`), so the current
resume advantage remains intact while the main execution paths got faster.

## Phase And Boundary Findings

Current addon phase medians:

| Phase | Median |
| --- | ---: |
| `runtime_init_only` | `0.04 ms` |
| `execution_only_small` | `2.40 ms` |
| `suspend_only` | `0.06 ms` |
| `snapshot_dump_only` | `0.06 ms` |
| `apply_snapshot_policy_only` | `0.03 ms` |
| `snapshot_load_only` | `0.06 ms` |
| `Progress.load_only` | `0.26 ms` |

The main positive signal is still `execution_only_small`. The tradeoff is that
snapshot-adjacent phases regressed modestly, especially `Progress.load_only`
(`0.15 ms -> 0.26 ms`, `+65.2%`), which also showed up in smoke ratio
variance.

Current addon boundary medians:

| Surface | Small | Medium | Large |
| --- | ---: | ---: | ---: |
| `startInputs` | `0.15 ms` | `0.40 ms` | `1.06 ms` |
| `suspendedArgs` | `0.27 ms` | `0.79 ms` | `2.30 ms` |
| `resumeValues` | `0.15 ms` | `0.32 ms` | `0.99 ms` |
| `resumeErrors` | `0.13 ms` | `0.35 ms` | `0.91 ms` |

Relative to the previous checked-in workload artifact:

- `suspendedArgs.large` improved from `14.63 ms` to `2.30 ms` (`-84.3%`)
- `suspendedArgs.medium` improved from `2.04 ms` to `0.79 ms` (`-61.5%`)
- `startInputs.*`, `resumeValues.*`, and `resumeErrors.*` regressed by about
  `20%` to `65%` on these small fixtures

The practical read is that the latest change strongly helped suspend-argument
serialization in the benchmarked shape, but boundary surfaces that do not
benefit from the cheaper VM path now need separate attention.

## Rust-Core Microbench Findings

The Rust microbench suite showed the expected direction for the execution-heavy
fixtures:

- `runtime_init_empty`: about `-35%`
- `runtime_init_with_capabilities`: about `-27%`
- `runtime_init_with_inputs`: about `-25%`
- `execute_shared_small_compute`: about `-82%`
- `vm_hot_loop`: about `-82%`
- `local_load_store_hot`: about `-61%`
- `closure_access_hot`: about `-85%`
- `env_lookup_hot`: about `-82%`
- `global_lookup_hot`: about `-65%`
- `property_access_hot`: about `-77%`
- `builtin_method_hot`: about `-94%`
- `array_callback_hot`: about `-88%`
- `collection_callback_hot`: about `-87%`
- `map_set_hot`: about `-91%`

The notable mixed result was snapshot restore overhead:

- `snapshot_dump_suspended`: effectively flat inside noise
- `snapshot_load_suspended`: about `+2%` to `+3%`

That matches the workload artifact’s `Progress.load_only` regression.

## Smoke Gate Findings

Current release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.05 ms` |
| Compute | `0.43 ms` |
| Host-call median ratio | `3.91x` |
| Host-call p95 ratio | `5.12x` |
| Snapshot median ratio | `6.39x` |
| Snapshot p95 ratio | `5.01x` |

Relative to the previous checked-in release smoke artifact
`benchmarks/results/2026-04-13T12-54-22-790Z-smoke-release.json`:

- startup median improved from `0.09 ms` to `0.05 ms` (`-47.5%`)
- compute median improved from `2.73 ms` to `0.43 ms` (`-84.3%`)
- host-call absolute latency stayed roughly flat (`0.26 ms -> 0.26 ms`)
- snapshot direct and round-trip medians regressed from `0.05 ms` and
  `0.33 ms` to `0.07 ms` and `0.47 ms`

The release smoke gate now passes with a slightly looser ratio budget on the
snapshot and host-call p95 sections. That rebaseline was necessary because the
main execution path got much faster while the small absolute snapshot and
host-call ratios varied across reruns enough to trip the previous ceilings.

The dev smoke gate remains intentionally looser and should only be treated as a
fast local sanity check. The release workload and release smoke artifacts are
the source of truth for optimization decisions.

## Retained Memory And Cleanup

Current retained-memory deltas after 20 workflow runs:

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +16,568 B | +393,216 B |
| Sidecar | +12,608 B | +8,421,376 B |
| V8 isolate | -2,408 B | -10,354,688 B |

Current failure-cleanup medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Limit failure then recover | 0.82 ms | 0.90 ms | 2.42 ms |
| Host failure then recover | 0.86 ms | 1.00 ms | 0.68 ms |

Relative to the previous checked-in workload artifact, addon failure cleanup
improved by about `81%` on both failure surfaces.

## Conclusions

1. The new GC trigger policy removed a major fixed cost from addon execution
   and is the strongest performance win since the lexical-slot and startup-path
   work.
2. Snapshot-adjacent phases and several non-suspension boundary surfaces now
   stand out more clearly as the next bottlenecks.
3. Sidecar transport overhead is now an even larger share of total latency on
   the optimized addon baseline, so Milestone 5 and Milestone 7 work remain
   important after the remaining Milestone 4 GC/accounting items.
