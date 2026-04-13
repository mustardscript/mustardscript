# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T16-07-24-551Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T16-06-51-826Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T15-32-43-544Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `1be4e65`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. Cached policy/capability JSON fragments materially reduced addon boundary cost

The latest change caches encoded policy/capability metadata on the Node side
instead of rebuilding the same JSON fragments for every repeated run, restore,
and resume call. Snapshot-key base64/digest data is also reused when an
`ExecutionContext` already owns stable restore metadata.

Relative to the tracked addon baseline
`benchmarks/results/2026-04-13T15-31-34-682Z-workloads.json`, the latest
checked-in workload artifact shows the intended effect on boundary-only work:

| Surface | Baseline | Current | Delta |
| --- | ---: | ---: | ---: |
| `startInputs.medium` | 0.35 ms | 0.27 ms | `-21.2%` |
| `startInputs.large` | 1.07 ms | 0.96 ms | `-10.6%` |
| `resumeValues.small` | 0.14 ms | 0.11 ms | `-18.4%` |
| `resumeValues.medium` | 0.31 ms | 0.24 ms | `-20.0%` |
| `resumeErrors.small` | 0.13 ms | 0.10 ms | `-20.2%` |
| `resumeErrors.large` | 0.91 ms | 0.85 ms | `-6.4%` |

The suspended-args path also moved in the right direction even though this
change did not directly alter its transport:

- `suspendedArgs.medium` improved by `11.1%`
- `suspendedArgs.large` improved by `9.7%`

### 2. Broader addon execution stayed flat-to-better while restore phases got cheaper

Relative to the same tracked workload baseline:

| Workload | Baseline | Current | Delta |
| --- | ---: | ---: | ---: |
| Warm run, small script | 0.97 ms | 0.89 ms | `-8.0%` |
| Warm run, code-mode search | 0.53 ms | 0.50 ms | `-5.5%` |
| Programmatic tool workflow | 1.48 ms | 1.41 ms | `-4.7%` |
| Host fanout, 100 calls | 0.37 ms | 0.37 ms | `+1.2%` |
| Suspend/resume, 20 boundaries | 2.33 ms | 2.32 ms | `-0.6%` |

The clearest secondary wins are in the addon phase split:

| Phase | Baseline | Current | Delta |
| --- | ---: | ---: | ---: |
| `execution_only_small` | 1.99 ms | 1.71 ms | `-14.5%` |
| `apply_snapshot_policy_only` | 0.03 ms | 0.02 ms | `-22.8%` |
| `snapshot_load_only` | 0.05 ms | 0.04 ms | `-22.7%` |
| `Progress.load_only` | 0.21 ms | 0.19 ms | `-8.5%` |
| `snapshot_dump_only` | 0.07 ms | 0.06 ms | `-13.2%` |

### 3. The tracked addon regression gate is down to tiny p95-only surfaces

`npm run bench:regress:workloads` still exits nonzero, but the remaining
regressions are now both tiny addon p95-only surfaces rather than broad median
slowdowns:

- `addon.latency.host_fanout_10` p95: `0.05 ms -> 0.05 ms` (`+10.1%`)
- `addon.latency.suspend_resume_1` p95: `0.15 ms -> 0.18 ms` (`+17.2%`)

Every boundary-only median and every major execution-path median in the tracked
addon report improved or stayed within noise.

### 4. Smoke gates still pass comfortably

Current release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.05 ms` |
| Compute | `0.44 ms` |
| Host-call median ratio | `3.73x` |
| Host-call p95 ratio | `4.12x` |
| Snapshot median ratio | `6.27x` |
| Snapshot p95 ratio | `5.15x` |

Relative to the previous checked-in release smoke artifact
`benchmarks/results/2026-04-13T15-31-19-504Z-smoke-release.json`:

- startup median improved by `14.3%`
- host-call ratio medians improved from `4.53x` to `3.73x`
- snapshot round-trip median improved by `10.6%`
- compute moved slower (`0.38 ms -> 0.44 ms`) but still stayed far inside the
  release smoke budgets

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
| Warm run, small script | 0.89 ms | 1.41 ms | `1.59x` |
| Programmatic tool workflow | 1.41 ms | 19.17 ms | `13.56x` |
| Host fanout, 100 calls | 0.37 ms | 6.62 ms | `17.96x` |

The shared Rust core got cheaper to execute, but sidecar mode is still mostly a
transport and session-state problem.

## Conclusions

1. Caching encoded policy/capability metadata on the Node side produced clear
   wins on the addon boundary-only start/resume surfaces without weakening the
   Rust-owned validation boundary.
2. The latest checked-in workload artifact also improved `warm_run_small`,
   `programmatic_tool_workflow`, `execution_only_small`, `snapshot_load_only`,
   and `Progress.load_only`, while keeping `host_fanout_100` and
   `suspend_resume_20` effectively flat.
3. Release and dev smoke gates still pass. The remaining tracked workload gate
   failure is now down to two tiny addon p95-only surfaces
   (`host_fanout_10` and `suspend_resume_1`) rather than a broad addon
   regression.
4. The next open performance work should keep pushing Milestone 5 boundary
   costs and then move into the still-open runtime-wide items: deeper
   string/key interning, remaining `Map` / `Set` accounting deltas, and sidecar
   transport/session-state overhead.
