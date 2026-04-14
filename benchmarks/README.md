# Smoke Benchmarks

These benchmarks are intentionally small and repeatable. They serve two
different roles:

- `npm run bench:smoke` is the fast local dev-profile sanity gate.
- release-profile artifacts are the source of truth for performance decisions
  and before/after comparisons.

## Command Matrix

Run the benchmark commands explicitly by build profile:

```sh
npm run bench:smoke:dev
npm run bench:smoke:release
npm run bench:workloads:dev
npm run bench:workloads:release
npm run bench:rust
```

Convenience aliases:

```sh
npm run bench:smoke
npm run bench:workloads
```

Those aliases currently map to the fast dev smoke run and the release workload
run respectively.

`npm run bench:rust` runs the Rust-core microbenchmark suite in
`crates/mustard/benches/runtime_core.rs`.

## Smoke Scope

The smoke suite gives maintainers a quick regression signal on:

- cold startup and compile-and-run latency
- steady-state synchronous execution cost
- host-call overhead relative to an equivalent guest-only baseline
- snapshot dump/load overhead relative to direct suspend/resume
- retained Node heap after repeated snapshot-heavy runs

The smoke thresholds live in `budgets.json`. They are deliberately broad and
profile-specific:

- dev smoke thresholds are only there to catch obvious local breakage
- release smoke thresholds are tighter, but still secondary to the full release
  workload report
- host-call and snapshot checks stay ratio-based so the gate remains meaningful
  across different development machines
- current snapshot ratio budgets intentionally allow a larger direct-vs-dumped
  gap because live addon `Progress` objects now resume through opaque native
  snapshot handles while `Progress.dump()` still measures full byte
  materialization

Smoke results now emit timestamped JSON artifacts under `benchmarks/results/`
with:

- benchmark kind and build profile
- git SHA
- fixture version
- machine metadata
- median, p95, mean, min, and max timings for each metric

## Workload Benchmarks

`benchmarks/workloads.ts` is the broader benchmark runner for measuring
startup and end-to-end execution latency across representative `mustard`
workloads. It emits a timestamped JSON report under `benchmarks/results/` with
machine metadata and latency summaries for:

- addon cold start vs warm run for a small compute script
- addon cold start vs warm run for a code-mode API search fixture
- addon host-call fanout at 1, 10, 50, and 100 host boundaries
- addon programmatic tool-calling workflow over synthetic team/budget/expense data
- representative programmatic tool-calling lanes derived from the audited
  example gallery:
  - `ptc_website_demo_{small,medium,large}` from
    `examples/programmatic-tool-calls/operations/triage-production-incident.js`
  - `ptc_incident_triage_{small,medium,large}` from
    `examples/programmatic-tool-calls/operations/triage-multi-region-auth-outage.js`
  - `ptc_fraud_investigation_{small,medium,large}` from
    `examples/programmatic-tool-calls/analytics/investigate-fraud-ring.js`
  - `ptc_vendor_review_{small,medium,large}` from
    `examples/programmatic-tool-calls/workflows/vendor-compliance-renewal.js`
- per-lane PTC transfer summaries for actual tool call counts plus
  JSON-encoded tool-bytes-in / result-bytes-out reduction ratios
- addon representative PTC boundary breakdowns for the website-sized public
  demo plus the medium primary lanes, separating host callback time, native
  boundary parse/decode, guest execution, and native boundary encode
- sidecar representative PTC breakdowns for the website-sized public demo plus
  the medium primary lanes, separating process startup, request transport,
  sidecar execution, and response materialization
- per-runtime weighted PTC medium-lane score under `runtime.ptc.weightedScore.medium`
- addon suspend/resume chains with snapshot reloads
- addon boundary-only measurements for start inputs, suspended args, resume
  values, and resume errors across small/medium/large nested payloads
- addon runtime counter snapshots for representative paths, including GC
  collection count, cumulative GC time, reclaimed bytes/allocations, full
  accounting refresh count, and queued/executed microtask breakdowns for
  async-heavy primary PTC lanes
- addon suspend/resume state-size summaries for serialized program bytes,
  dumped snapshot bytes, and retained live `Progress` heap deltas
- addon phase-split measurements for:
  - `runtime_init_only`
  - `execution_only_small`
  - `suspend_only`
  - `snapshot_dump_only`
  - `snapshot_load_only`
  - `apply_snapshot_policy_only`
  - `Progress.load_only`
- sidecar phase-split measurements for:
  - `startup_only`
  - `execution_only_small`
  - `transport_resume_only`
- the same workload classes over the sidecar transport
- the same workload classes over an `isolated-vm` V8 isolate baseline
- retained parent-process heap/RSS deltas after repeated workflow runs
- failure-and-recovery timing for runtime-limit and host-failure cases

The release workload artifact is the one to compare in performance writeups,
plan updates, and optimization commits.

The representative PTC scorecard now treats the three medium-sized primary
lanes as the main real-world signal:

- `ptc_incident_triage_medium`: `40%`
- `ptc_fraud_investigation_medium`: `35%`
- `ptc_vendor_review_medium`: `25%`

`addon.ptc.weightedScore.medium` is the headline rollup for addon optimization
work, and `addon.latency.ptc_website_demo_small` is the lane intended to back
the website’s “4-tool orchestration workflow” speed story.

The workload run also refreshes `website/src/generated/benchmarkData.ts` so the
website speed section reads from the latest representative website-demo lane
artifact instead of a hardcoded number.

## Rust Core Microbench Suite

The Rust-core suite lives under `crates/mustard/benches/runtime_core.rs` and is
the required companion measurement for engine-level optimization work. Run it
with:

```sh
npm run bench:rust
```

The suite covers:

- parse/validate and bytecode lowering
- program deserialize/validate
- runtime initialization with empty, capability-heavy, and input-heavy startup
- validated start vs trusted shared-program execution
- VM hot loops, lexical/env lookup, property access, and `Map`/`Set` hot paths
- representative async fanout hot paths for immediate `Promise.all`,
  staged derived-ID `Promise.all`, mixed fulfilled/rejected fanout, and
  fanout followed by `Map`/`Set`-backed local reduction
- structured boundary decode and suspend-time encode
- snapshot dump and load

When making a runtime performance change, attach before/after numbers from both
`npm run bench:workloads:release` and `npm run bench:rust`. The workload suite
captures end-to-end addon/sidecar behavior; the Rust suite shows where wins or
regressions landed inside the core.

## Comparison Workflow

After capturing a fresh artifact, diff it against the previous checked-in
baseline with:

```sh
npm run bench:compare -- --kind workloads --profile release
```

You can also compare explicit files:

```sh
npm run bench:compare -- --baseline benchmarks/results/old.json --candidate benchmarks/results/new.json
```

The comparison script reports median and p95 deltas for every comparable metric.

For fail-on-regression checks against the latest checked-in baseline artifact,
use:

```sh
npm run bench:regress:workloads
npm run bench:regress:smoke
```

Those commands resolve the candidate artifact from the newest local result, but
they only choose the baseline from git-tracked artifacts. The workload check is
scoped to `addon.*` metrics and currently fails above a `10%` regression; the
release smoke check currently fails above a `50%` regression to absorb the
known noise on tiny startup/compute samples while still catching large shifts.

For the isolate baseline, `suspend_resume_*` is a best-effort comparison that
re-enters a fresh isolate with explicit host-carried state because this harness
does not have equivalent continuation snapshotting for V8 isolates.

The retained-memory section is a post-GC delta, not a precise peak-memory
measurement. Small RSS deltas can be noisy or even negative because OS page
reclamation and allocator reuse are happening concurrently with the benchmark.

## Phase Split Definitions

The new addon-only phase metrics are intentionally narrow:

- `runtime_init_only` runs a precompiled trivial program to isolate startup from compile time
- `execution_only_small` resumes pre-created suspended progress objects so compile/decode work stays out of the timed region
- `suspend_only` measures reaching the first host boundary and materializing a `Progress`
- `snapshot_dump_only` measures `Progress.dump()` on an already-suspended execution
- `apply_snapshot_policy_only` measures the JS-side snapshot authentication and policy rebinding path used by `Progress.load(...)`
- `snapshot_load_only` measures raw native detached-snapshot inspection on an authenticated snapshot from the current addon path
- `Progress.load_only` measures the public JS wrapper path, including authenticated suspended-manifest verification on current dumps, before the post-measurement cleanup step
- `addon.boundary` isolates structured host-boundary work for start inputs,
  suspended args, resume values, and resume errors across small, medium, and
  large nested payloads while keeping compile time and unrelated guest work out
  of the timed region
- `addon.suspendState` records size-oriented suspend-state facts for the
  `suspend_resume_*` fixtures: serialized program bytes, dumped snapshot bytes,
  and retained live `Progress` memory deltas after GC while a batch of
  suspended executions remains live
- `addon.ptc.breakdown` records representative profiled addon runs for the
  website-small lane plus the medium primary lanes, splitting time into
  `hostCallbacks`, `guestExecution`, `boundaryParse`, `boundaryEncode`, and
  combined `boundaryCodec`

The sidecar phase metrics are intentionally simpler and map to protocol stages:

- `startup_only` measures spawning a fresh sidecar process and shutting it down
  cleanly without sending any requests
- `execution_only_small` reuses a precompiled small compute program inside a
  warm sidecar so process startup and compile time stay out of the timed region
- `transport_resume_only` replays an already-suspended minimal snapshot through
  `resume`, so detached snapshot bytes, auth metadata, and stdio request /
  response cost dominate the timed region while resumed guest work stays tiny
- `sidecar.ptc.breakdown` records representative profiled sidecar runs for the
  website-small lane plus the medium primary lanes. `processStartup` reuses
  `sidecar.phases.startup_only`, and each lane entry splits observed time into
  `requestTransport`, `execution`, and `responseMaterialization` (sidecar
  response preparation plus client-side frame decode/copy)

These definitions are not replacements for future Rust microbenches, but they
do make it possible to tell whether time is going into startup, resume
execution, or snapshot handling.

## Comparative Benchmark Plan: `mustard` vs V8 Isolates

The current smoke suite is useful for regression detection, but it does not yet
answer the product question that matters most: how `mustard` compares with a V8
isolate embedding for the workloads this project is actually targeting.

### Benchmark Goal

Measure whether `mustard` is competitive for bounded agent-runtime workloads,
especially:

- code-mode execution where a model writes code against a compact tool surface
  or typed SDK and executes that code in a sandbox
- programmatic tool calling where a model orchestrates many tool calls in code,
  filters intermediate results, and returns only a compact final answer
- resumable host-mediated execution where the runtime must suspend, persist
  state, and resume later with new host input

This comparison should not be framed as "general JavaScript speed." A V8
isolate is expected to win many raw compute microbenchmarks because it has a
full optimizing engine and a much broader language/runtime surface. The purpose
of this plan is to measure the tradeoff for the constrained orchestration
workloads `mustard` is designed for.

### Baselines

The benchmark matrix should start with:

- `mustard` addon mode
- `mustard` sidecar mode, reported separately because IPC is a deployment tradeoff
- a V8 isolate baseline implemented with `isolated-vm` or an equivalent
  minimal isolate embedding

Rules:

- Do not use `node:vm` as the isolate baseline. It is a useful oracle for some
  semantics tests, but it is not the V8-isolate comparison target.
- Keep the host API shape identical across runtimes: same tool names, same
  inputs, same returned data, same synthetic fixtures, same failure cases.
- Separate cold and warm measurements. A runtime that benefits from reuse
  should not hide its cold-start cost.

### Workload Matrix

The first comparative suite should cover five workloads.

#### 1. Cold Start and Single-Run Tool Script

Benchmark a fresh runtime instantiation that compiles and executes a short tool
orchestration script once.

Measure:

- instantiate plus first-result latency
- compile and validation cost
- memory retained after GC

This is the baseline for "one-off agent step" workloads.

#### 2. Code-Mode API Search

Model the Cloudflare-style Code Mode shape: preload a large synthetic typed API
surface, then run a small guest program that searches it and returns a compact
answer.

Fixture:

- synthetic OpenAPI-like dataset with thousands of endpoints and nested schemas
- guest code that filters by tags, path segments, and schema details
- final output limited to a handful of matching operations

Measure:

- startup plus first search latency
- warm repeated search latency
- memory footprint of the preloaded API surface
- result-size sensitivity when the guest returns a small summary vs a larger
  structured result

#### 3. Programmatic Tool Calling Fan-Out

Model the Anthropic-style programmatic tool-calling shape: a guest program
issues many host tool calls, aggregates the results, and emits only a compact
final answer.

Fixture:

- host tools such as `get_team_members`, `get_budget_by_level`, and
  `get_expenses`
- guest code that batches calls, fans out in parallel where supported, sums and
  filters results, and returns only the over-budget entries
- synthetic datasets large enough that naive result reflection into model
  context would be expensive

Measure:

- per-tool-call overhead
- end-to-end latency for 1, 10, 50, and 100 host calls
- overhead of large intermediate results that stay inside the sandbox instead
  of crossing back to the caller
- peak and retained memory during fan-out workloads

#### 4. Suspension, Resume, and Persistence

Measure the cost of stopping at explicit host boundaries and continuing later.

Fixture:

- guest code that suspends on a host capability multiple times
- persisted state between resumes for `mustard`
- best-effort equivalent async continuation flow for the V8 isolate baseline

Measure:

- suspend overhead per boundary
- resume latency
- serialized snapshot size and round-trip cost for `mustard`
- state rebuild cost for the isolate baseline when it cannot persist equivalent
  continuation state directly

This workload matters because suspend and resume is one of `mustard`'s intended
advantages, not just an implementation detail.

#### 5. Limits and Failure Cleanup

Benchmark failure behavior, not only success behavior.

Fixture:

- over-budget compute
- excessive allocations
- too many outstanding host calls
- host tool failure and cancellation cases

Measure:

- time to detect and surface the failure
- memory retained after failure cleanup
- ability to recover and run the next task cleanly

### Metrics

Each benchmark result should record at least:

- median, p95, and max latency
- startup vs warm latency
- peak RSS and post-GC heap usage
- snapshot or state-transfer bytes where applicable
- serialized program bytes and dumped snapshot bytes for suspend-heavy workloads
- success/failure outcome counts
- Node version, machine profile, and benchmark fixture version

Where possible, report both absolute numbers and ratios against the V8 isolate
baseline.

### Fairness Rules

To keep the results interpretable:

- use the same machine class and pin CPU governor settings where possible
- disable network access and external I/O; all host tools should use local
  synthetic fixtures
- pre-generate datasets so parsing fixture files does not dominate the runtime
  being measured
- benchmark language features both runtimes can express without changing the
  workload shape
- report `mustard` addon mode and sidecar mode separately rather than blending
  them into one number
- include at least one pure-compute microbenchmark, but treat it as a control
  metric, not the primary decision metric

### Execution Order

The implementation order should be:

1. Add a benchmark runner that emits structured JSON results and machine
   metadata.
2. Add a V8 isolate adapter with the same host-tool contract used by `mustard`.
3. Add the cold-start, code-mode, and programmatic-tool-calling fixtures.
4. Add suspend/resume and failure-cleanup fixtures.
5. Capture results on a dedicated machine class and publish a checked-in
   baseline report.
6. Turn the most stable comparisons into regression budgets for future work.

### Expected Readout

The benchmark should help answer these questions:

- Is `mustard` materially faster to cold-start for short-lived agent steps?
- How much host-call overhead does `mustard` add relative to a V8 isolate?
- Does `mustard`'s suspend/resume model justify its narrower language surface?
- What memory tradeoff do we pay for explicit limits and snapshot support?
- In which workloads should users prefer addon mode, sidecar mode, or a V8
  isolate instead?
