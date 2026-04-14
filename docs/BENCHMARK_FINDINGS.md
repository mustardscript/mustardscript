---
title: "Benchmark Findings"
description: "Performance metrics, optimization results, and latency measurements"
category: "Development"
order: 2
slug: "benchmark-findings"
lastUpdated: "2026-04-14"
---

# Benchmark Findings

This document summarizes the latest kept benchmark evidence from:

- workload suite: `benchmarks/results/2026-04-14T00-59-31-034Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in workload artifact: `9ab8b16`
- Workload fixture version: `6`
- Smoke fixture version: `2`

## Headline Results

### 1. Rust-core promise benches improved, but representative addon PTC latency stayed effectively flat

The latest kept representative workload artifact still leaves the addon
headline essentially unchanged:

| Metric | Median | p95 |
| --- | ---: | ---: |
| `ptc_website_demo_small` | `0.16 ms` | `0.17 ms` |
| `ptc_incident_triage_medium` | `0.59 ms` | `0.60 ms` |
| `ptc_fraud_investigation_medium` | `1.69 ms` | `1.80 ms` |
| `ptc_vendor_review_medium` | `0.21 ms` | `0.21 ms` |
| `addon.ptc.weightedScore.medium` | `0.88 ms` | `0.93 ms` |

The old synthetic control metric is still present:

| Metric | Median | p95 |
| --- | ---: | ---: |
| `programmatic_tool_workflow` | `1.45 ms` | `1.49 ms` |

That control remains useful for continuity, but it should no longer be treated
as the primary “real-world PTC” number. The weighted medium-lane score is now
the better addon optimization target.

Compared with the immediately previous kept representative artifact
`benchmarks/results/2026-04-14T00-42-49-648Z-workloads.json`, the primary
addon medians remained effectively flat:

- `addon.ptc.weightedScore.medium`: `0.87 ms -> 0.88 ms` (`+1.2%`)
- `addon.latency.ptc_incident_triage_medium`: `0.59 ms -> 0.59 ms` (`+0.8%`)
- `addon.latency.ptc_fraud_investigation_medium`: `1.68 ms -> 1.69 ms` (`+1.1%`)
- `addon.latency.ptc_vendor_review_medium`: `0.20 ms -> 0.21 ms` (`+3.5%`)
- `addon.latency.ptc_website_demo_small`: `0.16 ms -> 0.16 ms` (`+3.6%`)

That means this slice did not yet produce the representative medium-lane win
required to call Milestone 1 complete, even though the core async benches moved
in the right direction.

### 2. Primary PTC lanes still expose queued/executed microtask pressure directly

The addon runtime counters now include queued/executed microtask totals plus
resume/combinator breakdowns for the three primary medium lanes:

| Lane | Queued Microtasks | Peak Queue | Resume Jobs | Promise Reactions | Promise Combinators |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ptc_incident_triage_medium` | `24` | `1` | `1` | `0` | `23` |
| `ptc_fraud_investigation_medium` | `7` | `1` | `3` | `0` | `4` |
| `ptc_vendor_review_medium` | `6` | `1` | `2` | `0` | `4` |

This matters because the benchmark can now distinguish lanes dominated by
queued combinator work from lanes dominated by resumed async continuations
instead of treating “Promise-heavy” as a single opaque bucket.

### 3. The representative suite still exposes how much intermediate data stays inside the sandbox

Addon transfer summaries from the kept workload artifact:

| Lane | Calls | Tool Bytes In | Result Bytes Out | Reduction |
| --- | ---: | ---: | ---: | ---: |
| `ptc_website_demo_small` | `6` | `1109 B` | `227 B` | `4.89x` |
| `ptc_incident_triage_medium` | `20` | `4340 B` | `2463 B` | `1.76x` |
| `ptc_fraud_investigation_medium` | `6` | `26818 B` | `17579 B` | `1.53x` |
| `ptc_vendor_review_medium` | `5` | `2639 B` | `967 B` | `2.73x` |

This is the most important improvement in the benchmark itself: it now makes it
visible when `mustard` is reducing tool payloads locally instead of just
reporting one total latency number.

### 4. Cached promise accounting removed expensive full-driver walks in the Rust-core async benches

`npm run bench:rust` was rerun in this slice alongside the workload suite, and
the promise-heavy core benches improved after settlement/combinator accounting
switched from full promise/driver remeasurement to cached accounted-byte reuse:

- `runtime_execution/promise_all_immediate_fanout` now runs at about
  `0.392 ms -> 0.398 ms`, with Criterion reporting roughly `51%` to `52%`
  improvement versus the previous baseline.
- `runtime_execution/promise_all_settled_immediate` now runs at about
  `3.41 ms -> 3.47 ms`, about `4%` to `5%` faster.
- `runtime_execution/promise_all_derived_ids_fanout` now runs at about
  `6.30 ms -> 6.36 ms`, about `1%` to `3%` faster.
- `runtime_execution/promise_all_map_set_reduction` now runs at about
  `8.58 ms -> 8.67 ms`, about `1%` to `3%` faster.

The immediate-fanout win is real at the core-runtime level, but the kept
representative PTC artifact shows that those savings are still being diluted by
other costs in the full addon path.

### 5. Sidecar is still materially slower than addon on representative medium lanes

Current weighted medium-lane medians:

| Runtime | Median | p95 |
| --- | ---: | ---: |
| addon | `0.88 ms` | `0.93 ms` |
| sidecar | `2.66 ms` | `3.01 ms` |
| isolate | `0.39 ms` | `0.39 ms` |

Current weighted-score ratios:

| Ratio | Value |
| --- | ---: |
| `sidecar / addon` | `3.02x` |
| `isolate / addon` | `0.44x` |

The key point from this slice is not a new sidecar win. It is that core async
bookkeeping got cheaper while the representative medium-lane score stayed
effectively flat, which pushes the next likely win toward boundary transport
and local reduction costs rather than more bookkeeping-only cleanup.

### 6. Smoke was not the focus of this slice

This change set focused on cached promise-accounting reuse, reran the Rust-core
async benches, and refreshed representative workload evidence. The kept release
smoke artifact remains:

- `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Release smoke was not rerun as part of this representative-PTC benchmark
attribution slice because the smoke suite itself was not modified. The broader
repo verification still passed:

- `npm test`
- `npm run lint`
- `cargo test --workspace`
- `npm run test:use-cases`
- `npm run bench:rust`
- `npm run bench:workloads:release`

## Conclusions

1. `addon.ptc.weightedScore.medium` is still the right real-world signal for
   addon optimization work, and this slice kept it effectively flat rather than
   delivering a representative win.
2. Cached promise-accounting reuse clearly improved the Rust-core
   `Promise.all` benches, especially the immediate-fanout path, so repeated
   full-driver remeasurement is no longer the dominant cost there.
3. The primary representative lanes still show the same combinator-heavy async
   shapes, but the flat weighted score says those async bookkeeping savings are
   not yet the main end-to-end bottleneck.
4. Sidecar remains the largest remaining performance gap on representative
   medium lanes.
5. The next highest-value work is likely the unfinished addon boundary
   transport path and/or the local reduction primitives used most heavily by
   the fraud and incident lanes.
