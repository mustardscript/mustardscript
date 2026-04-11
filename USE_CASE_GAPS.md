# Use Case Gaps

This file tracks failures from the realistic programmatic tool-call gallery
under `examples/programmatic-tool-calls/`.

Audit command:

```sh
node scripts/audit-use-cases.js --json
```

Current audited summary on April 11, 2026:

- total cataloged use cases: `24`
- passing: `9`
- failing: `15`

The gallery is intentionally realistic. If a snippet exposes a runtime gap that
matters for real programmatic tool-call execution, the snippet should stay
realistic and this file should capture the gap instead of simplifying the use
case away.

## Failure Matrix

| Use case | File | Current failure | Real gap |
| --- | --- | --- | --- |
| `analytics_fraud_ring` | `examples/programmatic-tool-calls/analytics/investigate-fraud-ring.js` | `Runtime: Error: value is not callable` | Realistic fraud-cluster correlation wants `Array.from(setLike)` to normalize `Set` state for downstream analysis. The runtime currently fails unclearly instead of supporting or rejecting that surface explicitly. |
| `analytics_supplier_disruption` | `examples/programmatic-tool-calls/analytics/assess-supplier-disruption.js` | `Runtime: Error: value is not callable` | Supplier-risk planning also needs `Array.from` for set-to-array conversion. |
| `analytics_market_event_brief` | `examples/programmatic-tool-calls/analytics/prepare-market-event-brief.js` | `Runtime: Error: value is not callable` | Transcript and note extraction naturally wants `String.prototype.matchAll`, and this case also leans on `Object.fromEntries`-style shaping. |
| `analytics_capital_allocation` | `examples/programmatic-tool-calls/analytics/build-capital-allocation-brief.js` | `Validation: inconsistent validation state` | Comparator-based `Array.prototype.sort` is a normal part of allocation ranking logic. The runtime currently trips an internal validator/codegen inconsistency instead of handling or rejecting it cleanly. |
| `guard-payments-rollout` | `examples/programmatic-tool-calls/operations/guard-payments-rollout.js` | `Runtime: Error: value is not callable` | Canary scorecard ranking needs comparator-based `Array.prototype.sort`. |
| `reconcile-marketplace-payouts` | `examples/programmatic-tool-calls/operations/reconcile-marketplace-payouts.js` | `Validation: inconsistent validation state` | Multi-source financial reconciliation code still hits validator instability on a realistic workflow shape. This should either run or fail with a precise unsupported diagnostic. |
| `analyze-queue-backlog-regression` | `examples/programmatic-tool-calls/operations/analyze-queue-backlog-regression.js` | `Runtime: Error: synchronous array callback did not settle` | Async operational fan-out through `Promise.all(regions.map(...))` is still fragile when the callback reaches host capabilities. This is a real runtime correctness gap for programmatic tool execution. |
| `plan-database-failover` | `examples/programmatic-tool-calls/operations/plan-database-failover.js` | `Runtime: Error: value is not callable [0..1937]` | Sequential resumable operational workflows with structured decision objects still hit runtime correctness failures. |
| `coordinate-warehouse-exception` | `examples/programmatic-tool-calls/operations/coordinate-warehouse-exception.js` | `Validation: inconsistent validation state` | A realistic fulfillment-exception workflow still trips compiler or validator instability instead of completing or failing closed for a documented reason. |
| `assess-global-deployment-freeze` | `examples/programmatic-tool-calls/operations/assess-global-deployment-freeze.js` | `Runtime: Error: value is not callable` | This use case naturally wants `Date`, `Object.fromEntries`, and comparator-based `Array.prototype.sort`. All three are realistic for freeze decisions; the current failure mode is too generic. |
| `approval-exception-routing` | `examples/programmatic-tool-calls/workflows/approval-exception-routing.js` | `Runtime: TypeError: value is not iterable in the supported surface` | Approval routing built around `Set` accumulation and `for...of` normalization still hits iterable-surface correctness gaps. |
| `security-access-recertification` | `examples/programmatic-tool-calls/workflows/security-access-recertification.js` | `Validation: inconsistent validation state` | Realistic access-review loops over `Set` and `Map`-backed state still expose validator instability. |
| `vip-support-escalation` | `examples/programmatic-tool-calls/workflows/vip-support-escalation.js` | `Runtime: ReferenceError: \`Date\` is not defined` | SLA and escalation logic needs a documented time surface. `Date` is the direct missing runtime feature here; the case also naturally uses ranked prioritization. |
| `vendor-compliance-renewal` | `examples/programmatic-tool-calls/workflows/vendor-compliance-renewal.js` | `Runtime: Error: value is not callable` | Evidence-matrix shaping naturally wants `Object.fromEntries`. |
| `privacy-erasure-orchestration` | `examples/programmatic-tool-calls/workflows/privacy-erasure-orchestration.js` | `Runtime: TypeError: value is not iterable in the supported surface` | Resumable privacy orchestration over system lists and retention exceptions still fails on realistic iterable host values. |

## Gap Themes

### 1. Missing ranking and ordering helpers

Affected:

- `analytics_capital_allocation`
- `guard-payments-rollout`
- `assess-global-deployment-freeze`
- `vip-support-escalation`

Why it matters:

- ranking and prioritization are core patterns in rollout control, escalation,
  allocation, and planning workflows

Needed outcome:

- implement comparator-based `Array.prototype.sort`, or
- fail closed with an explicit unsupported diagnostic instead of generic runtime
  errors or validator failures

### 2. Missing object construction from entry pairs

Affected:

- `analytics_market_event_brief`
- `assess-global-deployment-freeze`
- `vendor-compliance-renewal`

Why it matters:

- realistic tool-driven code often reduces fetched records into
  `[key, value]` pairs and then normalizes them into plain objects

Needed outcome:

- implement `Object.fromEntries`, or
- reject it explicitly

### 3. Missing collection conversion helpers

Affected:

- `analytics_fraud_ring`
- `analytics_supplier_disruption`

Why it matters:

- graph, fraud, and supply-chain analysis frequently accumulate unique IDs in
  `Set`s and then convert them into arrays for joins, summaries, and downstream
  calls

Needed outcome:

- implement `Array.from` for the supported iterable surface, or
- document and diagnose a narrower supported alternative clearly

### 4. Missing match iteration helpers

Affected:

- `analytics_market_event_brief`

Why it matters:

- transcript, research-note, and event-brief parsing often needs repeated match
  extraction rather than a single match

Needed outcome:

- implement `String.prototype.matchAll`, or
- reject it explicitly

### 5. Missing time and clock primitives

Affected:

- `assess-global-deployment-freeze`
- `vip-support-escalation`

Why it matters:

- freshness checks, SLA math, freeze decisions, and escalation ordering all
  require a documented time surface

Needed outcome:

- add a conservative `Date` subset, or
- define and document an explicit host-provided clock pattern

### 6. Host capability calls inside array callback helpers are still fragile

Affected:

- `analyze-queue-backlog-regression`

Why it matters:

- realistic async analysis code naturally uses `Promise.all(items.map(...))`
- this is directly in-scope for programmatic tool-call execution

Needed outcome:

- fix callback-helper semantics for host capability calls reached from async
  guest code, or
- narrow the documented contract if that pattern is intentionally unsupported

### 7. Iterable-surface correctness is still incomplete for realistic workflows

Affected:

- `approval-exception-routing`
- `privacy-erasure-orchestration`

Also implicated:

- `security-access-recertification`

Why it matters:

- realistic workflows accumulate approvers, systems, or review subjects in
  sets, arrays, and other host-returned collection shapes and then iterate them

Needed outcome:

- make `for...of` and related collection normalization work consistently across
  the supported `Set`/array/iterator surface, including resumable workflows and
  host-returned values

### 8. Runtime correctness bugs on realistic resumable or structured workflows

Affected:

- `plan-database-failover`

Why it matters:

- resumable operational workflows are one of the README’s target shapes
- this failure does not look like an intentional unsupported-language boundary

Needed outcome:

- debug the runtime path behind the generic callable failure
- add a targeted regression test once fixed

### 9. Bytecode validation instability on realistic control flow

Affected:

- `analytics_capital_allocation`
- `reconcile-marketplace-payouts`
- `coordinate-warehouse-exception`
- `security-access-recertification`

Why it matters:

- internal validation inconsistencies are not an acceptable end-user failure
  mode for realistic guest programs

Needed outcome:

- fix compiler and validator inconsistencies
- where a true surface gap exists, replace the internal validation failure with
  a deliberate parse, validation, or runtime diagnostic
