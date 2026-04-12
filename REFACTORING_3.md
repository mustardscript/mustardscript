# Refactoring Wave 3

## Goal

`REFACTORING_RUNTIME.md` and `REFACTORING_2.md` removed the largest structural
problems in the repository. The remaining high-value refactor work is narrower
and mostly internal to the Rust runtime.

This wave should finish tightening the runtime seams that still concentrate
multiple independent concerns in the same file:

- validation logic still mixes bytecode analysis, snapshot graph integrity, and
  snapshot-policy capability enforcement
- async execution still mixes promise state transitions, reaction/combinator
  logic, microtask scheduling, host suspension, and resume flow
- conversion logic still mixes operator semantics, coercions, guest-safe error
  rendering, structured host-boundary codecs, and JSON translation
- builtin shared helpers still live in `builtins/mod.rs`, which means the
  builtin facade is not yet a true facade

This plan is about structure, not feature work.

## Constraints

- Keep the public Rust API stable: `execute`, `start`, `resume`,
  `resume_with_options`, serialization helpers, validation entry points, and
  public runtime types should keep the same names and behavior.
- Keep guest/runtime semantics in Rust. Do not push logic into the Node
  wrapper.
- Preserve current sidecar protocol shapes and shared bridge DTO shapes.
- Preserve fail-closed behavior, guest-safe diagnostics, and snapshot-policy
  enforcement.
- Prefer mechanical moves over semantic rewrites while extracting code.
- Keep the crate buildable after each milestone and run the existing runtime
  and integration tests at every boundary.
- Avoid over-fragmentation. The target is a few cohesive ownership modules, not
  dozens of tiny files.
- Do not mix this refactor wave with language-surface expansion.

## Current Hotspots

At the start of this wave, these are the highest-value refactor targets:

- `crates/jslite/src/runtime/validation.rs` is about 1.1k lines and mixes:
  bytecode validation, snapshot graph validation, and snapshot-policy capability
  validation. The snapshot and policy paths also repeat many of the same
  runtime-graph traversals.
- `crates/jslite/src/runtime/async_runtime.rs` is about 760 lines and mixes:
  promise state management, thenable adoption, reaction activation, promise
  combinators, microtask checkpoints, suspended host-call handling, idle-state
  progression, and resume logic.
- `crates/jslite/src/runtime/conversions.rs` is about 580 lines and mixes:
  unary/binary operators, numeric and string coercions, property-key
  normalization, error-object creation/rendering, structured host-boundary
  conversion, and JSON conversion.
- `crates/jslite/src/runtime/builtins/mod.rs` is still a helper bucket for
  string-search, RegExp replacement, index normalization, and `Date` helpers,
  even though `builtins/mod.rs` should mainly be module wiring plus any truly
  central shared types.

Lower-priority large files still exist, especially `builtins/arrays.rs` and
`builtins/strings.rs`, but those files are at least family-cohesive today.
They are less urgent than the cross-cutting ownership problems above.

## Proposed Target Layout

```text
crates/jslite/src/runtime/
  validation/
    mod.rs
    bytecode.rs
    walk.rs
    snapshot.rs
    policy.rs
  async_runtime/
    mod.rs
    promises.rs
    reactions.rs
    scheduler.rs
  conversions/
    mod.rs
    operators.rs
    coercions.rs
    errors.rs
    boundary.rs
  builtins/
    mod.rs
    install.rs
    support.rs
    arrays.rs
    objects.rs
    strings.rs
    collections.rs
    promises.rs
    regexp.rs
    primitives.rs
```

The exact filenames may shift, but the ownership boundaries matter more than
the names.

## Ownership Rules

- `runtime/validation/mod.rs` should be a thin facade that re-exports
  `validate_bytecode_program`, `validate_snapshot`, and
  `validate_snapshot_policy`.
- Bytecode validation should stay independent from full runtime-state
  validation. It should depend on bytecode types, not on `Runtime`.
- Snapshot graph traversal should exist in one place. Snapshot integrity checks
  and snapshot-policy capability checks should reuse that traversal rather than
  each re-implementing the same loops over cells, objects, arrays, frames,
  promises, and microtasks.
- `runtime/async_runtime/mod.rs` should be a small facade around promise
  internals, reaction logic, and async scheduling/resume flow.
- Promise settlement primitives should live separately from reaction/combinator
  behavior, so changes to one do not force review noise in the other.
- Resume and idle-state progression should live together because they own the
  boundary between suspended host calls, microtasks, and top-level execution.
- `runtime/conversions/mod.rs` should become a thin facade over operator
  semantics, coercions, guest-safe error rendering, and host-boundary codecs.
- Structured host-boundary conversion and JSON conversion should live together.
  Primitive arithmetic and string coercion should not share a file with
  snapshot/host-boundary codecs.
- `runtime/builtins/mod.rs` should stop owning algorithms. Shared builtin
  helper functions should move into a dedicated support module with a clear
  name.

## Recommended Milestones

### Milestone 1: Split `runtime/validation.rs`

Purpose: separate three distinct validators and remove duplicated snapshot
traversal.

Checklist:

- [ ] Convert `crates/jslite/src/runtime/validation.rs` into
  `runtime/validation/mod.rs`
- [ ] Move bytecode validation into `runtime/validation/bytecode.rs`
- [ ] Introduce `runtime/validation/walk.rs` for shared runtime-graph traversal
- [ ] Move snapshot integrity checks into `runtime/validation/snapshot.rs`
- [ ] Move snapshot-policy capability enforcement into
  `runtime/validation/policy.rs`
- [ ] Keep `validate_bytecode_program`, `validate_snapshot`, and
  `validate_snapshot_policy` re-exported unchanged
- [ ] Preserve current validation diagnostics and fail-closed behavior

Exit criteria:

- `validation/mod.rs` is a thin facade
- snapshot validation and snapshot-policy validation no longer duplicate the
  same runtime-graph loops
- bytecode-validation work stops colliding with snapshot-policy work

### Milestone 2: Split `runtime/async_runtime.rs`

Purpose: separate promise primitives from scheduling and host-resume flow.

Checklist:

- [ ] Convert `crates/jslite/src/runtime/async_runtime.rs` into
  `runtime/async_runtime/mod.rs`
- [ ] Move promise state helpers and settlement primitives into
  `runtime/async_runtime/promises.rs`
- [ ] Move promise-reaction and combinator logic into
  `runtime/async_runtime/reactions.rs`
- [ ] Move microtask activation, idle-state progression, suspended host-call
  handling, and `Runtime::resume` into `runtime/async_runtime/scheduler.rs`
- [ ] Keep current async guest semantics, thenable adoption rules, and
  outstanding-host-call enforcement unchanged
- [ ] Preserve the existing `start()` / `resume()` / async root behavior

Exit criteria:

- promise settlement changes are isolated from resume/suspension edits
- microtask and host-suspension logic have one clear home
- existing async runtime tests keep passing without behavior changes

### Milestone 3: Split `runtime/conversions.rs`

Purpose: separate language-level operators from host-boundary codecs and
guest-safe error rendering.

Checklist:

- [ ] Convert `crates/jslite/src/runtime/conversions.rs` into
  `runtime/conversions/mod.rs`
- [ ] Move unary/binary operator helpers and BigInt arithmetic into
  `runtime/conversions/operators.rs`
- [ ] Move numeric, string, integer, and property-key coercions into
  `runtime/conversions/coercions.rs`
- [ ] Move guest-safe error-object creation and rendering into
  `runtime/conversions/errors.rs`
- [ ] Move structured-value conversion and JSON conversion into
  `runtime/conversions/boundary.rs`
- [ ] Keep `structured_to_json` re-exported unchanged
- [ ] Preserve current structured-boundary rejection rules and error messages

Exit criteria:

- host-boundary conversion changes no longer touch arithmetic/coercion code
- error rendering changes no longer live beside structured codecs
- `conversions/mod.rs` becomes a small facade

### Milestone 4: Turn `runtime/builtins/mod.rs` Into A Real Facade

Purpose: finish the builtin facade cleanup left over from the prior waves.

Checklist:

- [ ] Move shared builtin helper functions out of `runtime/builtins/mod.rs`
- [ ] Create a dedicated shared helper module such as
  `runtime/builtins/support.rs`
- [ ] Move string-search, replacement-template, index-normalization, and date
  helper code into that support module
- [ ] Keep shared types only where they genuinely span multiple builtin
  families
- [ ] Preserve current `String`, `RegExp`, and `Date` behavior exactly

Exit criteria:

- `builtins/mod.rs` is mostly module wiring and shared type exports
- common builtin algorithms have a named ownership home
- builtin facade edits stop pulling unrelated helper code into the same diff

### Milestone 5: Tighten Test Support Around The New Boundaries

Purpose: keep the refactor sustainable and prevent the new module seams from
drifting back together.

Checklist:

- [ ] Add small shared test helpers for constructing snapshots, promise states,
  and async resume scenarios where that reduces duplication
- [ ] Keep unit tests aligned with the new module boundaries, especially for
  validation, async runtime behavior, and structured-boundary conversion
- [ ] Preserve the current integration-test surface in `crates/jslite/tests/`
  and `tests/node/`
- [ ] Avoid moving broad behavioral coverage into production modules

Exit criteria:

- the refactor does not depend on giant fixture duplication
- tests continue to map cleanly to one subsystem
- new module boundaries are reinforced by targeted tests

## What This Wave Should Not Prioritize

- Do not re-open the Node wrapper split unless new wrapper complexity appears.
  `index.js` and `lib/*.js` are already much healthier than before.
- Do not re-open addon/sidecar DTO consolidation unless a new drift appears.
  The `jslite-bridge` extraction already addressed the main duplication there.
- Do not split `builtins/arrays.rs` or `builtins/strings.rs` purely by line
  count unless a concrete ownership problem appears. Those files are large, but
  they are currently semantically cohesive in a way `validation.rs`,
  `async_runtime.rs`, and `conversions.rs` are not.

## Recommended Execution Order

1. Split `validation.rs` first, because it has the clearest ownership seams and
   the most obvious duplicated traversal logic.
2. Split `async_runtime.rs` next, because promise and resume work is still one
   of the most review-sensitive areas in the runtime.
3. Split `conversions.rs` after async, because it will benefit from already
   having stable async and validation module boundaries.
4. Move builtin shared helpers out of `builtins/mod.rs` once the surrounding
   runtime support modules are stable.
5. Tighten tests and visibility after the extractions land.

## Success Criteria

This wave is complete when:

- the remaining large cross-cutting runtime files are replaced by thin facades
  and cohesive implementation modules
- snapshot validation and snapshot-policy enforcement share traversal
  infrastructure instead of repeating it
- promise settlement, reaction logic, and resume scheduling have distinct homes
- operator/coercion logic is no longer mixed with host-boundary codecs
- builtin shared helpers stop living in `builtins/mod.rs`
- public APIs, guest semantics, diagnostics, and wire formats remain unchanged
