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

- workload suite: `benchmarks/results/2026-04-14T03-41-06-633Z-workloads.json`
- phase-2 broad PTC suite:
  `benchmarks/results/2026-04-14T07-56-52-011Z-ptc_broad_release-release.json`
- phase-2 holdout PTC suite:
  `benchmarks/results/2026-04-14T07-38-31-068Z-ptc_holdout_release-release.json`
- release smoke suite: `benchmarks/results/2026-04-13T23-00-15-361Z-smoke-release.json`

Machine and environment:

- CPU: Apple M4, 10 cores
- OS: `darwin 25.2.0`
- Arch: `arm64`
- Node: `v24.12.0`
- Git SHA in broad artifact: `afd8cf6`
- Workload fixture version: `9`
- Smoke fixture version: `2`

## Phase-2 Broad Baseline

The current kept broad-panel phase-2 artifact comes from:

- `npm run bench:ptc:broad`
- artifact:
  `benchmarks/results/2026-04-14T07-56-52-011Z-ptc_broad_release-release.json`

That artifact keeps the phase-1 representative weighted score in the full
`workloads` run, but it adds the new balanced real-gallery scorecard for the
phase-2 optimization loop, the skewed headline seed companions, and the
expanded durable panel.

Current broad-panel position versus isolate on the kept phase-2 artifact:

| Metric | Addon | Sidecar | Isolate |
| --- | ---: | ---: | ---: |
| `ptc.phase2.scorecards.headlineScore.medium` | `0.70 ms` | `2.53 ms` | `0.72 ms` |
| `ptc.phase2.scorecards.broadScore.medium` | `0.63 ms` | `2.16 ms` | `0.68 ms` |
| `ptc.phase2.scorecards.durableScore.medium` | `0.77 ms` | `0.99 ms` | `0.17 ms` |
| `ptc.phase2.scorecards.p90LaneRatio.medium` | `1.40x` | `4.37x` | `1.00x` |
| `ptc.phase2.scorecards.worstLaneRatio.medium` | `1.67x` | `6.77x` | `1.00x` |

Matching holdout evidence now comes from:

- `npm run bench:ptc:holdout`
- artifact:
  `benchmarks/results/2026-04-14T07-38-31-068Z-ptc_holdout_release-release.json`

Current holdout-panel position versus isolate on that artifact:

| Metric | Addon | Sidecar | Isolate |
| --- | ---: | ---: | ---: |
| `ptc.phase2.scorecards.holdoutScore.medium` | `0.67 ms` | `1.80 ms` | `0.75 ms` |

The current broad-panel artifact is intentionally a baseline, not a victory lap.
It proves the phase-2 portfolio is wired end to end and artifact-backed, and
it gives future optimization work a real broad-panel number to beat instead of
only the older three-lane weighted score.

The current kept broad artifact also verifies the new phase-2 variation layer:

- every headline lane now has a deterministic `medium_skewed` companion metric
- the broad release command exact-checks those skewed headline seeds across
  addon, sidecar, and isolate
- the durable panel now covers:
  - `ptc_vendor_review_durable_medium`
  - `ptc_plan-database-failover_durable_medium`
  - `ptc_privacy-erasure-orchestration_durable_medium`

The current kept broad artifact also adds the first deeper phase-2 attribution
slice:

- untimed representative addon counters for the six headline lanes now record:
  - static/computed property reads
  - object/array allocations
  - `Map.get` / `Map.set`
  - `Set.add` / `Set.has`
  - string case conversion
  - literal string search
  - regex search / replacement
  - comparator-based sort invocations
- representative addon and sidecar breakdowns now cover the same six headline
  gallery lanes instead of stopping at the older three-lane phase-1 subset
- the new attribution fields are collected on dedicated representative runs, so
  the broad scorecard remains the real release timing baseline rather than an
  instrumentation-only profile

## Headline Results

### 1. Binary live-addon boundary transport cut the representative addon PTC score again

Compared with the previous kept representative artifact
`benchmarks/results/2026-04-14T03-01-24-879Z-workloads.json`, the new kept
artifact moved the representative addon scorecard in the right direction:

| Metric | Previous Kept | Latest Kept | Delta |
| --- | ---: | ---: | ---: |
| `addon.ptc.weightedScore.medium` | `0.71 ms` | `0.66 ms` | `-7.0%` |
| `addon.latency.ptc_website_demo_small` | `0.15 ms` | `0.13 ms` | `-11.6%` |
| `addon.latency.ptc_fraud_investigation_medium` | `1.46 ms` | `1.33 ms` | `-8.7%` |
| `addon.latency.ptc_vendor_review_medium` | `0.22 ms` | `0.19 ms` | `-11.1%` |
| `addon.latency.ptc_incident_triage_medium` | `0.37 ms` | `0.37 ms` | `+0.2%` |

This slice replaced the live addon start/resume JSON path with a binary request
format, removed the JS-side structured DTO materialization on the hot addon
path, and kept the public wrapper on thin native-handle calls.

### 2. The win landed in native boundary decode/codec, not in guest execution

Representative addon breakdowns on the primary lanes moved like this:

| Metric | Previous Kept | Latest Kept | Delta |
| --- | ---: | ---: | ---: |
| `ptc_website_demo_small boundaryCodec` | `0.012 ms` | `0.007 ms` | `-42.8%` |
| `ptc_incident_triage_medium boundaryCodec` | `0.036 ms` | `0.021 ms` | `-40.5%` |
| `ptc_fraud_investigation_medium boundaryCodec` | `0.183 ms` | `0.078 ms` | `-57.4%` |
| `ptc_vendor_review_medium boundaryCodec` | `0.026 ms` | `0.012 ms` | `-53.6%` |
| `ptc_fraud_investigation_medium guestExecution` | `0.831 ms` | `0.827 ms` | `-0.5%` |

That matters because it isolates the kept improvement correctly. The binary
transport work materially reduced live addon boundary overhead while guest
execution stayed essentially flat on the representative fraud lane.

### 3. The Node wrapper stayed thin and the binary path still fails closed

The public wrapper now uses native buffer entrypoints for live addon
`run()` / `start()` / `resume()` traffic, while raw detached snapshot restore
paths keep their existing JSON/policy flow.

The binary Rust decoder in `crates/mustard-node/src/boundary_binary.rs`
revalidates the payload shape and rejects:

- malformed or truncated binary payloads
- unknown payload kinds or structured tags
- nesting beyond the host-boundary depth limit
- arrays beyond the existing host-boundary array-length limit
- non-finite or negative integer runtime limits

The JS encoder still preserves the existing fail-closed boundary checks for
proxies, accessor properties, cycles, sparse arrays, and unsupported values,
but it no longer allocates a structured JSON DTO tree on the hot addon path.

### 4. The remaining async-clone checkbox no longer points at the representative bottleneck

The remaining open Milestone 1 item was audited against the current async
runtime and the Rust-core bench suite.

What the code now does:

- settled awaiters queue `ResumeAsync { source: PromiseKey }` instead of cloned
  outcomes
- settled combinator work queues `PromiseCombinatorInput::Promise(...)` and
  resolves from the settled source on activation
- promise settlement drains awaiters/reactions/dependents with `std::mem::take`
  and schedules keyed work rather than cloning `PromiseOutcome` into queue
  payloads

What the current Rust benches show:

- `promise_all_immediate_fanout`: about `0.38 ms`
- `promise_all_settled_immediate`: about `3.39 ms`
- `promise_all_derived_ids_fanout`: about `6.21 ms`
- `promise_all_map_set_reduction`: about `8.44 ms`

Those async benches remain materially below the dominant synthetic local
hotspots such as `token_normalize` (`~70 ms`) and `top_k_sort` (`~208 ms`), and
the representative addon breakdowns are now dominated by guest execution or
already-reduced boundary work instead of unresolved queue-time outcome cloning.

### 5. The website export and verification are aligned with the kept artifact

`website/src/generated/benchmarkData.ts` now points to
`2026-04-14T03-41-06-633Z-workloads.json` and reports
`ptc_website_demo_small` at `0.133 ms` median and `0.159 ms` p95 for addon.

The broader repo verification for the current kept state passed:

- `npm run bench:ptc:broad`
- `npm run bench:ptc:holdout`
- `npm run bench:ptc:sentinel`
- `npm run bench:workloads:dev -- --mode ptc_headline_release`
- `cargo test --workspace`
- `npm test`
- `npm run lint`
- `npm run test:use-cases`

## Conclusions

1. The current best remaining addon PTC win came from making live start/resume
   transport cheaper, not from another guest-runtime change.
2. The binary addon path cut representative boundary codec time by about
   `40%` to `57%` on the primary lanes and lowered the representative addon
   weighted score from `0.71 ms` to `0.66 ms`.
3. The current async promise-settlement code no longer has an obvious separate
   representative bottleneck after the earlier key-based queueing cleanup, so
   the remaining Milestone 1 checkbox was closed by audit rather than by a new
   runtime rewrite.
4. From the first representative PTC scorecard at `0.88 ms` to the current
   kept artifact at `0.66 ms`, the addon weighted medium-lane score is down by
   about `25%`, which satisfies the plan’s requirement that large gains be
   measured on the representative suite before calling the work done.
