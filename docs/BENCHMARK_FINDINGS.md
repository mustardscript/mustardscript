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

- workload suite: `benchmarks/results/2026-04-14T01-49-28-550Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in workload artifact: `cef6000`
- Workload fixture version: `6`
- Smoke fixture version: `2`

## Headline Results

### 1. Representative sidecar PTC latency stayed effectively flat while lane-level attribution landed

The latest kept representative workload artifact leaves the sidecar headline
effectively unchanged while adding the missing representative PTC attribution:

| Metric | Median | p95 |
| --- | ---: | ---: |
| `ptc_website_demo_small` | `0.55 ms` | `0.58 ms` |
| `ptc_incident_triage_medium` | `3.25 ms` | `4.18 ms` |
| `ptc_fraud_investigation_medium` | `3.32 ms` | `3.33 ms` |
| `ptc_vendor_review_medium` | `0.75 ms` | `0.76 ms` |
| `sidecar.ptc.weightedScore.medium` | `2.65 ms` | `3.03 ms` |

The old synthetic control metric is still present:

| Metric | Median | p95 |
| --- | ---: | ---: |
| `programmatic_tool_workflow` | `1.44 ms` | `1.47 ms` |

Compared with the immediately previous kept representative artifact
`benchmarks/results/2026-04-14T01-36-43-009Z-workloads.json`, the primary
sidecar medians remained effectively flat:

- `sidecar.ptc.weightedScore.medium`: `2.67 ms -> 2.65 ms` (`-0.5%`)
- `sidecar.latency.ptc_incident_triage_medium`: `3.25 ms -> 3.25 ms` (`+0.1%`)
- `sidecar.latency.ptc_fraud_investigation_medium`: `3.37 ms -> 3.32 ms` (`-1.3%`)
- `sidecar.latency.ptc_vendor_review_medium`: `0.75 ms -> 0.75 ms` (`+0.2%`)
- `sidecar.latency.ptc_website_demo_small`: `0.67 ms -> 0.55 ms` (`-18.1%`)

This was still attribution work, not a sidecar speedup. The useful outcome is
that the benchmark now shows where representative sidecar time lands instead of
leaving startup, transport, execution, and response handling as one opaque
bucket.

### 2. Representative sidecar lanes now show startup, transport, execution, and response materialization separately

Representative sidecar PTC breakdowns from the kept workload artifact:

- `processStartup`: `8.49 ms` median, `278.22 ms` p95

| Lane | Request Transport | Execution | Response Materialization |
| --- | ---: | ---: | ---: |
| `ptc_website_demo_small` | `0.17 ms` | `0.11 ms` | `0.16 ms` |
| `ptc_incident_triage_medium` | `0.47 ms` | `0.52 ms` | `1.15 ms` |
| `ptc_fraud_investigation_medium` | `0.39 ms` | `1.06 ms` | `0.72 ms` |
| `ptc_vendor_review_medium` | `0.14 ms` | `0.12 ms` | `0.23 ms` |

The important signal here is that `ptc_incident_triage_medium` is not mainly a
guest-execution problem on sidecar: the kept run spends about `1.15 ms` in
response materialization and another `0.47 ms` in request transport versus
about `0.52 ms` inside sidecar execution. `ptc_fraud_investigation_medium`
still spends the largest absolute execution time, but it also pays a visible
`0.72 ms` response-materialization cost. `ptc_vendor_review_medium` already
shows relatively small transport overhead at about `0.14 ms`.

### 3. Representative addon lanes still show boundary parse/encode cost separately from guest execution

Representative addon PTC boundary breakdowns from the kept workload artifact:

| Lane | Host Callbacks | Guest Execution | Boundary Parse | Boundary Encode | Boundary Codec |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ptc_website_demo_small` | `0.01 ms` | `0.07 ms` | `0.01 ms` | `0.00 ms` | `0.01 ms` |
| `ptc_incident_triage_medium` | `0.01 ms` | `0.43 ms` | `0.03 ms` | `0.01 ms` | `0.04 ms` |
| `ptc_fraud_investigation_medium` | `0.01 ms` | `1.06 ms` | `0.16 ms` | `0.03 ms` | `0.19 ms` |
| `ptc_vendor_review_medium` | `0.00 ms` | `0.10 ms` | `0.02 ms` | `0.01 ms` | `0.03 ms` |

The addon signal is still useful because it keeps the native boundary codec
cost explicit. `ptc_fraud_investigation_medium` remains the clearest
boundary-heavy addon lane, with about `0.19 ms` of native parse+encode on top
of about `1.06 ms` of guest execution.

### 4. Primary PTC lanes still expose queued/executed microtask pressure directly

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

### 5. The representative suite still exposes how much intermediate data stays inside the sandbox

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

### 6. The weighted sidecar/addon score is now just under the milestone target, but not because of a decisive sidecar speedup

Current weighted medium-lane medians:

| Runtime | Median | p95 |
| --- | ---: | ---: |
| addon | `0.90 ms` | `0.92 ms` |
| sidecar | `2.65 ms` | `3.03 ms` |
| isolate | `0.39 ms` | `0.41 ms` |

Current weighted-score ratios:

| Ratio | Value |
| --- | ---: |
| `sidecar / addon` | `2.94x` |
| `isolate / addon` | `0.44x` |

The current kept artifact is technically inside the Milestone 4 weighted-score
target, but this is not a decisive sidecar win. The ratio only moved from
about `3.03x` to `2.94x`, and that came from small shifts on both addon and
sidecar medians. The real value of this slice is that the score now comes with
lane-level attribution, so follow-up work can target response handling and
transport costs directly instead of treating sidecar overhead as one monolith.

### 7. Smoke was not the focus of this slice

This change set focused on sidecar lane-level attribution and refreshed
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
   addon optimization work, and the new artifact keeps the addon score in the
   same range while preserving the addon boundary breakdowns.
2. Sidecar representative PTC attribution has now landed. `processStartup`,
   `requestTransport`, `execution`, and `responseMaterialization` are all
   visible in the kept workload artifact.
3. `ptc_incident_triage_medium` is the clearest sidecar response-heavy lane in
   the current artifact: about `1.15 ms` of response materialization and
   `0.47 ms` of request transport versus about `0.52 ms` of execution.
4. `ptc_fraud_investigation_medium` remains the clearest addon boundary-heavy
   lane: about `0.19 ms` of native boundary codec on top of about `1.06 ms` of
   guest execution.
5. The next highest-value work is the unfinished addon transport path plus
   targeted sidecar reductions in response materialization and request
   transport on the primary representative lanes.
