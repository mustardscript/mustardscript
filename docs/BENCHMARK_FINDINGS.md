# Benchmark Findings

This document summarizes the latest checked-in benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-13T19-39-37-535Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T19-43-14-945Z-smoke-release.json`
- dev smoke suite: `benchmarks/results/2026-04-13T19-43-15-041Z-smoke-dev.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in artifacts: `312c953`
- Workload fixture version: `5`
- Smoke fixture version: `2`

## Headline Results

### 1. Sidecar workload artifacts now split startup, hot execution, and transport-only cost

The workload runner now records sidecar phase metrics alongside the existing
addon and sidecar latency tables:

| Sidecar Phase | Median | p95 |
| --- | ---: | ---: |
| `startup_only` | `8.80 ms` | `277.92 ms` |
| `execution_only_small` | `1.42 ms` | `1.56 ms` |
| `transport_resume_only` | `0.12 ms` | `0.14 ms` |

This separates the major sidecar cost buckets cleanly:

- cold sidecar latency is still dominated by process startup variance
- warm small-script latency is now directly attributable to hot sidecar
  execution (`1.42 ms` median)
- the protocol-only resume round trip for a tiny suspended snapshot is much
  smaller (`0.12 ms` median), so the largest remaining sidecar gap is not just
  raw request framing on trivial resumes

### 2. The latest workload artifact keeps the same overall sidecar shape, but now explains it better

Selected current release medians:

| Workload | Addon | Sidecar | Ratio |
| --- | ---: | ---: | ---: |
| `warm_run_small` | `0.97 ms` | `1.42 ms` | `1.46x` |
| `programmatic_tool_workflow` | `1.54 ms` | `18.91 ms` | `12.25x` |
| `host_fanout_100` | `0.38 ms` | `6.76 ms` | `17.74x` |
| `suspend_resume_20` | `2.29 ms` | `1.33 ms` | `0.58x` |

The new phase split changes the diagnosis more than the totals:

- `warm_run_small` is now easy to read as "mostly hot execution in a reused
  sidecar session"
- `programmatic_tool_workflow` and `host_fanout_100` are still dominated by
  repeated protocol crossings and host-boundary churn rather than one-time
  startup alone
- sidecar still keeps its existing advantage on resumable execution

### 3. Smoke budgets still pass on the refreshed artifact set

Release smoke medians:

| Metric | Current |
| --- | ---: |
| Startup | `0.03 ms` |
| Compute | `0.20 ms` |
| Host-call median ratio | `4.42x` |
| Host-call p95 ratio | `4.83x` |
| Snapshot median ratio | `7.48x` |
| Snapshot p95 ratio | `5.74x` |

Dev smoke also stayed inside budget with startup `0.12 ms`, compute
`1.64 ms`, host-call median ratio `3.71x`, and snapshot median ratio `9.25x`.
The release snapshot median ratio passed narrowly under the current `7.5x`
budget, so snapshot-heavy smoke noise remains something to watch when later
runtime work touches suspend/resume internals again.

## Conclusions

1. The Milestone 7 measurement item is now closed: sidecar startup, warm
   execution, and minimal transport costs are benchmarked separately and are
   documented in the checked-in workload artifact.
2. The new evidence says the next sidecar wins should target protocol shape and
   state reuse, not guest execution itself. `transport_resume_only` is cheap on
   a tiny replay, while workflow and large host-fanout paths are still far from
   the addon baseline.
3. The remaining sidecar milestone work is now clearer: binary framing,
   binary program/snapshot transport, cached `program_id` / `snapshot_id`
   session state, and protocol hardening are the likely next sources of
   material improvement.
