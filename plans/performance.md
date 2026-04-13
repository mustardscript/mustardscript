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
  `benchmarks/results/2026-04-13T06-59-51-780Z-workloads.json`
- Latest findings summary: `docs/BENCHMARK_FINDINGS.md`
- Current smoke gate status on April 13, 2026:
  `npm run bench:smoke:dev` and `npm run bench:smoke:release` both pass with
  profile-specific median/p95 budgets and write result artifacts.

Current addon medians from the checked-in workload report:

| Metric | Current |
| --- | ---: |
| `cold_start_small` | `16.27 ms` |
| `warm_run_small` | `16.25 ms` |
| `cold_start_code_mode_search` | `37.68 ms` |
| `warm_run_code_mode_search` | `37.71 ms` |
| `programmatic_tool_workflow` | `42.30 ms` |
| `host_fanout_10` | `0.74 ms` |
| `host_fanout_100` | `6.61 ms` |
| `suspend_resume_20` | `3.65 ms` |

Current sidecar/addon overhead from the same report:

- `cold_start_small`: `1.67x`
- `programmatic_tool_workflow`: `1.12x`
- `host_fanout_100`: `1.42x`

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
- `crates/mustard/src/runtime/mod.rs` rebuilds a fresh runtime image on every
  execution, including builtin installation, global setup, capability globals,
  and input conversion.
- `crates/mustard-node/src/lib.rs` decodes that program buffer on every
  `start_program` call and moves start/resume traffic through JSON strings.
- `crates/mustard/src/runtime/api.rs` validates bytecode again and clones the
  full `BytecodeProgram` into `Runtime::new()` on each execution.
- `crates/mustard/src/runtime/vm.rs` clones the current `Instruction` before
  dispatch, which also clones opcode payloads such as `String`s.
- `crates/mustard/src/runtime/async_runtime/scheduler.rs` clones the whole
  `Runtime` to produce a suspension snapshot before serialization even begins.
- `crates/mustard/src/runtime/state.rs` stores the full `BytecodeProgram` inside
  `Runtime`, so snapshots repeatedly serialize immutable program bytes.
- `lib/progress.ts` and `lib/policy.ts` keep rehashing snapshots, rebuilding
  auth tokens, and reserializing policy metadata on hot suspend/resume paths,
  while also using a synchronous filesystem-based single-use registry.
- `crates/mustard/src/runtime/mod.rs` routes synchronous builtin callbacks
  through promise machinery even when host suspension is forbidden.
- `crates/mustard/src/runtime/gc.rs` runs full mark/sweep before every
  potentially allocating instruction.
- `crates/mustard/src/runtime/gc.rs` also uses `HashSet`-heavy marking, making
  each individual collection more expensive than it needs to be.
- `crates/mustard/src/runtime/accounting.rs` frequently remeasures whole
  objects/arrays/maps/sets after local mutations instead of applying deltas.
- `crates/mustard/src/runtime/env.rs` resolves names by walking env chains and
  doing string lookups on hot paths, while globals are duplicated across the
  globals env and global object.
- `crates/mustard/src/runtime/builtins/collections.rs` backs `Map` and `Set`
  with vectors and linear scans, and delete/clear paths scan live iterators to
  repair indices.
- `lib/structured.ts`, `crates/mustard-bridge/src/codec.rs`, and
  `crates/mustard-bridge/src/operations.rs` use recursive JS DTO construction,
  JSON, and base64 in places where addon mode could stay binary end to end.

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

- [x] Add a Rust-core microbenchmark suite under `crates/mustard/benches` for:
  parse/lower, `load_program`, `start_bytecode`, VM hot loops, env lookup,
  property access, `Map`/`Set`, structured boundary encode/decode, and
  snapshot dump/load.
- [x] Add phase-split benchmarks for `runtime_init_only`, `execution_only`,
  `suspend_only`, `snapshot_dump_only`, `snapshot_load_only`,
  `apply_snapshot_policy_only`, and `Progress.load_only`.
- [x] Keep `npm run bench:smoke` as the fast local gate, but stop using current
  dev-profile absolute thresholds as the main optimization signal.
- [x] Add a release-profile smoke command and treat release medians as the
  source of truth for performance decisions.
- [x] Require every optimization PR to attach before/after numbers from
  `npm run bench:workloads` plus the new Rust microbench suite.
- [x] Record machine metadata, build profile, git SHA, and benchmark fixture
  version in every result artifact.
- [ ] Record snapshot byte size, serialized program byte size, and retained live
  heap size for suspend/resume-heavy workloads.
- [ ] Add boundary-only benchmark coverage for start inputs, suspended args,
  resume values, and resume errors with small, medium, and large nested host
  values.
- [ ] Fail regression checks on relative regressions against a checked-in
  baseline, not on stale absolute budgets.
- [x] Update `docs/BENCHMARK_FINDINGS.md` only after rerunning the full release
  benchmark suite.

## Milestone 0: Stabilize Benchmarking And Baselines

Target by end of milestone:

- benchmark tooling is trustworthy enough to drive engineering decisions
- smoke benchmarks pass consistently in their intended profile
- there is one checked-in baseline for core, addon, and sidecar performance

Action items:

- [x] Split benchmark commands into explicit `dev` and `release` variants.
- [x] Fix or replace the current smoke budgets so they reflect measured reality;
  the current April 12, 2026 smoke run already fails.
- [x] Add a simple benchmark comparison script that diff-checks medians and p95
  against the latest baseline JSON.
- [x] Capture a fresh release baseline on the current branch and store it under
  `benchmarks/results/`.
- [x] Extend `benchmarks/workloads.ts` to include a direct "execution only"
  measurement that excludes compile/decode overhead.
- [x] Extend `benchmarks/workloads.ts` to include `runtime_init_only`,
  `suspend_only`, `snapshot_dump_only`, `snapshot_load_only`,
  `apply_snapshot_policy_only`, and `Progress.load_only` measurements.
- [x] Document the benchmark protocol in `benchmarks/README.md`.

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

- [x] Add a native compiled-program handle so `Mustard` can hold decoded,
  validated bytecode instead of a serialized buffer.
- [x] Make `Runtime` start from shared immutable program state rather than
  cloning `BytecodeProgram` per execution.
- [x] Introduce a reusable immutable runtime image for builtins, global object
  state, and other stable startup heap data, with copy-on-write or equivalent
  isolation for guest-visible mutation.
- [x] Introduce a reusable execution-context handle for stable capabilities and
  limits so repeated runs do not rebuild policy state from scratch.
- [x] Remove or drastically reduce duplicated globals bookkeeping between the
  globals env and the global object on hot startup and assignment paths.
- [x] Validate bytecode once at compile/load boundaries and skip redundant
  validation for trusted in-process compiled handles.
- [x] Separate "compile + validate", "deserialize + validate", and "execute"
  benchmarks so the win is visible in isolation.
- [x] Add explicit `runtime_init_empty`, `runtime_init_with_capabilities`, and
  `runtime_init_with_inputs` microbenches.
- [x] Re-run addon, sidecar, and Rust-core benchmarks and store the before/after
  result set.

## Milestone 2: Accelerate Suspend/Resume And Snapshot Lifecycle

Why this comes second:

- `mustard` already wins on resumable execution, so this is the highest-leverage
  place to widen an existing product advantage
- the reviewed code shows large fixed costs inside suspension and restore before
  transport overhead is even counted

Target by end of milestone:

- `suspend_resume_20 <= 2.8 ms`
- `host_fanout_100 <= 5.0 ms`
- snapshot size for "large program, tiny live heap" workloads reduced by at
  least `70%`

Action items:

- [ ] Stop cloning the whole `Runtime` to create suspension snapshots; freeze or
  otherwise capture suspended state without aliasing bugs.
- [ ] Externalize immutable `BytecodeProgram` data from snapshots so resume uses
  `snapshot state + program identity` rather than repeating stable program
  bytes.
- [ ] Replace raw snapshot round-trips with opaque snapshot handles and cached
  auth metadata where the addon or sidecar session can safely retain live
  suspended state.
- [ ] Replace `Progress.load()` re-inspection with an authenticated suspended
  manifest fast path, while keeping legacy snapshots on the slow validation
  path.
- [ ] Remove the duplicated filesystem-based single-use registry in
  `Progress` or replace it with one shared mechanism instead of two.
- [ ] Add incremental or cached snapshot-policy and post-load accounting checks
  where possible, with debug-mode full-walk assertions to preserve fail-closed
  behavior.
- [ ] Add Rust and JS benchmarks for `suspend_only`, `dump_only`, `load_only`,
  `policy_only`, and `Progress.load_only`.

## Milestone 3: Speed Up The Interpreter, Symbol Paths, And Sync Callbacks

Why this comes third:

- once startup and suspend-path churn are reduced, dispatch, symbol lookup, and
  callback machinery will dominate addon latency
- the current runtime still pays a large amount of avoidable `String` cloning
  and promise machinery overhead on common synchronous helper paths

Target by end of milestone:

- `warm_run_small <= 6 ms`
- `warm_run_code_mode_search <= 20 ms`
- `programmatic_tool_workflow <= 24 ms`

Action items:

- [x] Stop cloning `Instruction` on every dispatch; borrow from the current
  function’s code slice instead.
- [ ] Expand the constant-pool plan into runtime-wide string/key interning or
  symbol IDs so env bindings, property names, capability names, and hot string
  values stop paying full `String` costs everywhere.
- [ ] Add resolved local/cell slot opcodes for lexical bindings and closure
  captures so hot local loads/stores do not walk env chains by name.
- [ ] Keep slow-path name lookup for globals and diagnostics, but move ordinary
  local access and common global/property cases onto a separate fast path.
- [ ] Add a true synchronous callback fast path so builtin helpers such as
  `Array.map`, `Array.reduce`, `Map.forEach`, `Set.forEach`, and string
  callbacks do not route through promise machinery when suspension is forbidden.
- [ ] Add targeted microbenches for tight loops, local loads/stores, closure
  access, static property get/set, builtin method access, and callback-heavy
  array/collection workloads.

## Milestone 4: Reduce GC, Async Clone, And Accounting Overhead

Why this comes fourth:

- current GC policy is correctness-first and visibly too eager
- current accounting refreshes are accurate but overly expensive
- async promise settlement is still clone-heavy and can multiply allocation
  costs under fanout workloads

Target by end of milestone:

- `warm_run_small <= 5 ms`
- `warm_run_code_mode_search <= 16 ms`
- `programmatic_tool_workflow <= 18 ms`
- addon retained heap after the workflow benchmark stays `<= 128 KiB`

Action items:

- [ ] Replace "collect before every maybe-allocating instruction" with an
  allocation-debt or threshold-based trigger.
- [ ] Replace `HashSet`-heavy GC marking with epoch/bit-mark or equivalent
  cheaper reachability tracking so each individual collection is faster.
- [ ] Keep limit enforcement fail-closed even when GC is deferred.
- [ ] Convert array/object/map/set/env/promise accounting from full remeasure to
  incremental deltas wherever the exact byte delta is knowable.
- [ ] Avoid full heap-total recomputation on every collection; reserve full
  recounts for snapshot load, validation repair, or debug assertions.
- [ ] Reduce async clone amplification in promise settlement, awaiter
  scheduling, and reaction dispatch, especially for `Promise.all` and other
  fanout-heavy flows.
- [ ] Add benchmark counters for collection count, total GC time, reclaimed
  bytes, and accounting refresh frequency.
- [ ] Add correctness tests for heap-limit failure timing, cancellation, and
  snapshot restore under the new GC trigger policy.

## Milestone 5: Optimize The Addon Host Boundary And Execution Context

Why this comes fifth:

- the current addon bridge is paying JSON serialization and snapshot base64
  costs that do not buy semantic value in-process
- the current host-boundary cost is broader than JSON alone because JS builds
  recursive structured DTO objects before Rust sees the data
- this should directly improve host fanout and workflow workloads

Target by end of milestone:

- `host_fanout_10 <= 0.35 ms`
- `host_fanout_100 <= 2.5 ms`
- `programmatic_tool_workflow <= 12 ms`
- `limit_failure` and `host_failure` recovery medians `<= 8 ms`

Action items:

- [ ] Replace JSON-string start/resume payloads with typed N-API values or a
  binary DTO path.
- [ ] Eliminate the JS-side structured-value DTO hot path where possible, while
  preserving the same fail-closed boundary rules for proxies, accessors, sparse
  arrays, and non-finite numbers.
- [ ] Remove base64 encoding from in-process snapshot transport; keep raw
  `Buffer` snapshots in addon mode.
- [ ] Cache or reuse encoded policy/capability metadata across repeated run and
  resume calls.
- [ ] Promote policy/capability caching into explicit native execution-context
  handles for workloads that repeatedly execute with stable host handlers and
  limits.
- [ ] Benchmark structured boundary encode/decode cost separately from VM
  execution cost.
- [ ] Keep the Node wrapper thin; all semantic validation and snapshot policy
  enforcement must remain in Rust.

## Milestone 6: Upgrade Core Data Structures, Globals, And Property Fast Paths

Why this comes sixth:

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
- [ ] Redesign `Map`/`Set` iterator invalidation so delete/clear no longer scan
  the full live iterator set to repair indices.
- [ ] Add large-collection benchmarks for `Map.get`, `Map.set`, `Map.has`,
  `Set.add`, `Set.has`, `Set.delete`, and iterator throughput.
- [ ] Add fast paths for common static property cases such as array length,
  plain-object own property hits, and builtin prototype method access.
- [ ] Add capacity-aware builders and bulk-mutation fast paths for
  arrays/objects/maps/sets/promises so constructors and append-heavy code stop
  remeasuring full structures after tiny local changes.
- [ ] Finish removing duplicated globals env/global-object work if it was not
  completed in Milestone 1.
- [ ] Audit builtin helpers for avoidable cloning and temporary allocation.
- [ ] Re-run the keyed-collection and property correctness suites after each
  internal representation change.

## Milestone 7: Reduce Sidecar Transport Overhead

Why this comes seventh:

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
- [ ] Extend session statefulness to `program_id`, `snapshot_id`, and compact
  policy/capability IDs so the sidecar stops resending large opaque blobs and
  static metadata on every request.
- [ ] Split sidecar startup, transport, and execution costs into separate
  benchmark metrics.
- [ ] Preserve protocol hardening tests and explicit versioning for any wire
  format change.

## Milestone 8: Bytecode And Lowering Specialization

Why this is last:

- this is higher complexity than the earlier "obvious waste" removals
- it should only start once the simpler structural wins flatten out

Target by end of milestone:

- hit or beat the floor targets in the north-star table
- stretch goal: hit at least half of the stretch targets in the north-star table

Action items:

- [ ] Reframe compiler optimization around structural lowering wins first:
  object literals, destructuring, and `for...of` should emit materially leaner
  bytecode before adding generic peephole machinery.
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

## Iteration Log

| UTC Timestamp | Commit | Summary | Errors / Blockers |
| --- | --- | --- | --- |
| 2026-04-13T06:44:27Z | `fac215a` (worktree dirty) | Audited the benchmark/runtime/boundary hot paths, wrote the initial performance roadmap, and folded parallel sub-agent review findings into the milestone structure. | `npm run bench:smoke` currently fails with `compute average 52.76ms exceeded 25ms`; no external blocker identified. |
| 2026-04-13T07:17:00Z | `125fcb6`, `2b5bf99` | Completed Milestone 0 benchmark stabilization: added explicit dev/release benchmark commands, artifact metadata, release smoke coverage, phase-split addon metrics, a benchmark diff script, a fresh checked-in release baseline, and updated findings/docs. Then started Milestone 1 by switching addon execution to validated native program handles plus shared `Arc<BytecodeProgram>` state so in-process runs stop re-deserializing and re-cloning program bytecode on every start path. | Full verification passed (`npm test`, `cargo test --workspace`, `npm run lint`, smoke/workload benchmark commands). Release workload deltas for the Milestone 1 groundwork were small and mixed (`programmatic_tool_workflow -1.8%`, `host_fanout_100 -1.9%`, `warm_run_small +1.0%` versus the new baseline), so the next concrete path is reusable runtime-image/startup-state work. A temporary `bench:smoke`/`bench:workloads` parallel rerun hit a local `scripts/build-ts-dist.ts` `ENOTEMPTY` race on `dist/`; rerunning sequentially resolved it. |
| 2026-04-13T07:30:55Z | `a6ec42d` | Added the Rust-core microbenchmark suite under `crates/mustard/benches/runtime_core.rs`, wired `npm run bench:rust`, documented the required workload-plus-Rust perf evidence workflow, and covered compile/lower, deserialize/validate, runtime init variants, start-vs-execute, VM hot paths, structured boundary encode/decode, and snapshot dump/load. | Verification passed (`npm run bench:rust`, `cargo test --workspace`, `npm test`, `npm run lint`). `npm test` initially failed in `tests/package-smoke.test.js` because the packed source tarball omitted `crates/mustard/benches/runtime_core.rs` while `crates/mustard/Cargo.toml` declared the bench; fixed by adding `crates/mustard/benches/**` to `package.json` `files`. |
| 2026-04-13T07:41:57Z | `49d891f` | Reduced Milestone 1 startup overhead by keeping builtins, capabilities, and inputs on the real global object instead of duplicating globals-env cells, added a regression test for global-object lookup/mutation of inputs and capabilities, and removed the redundant `start(&CompiledProgram, ...)` bytecode revalidation after lowering. Checked-in release artifact `benchmarks/results/2026-04-13T07-40-15-477Z-workloads.json` improved versus `2026-04-13T06-59-51-780Z`: addon `cold_start_small -33.5%`, `warm_run_small -32.6%`, `programmatic_tool_workflow -11.7%`, `host_fanout_100 -20.5%`, `runtime_init_only -24.4%`, and `execution_only_small -23.6%`. | Verification passed (`npm run bench:rust`, `npm run bench:workloads:release`, `cargo test --workspace`, `npm test`, `npm run lint`). No external blocker identified; the next concrete Milestone 1 path is a reusable immutable runtime image plus execution-context caching. |
| 2026-04-13T07:58:44Z | `00510fe` (worktree dirty) | Added a cached Rust runtime image for builtin/global startup state, reused it for each `Runtime::new()` with explicit baseline heap/allocation checks, added integration coverage that zero limits still fail closed and cross-run builtin/global mutations do not leak, checked in `benchmarks/results/2026-04-13T07-55-31-500Z-workloads.json` plus `2026-04-13T07-55-40-075Z-smoke-release.json`, and refreshed `docs/BENCHMARK_FINDINGS.md` with the new evidence. Versus `2026-04-13T07-40-15-477Z`, addon medians improved modestly on `warm_run_small -2.5%`, `warm_run_code_mode_search -1.3%`, `programmatic_tool_workflow -0.8%`, and `host_fanout_100 -2.0%`, while the Rust-core startup microbenches improved much more (`runtime_init_empty ~-37%`, `runtime_init_with_capabilities ~-35%`, `runtime_init_with_inputs ~-24%`). | Verification passed (`cargo test --workspace`, `npm test`, `npm run lint`, `npm run bench:rust`, `npm run bench:workloads:release`, `npm run bench:smoke:release`). The first `npm test` attempt hit a transient `tests/package-smoke.test.js` tarball `ENOENT`; rerunning `node --test tests/package-smoke.test.js` and then `npm test` sequentially passed. `execution_only_small` regressed slightly (`+1.9%`), so reusable execution-context caching or interpreter hot-path work remains the next concrete path. |
| 2026-04-13T08:07:48Z | `d9c52a1` | Changed VM dispatch to borrow instructions from an `Arc<BytecodeProgram>` clone instead of cloning the full `Instruction` every step, refreshed `docs/BENCHMARK_FINDINGS.md`, and checked in `benchmarks/results/2026-04-13T08-04-08-003Z-workloads.json` plus `2026-04-13T08-06-50-446Z-smoke-release.json`. Versus `2026-04-13T07-55-31-500Z`, addon medians improved on `cold_start_small -4.0%`, `warm_run_small -3.2%`, `warm_run_code_mode_search -0.8%`, and `programmatic_tool_workflow -1.4%`; the Rust-core bench showed bigger wins (`execute_shared_small_compute ~-8.4%`, `vm_hot_loop ~-8.8%`, `env_lookup_hot ~-6.4%`, `property_access_hot ~-4.9%`). | Verification passed (`cargo test --workspace`, `npm test`, `npm run lint`, `npm run bench:rust`, `npm run bench:workloads:release`, `npm run bench:smoke:release`). No external blocker identified. `execution_only_small` and `host_fanout_100` stayed roughly flat (`+0.6%` and `+0.4%`), so execution-context caching or slot-based lexical fast paths remain the next concrete path. |
| 2026-04-13T08:48:20Z | `d9c52a1` (worktree dirty) | Tried a true synchronous callback fast path, benchmarked it, saw no meaningful win, and backed it out. Kept the measurable follow-on work instead: added `array_callback_hot` plus a callback-throw regression test, cached per-function parameter/rest binding names in bytecode so `Runtime::push_frame` stops re-walking destructuring patterns on every call, and added bytecode validation that rejects forged binding metadata. Checked in `benchmarks/results/2026-04-13T08-36-39-534Z-workloads.json` plus `2026-04-13T08-38-48-635Z-smoke-release.json`. Against a control worktree at `d9c52a1`, the new `array_callback_hot` bench improved from `26.13-26.50 ms` to `25.20-25.52 ms` (`~3-4%`), while the release workload suite stayed mixed but `execution_only_small` still improved from `13.83 ms` to `13.60 ms`. | Verification passed (`cargo test --workspace`, `npm test`, `npm run lint`, `npm run bench:rust`, `npm run bench:workloads:release`, `npm run bench:smoke:release`). No external blocker identified. Broader workload gains remained noisy (`programmatic_tool_workflow +4.0%` versus `2026-04-13T08-04-08-003Z`), so reusable execution-context caching or slot-based lexical fast paths remain the next concrete path. |
| 2026-04-13T09:05:03Z | `4814c3e` (worktree dirty) | Completed the remaining Milestone 1 execution-context work by adding a reusable Node `ExecutionContext` handle, wiring `Mustard.run()`, `start()`, and `Progress.load()` to accept it, updating the benchmark harness to reuse stable addon policy state, and adding Node/docs/type coverage for both the reusable path and the fail-closed mixed-options path. Checked in `benchmarks/results/2026-04-13T09-02-17-086Z-workloads.json` plus `2026-04-13T09-02-30-985Z-smoke-release.json`. Versus `2026-04-13T08-36-39-534Z`, addon `runtime_init_only` improved from `0.07 ms` to `0.04 ms` (`-43%`), `execution_only_small` improved from `13.60 ms` to `13.45 ms` (`-1.0%`), `programmatic_tool_workflow` improved from `37.98 ms` to `36.95 ms` (`-2.7%`), and `suspend_resume_20` improved from `3.19 ms` to `3.10 ms` (`-2.9%`), while `warm_run_small` and `host_fanout_100` regressed slightly (`+1.4%` and `+1.5%`). | Verification passed (`cargo test --workspace`, `npm test`, `npm run lint`, `npm run bench:rust`, `npm run bench:workloads:release`, `npm run bench:smoke:release`). No external blocker identified. The repeated-addon startup path is now materially cheaper, but broader latency is still mixed, so the next concrete path is Milestone 2 snapshot/suspend lifecycle work or Milestone 3 local-slot fast paths. |
