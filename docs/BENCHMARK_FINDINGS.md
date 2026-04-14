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

- workload suite: `benchmarks/results/2026-04-14T03-01-24-879Z-workloads.json`
- release smoke suite: `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in workload artifact: `bc3158a`
- Workload fixture version: `6`
- Smoke fixture version: `2`

## Headline Results

### 1. Real local-reduction cleanup cut the representative addon PTC score materially

Compared with the previous kept representative artifact
`benchmarks/results/2026-04-14T02-12-56-211Z-workloads.json`, the primary
medium-lane scorecard moved materially on addon and modestly on sidecar:

| Metric | Previous Kept | Latest Kept | Delta |
| --- | ---: | ---: | ---: |
| `addon.ptc.weightedScore.medium` | `0.87 ms` | `0.71 ms` | `-18.4%` |
| `sidecar.ptc.weightedScore.medium` | `2.64 ms` | `2.50 ms` | `-5.6%` |
| `addon.latency.ptc_incident_triage_medium` | `0.59 ms` | `0.37 ms` | `-38.1%` |
| `addon.latency.ptc_fraud_investigation_medium` | `1.65 ms` | `1.46 ms` | `-11.5%` |
| `addon.latency.ptc_vendor_review_medium` | `0.23 ms` | `0.22 ms` | `-6.3%` |
| `addon.latency.ptc_website_demo_small` | `0.16 ms` | `0.15 ms` | `-6.0%` |

This slice was real runtime speed work, not just measurement coverage. The kept
changes cached compiled regexes across calls, avoided repeated `Vec<char>`
materialization in hot string helpers, fast-pathed regex matches that do not
need capture allocation, and removed a full-array clone from
`Array.prototype.join`.

### 2. The kept win landed in guest execution, not addon boundary codec

Representative addon breakdowns on the primary medium lanes moved like this:

| Metric | Previous Kept | Latest Kept | Delta |
| --- | ---: | ---: | ---: |
| `ptc_incident_triage_medium guestExecution` | `0.46 ms` | `0.20 ms` | `-55.7%` |
| `ptc_fraud_investigation_medium guestExecution` | `1.00 ms` | `0.83 ms` | `-17.1%` |
| `ptc_fraud_investigation_medium boundaryCodec` | `0.18 ms` | `0.18 ms` | `+0.4%` |

That matters because it rules out the wrong conclusion. The representative gain
did not come from addon boundary transport; it came from reducing temporary
allocation and cloning inside the guest runtime on the incident and fraud
shapes that are heavy on regex/string work.

### 3. Rust-core local-reduction microbenches stayed mostly flat on the same kept code

`npm run bench:rust` on the kept runtime state reported:

- `ptc_local_reduction/map_join_update`: `19.65 ms`
- `ptc_local_reduction/set_dedupe`: `20.08 ms`
- `ptc_local_reduction/token_normalize`: `70.95 ms`
- `ptc_local_reduction/top_k_sort`: `211.43 ms`
- `ptc_local_reduction/array_from_object_from_entries`: `17.60 ms`

These benches were directionally flat overall, which matches the workload
story. The representative scorecard improved because the real audited lanes were
paying avoidable temporary-allocation cost in string/regex-heavy helpers, not
because every synthetic local-reduction kernel got broadly faster. Sort and
token normalization still remain the largest synthetic local hot spots.

### 4. A narrower collection-promotion experiment was rejected and reverted

A temporary experiment lowering `COLLECTION_LOOKUP_PROMOTION_LEN` from `32` to
`12` made the representative addon score worse in the rerun. The rejected
candidate reached `addon.ptc.weightedScore.medium 0.74 ms`, about `4.1%` slower
than the kept `0.71 ms`, and also worsened `ptc_fraud_investigation_medium`.
That candidate was fully reverted before the kept artifact was regenerated, so
the checked-in result is the post-revert runtime state.

### 5. The website export and repo verification are aligned with the kept artifact

`website/src/generated/benchmarkData.ts` now points to
`2026-04-14T03-01-24-879Z-workloads.json` and reports
`ptc_website_demo_small` at `0.151 ms` median and `0.163 ms` p95 for addon.

The broader repo verification for this slice passed:

- `npm test`
- `npm run lint`
- `cargo test --workspace`
- `npm run test:use-cases`
- `npm run bench:rust`
- `npm run bench:workloads:release`

## Conclusions

1. The current best local-reduction win came from eliminating avoidable
   temporary allocation and cloning on real representative PTC lanes, not from
   boundary transport changes.
2. Milestone 3 now has benchmark-backed evidence that the primary addon
   incident, fraud, and vendor lanes each improved materially or meaningfully
   on the representative scorecard.
3. Addon boundary transport is still open work. On the kept fraud lane,
   `boundaryParse` plus `boundaryCodec` still cost about `0.33 ms` while guest
   execution costs about `0.83 ms`, so Milestone 2 remains a real opportunity.
4. Promise clone amplification in settlement and awaiter scheduling is also
   still open. The representative score improved here without finishing the
   remaining Milestone 1 async clone cleanup.
5. The next highest-value plan items remain the unfinished addon start/resume
   transport path and the remaining async settlement-clone reductions.
