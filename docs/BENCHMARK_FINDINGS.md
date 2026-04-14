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

- workload suite: `benchmarks/results/2026-04-14T01-36-43-009Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in workload artifact: `c1e3ebc`
- Workload fixture version: `6`
- Smoke fixture version: `2`

## Headline Results

### 1. Representative addon PTC latency stayed effectively flat while boundary attribution landed

The latest kept representative workload artifact leaves the addon headline
essentially unchanged while adding per-lane boundary attribution:

| Metric | Median | p95 |
| --- | ---: | ---: |
| `ptc_website_demo_small` | `0.16 ms` | `0.16 ms` |
| `ptc_incident_triage_medium` | `0.59 ms` | `0.60 ms` |
| `ptc_fraud_investigation_medium` | `1.68 ms` | `1.74 ms` |
| `ptc_vendor_review_medium` | `0.22 ms` | `0.23 ms` |
| `addon.ptc.weightedScore.medium` | `0.88 ms` | `0.91 ms` |

The old synthetic control metric is still present:

| Metric | Median | p95 |
| --- | ---: | ---: |
| `programmatic_tool_workflow` | `1.44 ms` | `1.47 ms` |

Compared with the immediately previous kept representative artifact
`benchmarks/results/2026-04-14T00-59-31-034Z-workloads.json`, the primary
addon medians remained effectively flat:

- `addon.ptc.weightedScore.medium`: `0.88 ms -> 0.88 ms` (`-0.4%`)
- `addon.latency.ptc_incident_triage_medium`: `0.59 ms -> 0.59 ms` (`-0.1%`)
- `addon.latency.ptc_fraud_investigation_medium`: `1.69 ms -> 1.68 ms` (`-0.9%`)
- `addon.latency.ptc_vendor_review_medium`: `0.21 ms -> 0.22 ms` (`+4.2%`)
- `addon.latency.ptc_website_demo_small`: `0.16 ms -> 0.16 ms` (`-3.4%`)

This slice was benchmark attribution work, not a latency win: it kept the
headline stable while landing the missing addon boundary breakdowns needed to
decide the next transport optimization path.

### 2. Representative addon lanes now show boundary parse/encode cost separately from guest execution

Representative addon PTC boundary breakdowns from the kept workload artifact:

| Lane | Host Callbacks | Guest Execution | Boundary Parse | Boundary Encode | Boundary Codec |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ptc_website_demo_small` | `0.01 ms` | `0.06 ms` | `0.01 ms` | `0.00 ms` | `0.01 ms` |
| `ptc_incident_triage_medium` | `0.01 ms` | `0.43 ms` | `0.02 ms` | `0.01 ms` | `0.04 ms` |
| `ptc_fraud_investigation_medium` | `0.01 ms` | `1.03 ms` | `0.16 ms` | `0.02 ms` | `0.18 ms` |
| `ptc_vendor_review_medium` | `0.00 ms` | `0.10 ms` | `0.02 ms` | `0.01 ms` | `0.03 ms` |

The important signal here is not that host callbacks are expensive in these
deterministic fixtures; they are not. The useful result is that the benchmark
now shows boundary codec cost directly, and it is most visible on
`ptc_fraud_investigation_medium`, where parse+encode is about `0.18 ms`
relative to about `1.03 ms` of guest execution.

### 3. Primary PTC lanes still expose queued/executed microtask pressure directly

The addon runtime counters still include queued/executed microtask totals plus
resume/combinator breakdowns for the three primary medium lanes:

| Lane | Queued Microtasks | Peak Queue | Resume Jobs | Promise Reactions | Promise Combinators |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ptc_incident_triage_medium` | `24` | `1` | `1` | `0` | `23` |
| `ptc_fraud_investigation_medium` | `7` | `1` | `3` | `0` | `4` |
| `ptc_vendor_review_medium` | `6` | `1` | `2` | `0` | `4` |

This still matters because the benchmark can distinguish lanes dominated by
queued combinator work from lanes dominated by resumed async continuations
instead of treating “Promise-heavy” as a single opaque bucket.

### 4. The representative suite still exposes how much intermediate data stays inside the sandbox

Addon transfer summaries from the kept workload artifact:

| Lane | Calls | Tool Bytes In | Result Bytes Out | Reduction |
| --- | ---: | ---: | ---: | ---: |
| `ptc_website_demo_small` | `6` | `1109 B` | `227 B` | `4.89x` |
| `ptc_incident_triage_medium` | `20` | `4340 B` | `2463 B` | `1.76x` |
| `ptc_fraud_investigation_medium` | `6` | `26818 B` | `17579 B` | `1.53x` |
| `ptc_vendor_review_medium` | `5` | `2639 B` | `967 B` | `2.73x` |

This is still the key benchmark-shape improvement: it makes it visible when
`mustard` is reducing tool payloads locally instead of just reporting one total
latency number.

### 5. Sidecar is still materially slower than addon on representative medium lanes

Current weighted medium-lane medians:

| Runtime | Median | p95 |
| --- | ---: | ---: |
| addon | `0.88 ms` | `0.91 ms` |
| sidecar | `2.67 ms` | `3.23 ms` |
| isolate | `0.40 ms` | `0.40 ms` |

Current weighted-score ratios:

| Ratio | Value |
| --- | ---: |
| `sidecar / addon` | `3.03x` |
| `isolate / addon` | `0.45x` |

The key point from this slice is not a new sidecar win. It is that the kept
artifact now shows where addon boundary time is going, while sidecar remains
well above the target ratio on the weighted medium-lane score.

### 6. Smoke was not the focus of this slice

This change set focused on addon boundary attribution and refreshed
representative workload evidence. The kept release smoke artifact remains:

- `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Release smoke was not rerun as part of this representative-PTC benchmark
attribution slice because the smoke suite itself was not modified. The broader
repo verification still passed:

- `npm test`
- `npm run lint`
- `cargo test --workspace`
- `npm run test:use-cases`
- `npm run bench:workloads:release`

## Conclusions

1. `addon.ptc.weightedScore.medium` is still the right real-world signal for
   addon optimization work, and this slice intentionally kept it effectively
   flat while improving measurement quality.
2. The new addon PTC breakdowns make boundary codec cost explicit instead of
   forcing future transport work to infer it from synthetic boundary-only
   probes.
3. `ptc_fraud_investigation_medium` is still the clearest boundary-heavy addon
   lane: its representative profiled run spends about `0.18 ms` in native
   boundary parse+encode on top of about `1.03 ms` of guest execution.
4. Sidecar remains materially above the target weighted-score ratio on the
   representative medium lanes.
5. The next highest-value work is still the unfinished addon boundary
   transport path plus the missing sidecar lane-level attribution that would
   split process/transport/materialization costs on the same representative
   PTC shapes.
