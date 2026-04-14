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

- workload suite: `benchmarks/results/2026-04-14T00-42-49-648Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in workload artifact: `a14fb52`
- Workload fixture version: `6`
- Smoke fixture version: `2`

## Headline Results

### 1. Addon weighted PTC latency stayed effectively flat while the benchmark gained async attribution

The latest kept representative workload artifact keeps the addon headline nearly
unchanged:

| Metric | Median | p95 |
| --- | ---: | ---: |
| `ptc_website_demo_small` | `0.16 ms` | `0.17 ms` |
| `ptc_incident_triage_medium` | `0.59 ms` | `0.60 ms` |
| `ptc_fraud_investigation_medium` | `1.68 ms` | `1.73 ms` |
| `ptc_vendor_review_medium` | `0.20 ms` | `0.21 ms` |
| `addon.ptc.weightedScore.medium` | `0.87 ms` | `0.90 ms` |

The old synthetic control metric is still present:

| Metric | Median | p95 |
| --- | ---: | ---: |
| `programmatic_tool_workflow` | `1.44 ms` | `1.49 ms` |

That control remains useful for continuity, but it should no longer be treated
as the primary “real-world PTC” number. The weighted medium-lane score is now
the better addon optimization target.

Compared with the immediately previous kept representative artifact
`benchmarks/results/2026-04-14T00-14-51-582Z-workloads.json`, the main
representative medians stayed effectively flat while this slice added
attribution:

- `addon.ptc.weightedScore.medium`: `0.88 ms -> 0.87 ms` (`-0.9%`)
- `addon.latency.ptc_incident_triage_medium`: `0.59 ms -> 0.59 ms` (`-0.3%`)
- `addon.latency.ptc_fraud_investigation_medium`: `1.70 ms -> 1.68 ms` (`-1.2%`)
- `addon.latency.ptc_vendor_review_medium`: `0.20 ms -> 0.20 ms` (`+0.1%`)

### 2. Primary PTC lanes now expose queued/executed microtask pressure directly

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

### 4. The Rust-core suite now covers the missing PTC async shapes

Current weighted medium-lane medians:

- existing immediate fanout coverage remains in
  `runtime_execution/promise_all_immediate_fanout`
- mixed fulfilled/rejected fanout remains in
  `runtime_execution/promise_all_settled_immediate`
- the suite now also covers
  `runtime_execution/promise_all_derived_ids_fanout`
- and
  `runtime_execution/promise_all_map_set_reduction`

`npm run bench:rust` was rerun in this slice alongside the workload suite, so
future async-fanout optimization work now has PTC-shaped core benches instead
of only the smaller immediate-fanout control.

### 5. Sidecar is still materially slower than addon on representative medium lanes

Current weighted medium-lane medians:

| Runtime | Median | p95 |
| --- | ---: | ---: |
| addon | `0.87 ms` | `0.90 ms` |
| sidecar | `2.66 ms` | `2.98 ms` |
| isolate | `0.41 ms` | `0.43 ms` |

Current weighted-score ratios:

| Ratio | Value |
| --- | ---: |
| `sidecar / addon` | `3.04x` |
| `isolate / addon` | `0.47x` |

The key point from this slice is not a new sidecar win. It is that the
benchmark can now attribute async work precisely while keeping the existing
sidecar/addon gap visible.

### 6. Smoke was not the focus of this slice

This change set focused on PTC async attribution, missing Rust-core PTC
microbenches, and refreshed representative workload evidence. The kept release
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
   addon optimization work, and this slice kept it essentially flat while
   adding better attribution.
2. The primary representative lanes now show whether async cost is coming from
   resumed awaits or from queued promise combinators, which is exactly the
   missing evidence needed before more async-runtime tuning.
3. The Rust-core suite now covers staged derived-ID fanout and fanout-plus-
   reduction shapes, so future core changes no longer have to rely only on the
   smaller immediate-fanout bench.
4. Sidecar remains the largest remaining performance gap on representative
   medium lanes, but the benchmark story is now more complete on both the core
   and end-to-end sides.
5. The next highest-value work is still the unfinished async clone-amplification
   reduction called out in `plans/performance.md`, now backed by direct
   PTC-lane microtask counts instead of inference alone.
