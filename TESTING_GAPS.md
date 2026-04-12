# Testing Gaps and Execution Plan
[PLAN HAS BEEN COMPLETED]
[PLAN DONE AT COMMIT 123f148]

## Purpose

Make `jslite` testing materially stronger by increasing signal per test-minute,
not by adding a long tail of isolated examples.

This document is an execution plan for the current repository state audited on
April 12, 2026. It is intended to be used as a living checklist.

## Verified Foundations Already In Place

- [x] Rust unit and integration coverage across parser, runtime, serialization,
      limits, security, GC, and sidecar protocol
- [x] Node end-to-end coverage across the public API
- [x] Generated Node differential tests via `fast-check`
- [x] Curated `test262` pass and unsupported manifests
- [x] Rust hostile-input and property suites
- [x] Fuzz targets for parser, IR lowering, bytecode validation and execution,
      snapshot loading, and sidecar protocol
- [x] Package and release smoke coverage
- [x] Coverage-audit tests that assert presence of important regression tests
- [x] A machine-readable conformance contract already exists in
      `tests/node/conformance-contract.js`
- [x] Canonical outcome normalization already exists in
      `tests/node/runtime-oracle.js`
- [x] A first metamorphic/generated AST layer already exists in
      `tests/node/ast-conformance.js`
- [x] Property snapshot round-trip coverage already exists in
      `crates/jslite/tests/property_snapshot_roundtrip.rs`
- [x] Sidecar happy-path and hostile protocol coverage already exists in
      `crates/jslite-sidecar/tests/protocol.rs` and
      `crates/jslite-sidecar/tests/hostile_protocol.rs`

## Product Claims The Test Suite Must Prove

Every major testing investment should strengthen one or more of these claims:

1. Supported guest programs produce the same observable outcome as Node
   wherever `jslite` promises parity.
2. Unsupported syntax and semantics fail closed at the correct phase with the
   correct error category.
3. The structured host boundary preserves allowed values exactly and rejects
   forbidden values consistently.
4. Limits, cancellation, and guest-safe errors are deterministic and do not
   leak host internals.
5. Snapshot and progress flows are correct, single-use, authenticated, and safe
   across suspend, dump, load, and resume.
6. Addon mode and sidecar mode agree on semantics for the same guest program
   unless a difference is explicitly documented.

Anything that does not materially strengthen one of those claims is lower
priority.

## Repo-Aligned Constraints

- Extend the existing conformance contract in
  `tests/node/conformance-contract.js`; do not create a competing second source
  of truth unless the current file becomes structurally insufficient.
- Extend the existing canonical outcome and trace normalization in
  `tests/node/runtime-oracle.js`; do not introduce a parallel canonicalization
  layer without a specific need.
- Keep the Node test layer aligned with the current repository style. The
  existing Node test surface is CommonJS JavaScript, not a new TypeScript-first
  testing framework.
- Keep generators split by purpose. Do not build one mega-generator.
- Respect the current sidecar protocol contract in `docs/SIDECAR_PROTOCOL.md`.
  Today that contract covers `compile`, `start`, and `resume`. Hard-stop
  behavior is process termination, not an in-band `cancel` protocol method.
- Prefer contract audits over line-coverage goals.
- Prefer deterministic schedule exploration over wall-clock timing tests.
- Prefer generated, stateful, and cross-layer checks over many new one-off
  examples.

## Priority Order

1. Tighten contract-driven parity coverage.
2. Promote fail-closed rejection coverage to a first-class contract.
3. Add state-machine coverage for snapshot and progress lifecycle rules.
4. Expand boundary and limits coverage as explicit public contracts.
5. Add deterministic async schedule exploration.
6. Upgrade fuzzing from compile-check-only to executed smoke plus scheduled
   sanitizer runs.
7. Add shared cross-layer and addon-vs-sidecar equivalence coverage.
8. Expand sidecar protocol state coverage within the documented protocol.
9. Harden contract audits so docs and tests cannot drift silently.
10. Only then spend time on metamorphic expansion, mutation checks, and
    performance micro-contracts.

## Tier 1: Highest ROI

### 1. Contract-Driven Parity Expansion

Use the existing conformance contract and runtime oracle. The next gain is to
split supported differential coverage by semantic family instead of continuing
to grow one broad generator.

Checklist:

- [x] Split supported parity generation into independent feature families with
      isolated shrinking and failure reporting.
- [x] Add a control-flow family covering loops, `break`, `continue`, and abrupt
      completion interactions.
- [x] Add an exceptions family covering `throw`, `try`, `catch`, `finally`,
      nested `finally`, and rethrow behavior.
- [x] Add an objects/arrays family covering property order, holes, enumeration,
      `Object.keys` / `values` / `entries`, and `JSON.stringify`.
- [x] Add a keyed-collections family covering `Map` / `Set`, SameValueZero,
      insertion order, and mutation during iteration.
- [x] Add an async/promise family covering documented promise chains,
      combinators, and guest microtask behavior.
- [x] Add a capability-trace family covering deterministic console and host
      capability traces plus suspend/resume behavior.
- [x] Make every generated failure print the seed, minimized program, and
      canonical diff.
- [x] Promote minimized parity failures into stable regressions.

Done when:

- [x] Every family runs independently.
- [x] Every failure is reproducible from one seed.
- [x] Every failure renders a canonical outcome or trace diff instead of a raw
      object mismatch.

### 2. Fail-Closed Rejection Matrix

The repository is explicitly fail-closed. Unsupported-feature coverage belongs
in Tier 1, not as a late follow-up.

Checklist:

- [x] Extend `tests/node/conformance-contract.js` so unsupported entries record
      expected phase and expected diagnostic category.
- [x] Audit `docs/LANGUAGE.md` against the conformance contract and add missing
      unsupported classes to the machine-readable contract.
- [x] Add generated rejection families for unsupported syntax and validator-only
      exclusions that are currently only sampled.
- [x] Add curated regressions where exact failure class or phase is especially
      important.
- [x] Add contract-audit coverage that fails when a documented unsupported class
      has no mapped test coverage.
- [x] Assert phase and category rather than brittle full-message equality,
      except where the exact wording is itself part of the contract.

Priority unsupported buckets:

- [x] Modules and dynamic import
- [x] `eval` and `Function`
- [x] Classes
- [x] Generators and `yield`
- [x] `var`
- [x] Default parameters and default destructuring
- [x] Free `arguments`
- [x] `delete`
- [x] `instanceof`
- [x] Ambient host globals such as `process`, `require`, `module`, timers, and
      `fetch`
- [x] Symbols and symbol-based protocols
- [x] Typed arrays and related binary surfaces
- [x] `Intl`
- [x] `Proxy`
- [x] Accessors and descriptor-dependent behavior

Done when:

- [x] Every documented unsupported class has a contract entry.
- [x] Every contract entry maps to generated or curated coverage, and to a phase
      and category expectation.
- [x] The audit suite fails when docs and rejection coverage drift apart.

### 3. Snapshot and Progress Lifecycle Model

Snapshot safety is not an edge feature. It is part of the public product
contract and already has meaningful security semantics in the docs and tests.

Checklist:

- [x] Add model-based tests for `run`, `start`, `dump`, `load`, `resume`,
      `resumeError`, and cancellation sequences.
- [x] Encode same-process `Progress.load(...)` behavior separately from
      fresh-process restore behavior.
- [x] Assert single-use behavior across direct resume, resumeError, cancel, and
      replay attempts.
- [x] Assert `snapshotKey` authentication and tamper rejection.
- [x] Assert policy reassertion across load and resume.
- [x] Assert post-load limit/accounting checks instead of trusting serialized
      runtime state.
- [x] Assert stale-token and replay rejection behavior.
- [x] Print minimized action histories on failures.
- [x] Add a thin Node-layer mirror for the public wrapper behavior where the
      Rust core model alone is not sufficient.

Key invariants:

- [x] Dump only succeeds at valid suspension points.
- [x] Completed progress cannot resume.
- [x] Consumed progress cannot be reused.
- [x] Replayed or stale snapshots fail closed.
- [x] Fresh-process restore requires explicit capabilities, limits, and
      `snapshotKey`.
- [x] Same-process restore and fresh-process restore remain distinct and tested
      as distinct contracts.

Done when:

- [x] Short stateful sequences run in presubmit.
- [x] Longer sequences run outside the critical PR lane.
- [x] Every failing case prints the action history and minimized sequence.

### 4. Boundary and Limits As First-Class Contracts

Boundary behavior and resource enforcement are not just implementation details.
They are part of the public API contract.

Checklist:

- [x] Expand generated structured-value round-trip coverage for allowed values.
- [x] Cover numeric edge cases explicitly: `NaN`, `Infinity`, `-Infinity`,
      `-0`, and `undefined`.
- [x] Add rejected-boundary coverage for functions, symbols, `BigInt`, `Date`,
      `RegExp`, `Map`, `Set`, typed arrays, custom prototypes, accessors, class
      instances, and cycles.
- [x] Add lifecycle misuse families around `run`, `start`, `resume`,
      `resumeError`, and `cancel`.
- [x] Add cross-process `Progress.load(...)` boundary coverage for policy and
      `snapshotKey` mismatch cases.
- [x] Add one tiny reproducer for each resource limit type.
- [x] Assert precise error kind or category and guest-safe messages.
- [x] Assert that limit failures do not leak host paths or internal details.
- [x] Assert suspend/load/resume behavior for limits and cancellation where the
      docs promise specific behavior.

Limit families:

- [x] Instruction budget
- [x] Heap byte limit
- [x] Allocation budget
- [x] Call-depth limit
- [x] Outstanding host-call limit
- [x] Cancellation

Done when:

- [x] Every allowed boundary value round-trips exactly through the public API.
- [x] Every rejected boundary value fails in the correct layer.
- [x] Every limit can be triggered independently with a tiny reproducer.
- [x] Limit and cancellation failures are deterministic and guest-safe.

### 5. Deterministic Async Schedule Exploration

Current async coverage is meaningful, but it is still mostly representative.
The next step is deterministic interleaving exploration, not more timing tests.

Checklist:

- [x] Build a deterministic deferred-promise harness for Node-layer async tests.
- [x] Enumerate bounded resolve/reject orderings instead of using wall-clock
      sleeps as the primary technique.
- [x] Record canonical event traces for capability call, resolve/reject,
      microtask checkpoint, guest continuation, and completion/failure.
- [x] Compare `jslite` against Node only where parity is promised.
- [x] Compare against the documented contract where `jslite` intentionally fails
      closed.

Priority scenarios:

- [x] Nested `await`
- [x] `Promise.all`
- [x] `Promise.allSettled`
- [x] `Promise.race`
- [x] `Promise.any`
- [x] `then`, `catch`, and `finally`
- [x] Rejection flowing through `finally`
- [x] Host resolve vs host reject ordering
- [x] Cancellation racing with host completion
- [x] Suspend and resume while promise work is still pending

Done when:

- [x] Small schedules are exhaustively explored up to a fixed bound.
- [x] Every failure is reproducible and prints a human-readable trace diff.
- [x] The suite no longer depends on flaky timing races for confidence here.

### 6. Executed Fuzzing In CI

The repository already has the right fuzz targets. The missing piece is running
them meaningfully, not adding many more targets.

Checklist:

- [x] Replace the compile-only fuzz check in `scripts/run-hardening.sh` with
      short executed fuzz smoke for selected targets.
- [x] Run short PR-lane fuzz smoke for `parser`, `snapshot_load`, and
      `sidecar_protocol`.
- [x] Add scheduled sanitizer-backed fuzzing with persisted corpora and crash
      artifacts.
- [x] Seed fuzz corpora from minimized regressions, curated supported programs,
      curated unsupported programs, serialized snapshots, and sidecar protocol
      fixtures.
- [x] Promote fuzz-found failures into stable regression coverage.

Done when:

- [x] CI executes real fuzz work instead of only proving the targets compile.
- [x] Nightly or scheduled jobs grow corpora over time.
- [x] Every fuzz-found bug has a path into the permanent regression corpus.

## Tier 2: Strong Follow-Up

### 7. Cross-Layer Equivalence Corpus

The same supported guest program should agree across the layers that claim to
share semantics.

Checklist:

- [x] Build a shared representative corpus of supported guest programs.
- [x] Assert equivalence across Rust `execute` vs `start`-to-completion.
- [x] Assert equivalence across fresh compile vs compile/load-program round trip.
- [x] Assert equivalence across direct execution vs dump/load/resume paths where
      suspension is involved.
- [x] Assert equivalence across addon mode and sidecar mode.
- [x] Add an allowlist for any documented mode-specific differences instead of
      accepting silent drift.

Done when:

- [x] Every corpus program has one canonical expected outcome.
- [x] Any mode-specific difference is documented and intentionally allowlisted.

### 8. Sidecar Protocol State Coverage

The sidecar already has happy-path and hostile malformed-input coverage. The
next gap is valid-but-adversarial protocol sequencing within the protocol that
actually exists today.

Checklist:

- [x] Add protocol-sequence tests for valid and invalid ordering across
      `compile`, `start`, and `resume`.
- [x] Add duplicate-ID coverage if IDs are meant to be host-chosen but still
      protocol-safe.
- [x] Add resume-after-completion misuse coverage.
- [x] Add mismatched capability/policy-set resume coverage.
- [x] Add concurrent suspended-execution coverage where the current protocol and
      sidecar implementation support it.
- [x] Add addon-vs-sidecar equivalence coverage for a small shared corpus.
- [x] Keep hard-stop testing at the process boundary; do not invent an in-band
      sidecar `cancel` method before the protocol changes.

Done when:

- [x] Valid-but-adversarial protocol sequences are covered in addition to
      malformed line handling.
- [x] Protocol misuse fails closed and does not leave ambiguous live state.

### 9. Contract Audits Instead Of Coverage Percentages

The repository already has coverage-audit tests. The next step is to tie those
audits more directly to product obligations.

Checklist:

- [x] Add audits that fail when a documented built-in lacks parity or rejection
      coverage as appropriate.
- [x] Add audits that fail when a public API method lacks misuse-path coverage.
- [x] Add audits that fail when sidecar protocol methods lack valid-flow or
      hostile-input coverage.
- [x] Add audits that fail when new conformance-contract entries are added
      without the expected test bucket.

Done when:

- [x] Docs, conformance contract, and coverage obligations cannot drift
      silently.

## Tier 3: Quality Multipliers

### 10. Expand The Existing Metamorphic Layer

Metamorphic testing is already present in the AST conformance layer. The work
here is to expand it, not to invent a separate new framework.

Checklist:

- [x] Add more semantics-preserving rewrites inside the existing AST/conformance
      infrastructure.
- [x] Add snapshot round-trip insertion where it is semantics-preserving and
      contractually meaningful.
- [x] Add rewrite families that specifically stress lowering and bytecode
      generation paths not yet covered by the current transforms.

### 11. Targeted Mutation Checks For Critical Guards

Checklist:

- [x] Add narrow mutation-style checks for validator rejection conditions.
- [x] Add narrow mutation-style checks for snapshot authorization and replay
      guards.
- [x] Add narrow mutation-style checks for limit comparisons.
- [x] Add narrow mutation-style checks for structured boundary rejection paths.
- [x] Keep mutation runs out of the fast PR path.

### 12. Stable Performance Micro-Contracts

Checklist:

- [x] Add narrow performance contract checks for cold start, host-call overhead,
      snapshot dump/load cost, and bounded memory growth.
- [x] Use relative thresholds where possible.
- [x] Keep noisy performance checks outside correctness-critical presubmit jobs.

## Not The Right First Move

- Do not create a second conformance manifest and a second canonicalization
  system before the existing ones are exhausted.
- Do not convert the Node test layer into a new TypeScript-first framework just
  to support this plan.
- Do not front-load a separate testing-strategy document before this execution
  plan is tightened and used.
- Do not add broad flaky timing tests as a substitute for schedule exploration.
- Do not chase statement or line coverage as a proxy for semantic confidence.
- Do not add many redundant example tests for behavior already covered by faster
  generated or stateful suites.
- Do not invent sidecar protocol methods that the repository does not yet
  document or implement.

## Success Criteria

- [x] Every supported semantic family has generated parity coverage.
- [x] Every documented unsupported class has mapped rejection coverage with
      expected phase and category.
- [x] Snapshot and progress bugs are found by lifecycle/stateful tests, not by
      ad hoc regressions alone.
- [x] Async ordering regressions show up as deterministic trace diffs, not flaky
      timing failures.
- [x] Boundary and limit behavior are covered as explicit public contracts.
- [x] Addon mode and sidecar mode agree on canonical outcomes where they are
      supposed to agree.
- [x] Selected fuzzers run continuously and feed the regression corpus.
- [x] Docs, conformance data, and coverage obligations cannot silently drift.

That is the path to a materially stronger `jslite` test suite: sharper
contracts, better generators, stronger stateful checks, and tighter alignment
between docs, runtime promises, and executed verification.

## Iteration Log

| Iteration | UTC Timestamp | Summary | Commit | Errors / Blockers |
| --- | --- | --- | --- | --- |
| 1 | 2026-04-12T08:47:24Z | Closed Tier 1 sections 1 through 4: split supported and rejection properties into contract-backed families, added curated rejection regressions, added stateful progress lifecycle properties with minimized action histories, and expanded negative host-boundary coverage across inputs, capability results, and resume payloads. | f9f1063, 2537dc0, 85e34b7, 890cca2 | A first deferred async-schedule harness was discarded instead of committed after auditing the wrapper behavior: the Node layer currently exposes one host suspension at a time, so the simple concurrently pending host-promise model was the wrong fit for section 5. |
| 2 | 2026-04-12T09:42:07Z | Closed sections 5 through 12: added deterministic async schedule exploration, upgraded hardening to executed fuzz smoke plus scheduled corpus growth, added shared Rust/addon/sidecar equivalence coverage and sidecar state sequencing tests, tightened contract audits, expanded AST metamorphic and snapshot round-trip checks, added off-fast-lane mutation guards, and replaced the benchmark smoke with stable performance micro-contracts. | 5e4e880, f7d34fb, 8d5360c, 123f148 | `npm run lint` initially failed on rustfmt drift in the new Rust tests; `cargo fmt --all` fixed it and the rerun passed. |
