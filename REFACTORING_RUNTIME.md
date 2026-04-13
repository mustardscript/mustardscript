# Refactoring `crates/mustard/src/runtime.rs`

## Goal

`crates/mustard/src/runtime.rs` is currently about 10.8k lines and mixes too
many responsibilities into one file. The goal of this refactor is to split it
into a `crates/mustard/src/runtime/` module tree that is easier to navigate,
review, test, and change without altering guest semantics.

This plan is intentionally about structure, not feature work.

## Constraints

- Keep the public Rust API stable: `execute`, `start`, `resume`,
  serialization helpers, and public runtime types should keep the same names
  and behavior.
- Keep guest/runtime semantics in Rust. Do not push logic into the Node
  wrapper.
- Preserve fail-closed behavior and existing diagnostics.
- Prefer mechanical moves over semantic rewrites while extracting code.
- Keep the crate buildable after each phase and run verification at every
  boundary.
- Avoid over-fragmentation. The target is cohesive modules, not 40 tiny files.

## Current Shape

Today `runtime.rs` roughly contains these concerns:

- Public API and bytecode surface: options, snapshots, bytecode types, entry
  points
- Program and snapshot serialization
- Bytecode and snapshot validation
- IR -> bytecode lowering and compiler control-flow patching
- Internal value model, heap object structs, frames, promises, and runtime
  state
- VM execution loop, calls, exceptions, async suspension, resume flow
- Heap accounting and mark-sweep GC
- Built-ins for arrays, objects, strings, maps, sets, promises, regexp, math,
  and JSON
- Property access, conversions, error rendering, iterator helpers
- A large `#[cfg(test)]` block

That makes almost every edit high-risk because unrelated concerns live in the
same file and the same review diff.

## Proposed Target Layout

```text
crates/mustard/src/runtime/
  mod.rs
  api.rs
  bytecode.rs
  serialization.rs
  validation.rs
  state.rs
  accounting.rs
  gc.rs
  env.rs
  properties.rs
  conversions.rs
  vm.rs
  async_runtime.rs
  exceptions.rs
  compiler/
    mod.rs
    bindings.rs
    statements.rs
    expressions.rs
    control.rs
  builtins/
    mod.rs
    install.rs
    arrays.rs
    objects.rs
    strings.rs
    collections.rs
    promises.rs
    regexp.rs
    primitives.rs
  tests/
    mod.rs
    execution.rs
    async_host.rs
    gc.rs
    serialization.rs
```

## Module Responsibilities

- `mod.rs`
  Public facade only. Re-export the current public API and keep `lib.rs`
  unchanged apart from the file move from `runtime.rs` to `runtime/mod.rs`.
- `api.rs`
  `ExecutionOptions`, `HostError`, resume payload/options, snapshots,
  suspensions, and the top-level `execute`/`start`/`resume` entry points.
- `bytecode.rs`
  `BytecodeProgram`, `FunctionPrototype`, `Instruction`, and bytecode-specific
  helpers.
- `serialization.rs`
  `dump_program`, `load_program`, `dump_snapshot`, `load_snapshot`, serial
  versioning, and serialized wrapper structs.
- `validation.rs`
  Bytecode validation and snapshot validation.
- `compiler/`
  IR lowering only. Split statement lowering, expression lowering, binding
  collection, and control-transfer patching so compiler work stops being mixed
  with runtime execution.
- `state.rs`
  Slotmap key types, `Value`, builtin enums, heap object structs, frame types,
  promise types, and the `Runtime` struct itself.
- `accounting.rs`
  Allocation accounting, heap-byte measurement, and `refresh_*_accounting`
  helpers.
- `gc.rs`
  Mark/sweep structs and collector logic.
- `env.rs`
  Environment creation, binding lookup/assignment, pattern initialization.
- `properties.rs`
  Property get/set, iterator creation/advance, unary/binary helpers, and
  collection semantics that are not builtin entry points.
- `conversions.rs`
  Structured value conversion, JSON conversion, string/number coercions, error
  object construction, and guest-safe rendering helpers.
- `vm.rs`
  Frame stepping, instruction dispatch, call/construct plumbing, root run loop.
- `async_runtime.rs`
  Promise state transitions, microtasks, host suspension, resume behavior.
- `exceptions.rs`
  Throw/unwind/finally handling and runtime fault propagation.
- `builtins/`
  Registration plus grouped builtin implementations.
- `tests/`
  Runtime tests grouped by concern instead of one bottom-of-file block.

## Dependency Rules

These rules matter more than the exact filenames:

- `mod.rs` should be thin and mostly re-exports.
- `state.rs` is the central internal type module. Most other runtime modules
  depend on it; it should depend on as little as possible.
- `compiler/*` may depend on IR, diagnostics, spans, and `bytecode.rs`, but not
  on `Runtime`.
- `builtins/*` should be implemented as grouped `impl Runtime` blocks, not via a
  trait hierarchy.
- Cross-module runtime helpers that must be shared across sibling modules should
  be `pub(super)`, not `pub`.
- Move helper functions to the module that owns their semantics:
  `measure_*` into `accounting.rs`, string search/replacement helpers into
  `builtins/strings.rs` or `regexp.rs`, property-key helpers into
  `properties.rs`, structured conversion helpers into `conversions.rs`.

## Mapping From Current File To Target Modules

- `1..317`: `api.rs`, `bytecode.rs`, `serialization.rs`
- `318..1275`: `validation.rs`
- `1276..2562`: `compiler/`
- `2563..3149`: `state.rs`
- `3151..5789`: `vm.rs`, `async_runtime.rs`, `exceptions.rs`, `gc.rs`,
  `accounting.rs`
- `5791..8098`: `builtins/`
- `8099..9102`: `env.rs`, `properties.rs`, `conversions.rs`
- `9644..end`: `tests/`

The exact line numbers will drift, but this is the correct extraction order.

## Recommended Refactor Sequence

### Phase 1: Convert `runtime.rs` into a module directory

- Rename `crates/mustard/src/runtime.rs` to `crates/mustard/src/runtime/mod.rs`.
- Keep all code compiling with no logical changes.
- Add module declarations only as files are extracted.

Reason: this creates the destination structure early and avoids repeated
renames later.

### Phase 2: Extract the public surface first

- Move public data types and top-level runtime entry points into `api.rs`.
- Move bytecode type definitions into `bytecode.rs`.
- Move dump/load helpers into `serialization.rs`.
- Move validation into `validation.rs`.

Result: callers and tests can keep importing `crate::runtime::*` while the file
starts shrinking quickly.

### Phase 3: Split out the compiler

- Move `Compiler`, `CompileContext`, binding collection, and lowering helpers
  into `compiler/`.
- Keep `lower_to_bytecode` re-exported from `mod.rs`.
- Prefer `statements.rs` and `expressions.rs` over one giant `compiler.rs`.

Result: parsing/lowering changes stop colliding with runtime/VM changes.

### Phase 4: Isolate runtime state from runtime behavior

- Move all internal enums and structs for values, heap objects, frames,
  promises, and `Runtime` into `state.rs`.
- Keep this phase mechanical: no behavior changes, no renamed semantics.

Result: data-model reviews become separate from execution logic reviews.

### Phase 5: Split execution into VM, async, and exceptions

- Move frame stepping and instruction dispatch into `vm.rs`.
- Move promise/microtask/host-suspension logic into `async_runtime.rs`.
- Move unwind/finally/exception plumbing into `exceptions.rs`.

Result: the run loop becomes understandable without scrolling across GC,
  builtins, and conversion code.

### Phase 6: Move accounting and GC together but separate from the VM loop

- Move heap byte measurement and allocation refresh logic into `accounting.rs`.
- Move mark/sweep structures and traversal into `gc.rs`.
- Keep GC entry points called from `Runtime` as they are; just change file
  ownership.

Result: memory-limit and collector work has a clear home and clearer tests.

### Phase 7: Extract environment, property, and conversion layers

- Move env/binding helpers into `env.rs`.
- Move property access, iterator helpers, and equality/property-key helpers into
  `properties.rs`.
- Move coercions, structured/json conversion, and guest-safe rendering into
  `conversions.rs`.

Result: builtin code can call shared helpers without being buried in the main
  runtime file.

### Phase 8: Split builtins by family

- `install.rs`: global registration and builtin wiring
- `arrays.rs`: array constructor/methods and callback helpers
- `objects.rs`: `Object.*`
- `strings.rs`: string methods and string-pattern helpers
- `collections.rs`: `Map`, `Set`, iterators, shared SameValueZero helpers
- `promises.rs`: `Promise` constructors/combinators/methods
- `regexp.rs`: `RegExp` construction and match helpers
- `primitives.rs`: `Math`, `JSON`, `Number`, `Boolean`, `Error`

Result: builtin changes become localized and easier to test.

### Phase 9: Move tests out of production files

- Create `runtime/tests/` and split tests by concern.
- Keep common helpers in `tests/mod.rs`.
- Preserve the current assertions first; only improve test ergonomics after the
  move is stable.

Result: production code stops carrying a 1k+ line test tail.

### Phase 10: Clean up module boundaries

- Reduce oversized `use` lists.
- Tighten visibility to `pub(super)` or private where possible.
- Add short module-level docs where a file owns a tricky subsystem.
- Only after the split is stable, consider a second pass if any new file is
  still too large.

## What Not To Do

- Do not combine refactoring with behavior changes.
- Do not introduce traits or generic abstraction layers just to “organize” the
  code.
- Do not split cyclic runtime state into too many tiny files on the first pass;
  `state.rs` may stay somewhat large initially.
- Do not change snapshot or bytecode formats as part of this move.
- Do not change public names if re-exports solve the problem.

## Verification Plan

After each phase:

- `cargo test -p mustard`

At the end of each substantial milestone:

- `cargo test --workspace`
- `npm test`
- `npm run lint`

If a phase is purely mechanical and a full workspace run is too expensive, keep
the smaller Rust verification loop tight, then run the full suite before
merging the milestone.

## Done Criteria

The refactor is complete when all of these are true:

- `crates/mustard/src/runtime.rs` no longer exists as a monolithic file
- `crates/mustard/src/runtime/mod.rs` is a thin facade
- Compiler, VM, GC, conversions, and builtins live in separate modules
- Runtime tests are no longer embedded in the production module file
- Public API is unchanged
- `cargo test --workspace`, `npm test`, and `npm run lint` pass

## First Milestone I Would Land

If this work starts now, the safest first commit is:

1. Move `runtime.rs` to `runtime/mod.rs`
2. Extract `api.rs`, `bytecode.rs`, `serialization.rs`, and `validation.rs`
3. Re-export the same public API from `mod.rs`
4. Run `cargo test -p mustard`

That first cut removes a large amount of top-level noise without touching the
VM semantics yet.
