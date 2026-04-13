# Refactoring Wave 2

## Goal

`REFACTORING_RUNTIME.md` addressed the largest structural problem in the
repository: the old monolithic runtime file. That work is now largely landed,
but several maintainability hotspots remain.

The goal of this follow-on refactor is to reduce the next set of mixed-
responsibility files and duplicated boundaries without changing guest
semantics, public APIs, wire formats, or security posture.

This plan is intentionally about structure, not feature work.

## Constraints

- Keep the public Rust API stable: `compile`, `lower_to_bytecode`, runtime
  entry points, serialization helpers, and public types should keep the same
  names and behavior.
- Keep the public JavaScript API stable: `Mustard`, `Progress`, `MustardError`,
  `run()`, `start()`, `load()`, `resume()`, and `resumeError()` should keep
  the same names and behavior.
- Keep guest/runtime semantics in Rust. Do not push logic into the Node
  wrapper.
- Preserve sidecar request and response shapes unless an explicit protocol
  versioning change is made separately.
- Preserve fail-closed behavior and existing diagnostics.
- Prefer mechanical moves over semantic rewrites while extracting code.
- Keep the repo buildable after each milestone and run verification at every
  boundary.
- Avoid over-fragmentation. The target is cohesive modules, not dozens of tiny
  files.
- Do not combine refactoring milestones with feature additions unless a
  separate implementation task explicitly requires it.

## Current Hotspots

At the start of this wave, these were the highest-value refactor targets:

- `crates/mustard/src/runtime/compiler/mod.rs` is still about 1.3k lines and
  mixes root/function setup, statement lowering, expression lowering,
  assignment lowering, and control-transfer patching.
- `index.js` is still over 600 lines and mixes native error normalization,
  structured-value codecs, policy encoding, abort/cancellation bridging,
  `Progress` lifecycle, host capability orchestration, and the public API.
- `crates/mustard-node/src/lib.rs` and `crates/mustard-sidecar/src/lib.rs`
  duplicate most of their DTOs and boundary encode/decode logic.
- `tests/node/basic.test.js` is nearly 900 lines and mixes builtins, host
  boundary behavior, exceptions, limits, progress objects, serialization, and
  error sanitization.
- `crates/mustard/tests/coverage_audit.rs` contains ad hoc IR traversal helpers
  that will become harder to reuse if more structural assertions are added.
- `crates/mustard/src/runtime/mod.rs` is much smaller than the original
  monolith, but it still owns more shared glue than a true facade should.

These files are not all equally urgent, but they all have the same underlying
problem: unrelated concerns still collide in the same file or the same review
diff.

## Proposed Target Layout

```text
crates/mustard/src/parser/
  mod.rs
  scope.rs
  patterns.rs
  statements.rs
  expressions.rs
  operators.rs
  tests/
    mod.rs
    acceptance.rs
    rejections.rs

crates/mustard/src/runtime/compiler/
  mod.rs
  context.rs
  bindings.rs
  statements.rs
  expressions.rs
  assignments.rs
  control.rs

crates/mustard-bridge/
  Cargo.toml
  src/
    lib.rs
    dto.rs
    codec.rs
    operations.rs

lib/
  errors.js
  structured.js
  policy.js
  cancellation.js
  progress.js
  runtime.js

tests/node/
  builtins.test.js
  exceptions.test.js
  host-boundary.test.js
  limits.test.js
  progress.test.js
  serialization.test.js
  support/
    helpers.js
```

The exact filenames may shift, but the boundary intent matters more than the
precise names.

## Ownership Rules

- `parser/mod.rs` should be a thin entry facade around `compile()` and parser
  module wiring.
- Parser scope tracking and binding collection should live together and should
  not be mixed into large expression or statement matches.
- `runtime/compiler/mod.rs` should become a small facade that owns
  `lower_to_bytecode` plus any minimal orchestration that truly spans all
  compiler submodules.
- Control-transfer patching for loops, `try`, `catch`, `finally`, `return`,
  `break`, and `continue` should live together instead of being spread across
  statement and expression lowering.
- Shared addon/sidecar DTOs and shared Rust boundary operations should live in
  one reusable internal crate rather than duplicated in two adapter crates.
- Node-specific concerns such as the cancellation-token registry must remain in
  `crates/mustard-node`.
- Sidecar-specific concerns such as line-delimited request framing must remain
  in `crates/mustard-sidecar`.
- `index.js` should become a public facade, not the implementation home for
  every wrapper concern.
- Shared test helpers should live in test-support modules, not be copy-pasted
  into large integration files.

## Recommended Milestones

### Milestone 1: Split `runtime/compiler/mod.rs`

Purpose: finish the compiler decomposition that `REFACTORING_RUNTIME.md`
explicitly called for.

Checklist:

- [x] Extract compiler context types into `context.rs`
- [x] Move statement lowering into `statements.rs`
- [x] Move expression lowering into `expressions.rs`
- [x] Move assignment lowering into `assignments.rs`
- [x] Move `try`/`catch`/`finally` control-transfer patching into `control.rs`
- [x] Keep `bindings.rs` focused on binding collection and simple mapping
- [x] Keep `lower_to_bytecode` re-exported unchanged from `runtime/compiler/mod.rs`
- [x] Preserve bytecode output and validation behavior

Exit criteria:

- `runtime/compiler/mod.rs` is a thin facade instead of a 1k+ implementation
  file
- Compiler edits stop colliding with unrelated runtime code
- Existing compiler and bytecode tests still pass unchanged

### Milestone 2: Split `parser.rs`

Purpose: separate parse entry, scope rules, lowering, and tests so parser work
stops concentrating in one file.

Checklist:

- [x] Rename `crates/mustard/src/parser.rs` to `crates/mustard/src/parser/mod.rs`
- [x] Extract scope tracking and binding registration into `scope.rs`
- [x] Extract pattern lowering helpers into `patterns.rs`
- [x] Extract statement lowering into `statements.rs`
- [x] Extract expression lowering into `expressions.rs`
- [x] Extract operator/property-name helpers into `operators.rs`
- [x] Move parser unit tests into `parser/tests/`
- [x] Keep `compile()` stable and keep current diagnostics behavior

Exit criteria:

- `parser/mod.rs` is a small facade around parse-and-lower orchestration
- Parser diagnostics and forbidden-form behavior remain unchanged
- Parser tests are no longer embedded in production code

### Milestone 3: Consolidate Shared Rust Boundary Code

Purpose: eliminate addon/sidecar drift in DTOs and compile/start/resume helper
logic.

Checklist:

- [x] Create a shared internal workspace crate for bridge DTOs and helpers
- [x] Move `StartOptionsDto`, `RuntimeLimitsDto`, `SnapshotPolicyDto`,
  `StepDto`, and `ResumeDto` into shared code
- [x] Move common encode/decode helpers into shared code
- [x] Move shared compile/start/resume/inspect operations into shared code
- [x] Keep Node-specific cancellation-token registry logic only in
  `crates/mustard-node`
- [x] Keep sidecar request/response envelope types and line framing only in
  `crates/mustard-sidecar`
- [x] Preserve current JSON shapes and sidecar protocol shapes

Exit criteria:

- Shared boundary DTOs exist in one place
- Addon and sidecar stop carrying near-duplicate conversion code
- No user-visible protocol or addon API behavior changes

### Milestone 4: Modularize the Node Wrapper

Purpose: keep the wrapper thin in practice, not just in principle.

Checklist:

- [x] Keep root `index.js` as the package entry and public facade
- [x] Extract native error normalization into `lib/errors.js`
- [x] Extract structured-value encode/decode logic into `lib/structured.js`
- [x] Extract policy and snapshot-policy helpers into `lib/policy.js`
- [x] Extract abort/cancellation bridging into `lib/cancellation.js`
- [x] Extract `Progress` state and lifecycle logic into `lib/progress.js`
- [x] Extract `Mustard` run/start orchestration into `lib/runtime.js`
- [x] Preserve CommonJS entry behavior and existing TypeScript declarations

Exit criteria:

- `index.js` is mostly wiring and exports
- Wrapper-specific changes become localized
- Public JS API and runtime behavior remain unchanged

### Milestone 5: Split Oversized Test Files and Add Shared Test Support

Purpose: make behavior reviews and failures easier to localize.

Checklist:

- [x] Break `tests/node/basic.test.js` into concern-focused files
- [x] Add `tests/node/support/helpers.js` for repeated runtime and assertion
  patterns
- [x] Keep current behavior assertions first; improve ergonomics only after the
  split is stable
- [x] Move parser tests out of production modules if Milestone 2 has not
  already done so
- [x] Extract reusable IR traversal helpers from
  `crates/mustard/tests/coverage_audit.rs` if multiple tests need them
  and otherwise keep the local helpers single-consumer
- [x] Preserve or improve the same coverage surface after the split

Exit criteria:

- Large multi-concern test files are gone
- Repeated test plumbing is centralized
- Test failures map more cleanly to one subsystem

### Milestone 6: Facade Cleanup and Boundary Tightening

Purpose: clean up whatever broad glue remains after the earlier extractions.

Checklist:

- [x] Reduce remaining shared-logic weight in `crates/mustard/src/runtime/mod.rs`
- [x] Tighten visibility to `pub(super)` or private where possible
- [x] Reduce oversized `use` lists
- [x] Add short module-level docs where a file owns a tricky subsystem
- [x] Remove stale comments or imports left behind by the earlier moves
- [x] Re-run full verification and leave the tree in a stable state

Exit criteria:

- Facade files are mostly re-exports and orchestration
- Internal boundaries are clearer and narrower
- The repo is cleaner without changing semantics

## What Not To Do

- Do not combine these refactors with new language/runtime features.
- Do not change bytecode or snapshot formats as part of structural cleanup.
- Do not change sidecar request/response shapes as part of the shared-bridge
  extraction.
- Do not introduce traits or generic abstraction layers just to “organize”
  code.
- Do not move guest semantics into JavaScript to make files look smaller.
- Do not rewrite tests at the same time you move them unless the old test
  shape is truly broken.
- Do not split cohesive small files just for symmetry.

## Verification Plan

After each milestone:

- `cargo test -p mustard`

After milestones that touch addon, sidecar, wrapper, or cross-boundary tests:

- `cargo test --workspace`
- `npm test`

At the end of each substantial milestone:

- `npm run lint`

If a milestone is purely mechanical inside one Rust area, keep the smaller Rust
loop tight first, then run the full suite before considering the milestone
done.

## Done Criteria

This refactoring wave is complete when all of these are true:

- `runtime/compiler/mod.rs` is no longer a 1k+ mixed-responsibility file
- `parser.rs` no longer exists as a monolithic file
- Addon and sidecar boundary DTOs are no longer duplicated
- `index.js` is a thin facade over smaller wrapper modules
- `tests/node/basic.test.js` has been split by concern
- The repo still passes:
  - `cargo test --workspace`
  - `npm test`
  - `npm run lint`

## Suggested Execution Order

1. Milestone 1: split `runtime/compiler`
2. Milestone 2: split `parser`
3. Milestone 3: consolidate shared Rust boundary code
4. Milestone 4: modularize the Node wrapper
5. Milestone 5: split oversized tests and add shared support
6. Milestone 6: facade cleanup and boundary tightening

This order starts with the clearest remaining Rust implementation hotspot, then
addresses the parser, then removes bridge duplication, and only after that
shrinks the wrapper and test surface around those stable internal boundaries.

## First Milestone I Would Land

The safest first cut is Milestone 1:

1. Extract `context.rs`
2. Move statement lowering into `statements.rs`
3. Move expression lowering into `expressions.rs`
4. Move assignment and control-transfer helpers into `assignments.rs` and
   `control.rs`
5. Keep `lower_to_bytecode` re-exported from `runtime/compiler/mod.rs`
6. Run `cargo test -p mustard`

That delivers a concrete maintainability win in the most obvious remaining Rust
hotspot without touching public APIs or transport boundaries yet.
