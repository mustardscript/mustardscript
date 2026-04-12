# Testing Gaps and 10x Improvement Plan

## Goal

Make `jslite` testing materially stronger by increasing signal per test-minute, not
by adding a long tail of isolated examples.

The target is:

- faster detection of semantic regressions
- stronger evidence that unsupported features fail closed
- better confidence in snapshot, boundary, and sidecar safety
- better regression localization when failures happen

This plan is based on the current repository state audited on April 12, 2026.

## Current Baseline

The repo already has a stronger baseline than most early runtimes:

- Rust unit and integration tests across parser, runtime, serialization, limits,
  security, GC, and sidecar protocol
- Node end-to-end tests across the public API
- generated Node differential tests via `fast-check`
- curated `test262` pass and unsupported manifests
- hostile-input/property tests in Rust
- libFuzzer targets for parser, IR lowering, bytecode validation and execution,
  snapshot loading, and sidecar protocol
- package/release smoke coverage
- coverage audit tests that assert presence of important regression tests

That means the next 10x gain does not come from "add more unit tests." It comes
from widening the testing model in a few targeted places where one investment
covers many behaviors at once.

## Testing Principles

The next testing work should favor:

- generated or model-based suites over hand-authored one-off examples
- outcome comparison over implementation-detail assertions
- state-machine testing for resumable execution and protocol flows
- negative-space testing for unsupported features and fail-closed behavior
- deterministic replay of complex async/suspension traces
- automatic corpus growth from discovered failures

The next testing work should avoid:

- chasing line coverage percentages as a primary goal
- adding many redundant example tests for already-covered semantics
- expensive integration tests that duplicate faster lower-level checks
- performance benchmarks presented as correctness tests without stable budgets

## Highest-Leverage Gaps

## 1. Semantic differential coverage is still too narrow

### Why this matters

The project's core claim is semantic correctness for a documented subset. The
highest-signal test is still: "for supported programs, `jslite` and Node expose
the same outcome."

The repo already has this, but the generator surface is still relatively small
compared with the language/runtime surface now implemented.

### Current evidence

- `tests/node/property-differential.test.js`
- `tests/node/property-generators.js`
- `tests/node/runtime-oracle.js`
- curated differentials in `tests/node/differential.test.js`

### Gap

The current generators exercise useful slices, but not enough combinations of:

- exceptions plus control flow
- object/array/property-order interactions
- collection semantics after mutation
- async/promise scheduling behavior
- trace-sensitive host capability flows
- snapshot-preserved behavior across resume

### 10x upgrade

Build a feature-family generator matrix instead of a single growing generator.

Recommended families:

- control flow plus abrupt completion
- objects/arrays plus enumeration and key ordering
- keyed collections plus mutation during iteration
- exceptions plus `try`/`catch`/`finally`
- async/promise chains plus guest microtasks
- host capability traces plus deterministic suspension/resume

Each family should generate programs inside a bounded supported subset and
compare canonical outcomes against Node.

### Why this is high-signal

One generator family can replace dozens of brittle examples and catches
combinatorial regressions that hand-written tests miss.

## 2. There is not yet enough metamorphic testing

### Why this matters

Differential tests are strong, but they are not enough on their own. Some
equivalent program rewrites should preserve behavior regardless of the oracle.

Metamorphic testing is fast and good at finding compiler, lowering, and runtime
mismatches without needing new fixtures.

### Gap

The repo has some generated AST and trace work, but not a broad metamorphic
suite that encodes semantics-preserving rewrites as reusable properties.

### 10x upgrade

Add metamorphic properties for transformations such as:

- wrapping an expression in an identity `const temp = expr; temp`
- block insertion where scope is unchanged
- `if (true) a else b` normalization
- alpha-renaming of local bindings
- equivalent literal construction order where semantics should not change
- desugaring-safe rewrites already known to be within the supported subset
- snapshot round-trip insertion at every suspension point

For each generated program:

1. run original
2. apply one rewrite
3. run rewritten form
4. assert canonical equivalence

### Why this is high-signal

This catches bugs in parsing, lowering, bytecode generation, and resumption
without requiring a full second implementation.

## 3. Snapshot/resume needs full state-machine testing

### Why this matters

`jslite` is not just an evaluator; resumable execution is a primary product
claim. Snapshot bugs are high impact because they can break correctness,
replay protection, accounting, or policy enforcement.

### Current evidence

- snapshot round-trip tests exist
- progress and security tests exist
- serialization and policy checks exist

### Gap

Most current coverage is scenario-based. The missing layer is stateful model
testing of the full snapshot lifecycle.

### 10x upgrade

Introduce model-based/state-machine tests for:

- `run -> suspend -> dump -> load -> inspect -> resume`
- repeated load attempts
- invalid payload type transitions
- policy changes across load and resume
- cancellation before suspend, while suspended, and after resume
- mixed success/error resume payloads
- snapshot reuse, replay, and stale-token handling
- accounting invariants before and after load

The model should track:

- whether a snapshot is live, consumed, rejected, or completed
- which capabilities are authorized
- whether limits became stricter or looser
- whether the runtime should complete, suspend again, or fail closed

### Why this is high-signal

This compresses a large correctness and security surface into a bounded set of
state transitions and is far stronger than adding more one-off progress tests.

## 4. Async testing needs schedule exploration, not just examples

### Why this matters

The hardest regressions in runtimes often come from interleavings:

- promise adoption timing
- microtask ordering
- host promise resolution order
- suspension while promises are live
- cancellation racing with resolution

### Gap

The current async tests cover representative behavior, but not systematic
schedule exploration.

### 10x upgrade

Build a deterministic schedule harness that:

- uses explicit deferred host promises
- enumerates small resolution/rejection orderings
- records canonical event traces
- compares `jslite` traces to Node traces where parity is expected

Focus on small exhaustive interleavings rather than broad random timing.

Priority trace classes:

- two or three concurrent promise chains
- `Promise.all`, `allSettled`, `race`, `any`
- nested `await`
- `finally` after rejection and after host failure
- suspend/resume in the presence of pending promise work
- cancellation racing with host completion

### Why this is high-signal

Small state-space schedule exploration finds async bugs faster than large,
slow, timing-dependent integration tests.

## 5. The Node/public-boundary contract needs broader generated testing

### Why this matters

Users consume the Node wrapper, not just the Rust core. Boundary bugs can break
correctness even if the runtime is internally correct.

### Current evidence

- `tests/node/property-boundary.test.js`
- host-boundary and security tests
- type tests and package smoke tests

### Gap

The current boundary properties are useful, but still narrower than the public
surface:

- structured value edge cases
- host error mapping permutations
- lifecycle misuse sequences
- cancellation token misuse
- cross-process `Progress.load()` restoration flows

### 10x upgrade

Expand generated public-API tests into contract families:

- structured inputs/outputs with numeric edge cases and nested shapes
- host-thrown error shapes with missing/extra fields
- progress lifecycle misuse sequences
- snapshotKey and policy mismatch matrices
- cancellation sequences across `run`, `start`, `resume`, and `resumeError`
- mixed sync/async capability behavior under identical guest programs
- type-level contract tests that derive runtime examples from `.d.ts`

### Why this is high-signal

This protects the public API and catches wrapper/addon regressions that Rust
tests cannot see.

## 6. Sidecar testing needs protocol state coverage, not just hostile lines

### Why this matters

The sidecar is the stronger-isolation path. Protocol mistakes can cause
fail-open behavior, replay issues, or host/guest desynchronization.

### Current evidence

- `crates/jslite-sidecar/tests/protocol.rs`
- `crates/jslite-sidecar/tests/hostile_protocol.rs`
- sidecar fuzz target

### Gap

Hostile malformed inputs are covered better than valid-but-adversarial protocol
sequences.

### 10x upgrade

Add sidecar protocol state-machine tests for:

- valid request ordering and illegal ordering
- duplicate IDs
- resume after completion
- resume against mismatched capability/policy sets
- cancellation of unknown or already-completed execution
- partial write/read framing and recovery behavior
- multiple concurrent suspended executions
- protocol equivalence between addon mode and sidecar mode for the same guest
  scenarios

### Why this is high-signal

This tests the real deployment boundary and catches bugs that unit-level codec
tests and malformed-input fuzzing do not reach.

## 7. Unsupported-feature testing should be generated from the language contract

### Why this matters

A fail-closed runtime lives or dies on negative-space coverage. Unsupported
syntax and semantics should be systematically rejected, not only sampled.

### Current evidence

- parser rejection tests
- unsupported `test262` manifest
- unsupported program generators

### Gap

The language contract and rejection coverage are connected informally, not as a
single generated source of truth.

### 10x upgrade

Create a contract-driven rejection matrix derived from `docs/LANGUAGE.md` and
the machine-readable conformance contract. Every documented unsupported class
should have:

- at least one curated regression case
- at least one generated family
- an expected diagnostic category
- explicit confirmation of constructor-time rejection vs runtime rejection

Add a meta-test that fails if a documented unsupported feature class lacks
coverage mapping.

### Why this is high-signal

This prevents silent drift between docs, validator behavior, and tests.

## 8. Cross-layer equivalence testing is missing

### Why this matters

This repo has several layers:

- parser/IR
- bytecode lowering
- direct runtime execution
- Node addon/wrapper
- sidecar transport

The same program should agree across those layers when the surface promises it.

### Gap

Most tests stay within one layer at a time. There is not enough systematic
cross-layer equivalence coverage.

### 10x upgrade

For generated supported programs, assert equivalence across:

- `execute` vs `start`-to-completion
- raw Rust execution vs Node wrapper execution
- addon mode vs sidecar mode
- original snapshot vs dump/load/resume snapshot
- compile/load-program round trip vs fresh compile

Use canonical outcomes and canonical traces rather than raw object identity.

### Why this is high-signal

This exposes wiring and serialization bugs that are invisible when each layer is
only tested in isolation.

## 9. Fuzzing exists, but continuous fuzzing is still underpowered

### Why this matters

The repo already has the right fuzz targets. The bigger gap is execution model,
not target count.

### Current evidence

- `fuzz/fuzz_targets/*`
- `scripts/run-hardening.sh` only checks target buildability

### Gap

CI proves the targets compile, but it does not prove they are being exercised
continuously or with sanitizers.

### 10x upgrade

Add a two-tier fuzzing strategy:

- short budgeted fuzz smoke in CI for selected targets
- long-running sanitizer-backed fuzzing outside normal PR CI on a schedule

Recommended policy:

- PR CI: 30-90 second smoke runs on parser, snapshot, and sidecar protocol
- scheduled jobs: ASan/UBSan fuzzing with persisted corpora and crash artifact
  upload
- seed corpora from `test262`, curated regressions, serialized snapshots, and
  protocol fixtures

### Why this is high-signal

The repo already invested in fuzz targets. Running them meaningfully turns that
investment into real coverage.

## 10. Mutation testing is missing on the most security-sensitive logic

### Why this matters

Passing tests do not always mean the tests would catch the bug that matters.
Mutation testing answers a stronger question: "would the suite fail if the
guard were wrong?"

### Gap

There is no mutation-style check focused on validation, snapshot policy, limits,
or boundary sanitization logic.

### 10x upgrade

Add targeted mutation testing for hot paths such as:

- validator rejection conditions
- snapshot policy authorization checks
- accounting/limit comparisons
- structured boundary reject paths
- sidecar request validation
- progress single-use and authentication checks

Use small targeted mutation runs, not whole-repo brute force mutation testing.

### Why this is high-signal

It validates the quality of the tests around the exact guardrails this project
cares about most.

## 11. Coverage auditing should become contract auditing

### Why this matters

The repo already has coverage-audit tests. That idea should be pushed further.

### Gap

Current audits assert presence of certain regression coverage, but not full
alignment between:

- docs
- supported surface
- unsupported surface
- built-in surface
- tests

### 10x upgrade

Add generated audits that fail when:

- a documented built-in lacks either parity tests or rejection tests
- a public API method lacks misuse-path coverage
- a sidecar protocol method lacks hostile-input and valid-flow coverage
- a new conformance-contract entry is added without the right test bucket

This is less about percentage coverage and more about coverage obligations.

### Why this is high-signal

It turns repository promises into enforceable checks and reduces forgotten test
work when features expand.

## 12. Performance-regression testing should use tight, stable contract checks

### Why this matters

For a runtime, some regressions are not semantic. A change can preserve
correctness while making startup, memory, or suspension cost much worse.

### Gap

The repo has benchmarks and smoke scripts, but not stable regression gates with
careful budgets.

### 10x upgrade

Add narrow performance contract tests for:

- cold start overhead
- host call overhead
- snapshot dump/load cost
- memory growth under bounded object/array workloads

These should be:

- relative, not absolute, when possible
- based on fixed micro-workloads
- quarantined from normal correctness tests if too noisy

### Why this is high-signal

This guards the runtime's product shape without turning the suite into a noisy
benchmark lab.

## Prioritized Roadmap

## Tier 1: Highest ROI

1. Expand semantic differential generators by feature family.
2. Add snapshot/progress state-machine tests.
3. Add deterministic async schedule exploration.
4. Expand generated Node boundary contract tests.
5. Upgrade fuzzing from compile-check-only to executed smoke plus scheduled fuzz.

These five items would produce the biggest real jump in confidence.

## Tier 2: Strong follow-up

1. Add metamorphic property suites.
2. Add sidecar protocol state-machine coverage.
3. Add cross-layer equivalence checks.
4. Add contract-driven unsupported-feature coverage audits.

These make the suite harder to drift and better at finding deep regressions.

## Tier 3: Quality multipliers

1. Add targeted mutation testing for critical guards.
2. Add stable performance-regression contracts.
3. Auto-promote minimized failing generated cases into regression corpus.

These improve long-term trust in the suite, especially as the feature surface
grows.

## Concrete Implementation Suggestions

### A. Add a dedicated test architecture document

Create a small `docs/TESTING_STRATEGY.md` that defines:

- test layers
- what belongs in Rust vs Node vs sidecar tests
- when to use example, property, metamorphic, state-machine, fuzz, or mutation
  testing
- which classes must stay fast in PR CI

This would prevent testing work from becoming ad hoc.

### B. Keep generators split by purpose

Do not build one mega-generator. Maintain separate generators for:

- supported parity
- unsupported validation
- async/interleaving traces
- snapshot lifecycle transitions
- public-boundary values
- sidecar protocol actions

That keeps failures local and shrinking useful.

### C. Standardize canonical outcomes

Use one shared canonical shape across layers for:

- fulfilled values
- thrown guest values
- guest-safe errors
- trace events
- suspension metadata

This removes a lot of assertion noise and makes cross-layer testing easier.

### D. Promote interesting failures into a permanent corpus

Whenever a property/fuzz test finds a bug:

- minimize the case
- add it to a stable regression corpus
- classify it by semantic family

This compounds testing value over time.

## What Not To Do

- Do not respond by adding many random unit tests for isolated expressions.
- Do not add broad flaky timing tests that depend on wall-clock races.
- Do not chase statement coverage as a substitute for semantic confidence.
- Do not put slow fuzzing or broad mutation runs on the critical PR path.
- Do not duplicate the same scenario in Rust, Node, and sidecar unless the
  purpose is explicit cross-layer equivalence.

## Definition Of A Better Test Suite

Testing is 10x better when the repo can make stronger claims such as:

- generated supported programs match Node across broad feature families
- generated unsupported programs fail closed with the documented diagnostic
  class
- snapshot/progress behavior holds across full lifecycle state transitions
- small async interleavings are explored deterministically
- addon mode and sidecar mode agree on canonical outcomes where they should
- hostile byte streams and hostile protocol messages are continuously fuzzed
- critical guards are proven meaningful by mutation-style checks
- docs, conformance contracts, and coverage obligations cannot silently drift

That is the path to materially better confidence, not a larger pile of tests.
