# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T16-21-44-968Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T16-22-10-011Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T15-32-43-544Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `574eb03`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. `Map` / `Set` accounting now stays on exact delta updates

The latest runtime change removes full `Map` / `Set` accounting refreshes from
hot `set` / `add` / `delete` / `clear` mutations and replaces them with exact
byte deltas plus regression coverage that the cached totals still match a full
heap walk.

The direct Rust-core signal is the targeted `map_set_hot` microbench from
`npm run bench:rust`, which improved by about `8.2%` in the latest rerun.

### 2. Broad addon workloads stayed mostly flat, but did not turn that microbench win into a broad latency gain

Relative to the tracked addon workload baseline
`benchmarks/results/2026-04-13T16-07-24-551Z-workloads.json`, the latest
checked-in workload artifact is mixed:

| Workload | Baseline | Current | Delta |
| --- | ---: | ---: | ---: |
| Warm run, small script | 0.89 ms | 0.90 ms | `+1.4%` |
| Warm run, code-mode search | 0.50 ms | 0.50 ms | `+0.0%` |
| Programmatic tool workflow | 1.41 ms | 1.47 ms | `+3.9%` |
| Host fanout, 100 calls | 0.37 ms | 0.39 ms | `+4.5%` |
| Suspend/resume, 20 boundaries | 2.32 ms | 2.35 ms | `+1.4%` |

The phase-split numbers were similarly close to flat overall:

| Phase | Baseline | Current | Delta |
| --- | ---: | ---: | ---: |
| `runtime_init_only` | 0.03 ms | 0.03 ms | `+16.1%` |
| `execution_only_small` | 1.71 ms | 1.70 ms | `-0.4%` |
| `snapshot_load_only` | 0.04 ms | 0.04 ms | `+3.5%` |
| `Progress.load_only` | 0.19 ms | 0.19 ms | `-1.1%` |

This is still useful evidence: the `Map` / `Set` accounting change is
correct and makes the targeted keyed-collection mutation path cheaper, but it
did not materially improve the main addon workloads on its own.

### 3. The tracked workload regression gate still fails on small boundary-only surfaces

`npm run bench:regress:workloads` still exits nonzero against the tracked
baseline because several addon boundary metrics remain above the configured
`10%` threshold:

- `addon.boundary.resumeErrors.small`: `0.10 ms -> 0.11 ms` median (`+10.1%`),
  `0.10 ms -> 0.13 ms` p95 (`+24.4%`)
- `addon.boundary.resumeValues.medium`: `0.24 ms -> 0.27 ms` median (`+12.3%`),
  `0.26 ms -> 0.31 ms` p95 (`+18.3%`)
- `addon.boundary.resumeValues.small`: `0.11 ms -> 0.13 ms` median (`+16.8%`)
- `addon.phases.execution_only_small`: `1.71 ms -> 1.89 ms` p95 (`+10.3%`)

Those regressions are not on the keyed-collection mutation path that changed in
this iteration, so the next performance chunk should continue on the still-open
boundary/runtime-wide items instead of treating this accounting change as a
complete workload win.

### 4. Smoke gates still pass comfortably

Current release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.05 ms` |
| Compute | `0.38 ms` |
| Host-call median ratio | `4.44x` |
| Host-call p95 ratio | `5.00x` |
| Snapshot median ratio | `6.63x` |
| Snapshot p95 ratio | `5.32x` |

Relative to the previous tracked release smoke artifact
`benchmarks/results/2026-04-13T16-06-51-826Z-smoke-release.json`:

- compute median improved by `14.9%`
- host-call median ratio improved by `14.2%`
- snapshot round-trip median moved slightly slower (`+2.7%`) but remained well
  inside the release smoke budgets
- startup median moved slightly slower (`+5.5%`) but also stayed far inside the
  release smoke budgets

The smoke regression gate still passes.

## Sidecar And Microbench Notes

Current sidecar/addon ratios from the new workload artifact:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Warm run, small script | 0.90 ms | 1.44 ms | `1.61x` |
| Programmatic tool workflow | 1.47 ms | 18.77 ms | `12.76x` |
| Host fanout, 100 calls | 0.39 ms | 7.55 ms | `19.31x` |

Sidecar remains dominated by transport and session-state overhead rather than
the keyed-collection accounting path.

## Conclusions

1. The runtime now applies exact accounting deltas for hot `Map` / `Set`
   mutations, and the targeted Rust microbench confirms that this path got
   cheaper.
2. The broader addon workloads did not materially improve from this change
   alone, so the remaining performance plan items should stay open.
3. Release smoke still passes, but the tracked workload regression gate still
   fails on several small boundary-only resume/start surfaces.
4. The next concrete paths should stay on the still-open runtime-wide and
   boundary-heavy work: deeper string/key interning, the remaining boundary
   transport optimizations, and later sidecar/session-state reductions.
