# Smoke Benchmarks

These benchmarks are intentionally small and repeatable. They are meant to give
maintainers a quick signal on regressions in:

- cold startup and compile-and-run latency
- steady-state synchronous execution cost
- host-call overhead relative to an equivalent guest-only baseline
- snapshot dump/load overhead relative to direct suspend/resume
- retained Node heap after repeated snapshot-heavy runs

Run them with:

```sh
npm run bench:smoke
```

The thresholds live in `budgets.json`. Cold start and retained heap still use
broad absolute ceilings, while the host-call and snapshot contracts use ratios
against a direct in-process baseline so they stay meaningful across different
development machines.

## Comparative Benchmark Plan: `jslite` vs V8 Isolates

The current smoke suite is useful for regression detection, but it does not yet
answer the product question that matters most: how `jslite` compares with a V8
isolate embedding for the workloads this project is actually targeting.

### Benchmark Goal

Measure whether `jslite` is competitive for bounded agent-runtime workloads,
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
workloads `jslite` is designed for.

### Baselines

The benchmark matrix should start with:

- `jslite` addon mode
- `jslite` sidecar mode, reported separately because IPC is a deployment tradeoff
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
- persisted state between resumes for `jslite`
- best-effort equivalent async continuation flow for the V8 isolate baseline

Measure:

- suspend overhead per boundary
- resume latency
- serialized snapshot size and round-trip cost for `jslite`
- state rebuild cost for the isolate baseline when it cannot persist equivalent
  continuation state directly

This workload matters because suspend and resume is one of `jslite`'s intended
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
- report `jslite` addon mode and sidecar mode separately rather than blending
  them into one number
- include at least one pure-compute microbenchmark, but treat it as a control
  metric, not the primary decision metric

### Execution Order

The implementation order should be:

1. Add a benchmark runner that emits structured JSON results and machine
   metadata.
2. Add a V8 isolate adapter with the same host-tool contract used by `jslite`.
3. Add the cold-start, code-mode, and programmatic-tool-calling fixtures.
4. Add suspend/resume and failure-cleanup fixtures.
5. Capture results on a dedicated machine class and publish a checked-in
   baseline report.
6. Turn the most stable comparisons into regression budgets for future work.

### Expected Readout

The benchmark should help answer these questions:

- Is `jslite` materially faster to cold-start for short-lived agent steps?
- How much host-call overhead does `jslite` add relative to a V8 isolate?
- Does `jslite`'s suspend/resume model justify its narrower language surface?
- What memory tradeoff do we pay for explicit limits and snapshot support?
- In which workloads should users prefer addon mode, sidecar mode, or a V8
  isolate instead?
