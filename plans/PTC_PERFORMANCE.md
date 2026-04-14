# MustardScript Programmatic Tool Calling Performance Plan

## Objective

Make programmatic tool calling the primary performance target for `mustardscript`.
The current repo already has meaningful benchmark coverage, but the most
important real-world workload is still underrepresented by the synthetic
`programmatic_tool_workflow` fixture. This plan replaces that as the main
optimization target with a benchmark suite derived from the website story and
the audited programmatic tool-call gallery.

Success means:

- benchmark decisions are driven by representative programmatic tool-call flows,
  not by tiny compute scripts
- the public website speed story is backed by checked-in benchmark artifacts,
  not hardcoded numbers
- optimization work is prioritized against the benchmark shapes users are
  actually likely to run: fanout, local joins, dedupe, ranking, compact final
  answers, and selective writeback or resume
- large gains are measured on the new PTC suite before work is called done

## Audited Baseline

Audited inputs and evidence:

- Existing plan: `plans/performance.md`
- Existing workload harness: `benchmarks/workloads.ts`
- Existing benchmark protocol: `benchmarks/README.md`
- Latest checked-in workload artifact:
  `benchmarks/results/2026-04-14T00-42-49-648Z-workloads.json`
- Latest checked-in findings summary: `docs/BENCHMARK_FINDINGS.md`
- Public website speed story: `website/src/components/SpeedSection.tsx`
- Audited use-case docs: `docs/USE_CASE_EXAMPLES.md` and `docs/USE_CASE_GAPS.md`
- Audited use-case harness and tests:
  `scripts/audit-use-cases.ts` and `tests/node/use-cases.test.js`

Verified current gallery state in this worktree:

- `npm run test:use-cases` passes after building the addon in the worktree
- current audited gallery status remains `24 / 24` passing use cases across:
  - analytics
  - operations
  - workflows

Current audited gallery characteristics across the 24 cataloged use cases:

- `30` uses of `Promise.all`
- `39` `await` expressions
- `30` `new Map(...)` sites
- `8` `new Set(...)` sites
- `6` explicit `.sort(...)` sites
- about `10` workflows with an obvious final action/writeback step

Current checked-in release medians from
`benchmarks/results/2026-04-14T00-42-49-648Z-workloads.json`:

| Runtime | `programmatic_tool_workflow` | `host_fanout_100` | `suspend_resume_20` |
| --- | ---: | ---: | ---: |
| addon | `1.44 ms` | `0.40 ms` | `2.27 ms` |
| sidecar | `11.29 ms` | `4.26 ms` | `0.84 ms` |
| isolate | `0.92 ms` | `0.77 ms` | `10.04 ms` |

Current addon phase and boundary medians from the same artifact:

- `runtime_init_only`: `0.031 ms`
- `execution_only_small`: `1.902 ms`
- `startInputs.medium`: `0.339 ms`
- `suspendedArgs.medium`: `0.614 ms`
- `resumeValues.medium`: `0.275 ms`
- `resumeErrors.medium`: `0.244 ms`

Current addon counters for `programmatic_tool_workflow`:

- `gc_collections`: `1`
- `accounting_refreshes`: `0`
- `queued_microtasks`: `0`
- `queued_promise_combinators`: `0`

These numbers are useful, but they no longer reflect the workload we most care
about.

## Why The Current Workflow Benchmark Is Not Representative Enough

The current `createWorkflowSource()` fixture in `benchmarks/workloads.ts` is a
good synthetic sanity check, but it is too narrow to be the primary benchmark
for real programmatic tool calling.

What it does today:

- one synchronous read-only pass over `48` members with `41` active members
- `83` total host calls
- only `3` capability families:
  - `get_team_members`
  - `get_budget_by_level`
  - `get_expenses`
- local arithmetic plus a manual top-k sort

What it does not model well:

- async fanout with `Promise.all`, which is common in the audited gallery
- two-stage fanout where the first tool results determine the second call set
- Map/Set-heavy joins and dedupe over derived IDs
- string normalization, token extraction, and record classification
- read-heavy workflows with one final writeback action
- partial-failure, cancellation, and recovery behavior after some tool results
  have already been materialized
- a benchmarkable gap between “large intermediate data kept inside the sandbox”
  and “small final result returned to the host”

The earlier public-story mismatch is now closed in the current checkout:

- the website speed section reads `website/src/generated/benchmarkData.ts`
  instead of a hardcoded latency literal
- the generated export currently reports `0.155 ms` median / `0.170 ms` p95
  for `ptc_website_demo_small`
- the old `programmatic_tool_workflow` fixture remains useful only as a
  secondary control metric

## PTC Benchmark Requirements

The new primary PTC benchmark is not allowed to be a single toy script. The
suite must satisfy all of the following:

- at least one lane must match the website’s “4-tool orchestration workflow”
  story closely enough to drive the website speed section from real artifacts
- at least one lane must use first-stage parallel fanout with `Promise.all`
- at least one lane must use derived-ID second-stage fanout
- at least one lane must use both `Map` and `Set` for local joins or dedupe
- at least one lane must perform local ranking or top-k reduction
- at least one lane must include a final action capability or explicit durable
  boundary instead of being read-only
- every lane must return a compact final result that is materially smaller than
  the intermediate host data it consumed
- every lane must run equivalently on addon, sidecar, and isolate baselines
  with the same tool names, payloads, and expected outputs

## Benchmark Suite Design

The new suite should become a first-class part of `benchmarks/workloads.ts`
and, if needed for maintainability, be split into a dedicated helper module.

### Lane 1: `ptc_website_demo`

Source inspiration:

- website “4-tool orchestration workflow” story
- `examples/programmatic-tool-calls/operations/triage-production-incident.js`

Shape:

- `4` to `5` tool families
- one initial fanout stage
- compact incident summary result
- deliberately small enough to be used in smoke and on the website

Purpose:

- public demo number
- fast regression signal
- first-result latency that is still obviously a tool-orchestration workload

### Lane 2: `ptc_incident_triage`

Source inspiration:

- `examples/programmatic-tool-calls/operations/triage-multi-region-auth-outage.js`

Shape:

- regional fanout across alerts, SLOs, and error samples
- nested `Promise.all` fanout
- token extraction and string classification
- `Set`-backed dedupe and `Map`-backed token counting
- final ranked operational summary

Purpose:

- primary operations lane
- async orchestration and dedupe pressure
- string and collection behavior representative of on-call/triage workloads

### Lane 3: `ptc_fraud_investigation`

Source inspiration:

- `examples/programmatic-tool-calls/analytics/investigate-fraud-ring.js`

Shape:

- first-stage fetch for base records
- derived-ID extraction into account/entity/card sets
- second-stage parallel fanout from those derived IDs
- `Map` joins across chargebacks, identity signals, and device clusters
- narrative signal extraction plus final disposition

Purpose:

- primary analytics lane
- multi-stage fanout and join-heavy local reduction
- strong pressure on async settlement, structured boundary transport, and
  collection-heavy guest execution

### Lane 4: `ptc_vendor_review`

Source inspiration:

- `examples/programmatic-tool-calls/workflows/vendor-compliance-renewal.js`

Shape:

- parallel read-heavy enrichment
- policy evaluation over multiple evidence sets
- filtered writeback through a final action capability
- small final result with review ID, decision, and evidence summary

Purpose:

- workflow lane with final writeback
- representative of “read a lot, decide locally, then act once” behavior
- ensures the benchmark does not overfit to read-only analysis workloads

## Dataset And Scaling Rules

Each lane must ship with deterministic seeded datasets in three sizes:

- `small`: smoke and website-sized demo
- `medium`: primary release benchmark and engineering source of truth
- `large`: stress and scaling analysis

Scaling rules:

- scale record counts, not just numeric loop bounds
- scale both the number of tool calls and the size of per-tool payloads
- keep final returned results compact even when intermediate payloads grow
- keep the same high-level decision/output shape across sizes so correctness
  comparisons remain stable

Representative target dataset properties for `medium`:

- enough tool-returned records that local reduction is meaningful and visible
- enough fanout that async settlement and host-boundary overhead matter
- enough string/object/Map/Set work that interpreter wins still register
- enough result compaction that the benchmark captures the value of keeping
  intermediate data inside the sandbox

## Metrics That Must Be Captured

The new PTC suite should emit more than latency.

For every lane and runtime, record:

- cold median/p95/max
- warm median/p95/max
- execution-only median/p95/max
- per-lane tool call count
- awaited call count
- peak outstanding host call count reached by the guest program
- total bytes returned from host tools into the sandbox
- total bytes returned from the guest back to the host
- reduction ratio:
  `tool_bytes_into_guest / final_result_bytes_to_host`
- retained heap and RSS after repeated runs
- failure-cleanup timing for mid-workflow host failure and limit failure
- resume timing for any lane with an explicit durable boundary variant

The artifact should also record:

- fixture version
- exact lane weights for any composite score
- git SHA
- machine metadata
- Node version

## Scorecard And Regression Gates

The PTC suite should become the primary performance scorecard for the project.

Required scorecard outputs:

- per-lane medians
- per-size medians
- addon vs sidecar ratios
- addon vs isolate ratios
- a weighted `ptc_weighted_score` for the `medium` lane set

Initial weighting:

- `ptc_incident_triage.medium`: `40%`
- `ptc_fraud_investigation.medium`: `35%`
- `ptc_vendor_review.medium`: `25%`

Rules:

- the old synthetic `programmatic_tool_workflow` should be renamed to make its
  reduced scope explicit, or it should be kept only as a secondary control
  metric
- release regression checks must start reporting on `ptc_weighted_score` and on
  each primary lane individually
- no optimization should be called a performance win if it improves
  `warm_run_small` but regresses the weighted PTC score

## North-Star Targets

This plan is intentionally relative first. The first step is to land the
representative PTC suite and capture a fresh baseline artifact. After that
artifact exists, replace the relative targets below with exact numbers.

Required gains relative to the first representative PTC baseline:

| Metric | Floor Target | Stretch Target |
| --- | ---: | ---: |
| addon `ptc_weighted_score` warm median | `>= 2.5x faster` | `>= 4.0x faster` |
| addon `ptc_weighted_score` cold median | `>= 2.0x faster` | `>= 3.0x faster` |
| sidecar `ptc_weighted_score` warm median | `>= 2.0x faster` | `>= 3.5x faster` |
| sidecar/addon ratio on weighted score | `<= 3.0x` | `<= 1.75x` |
| addon vs isolate gap on weighted score | halve the initial gap | `<= 1.5x` isolate |
| mid-workflow host-failure cleanup median | `>= 2.0x faster` | `>= 3.0x faster` |
| bytes returned to host for primary lanes | no regression | improve compaction ratio by `>= 25%` |

Public-facing target:

- the website speed section should display a number produced directly from the
  checked-in `ptc_website_demo.small` artifact, not from a manually maintained
  literal

## Implementation Milestones

## Milestone 0: Land The Representative PTC Suite

Target by end of milestone:

- the repo has a representative PTC benchmark suite
- the website benchmark story and the checked-in artifact refer to the same
  workload
- a first representative baseline artifact is checked in

Action items:

- [x] Add the four new PTC lanes to the workload benchmark harness.
- [x] Back each lane with deterministic seeded fixtures and `small` / `medium`
  / `large` sizes.
- [x] Add exact expected-result checks so addon, sidecar, and isolate outputs
  are forced to stay aligned.
- [x] Rename or demote the current synthetic `programmatic_tool_workflow`
  metric so it stops reading like the primary real-world benchmark.
- [x] Add artifact metadata for tool-call count, awaited-call count,
  outstanding-call peak, tool-bytes-in, result-bytes-out, and reduction ratio.
- [x] Add a dedicated website-export path so `ptc_website_demo.small` can drive
  `website/src/components/SpeedSection.tsx`.
- [x] Capture and check in the first representative release artifact.

## Milestone 1: Reduce Async Fanout And Promise Settlement Overhead

Why this comes first after the suite lands:

- the audited gallery uses `Promise.all` heavily
- the open performance work in `plans/performance.md` already identifies async
  clone amplification as unfinished
- this is the most direct path to large wins on the new incident and fraud
  lanes

Target by end of milestone:

- addon weighted PTC warm median improves by at least `25%`
- addon isolate gap on the incident/fraud lanes narrows materially

Action items:

- [ ] Finish reducing promise-outcome clone amplification in settlement,
  combinators, and awaiter scheduling.
- [x] Add PTC-specific Rust microbench coverage for:
  - immediate `Promise.all`
  - staged `Promise.all` from derived IDs
  - mixed fulfilled/rejected fanout
  - fanout followed by Map/Set-backed local reduction
- [x] Add benchmark counters for queued microtasks and promise-reaction work so
  improvements can be attributed.
- [x] Re-run workload and Rust microbench suites and record before/after
  numbers for the new PTC lanes.

## Milestone 2: Make Host Boundary Transport Cheap Enough For PTC

Why this is next:

- representative PTC workloads are boundary-heavy by design
- JSON/DTO and sidecar framing costs are still too broad relative to the value
  they provide

Target by end of milestone:

- addon weighted PTC warm median improves by another `25%`
- host-heavy lanes stop being dominated by boundary conversion
- sidecar weighted ratio drops materially

Action items:

- [ ] Replace JSON-string hot paths for addon start/resume traffic with a typed
  or binary boundary path.
- [ ] Reduce JS-side structured DTO creation on hot host-boundary paths.
- [x] Add per-lane boundary breakdowns showing:
  - time in host tool callbacks
  - time in encode/decode
  - time in guest execution between boundaries
- [ ] Keep the Node wrapper thin and keep all semantic validation in Rust.
- [ ] Check in a fresh representative artifact showing the boundary win on the
  PTC lanes, not just on synthetic fanout metrics.

## Milestone 3: Optimize Local Reduction Primitives Used By Real PTC Flows

Why this matters:

- the audited gallery is full of local joins, Maps, Sets, token counting,
  filters, and sorts
- once async and boundary overhead drop, those guest-local operations become
  the next obvious cost center

Target by end of milestone:

- incident/fraud/vendor lanes each improve materially, not just the weighted
  average
- large-lane scaling is flatter than the first representative baseline

Action items:

- [x] Add targeted microbenches for:
  - `Map` join/update
  - `Set` dedupe
  - string token extraction and normalization
  - top-k ranking and comparator sort
  - `Object.fromEntries` and `Array.from` on PTC-shaped data
- [x] Use the new PTC lanes to decide whether string/key interning should be
  prioritized next.
- [x] Audit real example-inspired lanes for avoidable temporary allocation and
  cloning.
- [x] Re-run keyed-collection, builtin, async-runtime, and use-case tests after
  each internal representation change.

## Milestone 4: Sidecar PTC Parity

Why this gets its own milestone:

- the current sidecar/addon gap is still too large on the synthetic workflow
- real-world PTC usage will often want sidecar deployment
- reducing the gap on representative PTC lanes matters more than chasing tiny
  pure-compute wins

Target by end of milestone:

- weighted sidecar/addon ratio `<= 3.0x`
- incident and vendor lanes show materially lower transport overhead

Action items:

- [x] Replace line-delimited JSON/base64 framing with length-prefixed binary
  framing while preserving protocol hardening and versioning.
- [x] Keep program and snapshot bytes binary end to end.
- [x] Add PTC-sidecar metrics that separate:
  - process startup
  - request transport
  - execution
  - response materialization
- [x] Keep protocol tests and hostile-protocol coverage passing after the wire
  format changes.

## Milestone 5: Durable-Boundary PTC Variant

Why this is separate:

- resumability is one of `mustard`’s strongest differentiators
- the new representative suite should include one realistic workflow where
  there is a meaningful pause between enrichment and action

Target by end of milestone:

- at least one PTC lane has a durable suspend/resume variant
- resume performance improves without regressing ordinary warm PTC runs

Action items:

- [x] Add a durable-boundary variant of the workflow lane where the final
  writeback occurs after a persisted checkpoint.
- [x] Record snapshot bytes, detached manifest bytes, and resume-only timing for
  that lane.
- [x] Keep the final-action semantics and failure behavior aligned across addon
  and sidecar.
- [x] Preserve the current suspend/resume advantage over isolates on that lane.

## Measurement Rules

- [x] The source of truth for PTC performance is the release-profile benchmark
  artifact, not the website literal and not the tiny smoke compute numbers.
- [x] Every optimization PR touching runtime, boundary, or sidecar internals
  must include before/after numbers for the primary PTC lanes.
- [x] `docs/BENCHMARK_FINDINGS.md` must call out the weighted PTC score once the
  suite lands.
- [x] The website speed section must not be updated by hand once the export path
  exists.
- [x] No milestone may be checked off without benchmark evidence on the new PTC
  suite plus the relevant correctness tests.

## Verification And Completion Criteria

- [x] `npm run test:use-cases` stays green as the benchmark suite evolves.
- [x] `cargo test --workspace` passes for any runtime or sidecar changes.
- [x] `npm test` passes for any Node-wrapper or benchmark-harness changes.
- [x] `npm run lint` passes for every substantial milestone.
- [x] `npm run bench:workloads:release` produces a checked-in representative PTC
  artifact.
- [x] `npm run bench:rust` is run alongside any engine-level optimization work.
- [x] `website/src/components/SpeedSection.tsx` is driven from measured PTC
  data before this plan is considered complete.

## Iteration Log

| UTC Timestamp | Summary | Evidence | Blockers |
| --- | --- | --- | --- |
| 2026-04-13T21:48:32Z | Created the initial PTC-focused performance plan after auditing the current benchmark harness, website speed story, checked-in benchmark artifacts, and the cataloged programmatic tool-call gallery. | Audited `benchmarks/workloads.ts`, `benchmarks/README.md`, `website/src/components/SpeedSection.tsx`, `docs/USE_CASE_EXAMPLES.md`, `docs/USE_CASE_GAPS.md`, `scripts/audit-use-cases.ts`, `tests/node/use-cases.test.js`, and representative example files. Verified `npm run test:use-cases` in the worktree after building the addon. | Fresh worktree initially lacked local build tooling and addon artifacts. `npm run build` failed until `napi` was provided from the existing root checkout toolchain; after that, the use-case audit passed. |
| 2026-04-14T00:42:49Z | Committed as `a46e120`, added queued/executed microtask attribution for addon debug metrics, extended the Rust-core PTC async bench set with staged derived-ID and Map/Set reduction lanes, refreshed the representative workload artifact, and updated the findings/plan evidence. | Updated `crates/mustard/src/runtime/api.rs`, `accounting.rs`, `async_runtime/promises.rs`, `async_runtime/scheduler.rs`, `gc_trigger_tests.rs`, `crates/mustard/benches/runtime_core.rs`, `benchmarks/workloads.ts`, `benchmarks/README.md`, `docs/BENCHMARK_FINDINGS.md`, and `website/src/generated/benchmarkData.ts`. Verified `cargo test --workspace`, `npm test`, `npm run lint`, `npm run test:use-cases`, `npm run bench:rust`, `npm run bench:workloads:release`, and `npm run bench:compare -- --baseline benchmarks/results/2026-04-14T00-14-51-582Z-workloads.json --candidate benchmarks/results/2026-04-14T00-42-49-648Z-workloads.json`. The kept artifact shows `addon.ptc.weightedScore.medium 0.88 ms -> 0.87 ms` (`-0.9%`), `ptc_incident_triage_medium 0.59 ms -> 0.59 ms` (`-0.3%`), `ptc_fraud_investigation_medium 1.70 ms -> 1.68 ms` (`-1.2%`), and `ptc_vendor_review_medium 0.20 ms -> 0.20 ms` (`+0.1%`) while adding primary-lane microtask counters such as `ptc_incident_triage_medium queued_microtasks=24` / `queued_promise_combinators=23`. | The first `npm run bench:workloads:release` attempt failed in the new PTC counter loop with `ReferenceError: \`regions\` is not defined` because the harness incorrectly tried to seed lane inputs inside `ExecutionContext`; moving those lane inputs back to top-level start options fixed the run in the same iteration. |
| 2026-04-14T01:04:44Z | Committed as `a58b7ec`, reused cached promise accounting for settlement and combinator updates so Promise-heavy hot paths stop remeasuring whole promise driver state, then refreshed the representative artifact plus findings/website evidence. | Updated `crates/mustard/src/runtime/accounting.rs`, `async_runtime/promises.rs`, `async_runtime/reactions.rs`, `docs/BENCHMARK_FINDINGS.md`, `website/src/generated/benchmarkData.ts`, and `benchmarks/results/2026-04-14T00-59-31-034Z-workloads.json`. Verified `cargo test --workspace`, `npm test`, `npm run lint`, `npm run test:use-cases`, `npm run bench:rust`, `npm run bench:workloads:release`, and `npm run bench:compare -- --baseline benchmarks/results/2026-04-14T00-42-49-648Z-workloads.json --candidate benchmarks/results/2026-04-14T00-59-31-034Z-workloads.json`. Rust-core async benches improved materially (`promise_all_immediate_fanout` about `51%` to `52%` faster, `promise_all_settled_immediate` about `4%` to `5%` faster, and the derived-ID / Map-Set lanes about `1%` to `3%` faster), while the kept representative artifact stayed effectively flat on the primary addon PTC lanes (`addon.ptc.weightedScore.medium 0.87 ms -> 0.88 ms`, `ptc_incident_triage_medium 0.59 ms -> 0.59 ms`, `ptc_fraud_investigation_medium 1.68 ms -> 1.69 ms`, `ptc_vendor_review_medium 0.20 ms -> 0.21 ms`). | No correctness blocker remains, but Milestone 1 stays open because the representative addon weighted PTC score did not improve materially. The next likely win is addon boundary transport and/or local reduction work, especially on `ptc_fraud_investigation_medium`. |
| 2026-04-14T01:40:31Z | Committed as `6f7546b`, rejected a typed addon transport experiment after release evidence showed a large regression, then added profiled addon start/resume APIs plus representative PTC boundary breakdowns and refreshed the kept workload artifact, website export, and findings docs. | Updated `crates/mustard-node/src/lib.rs`, `benchmarks/workloads.ts`, `tests/node/addon-boundary-profile.test.js`, `benchmarks/README.md`, `docs/BENCHMARK_FINDINGS.md`, `website/src/generated/benchmarkData.ts`, and `benchmarks/results/2026-04-14T01-36-43-009Z-workloads.json`. Verified `cargo test -p mustard-node`, `node --test tests/node/addon-boundary-profile.test.js tests/node/execution-context.test.js tests/node/sidecar-equivalence.test.js tests/node/security-host-boundary.test.js`, `cargo test --workspace`, `npm test`, `npm run lint`, `npm run test:use-cases`, `npm run bench:workloads:release`, and `npm run bench:compare -- --baseline benchmarks/results/2026-04-14T00-59-31-034Z-workloads.json --candidate benchmarks/results/2026-04-14T01-36-43-009Z-workloads.json`. The kept artifact stayed effectively flat on the primary addon PTC scorecard (`addon.ptc.weightedScore.medium 0.88 ms -> 0.88 ms`, `-0.4%`) while adding `addon.ptc.breakdown` evidence such as `ptc_fraud_investigation_medium guestExecution 1.03 ms` and `boundaryCodec 0.18 ms`. | The typed addon transport candidate `2026-04-14T01-21-47-283Z-workloads.json` regressed the kept baseline materially (`addon.ptc.weightedScore.medium 0.88 ms -> 1.13 ms`, `+28.0%`, `ptc_fraud_investigation_medium 1.69 ms -> 2.21 ms`, `+30.3%`), so it was fully reverted instead of being kept. No external blocker remains; the next likely win is cheaper addon transport and sidecar lane-level attribution. |
| 2026-04-14T01:52:37Z | Committed as `fb0f2ff`, added representative sidecar PTC breakdown metrics by profiling sidecar start/resume responses, measuring client round-trip and decode cost, refreshing the kept workload artifact, and updating the findings/benchmark docs. | Updated `crates/mustard-sidecar/src/lib.rs`, `lib/sidecar.ts`, `benchmarks/workloads.ts`, `tests/node/sidecar-profile.test.js`, `benchmarks/README.md`, `docs/BENCHMARK_FINDINGS.md`, `website/src/generated/benchmarkData.ts`, and `benchmarks/results/2026-04-14T01-49-28-550Z-workloads.json`. Verified `cargo test -p mustard-sidecar`, `node --test tests/node/sidecar-profile.test.js tests/node/sidecar-equivalence.test.js`, `cargo test --workspace`, `npm test`, `npm run lint`, `npm run test:use-cases`, `npm run bench:workloads:release`, and `npm run bench:compare -- --baseline benchmarks/results/2026-04-14T01-36-43-009Z-workloads.json --candidate benchmarks/results/2026-04-14T01-49-28-550Z-workloads.json`. The kept artifact now records `sidecar.ptc.breakdown` with `processStartup 8.49 ms median`, `ptc_incident_triage_medium requestTransport 0.47 ms / execution 0.52 ms / responseMaterialization 1.15 ms`, `ptc_fraud_investigation_medium 0.39 ms / 1.06 ms / 0.72 ms`, and a weighted medium score that stayed effectively flat (`sidecar.ptc.weightedScore.medium 2.67 ms -> 2.65 ms`, `-0.5%`). | No external blocker remains. Milestone 4 attribution is now in place; the next likely work is either the still-unfinished addon transport path or Milestone 5's durable suspend/resume representative lane. |
| 2026-04-14T02:16:01Z | Committed as `741fb78`, added a durable-boundary vendor-review workflow lane plus resume-only benchmark/state accounting for addon, sidecar, and isolate, refreshed the kept workload artifact, and updated the findings/benchmark docs. | Updated `examples/programmatic-tool-calls/workflows/vendor-compliance-renewal-durable.js`, `benchmarks/ptc-fixtures.ts`, `benchmarks/workloads.ts`, `tests/node/ptc-benchmarks.test.js`, `tests/node/durable-ptc-equivalence.test.js`, `benchmarks/README.md`, `docs/BENCHMARK_FINDINGS.md`, `website/src/generated/benchmarkData.ts`, and `benchmarks/results/2026-04-14T02-12-56-211Z-workloads.json`. Verified `node --check benchmarks/workloads.ts`, `node --test tests/node/ptc-benchmarks.test.js tests/node/durable-ptc-equivalence.test.js`, `cargo test --workspace`, `npm test`, `npm run lint`, `npm run test:use-cases`, `npm run bench:workloads:release`, and `npm run bench:compare -- --baseline benchmarks/results/2026-04-14T01-49-28-550Z-workloads.json --candidate benchmarks/results/2026-04-14T02-12-56-211Z-workloads.json`. The kept artifact preserved the ordinary representative sidecar headline (`sidecar.ptc.weightedScore.medium 2.65 ms -> 2.64 ms`, `-0.3%`) while adding durable resume-only evidence on the primary medium lane (`addon 0.48 ms`, `sidecar 0.52 ms`, `isolate 0.56 ms`) plus persisted-state sizes (`addon 14930 B snapshot / 4770 B manifest`, `sidecar 25793 B snapshot / 502 B policy`, `isolate 2764 B carried state`). | No external blocker remains. The large durable lane still trails the isolate baseline (`0.73 ms` vs `0.57 ms`), but the milestone target only required one representative durable lane and the primary medium lane now preserves the suspend/resume edge with correctness coverage. |
| 2026-04-14T02:22:33Z | Committed as `5ca4708`, added the missing Rust-core PTC local-reduction microbenches so Map joins, Set dedupe, token normalization, top-k sort, and `Array.from` / `Object.fromEntries` projection all have direct bench coverage alongside the async fanout lanes. | Updated `crates/mustard/benches/runtime_core.rs` and `benchmarks/README.md`. Verified `npm run bench:rust`, `cargo test --workspace`, `npm test`, `npm run lint`, and `npm run test:use-cases`. The new `ptc_local_reduction` group now records `map_join_update 18.74 ms`, `set_dedupe 19.60 ms`, `token_normalize 68.62 ms`, `top_k_sort 207.23 ms`, and `array_from_object_from_entries 17.17 ms` medians on the current machine, making the missing Milestone 3 cost centers measurable. | No external blocker remains. This slice adds measurement coverage only; it does not yet decide the string/key interning priority or remove any local-allocation overhead from the representative lanes. |
| 2026-04-14T02:43:25Z | At HEAD `894ea23`, used the new representative PTC evidence to decide that string/key interning should not be prioritized ahead of sort and temporary-allocation work, and rejected two benchmark-negative local-allocation experiments instead of landing them. | Re-verified the rejected JS structured-DTO encoder candidate and a Rust callback/root-allocation candidate with `node --test tests/node/structured-encoding.test.js tests/node/security-host-boundary.test.js tests/node/addon-boundary-profile.test.js tests/node/security-progress-load.test.js`, `node --test tests/node/progress.test.js tests/node/execution-context.test.js tests/node/property-boundary.test.js`, `cargo test --workspace`, `npm test`, `npm run lint`, `npm run test:use-cases`, `npm run bench:rust`, `npm run bench:workloads:release`, and `npm run bench:compare -- --baseline benchmarks/results/2026-04-14T02-12-56-211Z-workloads.json --candidate ...`. The kept artifact still shows `ptc_fraud_investigation_medium` dominated by guest execution (`1.00 ms`) over addon boundary codec (`0.18 ms`), while the local-reduction bench group puts `top_k_sort 207.23 ms` and `token_normalize 68.62 ms` well ahead of `map_join_update 18.74 ms`, `set_dedupe 19.60 ms`, and `array_from_object_from_entries 17.17 ms`. Both discarded candidates regressed the representative addon scorecard (`addon.ptc.weightedScore.medium 0.87 ms -> 0.89 ms`, then `0.87 ms -> 0.90 ms`), so neither was kept. | No external blocker remains. The next concrete path is sorting and temporary-allocation work inside local reduction, not string/key interning and not the two rejected transport/allocation experiments. |
| 2026-04-14T02:46:34Z | At HEAD `8bd4a8e`, audited a narrower `Array.prototype.sort` comparator-allocation cleanup after the string/key interning priority decision, then reverted it when the representative workload scorecard stayed flat and the broader hot paths moved the wrong way. | Verified the sort-only candidate with `cargo test -p mustard --test builtin_surface --test cancellation`, `node --test tests/node/builtins.test.js tests/node/cancellation.test.js`, `cargo bench -p mustard --bench runtime_core -- --noplot top_k_sort`, `npm run bench:workloads:release`, and `npm run bench:compare -- --baseline benchmarks/results/2026-04-14T02-12-56-211Z-workloads.json --candidate benchmarks/results/2026-04-14T02-45-51-899Z-workloads.json`. The targeted microbench only improved `ptc_local_reduction/top_k_sort` by about `1%` to `2%`, while the release artifact left the primary addon score effectively flat (`addon.ptc.weightedScore.medium 0.87 ms -> 0.87 ms`, `+0.3%`, p95 `+4.1%`) and regressed several broader addon surfaces such as `warm_run_small 0.91 ms -> 0.96 ms` and `execution_only_small 1.87 ms -> 1.92 ms`. | No external blocker remains. Local-reduction work is still the right area, but this comparator-root tweak is too small and too noisy to keep as the next step. |
| 2026-04-14T03:07:47Z | At HEAD `bc3158a`, audited the real incident, fraud, and vendor lanes for avoidable temporary allocation and cloning, then kept a regex/string-helper cleanup slice that materially improved the representative addon scorecard while reverting a weaker collection-promotion experiment in the same iteration. | Updated `crates/mustard/src/runtime/builtins/regexp.rs`, `strings.rs`, `arrays.rs`, `runtime/mod.rs`, `runtime/serialization.rs`, `runtime/state.rs`, `tests/node/builtins.test.js`, `docs/BENCHMARK_FINDINGS.md`, `website/src/generated/benchmarkData.ts`, and `benchmarks/results/2026-04-14T03-01-24-879Z-workloads.json`. Verified `cargo test -p mustard --test builtin_surface --test async_runtime --test keyed_collections`, `node --test tests/node/builtins.test.js tests/node/async-runtime.test.js tests/node/keyed-collections.test.js`, `npm run test:use-cases`, `cargo test --workspace`, `npm test`, `npm run lint`, `npm run bench:rust`, and `npm run bench:workloads:release`. The kept artifact moved `addon.ptc.weightedScore.medium 0.87 ms -> 0.71 ms` (`-18.4%`), `ptc_incident_triage_medium 0.59 ms -> 0.37 ms`, `ptc_fraud_investigation_medium 1.65 ms -> 1.46 ms`, `ptc_vendor_review_medium 0.23 ms -> 0.22 ms`, and `ptc_website_demo_small 0.16 ms -> 0.15 ms`. The kept addon breakdowns show the win landing in guest execution (`ptc_incident_triage_medium 0.46 ms -> 0.20 ms`, `ptc_fraud_investigation_medium 1.00 ms -> 0.83 ms`) while fraud `boundaryCodec` stayed flat around `0.18 ms`. The same-state Rust benches stayed mostly flat on the local-reduction kernels (`map_join_update 19.65 ms`, `set_dedupe 20.08 ms`, `token_normalize 70.95 ms`, `top_k_sort 211.43 ms`, `array_from_object_from_entries 17.60 ms`), confirming that the representative gain came from removing real temporary-allocation overhead rather than from a broad synthetic-kernel speedup. | No external blocker remains. A temporary `COLLECTION_LOOKUP_PROMOTION_LEN=12` experiment regressed the representative addon weighted score to `0.74 ms` and worsened the fraud medium lane, so it was reverted before the kept workload artifact was regenerated. |
