# MustardScript Performance Plan

## Objective

Make `mustardscript` materially faster without weakening correctness, limits,
cancellation, snapshot validation, or the "Rust core / thin Node wrapper"
contract. Every optimization milestone must land with benchmark evidence, not
just profiler intuition.

## Audited Baseline

Current audited benchmark inputs and evidence:

- Existing end-to-end harness: `benchmarks/smoke.ts` and `benchmarks/workloads.ts`
- Latest checked-in workload report:
  `benchmarks/results/2026-04-13T04-35-04-948Z-workloads.json`
- Latest findings summary: `docs/BENCHMARK_FINDINGS.md`
- Quick local benchmark check on April 12, 2026:
  `npm run bench:smoke` currently fails because `compute average 52.76ms exceeded 25ms`

Current addon medians from the checked-in workload report:

| Metric | Current |
| --- | ---: |
| `cold_start_small` | `17.81 ms` |
| `warm_run_small` | `16.89 ms` |
| `cold_start_code_mode_search` | `39.00 ms` |
| `warm_run_code_mode_search` | `38.65 ms` |
| `programmatic_tool_workflow` | `42.47 ms` |
| `host_fanout_10` | `0.70 ms` |
| `host_fanout_100` | `6.67 ms` |
| `suspend_resume_20` | `3.64 ms` |

Current sidecar/addon overhead from the same report:

- `cold_start_small`: `1.36x`
- `programmatic_tool_workflow`: `1.12x`
- `host_fanout_100`: `1.44x`

Current relative position versus the isolate baseline:

- `mustard` is much slower on raw execution throughput and host-call-heavy paths
- `mustard` is already better on suspend/resume workloads
- the next phase should focus on 2x to 6x gains in addon mode before trying to
  close the entire isolate gap

## Audited Hotspots

These are concrete code-level reasons the current runtime is paying avoidable
cost:

- `lib/runtime.ts` stores a serialized program buffer in `Mustard`, so warm
  runs still cross the addon boundary with encoded bytecode instead of a native
  compiled-program handle.
- `crates/mustard-node/src/lib.rs` decodes that program buffer on every
  `start_program` call and moves start/resume traffic through JSON strings.
- `crates/mustard/src/runtime/api.rs` validates bytecode again and clones the
  full `BytecodeProgram` into `Runtime::new()` on each execution.
- `crates/mustard/src/runtime/vm.rs` clones the current `Instruction` before
  dispatch, which also clones opcode payloads such as `String`s.
- `crates/mustard/src/runtime/gc.rs` runs full mark/sweep before every
  potentially allocating instruction.
- `crates/mustard/src/runtime/accounting.rs` frequently remeasures whole
  objects/arrays/maps/sets after local mutations instead of applying deltas.
- `crates/mustard/src/runtime/env.rs` resolves names by walking env chains and
  doing string lookups on hot paths.
- `crates/mustard/src/runtime/builtins/collections.rs` backs `Map` and `Set`
  with vectors and linear scans, so membership and update cost scales poorly.
- `crates/mustard-bridge/src/codec.rs` and `crates/mustard-bridge/src/operations.rs`
  use JSON plus base64 snapshot transport in places where addon mode could stay
  binary end to end.

## North-Star Targets

These are cumulative targets for addon mode on the current benchmark suite.
Each has a realistic floor and an ambitious stretch target.

| Metric | Current | Floor Target | Stretch Target |
| --- | ---: | ---: | ---: |
| `cold_start_small` | `17.81 ms` | `<= 8.0 ms` | `<= 4.0 ms` |
| `warm_run_small` | `16.89 ms` | `<= 4.0 ms` | `<= 2.0 ms` |
| `cold_start_code_mode_search` | `39.00 ms` | `<= 18.0 ms` | `<= 10.0 ms` |
| `warm_run_code_mode_search` | `38.65 ms` | `<= 12.0 ms` | `<= 6.0 ms` |
| `programmatic_tool_workflow` | `42.47 ms` | `<= 10.0 ms` | `<= 5.0 ms` |
| `host_fanout_100` | `6.67 ms` | `<= 1.5 ms` | `<= 0.8 ms` |
| `suspend_resume_20` | `3.64 ms` | `<= 2.0 ms` | `<= 1.0 ms` |

North-star sidecar targets:

- keep sidecar slower than addon, but reduce `sidecar/addon` ratio to
  `<= 1.20x` on `warm_run_small`, `programmatic_tool_workflow`, and
  `host_fanout_100`
- preserve the current suspend/resume advantage over the isolate baseline

## Measurement Rules

- [ ] Add a Rust-core microbenchmark suite under `crates/mustard/benches` for:
  parse/lower, `load_program`, `start_bytecode`, VM hot loops, env lookup,
  property access, `Map`/`Set`, structured boundary encode/decode, and
  snapshot dump/load.
- [ ] Keep `npm run bench:smoke` as the fast local gate, but stop using current
  dev-profile absolute thresholds as the main optimization signal.
- [ ] Add a release-profile smoke command and treat release medians as the
  source of truth for performance decisions.
- [ ] Require every optimization PR to attach before/after numbers from
  `npm run bench:workloads` plus the new Rust microbench suite.
- [ ] Record machine metadata, build profile, git SHA, and benchmark fixture
  version in every result artifact.
- [ ] Fail regression checks on relative regressions against a checked-in
  baseline, not on stale absolute budgets.
- [ ] Update `docs/BENCHMARK_FINDINGS.md` only after rerunning the full release
  benchmark suite.

## Milestone 0: Stabilize Benchmarking And Baselines

Target by end of milestone:

- benchmark tooling is trustworthy enough to drive engineering decisions
- smoke benchmarks pass consistently in their intended profile
- there is one checked-in baseline for core, addon, and sidecar performance

Action items:

- [ ] Split benchmark commands into explicit `dev` and `release` variants.
- [ ] Fix or replace the current smoke budgets so they reflect measured reality;
  the current April 12, 2026 smoke run already fails.
- [ ] Add a simple benchmark comparison script that diff-checks medians and p95
  against the latest baseline JSON.
- [ ] Capture a fresh release baseline on the current branch and store it under
  `benchmarks/results/`.
- [ ] Extend `benchmarks/workloads.ts` to include a direct "execution only"
  measurement that excludes compile/decode overhead.
- [ ] Document the benchmark protocol in `benchmarks/README.md`.

## Milestone 1: Remove Structural Start/Run Overhead

Why this comes first:

- warm runs are almost as expensive as cold runs, which strongly suggests the
  runtime is paying start-path overhead on every execution
- the audited code confirms repeated decode, validate, and clone work

Target by end of milestone:

- `warm_run_small <= 10 ms`
- `warm_run_code_mode_search <= 28 ms`
- `programmatic_tool_workflow <= 32 ms`

Action items:

- [ ] Add a native compiled-program handle so `Mustard` can hold decoded,
  validated bytecode instead of a serialized buffer.
- [ ] Make `Runtime` start from shared immutable program state rather than
  cloning `BytecodeProgram` per execution.
- [ ] Validate bytecode once at compile/load boundaries and skip redundant
  validation for trusted in-process compiled handles.
- [ ] Separate "compile + validate", "deserialize + validate", and "execute"
  benchmarks so the win is visible in isolation.
- [ ] Re-run addon, sidecar, and Rust-core benchmarks and store the before/after
  result set.

## Milestone 2: Speed Up The Interpreter Hot Path

Why this comes second:

- once startup churn is removed, interpreter dispatch and local-variable access
  will dominate the remaining addon cost

Target by end of milestone:

- `warm_run_small <= 6 ms`
- `warm_run_code_mode_search <= 20 ms`
- `programmatic_tool_workflow <= 24 ms`

Action items:

- [ ] Stop cloning `Instruction` on every dispatch; borrow from the current
  function’s code slice instead.
- [ ] Introduce a bytecode constant pool or equivalent string interning so hot
  instructions do not carry owned `String` payloads.
- [ ] Add resolved local/cell slot opcodes for lexical bindings and closure
  captures so hot local loads/stores do not walk env chains by name.
- [ ] Keep slow-path name lookup for globals and diagnostics, but move ordinary
  local access onto a separate fast path.
- [ ] Add targeted microbenches for tight loops, local loads/stores, closure
  access, static property get/set, and arithmetic-heavy bytecode.

## Milestone 3: Reduce GC And Accounting Overhead

Why this comes third:

- current GC policy is correctness-first and visibly too eager
- current accounting refreshes are accurate but overly expensive

Target by end of milestone:

- `warm_run_small <= 5 ms`
- `warm_run_code_mode_search <= 16 ms`
- `programmatic_tool_workflow <= 18 ms`
- addon retained heap after the workflow benchmark stays `<= 128 KiB`

Action items:

- [ ] Replace "collect before every maybe-allocating instruction" with an
  allocation-debt or threshold-based trigger.
- [ ] Keep limit enforcement fail-closed even when GC is deferred.
- [ ] Convert array/object/map/set/env/promise accounting from full remeasure to
  incremental deltas wherever the exact byte delta is knowable.
- [ ] Avoid full heap-total recomputation on every collection; reserve full
  recounts for snapshot load, validation repair, or debug assertions.
- [ ] Add benchmark counters for collection count, total GC time, reclaimed
  bytes, and accounting refresh frequency.
- [ ] Add correctness tests for heap-limit failure timing, cancellation, and
  snapshot restore under the new GC trigger policy.

## Milestone 4: Optimize The Addon Host Boundary

Why this comes fourth:

- the current addon bridge is paying JSON serialization and snapshot base64
  costs that do not buy semantic value in-process
- this should directly improve host fanout and workflow workloads

Target by end of milestone:

- `host_fanout_10 <= 0.35 ms`
- `host_fanout_100 <= 2.5 ms`
- `programmatic_tool_workflow <= 12 ms`
- `limit_failure` and `host_failure` recovery medians `<= 8 ms`

Action items:

- [ ] Replace JSON-string start/resume payloads with typed N-API values or a
  binary DTO path.
- [ ] Remove base64 encoding from in-process snapshot transport; keep raw
  `Buffer` snapshots in addon mode.
- [ ] Cache or reuse encoded policy/capability metadata across repeated run and
  resume calls.
- [ ] Benchmark structured boundary encode/decode cost separately from VM
  execution cost.
- [ ] Keep the Node wrapper thin; all semantic validation and snapshot policy
  enforcement must remain in Rust.

## Milestone 5: Upgrade Core Data Structures And Property Fast Paths

Why this comes fifth:

- large collections and property-heavy workloads will remain expensive even
  after the interpreter and bridge are faster

Target by end of milestone:

- `warm_run_code_mode_search <= 12 ms`
- `programmatic_tool_workflow <= 10 ms`
- large `Map`/`Set` membership and update microbenches improve by at least `5x`

Action items:

- [ ] Replace vector-backed `Map`/`Set` lookup/update paths with an
  order-preserving hashed representation that still respects SameValueZero and
  iterator semantics.
- [ ] Add large-collection benchmarks for `Map.get`, `Map.set`, `Map.has`,
  `Set.add`, `Set.has`, `Set.delete`, and iterator throughput.
- [ ] Add fast paths for common static property cases such as array length,
  plain-object own property hits, and builtin prototype method access.
- [ ] Audit builtin helpers for avoidable cloning and temporary allocation.
- [ ] Re-run the keyed-collection and property correctness suites after each
  internal representation change.

## Milestone 6: Reduce Sidecar Transport Overhead

Why this comes sixth:

- sidecar is a deployment choice, not the first optimization target
- once addon mode is faster, the remaining sidecar gap becomes easier to see

Target by end of milestone:

- sidecar/addon ratio `<= 1.20x` on `warm_run_small`
- sidecar/addon ratio `<= 1.20x` on `programmatic_tool_workflow`
- sidecar/addon ratio `<= 1.20x` on `host_fanout_100`

Action items:

- [ ] Replace line-delimited JSON/base64 framing with length-prefixed binary
  messages while keeping a debug-friendly inspection mode.
- [ ] Keep program and snapshot bytes binary throughout the sidecar protocol.
- [ ] Reuse compiled program state inside long-lived sidecar sessions instead of
  recompiling or redecoding unnecessarily.
- [ ] Split sidecar startup, transport, and execution costs into separate
  benchmark metrics.
- [ ] Preserve protocol hardening tests and explicit versioning for any wire
  format change.

## Milestone 7: Bytecode Specialization And Compiler-Level Wins

Why this is last:

- this is higher complexity than the earlier "obvious waste" removals
- it should only start once the simpler structural wins flatten out

Target by end of milestone:

- hit or beat the floor targets in the north-star table
- stretch goal: hit at least half of the stretch targets in the north-star table

Action items:

- [ ] Add peephole passes to remove stack churn such as redundant `Dup`/`Pop`
  pairs and other common bytecode noise.
- [ ] Introduce superinstructions for the most common opcode sequences shown by
  profiling.
- [ ] Constant-fold and simplify obviously static bytecode at lower time.
- [ ] Evaluate monomorphic inline caches for static property reads only after
  benchmark evidence shows property dispatch is still dominant.
- [ ] Keep any specialization optional enough that diagnostics, validation, and
  snapshot compatibility remain understandable.

## Completion Criteria

- [ ] `cargo test --workspace` passes after each substantial milestone.
- [ ] `npm test` passes after each substantial milestone.
- [ ] `npm run lint` passes after each substantial milestone.
- [ ] `npm run bench:smoke` passes in its intended profile.
- [ ] `npm run bench:workloads` shows the milestone’s promised wins.
- [ ] Each completed milestone has a checked-in benchmark artifact and a short
  written result summary.
- [ ] No milestone is marked complete without explicit before/after numbers for
  the metrics it targeted.
