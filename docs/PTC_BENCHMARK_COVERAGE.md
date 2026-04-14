---
title: "PTC Benchmark Coverage"
description: "Phase-2 portfolio coverage matrix for the audited programmatic tool-call gallery"
category: "Development"
order: 3
slug: "ptc-benchmark-coverage"
lastUpdated: "2026-04-14"
---

# PTC Benchmark Coverage

This file is the checked-in phase-2 coverage matrix for the realistic
programmatic tool-calling gallery under
`examples/programmatic-tool-calls/`.

The machine-readable source of truth lives in
`benchmarks/ptc-portfolio.ts`. The benchmark harness uses that metadata to
define the headline, broad, holdout, and sentinel layers.

## Panel Summary

- headline panel: `6` medium lanes, balanced `2 / 2 / 2` across analytics,
  operations, and workflows
- broad panel: `12` medium lanes, balanced `4 / 4 / 4`
- holdout panel: the remaining `12` audited medium lanes
- durable panel: the synthetic vendor durable lane plus real audited
  `plan-database-failover` and `privacy-erasure-orchestration` checkpoints
- gallery canary: all `24` audited lanes with lower iteration count
- public lane: `ptc_website_demo_small`

## Headline Seed Matrix

The release harness now keeps the nominal audited headline lanes and a checked-in
skewed companion for each one. The skew lanes use the same guest source with a
second deterministic seed that distorts cardinality, string noise, or
writeback fanout without turning into a synthetic microbench.

| Headline Lane | Skew Metric | Skew Patterns |
| --- | --- | --- |
| `analytics_revenue_quality` | `ptc_analytics_revenue_quality_medium_skewed` | hotspot cardinality, duplicate-heavy collections, longer deal strings |
| `analytics_fraud_ring` | `ptc_analytics_fraud_ring_medium_skewed` | hotspot cardinality, duplicate-heavy joins, noisier identity/note strings |
| `triage-multi-region-auth-outage` | `ptc_triage-multi-region-auth-outage_medium_skewed` | duplicate-heavy alert dedupe, regional hotspot skew, longer error samples |
| `analyze-queue-backlog-regression` | `ptc_analyze-queue-backlog-regression_medium_skewed` | hotspot shard skew, larger intermediate payloads, noisier dead-letter strings |
| `vendor-compliance-renewal` | `ptc_vendor-compliance-renewal_medium_skewed` | larger intermediate payloads, duplicate-heavy evidence sets, lower signal-to-noise |
| `privacy-erasure-orchestration` | `ptc_privacy-erasure-orchestration_medium_skewed` | writeback fanout skew, retention-hold hotspot, larger intermediate payloads |

## Coverage Matrix

| Use Case | Category | Panel | Async Shape | Collections | Strings | Time | Writeback | Durable | Compaction |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `analytics_revenue_quality` | analytics | headline+broad | fanout+derived | `Map` | no | no | no | no | lower |
| `analytics_fraud_ring` | analytics | headline+broad | fanout+derived | `Map`+`Set` | yes | yes | no | no | moderate |
| `analytics_supplier_disruption` | analytics | broad | fanout+derived | `Map`+`Set` | no | no | no | no | moderate |
| `analytics_model_regression` | analytics | broad | fanout+derived | none | no | yes | no | no | lower |
| `analytics_market_event_brief` | analytics | holdout | fanout | `Map` | yes | yes | no | no | lower |
| `analytics_enterprise_renewal` | analytics | holdout | sequential resume | none | no | no | no | yes | lower |
| `analytics_market_abuse_review` | analytics | holdout | sequential resume | none | yes | yes | no | yes | moderate |
| `analytics_capital_allocation` | analytics | holdout | fanout | `Map` | no | no | no | no | lower |
| `triage-multi-region-auth-outage` | operations | headline+broad | fanout+derived | `Map`+`Set` | yes | yes | no | no | moderate |
| `reconcile-marketplace-payouts` | operations | broad | fanout+derived | `Map` | no | no | no | no | lower |
| `analyze-queue-backlog-regression` | operations | headline+broad | fanout+derived | `Map` | yes | yes | no | no | moderate |
| `plan-database-failover` | operations | broad | sequential resume | none | no | yes | yes | yes | moderate |
| `guard-payments-rollout` | operations | holdout | fanout+derived | none | no | no | no | no | lower |
| `stabilize-oncall-handoff` | operations | holdout | fanout | `Map` | yes | yes | no | no | moderate |
| `coordinate-warehouse-exception` | operations | holdout | fanout | `Map` | no | yes | no | no | moderate |
| `assess-global-deployment-freeze` | operations | holdout | fanout+derived | none | no | yes | no | no | moderate |
| `security-access-recertification` | workflows | broad | fanout+derived | `Map`+`Set` | no | no | yes | no | moderate |
| `vendor-compliance-renewal` | workflows | headline+broad | fanout | none | no | no | yes | no | moderate |
| `privacy-erasure-orchestration` | workflows | headline+broad | sequential resume | none | no | yes | yes | yes | moderate |
| `chargeback-evidence-assembly` | workflows | broad | fanout | none | no | yes | yes | no | moderate |
| `approval-exception-routing` | workflows | holdout | sequential resume | `Set` | no | no | yes | yes | moderate |
| `vip-support-escalation` | workflows | holdout | fanout | none | yes | yes | yes | no | moderate |
| `payout-batch-release-review` | workflows | holdout | fanout | `Map`+`Set` | no | no | yes | no | moderate |
| `enterprise-renewal-save-plan` | workflows | holdout | fanout | `Map` | no | no | yes | no | moderate |

## Sentinel Delegations

The audited gallery is intentionally the primary source of truth, but three
shape families are still tracked separately instead of being hidden inside the
real-gallery panels:

- `code_mode_search`: large preloaded typed-API search surfaces, preload
  footprint, and result-size sensitivity
- `result_materialization`: cases where guest work stays mostly fixed but
  boundary output expansion dominates the total
- `low_compaction_fanout`: realistic fanout with weaker final compaction than
  the main gallery lanes

Those families stay out of the primary gallery scorecards so the benchmark
portfolio does not stop reflecting real workloads, but they remain required
evidence whenever a change claims to improve generic interpreter, boundary, or
result-materialization behavior.

## Durable Panel

The durable benchmark panel now covers three resume paths:

- `ptc_vendor_review_durable_{small,medium,large}`:
  checkpoint at `checkpoint_vendor_review`
- `ptc_plan-database-failover_durable_medium`:
  checkpoint at `request_operator_approval`
- `ptc_privacy-erasure-orchestration_durable_medium`:
  checkpoint at the first `queue_erasure_job`

Those checkpoints keep the durable panel tied to real resumable workflow
shapes instead of one vendor-review-only synthetic path.
