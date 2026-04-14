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

- workload suite: `benchmarks/results/2026-04-14T02-12-56-211Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in workload artifact: `a948ead`
- Workload fixture version: `6`
- Smoke fixture version: `2`

## Headline Results

### 1. Durable checkpoint coverage landed without regressing the ordinary representative PTC scorecard

Compared with the previous kept representative artifact
`benchmarks/results/2026-04-14T01-49-28-550Z-workloads.json`, the primary
ordinary PTC medians stayed effectively flat while the durable checkpoint lane
was added:

| Metric | Previous Kept | Latest Kept | Delta |
| --- | ---: | ---: | ---: |
| `addon.ptc.weightedScore.medium` | `0.90 ms` | `0.87 ms` | `-3.2%` |
| `sidecar.ptc.weightedScore.medium` | `2.65 ms` | `2.64 ms` | `-0.3%` |
| `sidecar.latency.ptc_incident_triage_medium` | `3.25 ms` | `3.25 ms` | `-0.1%` |
| `sidecar.latency.ptc_fraud_investigation_medium` | `3.32 ms` | `3.30 ms` | `-0.7%` |
| `sidecar.latency.ptc_vendor_review_medium` | `0.75 ms` | `0.75 ms` | `+0.5%` |

This was benchmark-coverage work, not a general sidecar speedup. Some
sidecar-only startup and cold-start medians moved around more than the primary
PTC lanes in this rerun, so the kept decision stayed anchored on the
representative medium-lane scorecard instead of the noisier tiny startup paths.

### 2. The medium durable lane preserves the addon and sidecar resume-only edge over isolates

Durable resume-only medians from the kept workload artifact:

| Lane | Addon | Sidecar | Isolate |
| --- | ---: | ---: | ---: |
| `ptc_vendor_review_durable_small` | `0.39 ms` | `0.42 ms` | `0.57 ms` |
| `ptc_vendor_review_durable_medium` | `0.48 ms` | `0.52 ms` | `0.56 ms` |
| `ptc_vendor_review_durable_large` | `0.67 ms` | `0.73 ms` | `0.57 ms` |

The milestone gate is the medium lane because it is the representative durable
variant: there is a real checkpoint after enrichment, before the final review
writeback, and both addon and sidecar still resume faster than the isolate
baseline there. The large lane remains informative but not decisive for the
milestone because the isolate harness still has to emulate the pause with
explicit carried state instead of a true continuation snapshot.

Persisted state tracked for that durable checkpoint:

| Size | Addon Snapshot / Manifest | Sidecar Snapshot / Policy | Isolate Carried State |
| --- | ---: | ---: | ---: |
| `small` | `9721 B / 2451 B` | `20584 B / 502 B` | `1406 B` |
| `medium` | `14930 B / 4770 B` | `25793 B / 502 B` | `2764 B` |
| `large` | `25817 B / 9585 B` | `36680 B / 502 B` | `5630 B` |

The medium durable checkpoint now exposes the concrete storage tradeoff:
addon persists about `14.9 KB` of snapshot plus a `4.8 KB` detached manifest,
sidecar persists about `25.8 KB` of snapshot plus a `502 B` raw-resume policy,
and the isolate harness has to carry `2764 B` of explicit reconstructed state.

### 3. Restore semantics and final-action failure behavior are now aligned across addon and sidecar

`tests/node/durable-ptc-equivalence.test.js` now drives both restore paths from
the same durable checkpoint shape:

- addon: `Progress.dump()` -> `Progress.load(...)` -> resume approval -> final action
- sidecar: raw snapshot capture -> authenticated raw `resume` -> final action

The test suite asserts both successful-result parity and matching final-action
failure messages (`durable final action failed`). This passed in the broader
repo verification under `npm test`, so the benchmark lane is not just measured;
it is covered for success and failure semantics across both runtime surfaces.

### 4. Representative PTC attribution remains in the kept artifact

The kept workload artifact still carries the representative sidecar and addon
lane breakdowns that landed in the previous slice. For the current kept run,
the medium sidecar lanes still break down as:

| Lane | Request Transport | Execution | Response Materialization |
| --- | ---: | ---: | ---: |
| `ptc_incident_triage_medium` | `0.44 ms` | `0.53 ms` | `1.14 ms` |
| `ptc_fraud_investigation_medium` | `0.37 ms` | `1.04 ms` | `0.71 ms` |
| `ptc_vendor_review_medium` | `0.15 ms` | `0.13 ms` | `0.23 ms` |

That matters because Milestone 5 added the durable resume-only coverage without
losing the earlier attribution needed for follow-up work on request transport
and response materialization.

### 5. Smoke was not the focus of this slice

This change set focused on durable PTC checkpoint coverage and refreshed
representative workload evidence. The kept release smoke artifact remains:

- `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Release smoke was not rerun because the smoke suite itself was not modified.
The broader repo verification still passed:

- `npm test`
- `npm run lint`
- `cargo test --workspace`
- `npm run test:use-cases`
- `npm run bench:workloads:release`

## Conclusions

1. The representative suite now includes a realistic durable-boundary PTC lane
   with checked-in resume-only timings plus persisted-state byte accounting.
2. The primary durable lane (`ptc_vendor_review_durable_medium`) preserves the
   addon and sidecar resume-only edge over isolates while modeling a real pause
   between enrichment and final writeback.
3. Ordinary representative PTC medians stayed effectively flat, so this slice
   should be read as measurement and coverage work rather than a runtime speedup.
4. The large durable lane is still useful evidence: explicit-state isolate
   re-entry becomes competitive there, which means future durable work can
   still target snapshot size and resume-path overhead.
5. The next highest-value work remains the unfinished addon-side transport path
   and any runtime changes that can move the primary representative weighted
   score materially, especially on the response-heavy and boundary-heavy lanes.
