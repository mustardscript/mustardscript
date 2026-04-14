# MustardScript Programmatic Tool Calling Phase 2 Plan

## Objective

Make the PTC benchmark portfolio broad enough to drive addon work toward
isolate-comparable latency without overfitting to a tiny set of showcase
lanes.

Phase 1 successfully made representative PTC the primary performance target.
Phase 2 must make that target robust:

- optimization decisions should be driven by a portfolio of real workloads, not
  by three medium lanes and one website demo
- the benchmark suite should cover the real workload shapes present in the
  audited gallery, not just the shapes that happened to be optimized first
- performance wins should survive holdout lanes and full-gallery canaries, not
  just the headline scorecard
- the suite should stay practical enough to run regularly, while still being
  broad enough to resist overfitting

## Audited Baseline

Audited inputs and evidence:

- Existing phase-1 plan: `plans/PTC_PERFORMANCE.md`
- Existing benchmark protocol: `benchmarks/README.md`
- Existing workload harness: `benchmarks/workloads.ts`
- Existing PTC fixtures: `benchmarks/ptc-fixtures.ts`
- Latest kept workload artifact:
  `benchmarks/results/2026-04-14T03-41-06-633Z-workloads.json`
- Latest findings summary: `docs/BENCHMARK_FINDINGS.md`
- Audited gallery docs: `docs/USE_CASE_EXAMPLES.md` and `docs/USE_CASE_GAPS.md`
- Audited gallery harness: `scripts/audit-use-cases.ts`
- Checked-in realistic use-case catalogs:
  `examples/programmatic-tool-calls/*/catalog.ts`

Verified current state in this worktree:

- current audited gallery status remains `24 / 24` passing use cases across:
  - analytics
  - operations
  - workflows
- the current PTC benchmark suite directly times only:
  - `ptc_website_demo`
  - `ptc_incident_triage`
  - `ptc_fraud_investigation`
  - `ptc_vendor_review`
  - `ptc_vendor_review_durable`
- the current headline weighted score only uses three medium lanes:
  - `ptc_incident_triage_medium`
  - `ptc_fraud_investigation_medium`
  - `ptc_vendor_review_medium`

Current narrow-suite headline from the kept artifact:

- addon `ptc.weightedScore.medium`: `0.6616 ms`
- isolate `ptc.weightedScore.medium`: `0.3995 ms`
- addon/isolate gap on the current weighted score: about `1.66x`

Current medium-lane position versus isolate on the same artifact:

- `ptc_website_demo_small`: addon is faster than isolate
- `ptc_incident_triage_medium`: addon is slightly faster than isolate
- `ptc_fraud_investigation_medium`: addon is materially slower than isolate
- `ptc_vendor_review_medium`: addon is faster than isolate

Current gallery breadth versus benchmark breadth:

- cataloged realistic PTC use cases: `24`
- directly benchmarked real PTC lanes today: `4`
- durable benchmarked lanes today: `1`
- current weighted score therefore reflects only a narrow slice of the audited
  gallery

Current catalog composition:

- analytics: `8` cases
- operations: `8` cases
- workflows: `8` cases

Representative unbenchmarked real workloads already present in the gallery:

- analytics:
  - `analyze-revenue-quality.js`
  - `assess-supplier-disruption.js`
  - `triage-model-regression.js`
  - `build-capital-allocation-brief.js`
- operations:
  - `reconcile-marketplace-payouts.js`
  - `analyze-queue-backlog-regression.js`
  - `plan-database-failover.js`
  - `assess-global-deployment-freeze.js`
- workflows:
  - `security-access-recertification.js`
  - `privacy-erasure-orchestration.js`
  - `chargeback-evidence-assembly.js`
  - `approval-exception-routing.js`

The phase-1 suite is good enough to find major bottlenecks. It is not yet good
enough to guarantee that a claimed PTC performance win generalizes across the
real workload gallery.

## Why The Current Suite Is Not Broad Enough

The current representative suite was the right phase-1 target, but it is too
narrow to be the only scorecard for phase 2.

What it does well:

- measures real programmatic tool-calling code instead of toy compute scripts
- covers one public demo lane, one operations lane, one analytics lane, and one
  workflow lane
- captures useful transfer and breakdown metrics
- already exposed the fraud lane as the dominant remaining addon bottleneck

What it does not yet protect against:

- overfitting to one hard lane while quietly regressing the rest of the gallery
- overweighting one workload shape just because it is already in the suite
- missing whole classes of realistic PTC work that the gallery already contains
- claiming parity based on a narrow score even if the 75th percentile or worst
  real workload is still far behind the isolate baseline

Concrete current gaps:

- no balanced benchmark panel across all three audited gallery categories
- no holdout set distinct from the headline scorecard
- no full-gallery performance canary
- no requirement that the benchmarked lanes cover the whole shape matrix of the
  audited examples
- no second durable lane to keep resumable-workflow performance honest
- no benchmark-seed variation beyond one deterministic dataset shape per lane

## Phase-2 Benchmark Requirements

The phase-2 PTC benchmark set must be:

- realistic:
  - every primary lane must come from a checked-in audited use case, or be a
    very close checked-in derivative of one
- diverse:
  - the suite must cover analytics, operations, and workflows evenly enough
    that no single category dominates the engineering score
- broad:
  - the benchmark portfolio must include headline, broad-panel, holdout, and
    gallery-canary layers
- maintainable:
  - not every tier needs to run on every edit, but each tier must have a clear
    role and a standard command path
- anti-overfit:
  - no optimization should be called a win if it improves the headline score
    while broad-panel or holdout signals regress materially
- equivalent:
  - addon, sidecar, and isolate must execute the same guest source with the
    same tool names, payloads, and expected outputs
- compact-answer aware:
  - every lane must still reward local reduction and compact final answers,
    rather than encouraging raw host round-tripping

## Required Coverage Matrix

The suite must explicitly cover the workload shapes that already exist in the
audited gallery.

Minimum coverage counts:

| Workload Shape | Headline Panel | Broad Panel | Holdout / Canary |
| --- | ---: | ---: | ---: |
| first-stage async fanout with `Promise.all` | `>= 3` lanes | `>= 6` lanes | yes |
| derived-ID second-stage fanout | `>= 2` lanes | `>= 4` lanes | yes |
| `Map`-heavy joins or counters | `>= 3` lanes | `>= 6` lanes | yes |
| `Set`-heavy dedupe | `>= 2` lanes | `>= 4` lanes | yes |
| local ranking or comparator sort | `>= 2` lanes | `>= 4` lanes | yes |
| string normalization / token classification | `>= 2` lanes | `>= 5` lanes | yes |
| repeated static property reads over host rows | `>= 4` lanes | `>= 8` lanes | yes |
| time-series / date / chronology reasoning | `>= 1` lane | `>= 3` lanes | yes |
| final action / writeback | `>= 1` lane | `>= 3` lanes | yes |
| durable suspend / resume | separate durable panel | `>= 2` lanes | yes |
| high compaction ratio (`toolBytesIn / resultBytesOut >= 3x`) | `>= 2` lanes | `>= 5` lanes | yes |
| lower-compaction but still realistic local reduction | `>= 1` lane | `>= 3` lanes | yes |

Before any lane is promoted into the phase-2 scorecard, the plan requires a
checked-in coverage matrix showing which examples satisfy which shapes.

## Benchmark Portfolio Design

Phase 2 should move from one weighted score to a benchmark portfolio with four
layers.

### Layer 0: Public Demo Lane

Keep the current website lane:

- `ptc_website_demo_small`

Purpose:

- website-backed public latency story
- fast smoke regression signal
- not the primary engineering score for parity work

Rule:

- the public demo metric must stay artifact-backed
- the public demo metric must not dominate internal performance decisions

### Layer 1: Headline Engineering Panel

This is the smallest panel that should decide whether addon work is moving
toward isolate-comparable PTC latency.

Required composition:

- `6` medium lanes
- `2` analytics
- `2` operations
- `2` workflows
- equal-weight by default
- no single lane allowed to exceed `20%` of the headline score

Recommended initial headline panel:

- analytics:
  - `analytics_fraud_ring`
  - `analytics_revenue_quality`
- operations:
  - `triage-multi-region-auth-outage`
  - `analyze-queue-backlog-regression`
- workflows:
  - `vendor-compliance-renewal`
  - `privacy-erasure-orchestration`

Why this set:

- fraud remains the known hard analytics gap
- revenue quality adds board-style aggregation and ranking
- auth outage keeps the incident-fanout shape already known to be important
- queue backlog regression adds time-series and backlog-explanation work
- vendor review keeps the current read-heavy writeback workflow shape
- privacy erasure adds resumable orchestration and retention-hold logic

### Layer 2: Broad Panel

This panel should be the main engineering source of truth for "real PTC
performance" beyond the narrow headline.

Required composition:

- `12` medium lanes
- `4` analytics
- `4` operations
- `4` workflows
- equal category weighting
- headline panel must be a subset of the broad panel

Recommended initial broad panel:

- analytics:
  - `analytics_revenue_quality`
  - `analytics_fraud_ring`
  - `analytics_supplier_disruption`
  - `analytics_model_regression`
- operations:
  - `triage-multi-region-auth-outage`
  - `reconcile-marketplace-payouts`
  - `analyze-queue-backlog-regression`
  - `plan-database-failover`
- workflows:
  - `security-access-recertification`
  - `vendor-compliance-renewal`
  - `privacy-erasure-orchestration`
  - `chargeback-evidence-assembly`

Why this set:

- it spans all three audited categories evenly
- it adds board summaries, supply shock analysis, ML regression triage,
  reconciliation, failover planning, access review, privacy orchestration, and
  evidence packet assembly
- it introduces more date/time, document/evidence, resumable, and assembly
  shapes than the current suite covers

### Layer 3: Holdout Panel

The holdout panel exists to catch benchmark-positive but portfolio-negative
changes.

Required composition:

- all real use cases not promoted into the broad panel
- reported on every full release benchmark run
- never used as an afterthought or omitted from comparison output

Recommended initial holdout panel:

- analytics:
  - `analytics_market_event_brief`
  - `analytics_enterprise_renewal`
  - `analytics_market_abuse_review`
  - `analytics_capital_allocation`
- operations:
  - `guard-payments-rollout`
  - `stabilize-oncall-handoff`
  - `coordinate-warehouse-exception`
  - `assess-global-deployment-freeze`
- workflows:
  - `approval-exception-routing`
  - `vip-support-escalation`
  - `payout-batch-release-review`
  - `enterprise-renewal-save-plan`

### Layer 4: Full-Gallery Canary

The full-gallery canary should run all `24` audited examples with:

- exact result validation
- at least one timed warm sample per runtime
- lower iteration count than the headline and broad release benchmarks

Purpose:

- ensure performance work is still aligned with the real gallery
- make it obvious when a generic engine change helps only benchmark-selected
  lanes
- create a practical "distribution check" before claiming broad PTC wins

### Layer 5: Sentinel Families

Sentinel families are targeted benchmark families that cover adjacent or
underrepresented workload classes. They are early-warning diagnostics for
generic engine work, not substitutes for real audited examples.

Rules:

- sentinel families do not replace headline, broad, holdout, or gallery-canary
  lanes
- sentinel families are not part of the primary PTC parity score by default
- sentinel families should be reported separately and reviewed whenever a
  change touches generic interpreter, property, collection, string, boundary,
  or result-materialization machinery
- a sentinel family may be promoted into the primary portfolio only if a real
  audited use case later demonstrates the same workload shape

Required initial sentinel families:

- `sentinel_code_mode_search`
  - model the large preloaded typed-API search shape from
    `benchmarks/README.md`
  - concrete variants:
    - `medium_compact`
    - `large_compact`
    - `large_structured`
  - measure:
    - preload memory footprint
    - first-search latency
    - warm repeated-search latency
    - result-size sensitivity
- `sentinel_result_materialization`
  - hold guest computation mostly fixed while varying the amount and structure
    of data that must be reflected back out
  - concrete variants:
    - `medium_summary`
    - `medium_structured`
    - `medium_expanded`
  - measure:
    - addon vs isolate output-materialization cost
    - boundary encode cost
    - result-bytes-out sensitivity
- `sentinel_low_compaction_fanout`
  - model realistic fanout workloads where large intermediate results stay
    inside the runtime but final compaction is weaker than the main PTC gallery
  - concrete variants:
    - `medium_high_compaction`
    - `medium_moderate_compaction`
    - `medium_low_compaction`
  - measure:
    - tool-bytes-in vs result-bytes-out curve
    - guest execution cost under weaker local reduction
    - memory retained during large-intermediate processing

Sentinel-family design constraints:

- each family must isolate one missing workload shape clearly enough that a
  regression has an obvious interpretation
- each family should be small enough to run routinely and stable enough to be
  useful in CI or release comparisons
- each family must avoid benchmark-id keyed behavior just as strictly as the
  real-gallery lanes do

## Durable Benchmark Panel

Phase 2 should not treat durable performance as a one-lane exception.

Required durable panel:

- keep `ptc_vendor_review_durable`
- add `plan_database_failover` as a durable representative lane
- add `privacy_erasure_orchestration` as a durable representative lane if the
  guest structure supports a clean persisted checkpoint

Purpose:

- preserve the current resume advantage over the isolate baseline
- ensure durable work remains representative of real resumable workflows, not
  just one vendor-review shape

## Dataset And Fixture Rules

The suite must stay deterministic, but it must stop being single-shape in the
places where that invites overfitting.

Required dataset rules:

- every promoted lane must ship with deterministic `small`, `medium`, and
  `large` sizes
- the `medium` dataset remains the primary release source of truth
- scale record counts, cardinality, string sizes, and fanout depth rather than
  just numeric loop counts
- keep final results compact and stable in shape across sizes
- keep tool names, payloads, and expected outputs equivalent across runtimes

Required variation rules for the headline panel:

- each headline lane must support:
  - one nominal seeded dataset
  - one skewed seeded dataset
- skewed variants should model real distortions such as:
  - duplicate-heavy joins
  - hotspot cardinality skew
  - noisier and longer strings
  - lower signal-to-noise ratios in source records
  - larger intermediate payloads with the same final answer shape
- sentinel families do not need the full `small` / `medium` / `large` matrix on
  day one, but each family must include:
  - at least one `medium` representative variant
  - any extra result-shape or compaction variants needed to expose the target
    behavior

The goal is not to create adversarial microbenches. The goal is to avoid
teaching the runtime to win only on one "friendly" fixture distribution.

## Scorecards And Metrics

Phase 2 should report multiple scores, not one weighted latency number.

Required score outputs:

- `ptc_headline_score.medium`
- `ptc_broad_score.medium`
- `ptc_holdout_score.medium`
- `ptc_category_score.analytics.medium`
- `ptc_category_score.operations.medium`
- `ptc_category_score.workflows.medium`
- `ptc_durable_score.medium`
- `ptc_p90_lane_ratio.medium`
- `ptc_worst_lane_ratio.medium`
- `ptc_sentinel_family.code_mode_search`
- `ptc_sentinel_family.result_materialization`
- `ptc_sentinel_family.low_compaction_fanout`

Per-lane outputs must continue to include:

- warm median/p95/max
- cold median/p95/max where applicable
- execution-only median/p95/max where applicable
- addon vs isolate ratios
- sidecar vs addon ratios
- tool call count
- awaited call count
- peak outstanding host call count
- tool-bytes-in
- result-bytes-out
- reduction ratio

Required new attribution counters for representative lanes:

- static property reads
- computed property reads
- object allocations
- array allocations
- `Map.get` / `Map.set`
- `Set.add` / `Set.has`
- string case conversion
- literal string search
- regex search / replacement
- comparator-based sort invocations

These counters do not replace the existing breakdowns. They provide the next
layer of evidence needed to decide whether a future optimization is helping
property access, collections, strings, or generic dispatch.

## Anti-Overfitting Rules

These rules are non-negotiable for phase-2 PTC work.

- No optimization may be called a PTC performance win if:
  - `ptc_headline_score` improves
  - but `ptc_broad_score` regresses materially
- No optimization may be called a broad PTC win if:
  - `ptc_broad_score` improves
  - but `ptc_holdout_score` or `ptc_p90_lane_ratio` regresses materially
- No engine change may key behavior on:
  - benchmark lane ids
  - example file names
  - capability names
  - literal field names specific to one benchmark fixture
- Any shape-based optimization must trigger on generic IR, bytecode, runtime
  value-shape, or builtin-usage patterns that can benefit multiple real lanes
- Any benchmark-portfolio change that adds or removes a primary lane must
  report one transition artifact with both the old and new scorecards
- The full-gallery canary must remain green for correctness while the benchmark
  suite evolves
- A change may not be described as a generic interpreter or runtime win unless
  the relevant sentinel-family deltas are also reported
- Sentinel regressions may be accepted only with an explicit note explaining
  why the real-gallery win is worth the tradeoff and why the regression does
  not threaten the intended product shape

## North-Star Targets

This plan should stay relative-first until the first phase-2 broad baseline is
checked in.

Required gains relative to the first phase-2 broad PTC baseline:

| Metric | Floor Target | Stretch Target |
| --- | ---: | ---: |
| addon `ptc_headline_score` warm median vs isolate | `<= 1.35x` | `<= 1.15x` |
| addon `ptc_broad_score` warm median vs isolate | `<= 1.50x` | `<= 1.25x` |
| addon `ptc_holdout_score` warm median vs isolate | `<= 1.60x` | `<= 1.35x` |
| addon `ptc_p90_lane_ratio.medium` vs isolate | `<= 1.75x` | `<= 1.40x` |
| addon `ptc_worst_lane_ratio.medium` vs isolate | `<= 2.25x` | `<= 1.60x` |
| category score spread | within `20%` | within `10%` |
| durable panel median vs isolate | keep current lead | widen current lead |

Public-facing target:

- keep the website speed section backed by `ptc_website_demo_small`
- do not use the website metric as the primary engineering parity score

## Benchmark Runner Profiles

The benchmark portfolio should be practical to run in layers.

Recommended command profile split:

- `ptc_public`
  - website demo lane only
- `ptc_headline_release`
  - `6` headline lanes
  - all runtimes
  - primary local optimization loop
- `ptc_broad_release`
  - `12` broad-panel lanes
  - all runtimes
  - release source of truth for engineering decisions
- `ptc_holdout_release`
  - `12` holdout lanes
  - all runtimes
  - mandatory before claiming broad wins
- `ptc_gallery_canary`
  - all `24` use cases
  - lower iteration count
  - correctness + warm-timing distribution check
- `ptc_sentinel_release`
  - all sentinel families
  - all runtimes
  - required when touching generic runtime or boundary machinery

This can live inside `benchmarks/workloads.ts` if the harness remains
maintainable. If not, split the PTC portfolio into a dedicated module.

## Implementation Milestones

## Milestone 0: Taxonomy And Coverage Audit

Target by end of milestone:

- every audited example is tagged with benchmark-relevant workload-shape
  metadata
- the phase-2 headline, broad, holdout, and durable panels are justified by a
  checked-in coverage matrix

Action items:

- [x] Add workload-shape metadata for all `24` cataloged use cases.
- [x] Record category, async shape, collection shape, string shape, durable
  shape, writeback shape, and compaction expectations for each example.
- [x] Check in a coverage matrix showing why the proposed panels are broad
  enough.
- [x] Record which missing workload shapes are intentionally delegated to
  sentinel families rather than the primary PTC gallery.
- [x] Capture a pilot artifact for the current suite plus a broad-panel dry run.

## Milestone 1: Land The Phase-2 Benchmark Portfolio

Target by end of milestone:

- the repo has a clear benchmark portfolio instead of one narrow weighted score
- the headline and broad panels are derived directly from the audited gallery

Action items:

- [x] Add the new headline panel lanes to the benchmark harness.
- [x] Add the remaining broad-panel lanes to the benchmark harness.
- [x] Add the holdout panel and full-gallery canary modes.
- [x] Add a `ptc_sentinel_release` mode with the three initial sentinel
  families.
- [x] Keep `ptc_website_demo_small` as a separate public metric.
- [x] Add exact expected-result checks for all promoted lanes across runtimes.
- [x] Check in the first phase-2 broad baseline artifact.

## Milestone 2: Add Dataset Variation And Durable Breadth

Target by end of milestone:

- the headline panel is not single-shape
- durable PTC performance is represented by more than one workflow

Action items:

- [x] Add nominal and skewed seeds for each headline lane.
- [x] Add at least two durable representative lanes beyond the current vendor
  durable lane.
- [x] Record which skew patterns each headline lane covers.
- [x] Verify that broad-panel correctness remains stable across seed variants.
- [x] Add the planned result-shape and compaction variants for sentinel
  families.

## Milestone 3: Add Anti-Overfit Scorecards And Regression Policy

Target by end of milestone:

- performance decisions are based on headline, broad, holdout, and worst-lane
  signals together
- benchmark regressions fail for the right reasons

Action items:

- [x] Add `ptc_headline_score`, `ptc_broad_score`, and `ptc_holdout_score`.
- [x] Add category scores plus `p90` and worst-lane ratios.
- [x] Add the sentinel-family score outputs and comparison reporting.
- [x] Update comparison tooling to report the new scorecards.
- [x] Update regression tooling so broad or holdout regressions block claimed
  wins.
- [x] Update `docs/BENCHMARK_FINDINGS.md` to report the new scorecard shape.

## Milestone 4: Add Deeper PTC Attribution

Target by end of milestone:

- engine work can be attributed to property access, collections, strings,
  dispatch, or boundary work using checked-in evidence

Action items:

- [x] Add the required new per-lane operation counters.
- [x] Extend representative addon breakdowns where needed.
- [x] Add any required sidecar or isolate attribution needed to keep addon
  comparisons honest.
- [x] Check in a fresh broad baseline with the deeper attribution fields.

## Milestone 5: Try V8-Inspired Bytecode Stream Optimization

Target by end of milestone:

- the repo has a concrete stack-VM analogue to the main Ignition lessons:
  optimize after lowering, keep hot values out of the materialized stack when
  possible, and reduce dispatch count with audited superinstructions

Guardrail:

- Mustard is a stack VM, not a register-plus-accumulator VM, so phase 2 should
  not copy Ignition literally
- the translation of the V8 ideas for Mustard is:
  - accumulator idea -> virtual `TOS` / `TOS1`
  - register-equivalence idea -> stack-value equivalence and deferred
    materialization of non-observable temporaries
  - bytecode-handler specialization -> superinstructions and back-patched hot
    handlers

Action items:

- [x] Add a post-lowering bytecode optimization stage after current lowering and
  peephole cleanup.
- [ ] Define optimizer flush boundaries at jumps, exception edges, `await`,
  calls, returns, throws, and any source-position boundary that must preserve
  observability.
- [ ] Track virtual `TOS` / `TOS1` in the optimizer so recent values do not
  have to be eagerly written back to the frame stack.
- [ ] Track stack-value equivalence classes so redundant reloads, stores, and
  `Push` / `Pop` churn can be removed when semantics allow it.
- [x] Add a dynamic-instruction counter to the representative addon artifact so
  reduced dispatch is measured directly instead of inferred.
- [ ] Add at least three generic superinstruction candidates derived from broad
  baseline traces rather than from one hand-picked benchmark.
- [x] Keep each optimization class behind a kill switch until broad-panel data
  proves it is a net win.

Success checks:

- [ ] Improve at least `3 / 6` headline lanes.
- [ ] Improve at least one broad-panel lane outside analytics.
- [ ] Keep holdout and durable scorecards flat or better.
- [ ] Report reduced dynamic dispatch count for the lanes that improved.

Reject if:

- [ ] The measured win appears only on `analytics_fraud_ring`.
- [ ] The optimizer needs lane ids, example names, or literal fixture field
  names to trigger.
- [ ] The gain only shows up on synthetic microbenches and not on the audited
  PTC portfolio.

## Milestone 6: Add Shape-Backed Rows And Property ICs

Target by end of milestone:

- repeated static property reads over boundary-decoded host rows are much closer
  to isolate cost without weakening generic object semantics

Why this is a V8-shaped idea:

- the analogue here is hidden-class-plus-inline-cache behavior, not a benchmark
  special case

Action items:

- [ ] Add shared shapes for homogeneous boundary-decoded host rows.
- [ ] Store row payloads in slot arrays or an equivalent compact shape-backed
  representation.
- [ ] Keep a precise fallback to the existing plain-object path on mutation,
  computed-property access that needs it, or any unsupported escape.
- [ ] Add `GetPropStatic` inline caches keyed by program counter plus shape id.
- [ ] Support at least monomorphic then small polymorphic fast paths before
  falling back to generic lookup.
- [ ] Add cache hit, miss, and deopt counters to the benchmark artifact.
- [ ] Measure whether row shaping helps both read-heavy workflows and
  analytics-heavy lanes, not just one fraud dataset.

Success checks:

- [ ] Improve all lanes dominated by repeated static property reads.
- [ ] Show a clear IC hit rate on the broad panel.
- [ ] Keep correctness identical on the full-gallery canary.
- [ ] Avoid material regressions on object mutation or escape-heavy holdouts.

Reject if:

- [ ] The fast path only works for one benchmark's exact record layout.
- [ ] The implementation bakes in tool-specific property names.
- [ ] The change helps property microbenches but does not move broad PTC
  artifacts.

## Milestone 7: Specialize Collections For Real PTC Shapes

Target by end of milestone:

- `Map` / `Set` heavy joins, counters, and dedupe passes are no longer a
  dominant addon tax on analytics and workflow lanes

Action items:

- [ ] Add collection attribution to identify the hottest `Map.get`, `Map.set`,
  `Set.add`, and `Set.has` sites on the broad baseline.
- [ ] Try earlier or eager hashed lookup for boundary-derived string-key
  collections when the value shape is already known to be string-heavy.
- [ ] Try a borrowed or interned lookup-key path that avoids repeated string
  cloning where lifetime and GC constraints allow it.
- [ ] Try a specialized counter-update path for
  `map.set(k, (map.get(k) ?? 0) + 1)` style loops.
- [ ] Try a specialized dedupe path for repeated `set.add(row.prop)` and
  `ids.add(id)` patterns.
- [ ] Preserve insertion-order and equality semantics exactly.
- [ ] Re-run both nominal and skewed datasets so collection wins are not tied to
  one friendly key distribution.

Success checks:

- [ ] Improve fraud plus at least one non-fraud lane that is collection-heavy.
- [ ] Keep broad-panel skewed datasets flat or better.
- [ ] Show reduced collection operation cost in attribution counters.

Reject if:

- [ ] The change only helps one duplicate-heavy fraud seed.
- [ ] The optimization weakens generic `Map` / `Set` semantics.
- [ ] The gain depends on hard-coded expectations about key names or catalog
  structure.

## Milestone 8: Specialize String Normalization And Search

Target by end of milestone:

- audited string-cleanup and token-search patterns are handled much closer to
  their isolate cost without creating benchmark-specific literal hacks

Action items:

- [ ] Inventory the actual normalization and search patterns used across the
  broad and holdout panels.
- [ ] Add a guarded one-pass ASCII fast path for the audited
  `toLowerCase` plus cleanup shape that appears in real PTC lanes.
- [ ] Add a guarded fast path for literal substring search and lightweight token
  classification when semantics are identical to the current builtin behavior.
- [ ] Fall back immediately to the current generic path for non-ASCII,
  unsupported regex behavior, or any pattern outside the audited shape.
- [ ] Report fast-path hit rate and fallback rate in the benchmark artifact.
- [ ] Verify the same implementation helps more than one category of workload.

Success checks:

- [ ] Improve at least two lanes that spend real time in string cleanup.
- [ ] Keep holdout lanes with different text distributions flat or better.
- [ ] Preserve exact result stability across nominal and skewed datasets.

Reject if:

- [ ] The win comes from matching literal suspicious-word lists or fixture text.
- [ ] The implementation silently narrows regex or Unicode behavior.
- [ ] The gain disappears once non-fraud text-heavy lanes are included.

## Milestone 9: Add Feedback-Directed Specialization And Back-Patching

Target by end of milestone:

- the interpreter can cheaply rewrite hot generic handlers into specialized
  handlers using broad-panel evidence rather than static guesswork

Action items:

- [ ] Add program-counter-local feedback for hot property, collection, and
  string sites.
- [ ] Back-patch eligible bytecodes to specialized handlers after a guarded warm
  threshold.
- [ ] Support invalidation when the observed shape becomes too polymorphic or
  when the specialization assumptions stop holding.
- [ ] Report patched-site count, hit rate, invalidation count, and deopt count.
- [ ] Start with a narrow audited set of specializations instead of a large
  speculative matrix.
- [ ] Require every new specialized handler to prove benefit on the broad panel
  before it stays enabled by default.

Success checks:

- [ ] Improve headline and broad scores together.
- [ ] Avoid material churn in holdout or full-gallery correctness behavior.
- [ ] Show that patched sites are generic hot patterns that appear across
  multiple lanes.

Reject if:

- [ ] Back-patching is effectively keyed to benchmark shape ids instead of real
  runtime feedback.
- [ ] The mechanism creates unstable run-to-run artifacts.
- [ ] The win is smaller than simpler non-feedback alternatives already in the
  queue.

## Milestone 10: Consider A Dedicated PTC Tier Only If Generic Path Stalls

Target by end of milestone:

- the repo has a clear go or no-go answer on whether a separate PTC execution
  tier is necessary after the generic engine opportunities have been exhausted

Entry condition:

- do not start this milestone until milestones `5` through `9` have been tried
  or explicitly rejected with artifact-backed evidence

Action items:

- [ ] Define the candidate PTC subset in terms of generic IR, bytecode, and
  runtime-shape properties rather than benchmark names.
- [ ] Require the subset to cover multiple categories and multiple audited
  examples before it is worth prototyping.
- [ ] Prototype a specialized row-oriented and lower-dispatch execution path
  only if the generic path still misses the broad-panel target by a meaningful
  margin.
- [ ] Keep a strict fail-closed fallback to the current generic runtime for any
  unsupported construct.
- [ ] Prove that the tier preserves diagnostics, failure behavior, and
  full-gallery correctness.
- [ ] Reject the tier if it cannot beat the simpler generic-path experiments on
  broad and holdout scorecards.

Success checks:

- [ ] If prototyped, improve headline, broad, and holdout together.
- [ ] Show wins across analytics, operations, and workflows rather than a
  single hard lane.
- [ ] Keep the public website demo metric artifact-backed but non-dominant.

Reject if:

- [ ] The design is benchmark-specific overfitting by another name.
- [ ] The specialization depends on example file names, tool names, or literal
  property names.
- [ ] The maintenance cost is not justified by the broad-panel gain.

## Verification Gates

- [x] `npm run test:use-cases` stays green as the benchmark portfolio evolves.
- [x] `cargo test --workspace` stays green for benchmark-harness and runtime
  changes.
- [x] `npm test` stays green for wrapper and harness changes.
- [x] `npm run lint` stays green for any Rust or Node changes.
- [x] No milestone is complete without a checked-in release artifact for the
  relevant new benchmark layer.
- [x] No broad PTC claim is complete without evidence from both the broad panel
  and the holdout panel.

## Iteration Log

| UTC Timestamp | Summary | Evidence | Blockers |
| --- | --- | --- | --- |
| 2026-04-14T08:04:25Z | Closed milestones 0-4 and started Milestone 5 by adding dynamic instruction counts to the representative PTC artifact plus a post-lowering bytecode optimizer pipeline with conservative stack-noop rewrites behind kill switches. | Commits `316ad5e`, `afd8cf6`, `3890be7`, `ff894d6`, `e4645fe`; verified `node --test tests/node/benchmark-compare.test.js`, `node scripts/audit-ptc-headline-seeds.ts --json`, `cargo test -p mustard --test runtime_debug_metrics`, `cargo test -p mustard stack_noop_peephole`, `npm run bench:ptc:broad`, `npm run bench:ptc:holdout`, `npm run bench:regress:ptc`, `cargo test --workspace`, `npm test`, `npm run lint`, `npm run test:use-cases`; kept artifacts `benchmarks/results/2026-04-14T08-07-26-092Z-ptc_broad_release-release.json`, `benchmarks/results/2026-04-14T07-38-31-068Z-ptc_holdout_release-release.json`, `benchmarks/results/2026-04-14T07-34-21-584Z-ptc_sentinel_release-release.json` | None; the async headline counter collector and a `cargo fmt --check` diff both failed once and were fixed in-loop. |
