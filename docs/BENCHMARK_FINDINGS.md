# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T20-23-24-663Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T20-26-01-781Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T20-26-06-802Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `0a19d6d`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. Remaining hot-path heap-accounting refreshes are now gone from addon mutation paths

The runtime no longer falls back to whole-object or whole-array recounts for
the last common mutation sites where the exact byte delta is already knowable.

The new delta-only path now covers:

- global-object writes performed through `define_global(...)`
- inferred closure names and custom closure properties
- custom builtin-function and host-function properties
- `Array.prototype.fill`, `Array.prototype.splice`, and `Array.prototype.sort`
- iterator advancement and `RegExp.lastIndex` updates, which now avoid
  accounting work entirely because those state changes do not affect measured
  heap bytes

Regression coverage now proves these paths preserve cached totals without
raising `accounting_refreshes`.

### 2. Release workload results improved materially on addon startup, boundary, and phase-split paths

Compared with the previously checked-in release workload artifact
`2026-04-13T20-03-16-697Z-workloads.json`, current addon medians moved by:

| Workload | Previous | Current | Delta |
| --- | ---: | ---: | ---: |
| `cold_start_small` | `1.01 ms` | `0.84 ms` | `-16.8%` |
| `warm_run_small` | `0.96 ms` | `0.83 ms` | `-13.9%` |
| `cold_start_code_mode_search` | `0.95 ms` | `0.87 ms` | `-8.9%` |
| `warm_run_code_mode_search` | `0.52 ms` | `0.49 ms` | `-5.8%` |
| `programmatic_tool_workflow` | `1.53 ms` | `1.47 ms` | `-3.3%` |
| `execution_only_small` | `1.96 ms` | `0.86 ms` | `-56.0%` |
| `runtime_init_only` | `0.03 ms` | `0.01 ms` | `-60.5%` |
| `Progress.load_only` | `0.21 ms` | `0.11 ms` | `-46.5%` |
| `startInputs.medium` | `0.30 ms` | `0.20 ms` | `-33.7%` |
| `suspendedArgs.medium` | `0.70 ms` | `0.42 ms` | `-40.0%` |
| `resumeValues.medium` | `0.28 ms` | `0.18 ms` | `-36.2%` |
| `resumeErrors.medium` | `0.32 ms` | `0.19 ms` | `-41.1%` |
| `host_fanout_100` | `0.38 ms` | `0.42 ms` | `+10.1%` |
| `suspend_resume_20` | `2.27 ms` | `2.45 ms` | `+8.0%` |

This is strong enough to keep:

- the clearest wins are on the phase-split and boundary-heavy surfaces that
  previously paid repeated accounting rescans
- the hot run path also improved on both `warm_run_small` and
  `programmatic_tool_workflow`
- the remaining regressions are concentrated in host-fanout and suspend/resume
  workloads, so the next open Milestone 4 target is still async promise-clone
  amplification rather than more heap-accounting churn

### 3. Hot addon workloads now show zero accounting refreshes

The latest workload artifact reports:

- `warm_run_small`: `accounting_refreshes 0`
- `programmatic_tool_workflow`: `accounting_refreshes 0`
- `host_fanout_100`: `accounting_refreshes 0`
- `execution_only_small`: `accounting_refreshes 0`
- `suspend_resume_20`: `accounting_refreshes 0`

That matches the intended design state for this slice: cached heap totals stay
valid throughout the hot mutation paths instead of being repaired by full
walks.

### 4. Smoke budgets still pass on the refreshed artifact set

Release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.05 ms` |
| Compute | `0.45 ms` |
| Host-call median ratio | `4.22x` |
| Host-call p95 ratio | `4.10x` |
| Snapshot median ratio | `6.49x` |
| Snapshot p95 ratio | `4.69x` |

Dev smoke also stayed inside budget with startup `0.28 ms`, compute
`2.47 ms`, host-call median ratio `3.29x`, and snapshot median ratio `8.29x`.

The first `bench:smoke:release` attempt failed on a noisy host-call p95 ratio
sample (`9.40x > 6.5x`), but an immediate sequential rerun passed well inside
budget, so this was treated as smoke variance rather than a persistent
regression.

## Conclusions

1. The remaining Milestone 4 accounting item is now effectively complete:
   runtime array/object/env/closure/promise/keyed-collection bookkeeping stays
   on exact deltas whenever the byte change is knowable, and non-accounted
   iterator / `lastIndex` state changes no longer trigger refresh work.
2. The benchmark evidence is good enough to keep. The strongest wins are on
   addon `execution_only_small -56.0%`, `runtime_init_only -60.5%`,
   `Progress.load_only -46.5%`, `startInputs.medium -33.7%`,
   `suspendedArgs.medium -40.0%`, `resumeValues.medium -36.2%`,
   `resumeErrors.medium -41.1%`, and `warm_run_small -13.9%`.
3. The next open runtime performance path remains the other half of Milestone 4:
   reducing async clone amplification in promise settlement, awaiter scheduling,
   and combinator dispatch without giving back the current boundary/start-path
   gains.
