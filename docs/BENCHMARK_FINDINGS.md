# Benchmark Findings

This document summarizes the latest checked-in release benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T13-38-06-436Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T13-38-09-400Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T13-38-19-895Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `b55f15c`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. Removing post-sweep full recounts shaved GC-adjacent fixed costs

Relative to the previous checked-in workload artifact
`benchmarks/results/2026-04-13T13-20-51-960Z-workloads.json`, the latest addon
medians moved modestly but consistently in the right direction on the main
execution paths:

| Workload | Previous | Current | Delta |
| --- | ---: | ---: | ---: |
| Cold start, small script | 1.03 ms | 1.02 ms | `-0.8%` |
| Warm run, small script | 0.97 ms | 0.96 ms | `-1.7%` |
| Warm run, code-mode search | 0.55 ms | 0.54 ms | `-1.4%` |
| Programmatic tool workflow | 1.77 ms | 1.69 ms | `-4.7%` |
| Host fanout, 100 calls | 0.41 ms | 0.39 ms | `-4.3%` |
| Suspend/resume, 20 boundaries | 2.32 ms | 2.23 ms | `-3.5%` |

The phase-level signal is that the GC-adjacent slices stopped giving back time:
`runtime_init_only` improved from `0.04 ms` to `0.03 ms` (`-2.7%` median,
`-26.9%` p95), `Progress.load_only` improved from `0.26 ms` to `0.25 ms`
(`-3.1%`), and `snapshot_load_only` improved from `0.06 ms` to `0.06 ms`
(`-4.4%`) instead of regressing.

### 2. Sidecar is still much slower than addon on the same workloads

Current medians from the new workload artifact:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Cold start, small script | 1.02 ms | 10.70 ms | `10.49x` |
| Warm run, small script | 0.96 ms | 1.70 ms | `1.77x` |
| Programmatic tool workflow | 1.69 ms | 18.84 ms | `11.15x` |
| Host fanout, 100 calls | 0.39 ms | 6.65 ms | `17.05x` |

The runtime-core work helped both modes, but the sidecar transport boundary is
now the dominant remaining cost on these fixtures.

### 3. `mustard` still wins decisively on resumable execution

Current medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Suspend/resume, 1 boundary | 0.15 ms | 0.10 ms | 0.97 ms |
| Suspend/resume, 5 boundaries | 0.62 ms | 0.34 ms | 2.88 ms |
| Suspend/resume, 20 boundaries | 2.23 ms | 1.28 ms | 10.37 ms |

The current resume advantage remains intact, and the `20`-boundary case
improved slightly instead of staying flat.

## Phase And Boundary Findings

Current addon phase medians:

| Phase | Median |
| --- | ---: |
| `runtime_init_only` | `0.03 ms` |
| `execution_only_small` | `2.39 ms` |
| `suspend_only` | `0.05 ms` |
| `snapshot_dump_only` | `0.06 ms` |
| `apply_snapshot_policy_only` | `0.03 ms` |
| `snapshot_load_only` | `0.06 ms` |
| `Progress.load_only` | `0.25 ms` |

The main positive signal is that the snapshot-adjacent phases that regressed
after the GC-trigger change no longer regress here. `snapshot_dump_only`,
`snapshot_load_only`, and `Progress.load_only` are all flat to slightly better
than the previous checked-in artifact.

Current addon boundary medians:

| Surface | Small | Medium | Large |
| --- | ---: | ---: | ---: |
| `startInputs` | `0.15 ms` | `0.39 ms` | `1.07 ms` |
| `suspendedArgs` | `0.27 ms` | `0.79 ms` | `2.24 ms` |
| `resumeValues` | `0.14 ms` | `0.32 ms` | `0.96 ms` |
| `resumeErrors` | `0.13 ms` | `0.35 ms` | `0.91 ms` |

Relative to the previous checked-in workload artifact:

- most boundary medians improved by about `1%` to `5%`
- `suspendedArgs.large` improved from `2.30 ms` to `2.24 ms` (`-2.6%`)
- the main remaining outlier is `suspendedArgs.medium` p95, which moved from
  `1.01 ms` to `1.15 ms` (`+14.0%`)

The practical read is that removing the post-sweep recount did not reopen the
boundary regressions from the prior milestone and modestly improved several
host-heavy surfaces, but the medium suspended-argument p95 path still needs
separate attention.

## Rust-Core Microbench Findings

This rerun of the Rust microbench suite was mostly flat to slightly noisy on
execution-heavy fixtures, which is expected because the current microbench set
does not isolate GC sweep cost directly. The one clear positive signal was:

- `collection_callback_hot`: about `-4%`

Most other runtime-core benches stayed inside noise bands or low-single-digit
movement. That matches the shape of the change: cheaper cleanup work after
collection, not a new dispatch or lowering optimization.

## Smoke Gate Findings

Current release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.04 ms` |
| Compute | `0.43 ms` |
| Host-call median ratio | `4.01x` |
| Host-call p95 ratio | `5.14x` |
| Snapshot median ratio | `7.42x` |
| Snapshot p95 ratio | `6.35x` |

Relative to the previous checked-in release smoke artifact
`benchmarks/results/2026-04-13T13-22-53-049Z-smoke-release.json`:

- startup median improved from `0.05 ms` to `0.04 ms` (`-13.2%`)
- compute median stayed effectively flat (`0.43 ms -> 0.43 ms`, `-0.4%`)
- host-call median improved from `0.26 ms` to `0.22 ms` (`-13.7%`)
- direct snapshot median improved from `0.07 ms` to `0.06 ms` (`-14.0%`)
- snapshot round-trip median stayed flat (`0.47 ms -> 0.47 ms`) while p95 rose
  modestly (`+7.9%`)

The release smoke gate still passes. The direct host-call and direct snapshot
surfaces both improved, but the round-trip snapshot p95 is still noisy enough
that this area should stay on the watch list.

The dev smoke gate remains intentionally looser and should only be treated as a
fast local sanity check. The release workload and release smoke artifacts are
the source of truth for optimization decisions.

## Retained Memory And Cleanup

Current retained-memory deltas after 20 workflow runs:

| Runtime | Heap delta | RSS delta |
| --- | ---: | ---: |
| Addon | +16,024 B | +442,368 B |
| Sidecar | +13,264 B | +8,732,672 B |
| V8 isolate | -2,856 B | -10,403,840 B |

Current failure-cleanup medians:

| Workload | Addon | Sidecar | V8 isolate |
| --- | ---: | ---: | ---: |
| Limit failure then recover | 0.80 ms | 0.91 ms | 2.42 ms |
| Host failure then recover | 0.84 ms | 1.05 ms | 0.68 ms |

Relative to the previous checked-in workload artifact, addon failure cleanup
improved slightly on both failure surfaces (`-1.6%` and `-2.8%`).

## Conclusions

1. Removing the post-sweep full recount completed another concrete Milestone 4
   GC/accounting item and produced small but consistent wins on addon
   workloads, smoke startup, host-call latency, and direct snapshot costs.
2. Snapshot-adjacent medians are no longer the obvious regression bucket, but
   medium suspended-argument p95 and snapshot round-trip p95 still need
   attention.
3. Sidecar transport overhead remains the dominant gap once addon execution is
   this cheap, so Milestone 5 and Milestone 7 work are still the biggest
   remaining latency opportunities.
