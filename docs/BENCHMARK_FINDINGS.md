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

- workload suite: `benchmarks/results/2026-04-14T00-14-51-582Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in workload artifact: `6c91b71`
- Workload fixture version: `6`
- Smoke fixture version: `2`

## Headline Results

### 1. The release workload suite now includes representative programmatic tool-calling lanes

The main workload benchmark is no longer limited to the old synthetic
team/budget/expense fixture. The latest kept artifact adds a representative PTC
lane family derived from the audited example gallery:

- `ptc_website_demo_*` from
  `examples/programmatic-tool-calls/operations/triage-production-incident.js`
- `ptc_incident_triage_*` from
  `examples/programmatic-tool-calls/operations/triage-multi-region-auth-outage.js`
- `ptc_fraud_investigation_*` from
  `examples/programmatic-tool-calls/analytics/investigate-fraud-ring.js`
- `ptc_vendor_review_*` from
  `examples/programmatic-tool-calls/workflows/vendor-compliance-renewal.js`

The suite now also records:

- a weighted medium-lane score under `runtime.ptc.weightedScore.medium`
- per-lane tool call counts
- per-lane JSON-encoded tool-bytes-in vs result-bytes-out reduction ratios
- top-level metadata identifying the benchmark lane intended to back the
  website’s “4-tool orchestration workflow” story

### 2. Addon weighted PTC latency is now the clearest real-world benchmark signal

Current addon medians from the representative PTC suite:

| Metric | Median | p95 |
| --- | ---: | ---: |
| `ptc_website_demo_small` | `0.16 ms` | `0.17 ms` |
| `ptc_incident_triage_medium` | `0.59 ms` | `0.60 ms` |
| `ptc_fraud_investigation_medium` | `1.68 ms` | `1.73 ms` |
| `ptc_vendor_review_medium` | `0.21 ms` | `0.21 ms` |
| `addon.ptc.weightedScore.medium` | `0.88 ms` | `0.91 ms` |

The old synthetic control metric is still present:

| Metric | Median | p95 |
| --- | ---: | ---: |
| `programmatic_tool_workflow` | `1.44 ms` | `1.49 ms` |

That control remains useful for continuity, but it should no longer be treated
as the primary “real-world PTC” number. The weighted medium-lane score is now
the better addon optimization target.

### 3. The representative suite now exposes how much intermediate data stays inside the sandbox

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

### 4. Sidecar improved materially on representative PTC lanes, but it still trails addon

Current weighted medium-lane medians:

| Runtime | Median | p95 |
| --- | ---: | ---: |
| addon | `0.88 ms` | `0.91 ms` |
| sidecar | `2.66 ms` | `3.13 ms` |
| isolate | `0.39 ms` | `0.41 ms` |

Current weighted-score ratios:

| Ratio | Value |
| --- | ---: |
| `sidecar / addon` | `3.02x` |
| `isolate / addon` | `0.45x` |

Compared with the immediately previous representative artifact
`benchmarks/results/2026-04-14T00-07-53-889Z-workloads.json`, this slice
reduced sidecar latency materially:

- `sidecar.ptc.weightedScore.medium`: `4.00 ms -> 2.66 ms` (`-33.5%`)
- `sidecar.latency.programmatic_tool_workflow`: `16.46 ms -> 11.18 ms` (`-32.0%`)
- `sidecar.latency.ptc_incident_triage_medium`: `5.27 ms -> 3.20 ms` (`-39.4%`)
- `sidecar.latency.ptc_website_demo_small`: `0.71 ms -> 0.52 ms` (`-27.0%`)

That means:

- addon is now being measured against a much better real-world signal
- sidecar is substantially better on suspend/resume-heavy PTC flows than it
  was one artifact ago
- sidecar still remains the main laggard relative to addon on representative
  medium lanes
- isolate remains the raw-throughput leader on the new medium lanes

### 5. Smoke was not the focus of this slice

This change set focused on the representative benchmark harness, website
integration, and a sidecar suspend/resume optimization. The kept release smoke
artifact remains:

- `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Release smoke was not rerun as part of this representative-PTC benchmark
landing slice because the smoke suite itself was not modified. The broader repo
verification still passed:

- `npm test`
- `npm run lint`
- `cargo test --workspace`
- `npm run test:use-cases`
- `npm run bench:workloads:release`

## Conclusions

1. The representative PTC suite is now real and checked in. The project no
   longer has to rely on the old synthetic workflow fixture as its main
   programmatic tool-calling benchmark.
2. `addon.ptc.weightedScore.medium` should now be treated as the main
   optimization target for real-world tool-calling performance work.
3. The new transfer summaries make it possible to reason about the actual value
   proposition of sandbox-local reduction instead of looking only at latency.
4. The last sidecar slice paid off: the representative sidecar weighted score
   dropped by about one third without changing the public protocol contract.
5. The next highest-value work is now narrower: startup-only overhead and
   remaining sidecar execution/resume cost on the new representative lanes, not
   more tuning of the old toy fixture.
