# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T16-35-07-798Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T16-34-56-941Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T16-35-03-175Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `c52b26a`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. Native execution-context handles now keep policy and capability state in the addon

The latest addon change promotes JS `ExecutionContext` reuse into explicit
native execution-context handles. Repeated `run()`, `start()`, and
`Progress.load()` calls now keep the parsed capability list and limits in the
addon instead of re-sending and reparsing the same policy JSON on every call.

Regression coverage now proves one `ExecutionContext` reuses a single native
handle across repeated starts and detached-snapshot loads.

### 2. Boundary-heavy addon medians improved across repeated start/load paths

Relative to the tracked addon workload baseline
`benchmarks/results/2026-04-13T16-21-44-968Z-workloads.json`, the latest
checked-in workload artifact shows the intended boundary-side wins:

| Workload | Baseline | Current | Delta |
| --- | ---: | ---: | ---: |
| Warm run, small script | 0.90 ms | 0.88 ms | `-2.9%` |
| Warm run, code-mode search | 0.50 ms | 0.49 ms | `-2.1%` |
| Programmatic tool workflow | 1.47 ms | 1.38 ms | `-6.1%` |
| Host fanout, 100 calls | 0.39 ms | 0.39 ms | `-0.4%` |
| Cold start, small script | 0.98 ms | 0.95 ms | `-2.7%` |

The clearest direct signals are on the boundary-only and public restore paths:

| Surface | Baseline | Current | Delta |
| --- | ---: | ---: | ---: |
| `startInputs.small` | 0.14 ms | 0.13 ms | `-7.3%` |
| `startInputs.medium` | 0.28 ms | 0.25 ms | `-10.1%` |
| `resumeValues.small` | 0.13 ms | 0.12 ms | `-10.1%` |
| `resumeErrors.small` | 0.11 ms | 0.10 ms | `-10.2%` |
| `Progress.load_only` | 0.19 ms | 0.18 ms | `-1.8%` |

The boundary wins did not materially move `host_fanout_100`, which is expected:
that workload is dominated by the remaining JS structured-value DTO path and
the still-open typed/binary boundary work.

### 3. Rust-core microbenches stayed mostly flat, which matches the scope of the change

`npm run bench:rust` stayed effectively flat on the core VM and startup benches,
which is the expected outcome for a mostly addon-boundary optimization. A few
non-targeted benches moved within noise, while the hot runtime loops and lookup
paths showed no meaningful regressions.

That separation is useful evidence: the win came from caching addon execution
context metadata, not from changing Rust guest semantics or the core VM.

### 4. Release smoke budgets still pass, but the relative smoke regression gate is noisy

Current release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.06 ms` |
| Compute | `0.44 ms` |
| Host-call median ratio | `4.48x` |
| Host-call p95 ratio | `5.42x` |
| Snapshot median ratio | `6.25x` |
| Snapshot p95 ratio | `2.50x` |

Relative to the previous tracked release smoke artifact
`benchmarks/results/2026-04-13T16-22-10-011Z-smoke-release.json`:

- the smoke budgets still pass comfortably
- `npm run bench:regress:smoke` currently exits nonzero on tiny-sample p95-only
  noise, especially `metrics.snapshot.direct` p95 (`0.11 ms -> 0.23 ms`)
- the public smoke gate remains useful, but the relative regression command is
  currently noisier than the workload suite on these sub-millisecond samples

## Sidecar And Gate Notes

Current sidecar/addon ratios from the new workload artifact remain dominated by
transport and session-state overhead:

| Workload | Addon | Sidecar | Sidecar / Addon |
| --- | ---: | ---: | ---: |
| Warm run, small script | 0.88 ms | 1.50 ms | `1.70x` |
| Programmatic tool workflow | 1.38 ms | 21.45 ms | `15.49x` |
| Host fanout, 100 calls | 0.39 ms | 7.28 ms | `18.67x` |

`npm run bench:regress:workloads` still exits nonzero, but the remaining issue
has narrowed to a small p95-only phase surface:

- `addon.phases.apply_snapshot_policy_only` p95: `0.04 ms -> 0.04 ms`
  (`+20.8%` on tiny absolute timings)

## Conclusions

1. Native execution-context handles are now real addon state, not just JS-side
   caching, and repeated `ExecutionContext` workloads no longer reparse the
   same capability/limit policy on every start or load.
2. The main addon gains landed where expected: repeated start/load boundary
   medians improved, `programmatic_tool_workflow` improved by about `6.1%`, and
   `warm_run_small` improved by about `2.9%`.
3. The remaining open Milestone 5 work is still the typed/binary structured
   boundary path; `host_fanout_100` stayed effectively flat because the JS DTO
   path still dominates there.
4. The next concrete paths remain the still-open boundary/runtime-wide items:
   typed or binary start/resume payloads, deeper string/key interning, and then
   later sidecar/session-state reductions.
