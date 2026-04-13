# Property-Based Testing for `mustard`

## Goal

Two properties matter most for this project:

1. For generated programs inside the documented `mustard` subset, `mustard` and
   Node.js should produce the same guest-visible outcome.
2. For generated programs outside the documented subset, `mustard` should reject
   them during validation with a clear `Validation` error instead of partially
   running them.

Separately, hostile inputs should never crash the process, corrupt runtime
state, hang past the documented limits, or leak host-only details.

Property-based testing can strengthen confidence in those claims. It cannot
prove that `mustard` is "not exploitable." The strongest realistic claim is:

- no known crashers, panics, sanitizer findings, or fail-open behaviors across
  the maintained generated-input and fuzz suites
- unsupported features fail closed
- resource limits and serialization boundaries remain intact under hostile input

That is materially better than example-based tests alone, but it is still not a
formal proof of memory safety.

## Current Baseline

As of April 11, 2026, the repo already has useful foundations:

- Node differential tests in [tests/node/differential.test.js](tests/node/differential.test.js)
- async differential coverage in [tests/node/async-runtime.test.js](tests/node/async-runtime.test.js)
- curated `test262` coverage in [tests/test262/harness.test.js](tests/test262/harness.test.js)
- hostile-input `proptest` coverage in [crates/mustard/tests/security_hostile_inputs.rs](crates/mustard/tests/security_hostile_inputs.rs)
- libFuzzer entry points in [fuzz/fuzz_targets](fuzz/fuzz_targets)
- sidecar hostile-protocol coverage in [crates/mustard-sidecar/tests/hostile_protocol.rs](crates/mustard-sidecar/tests/hostile_protocol.rs)

I also verified the current baseline on this working tree:

- `npm run test:differential` passed
- `cargo test -p mustard --test security_hostile_inputs` passed

That means the project already has:

- curated Node parity checks for supported examples
- arbitrary-byte and hostile-source coverage for parser, bytecode, snapshots,
  and sidecar protocol
- guest-safe failure checks for several security-sensitive boundaries

## What Is Missing

The current suites are good, but they do not yet give strong property-based
evidence for the two target guarantees.

### Gap 1: semantic differential coverage is curated, not generated

The existing Node differential suite is hand-written. It covers representative
cases well, but it does not explore large spaces of supported programs.

### Gap 2: current `proptest` coverage is mostly hostile-input safety, not parity

The properties in
[crates/mustard/tests/security_hostile_inputs.rs](crates/mustard/tests/security_hostile_inputs.rs)
mostly say:

- arbitrary bytes do not crash loaders
- arbitrary source does not leak host paths
- bounded execution of arbitrary compilable source does not fail unsafely

Those are valuable hardening checks, but they do not compare `mustard` to Node.

### Gap 3: the current Node oracle is too weak for generated testing

[tests/node/runtime-oracle.js](tests/node/runtime-oracle.js) currently normalizes
arrays and plain objects, but it does not canonically encode:

- `NaN`
- `-0`
- `Infinity` and `-Infinity`
- thrown outcomes
- boundary rejection cases

That is acceptable for the curated tests, but not for large generated suites.

### Gap 4: unsupported-feature checking is example-based

The repo has explicit unsupported fixtures and hand-written validation tests,
but not a generator that can produce unsupported forms and assert that
`mustard` rejects them at validation time.

### Gap 5: the Node boundary is under-tested by generated inputs

The Rust core already gets hostile-input pressure. The Node addon and wrapper do
not yet get equivalent generated coverage for:

- structured input encoding
- structured output decoding
- host error mapping
- cancellation interleavings
- `Progress` lifecycle misuse

### Gap 6: existing fuzz targets are checked for buildability, not exercised in CI

[scripts/run-hardening.sh](scripts/run-hardening.sh) currently does:

- `cargo test -p mustard --test security_hostile_inputs`
- `cargo test -p mustard-sidecar --test hostile_protocol`
- `cargo check --manifest-path fuzz/Cargo.toml --bins`

That is enough to keep the fuzz targets wired up, but not enough to claim that
continuous fuzzing is actively exploring them.

## Recommended Strategy

The right approach is not one giant property suite. It should be split into two
tracks.

## Track A: Node Parity Or Validation Failure

This track should answer:

- if a generated program is supported, does `mustard` match Node?
- if a generated program is unsupported, does `mustard` reject it during
  validation?

### Recommendation: put the semantic differential generator in Node tests

Use a JS property-testing library such as `fast-check` in the Node test layer.

Why this is the right place:

- Node itself is the oracle
- the public API surface is the thing users actually consume
- it exercises the thin wrapper, addon boundary, compiler, validator, and
  runtime together
- it avoids reimplementing a JS oracle in Rust

### Do not start from arbitrary raw source text

Generating arbitrary strings is low-value for semantic parity because most
inputs are invalid syntax or unsupported syntax. The current hostile-input suite
already covers that shape.

For parity, generate a bounded mini-AST or source DSL that only emits:

- documented supported syntax from [docs/LANGUAGE.md](docs/LANGUAGE.md)
- final values that can cross the structured host boundary

Then pretty-print that AST to source and run both engines on the same program.

### Phase the generator

Start narrow. Expand only after the oracle is solid.

Phase 1 generator:

- literals: `undefined`, `null`, booleans, finite numbers, strings
- arrays and plain objects
- `let` and `const`
- arithmetic, comparisons, boolean logic, nullish coalescing
- `if`, `while`, bounded `for`
- function declarations and calls
- array/object destructuring already known to work
- final expression returns only structured values

Phase 2 generator:

- optional chaining
- template literals
- array helper callbacks
- `Map` and `Set` programs that project back to structured values
- `try` / `catch` / `finally`
- recursion within bounded depth

Phase 3 generator:

- async functions
- `await`
- Promise combinators
- deterministic host capability suspension with fixed host stubs

### Canonical outcome shape

The property should compare tagged outcomes, not raw JS values.

Suggested shape:

```js
{
  type: 'value',
  value: canonicalStructuredValue
}
```

Later, after the value path is stable:

```js
{
  type: 'throw',
  error: canonicalThrownValue
}
```

The canonical value encoder should preserve the same numeric edge cases already
handled by the public boundary in [index.js](index.js):

- `NaN`
- `-0`
- `Infinity`
- `-Infinity`

That encoder should be shared conceptually with `encodeStructured()` instead of
relying on plain `assert.deepEqual()` over raw JS values.

### Separate supported and unsupported generators

Do not mix them in one property at first.

Supported-program property:

- `new Mustard(source)` must succeed
- `await runtime.run()` must complete
- Node and `mustard` must match on canonical outcome

Unsupported-program property:

- generate exactly one unsupported feature from [docs/LANGUAGE.md](docs/LANGUAGE.md)
- place it in an otherwise minimal program
- `new Mustard(source)` must throw `MustardValidationError`
- the message should mention the unsupported feature class

That directly matches the required contract:

- same result if supported
- validation rejection if unsupported

### Good unsupported-feature seeds

The initial unsupported generator should draw from the repo's documented
rejections:

- `import` / `export`
- dynamic `import()`
- `delete`
- classes
- generators and `yield`
- `with`
- `for...in`
- `for await...of`
- object spread
- array spread
- sequence expressions
- free `eval`
- free `Function`
- free `arguments`

### Useful metamorphic properties

Once the core parity property is stable, add metamorphic checks that do not need
Node to be clever:

- alpha-renaming bound identifiers preserves outcome
- inserting dead `if (false) { ... }` blocks preserves outcome
- wrapping the program in a block preserves outcome
- `run()` and `start()` followed by immediate resume-free completion agree
- `compile -> dump/load -> execute` agrees with direct `execute`

These are cheap and catch compiler/runtime mismatches well.

## Track B: Memory-Safety And Hardening Evidence

This track should answer:

- can hostile inputs crash or corrupt the core?
- do loaders fail closed?
- do limits and cancellation stay intact under generated stress?
- does the wrapper reject unsupported boundary values safely?

### Important constraint

Property-based testing is necessary here, but not sufficient.

For exploitability reduction, the real stack should be:

- Rust `proptest` for high-density invariant checks
- libFuzzer for unstructured mutation at critical boundaries
- sanitizer-backed fuzzing for crash and memory-corruption detection
- sidecar deployment plus OS-level sandboxing for adversarial workloads

That matches the threat model already documented in
[docs/SECURITY_MODEL.md](docs/SECURITY_MODEL.md).

### Keep Rust properties for internal invariants

Rust-side property tests should focus on invariants that do not need Node:

1. Source compile safety
   - generated valid supported programs compile or reject cleanly
   - diagnostics stay guest-safe

2. Bytecode round-trip invariants
   - `compile -> lower_to_bytecode -> dump_program -> load_program` preserves
     executable behavior

3. Snapshot round-trip invariants
   - for programs that suspend, `dump_snapshot -> load_snapshot -> resume`
     preserves behavior and accounting

4. Budget invariants
   - generated looping/allocation-heavy programs either finish or fail with a
     documented limit error
   - no panic, hang, or guest-visible host leak

5. Sidecar equivalence
   - generated structured requests sent through `handle_request_line()` produce
     the same success/error class as direct library calls

### Add a valid-program generator in Rust

The current hostile-input `proptest` mostly uses arbitrary bytes. Add a second
property layer that generates valid supported programs on purpose.

That matters because:

- arbitrary bytes spend most test cases in parse failure
- valid programs reach lowering, bytecode, control flow, heap, exceptions, and
  async states far more often

This valid-program generator does not need to cover the full language at first.
It only needs enough shape to drive real runtime states.

### Add generated tests for serialization boundaries

The repo already fuzzes and mutates serialized programs and snapshots. What is
missing is a higher-level property that says:

- valid serialized data round-trips to the same behavior
- invalid or corrupted data rejects cleanly before entering unsafe state

That should cover:

- plain programs
- suspending programs
- iterator snapshots
- keyed-collection snapshots
- promise/async snapshots

### Add generated Node-boundary tests

The public boundary deserves its own property tests in JS.

Suggested properties:

1. Supported structured values round-trip
   - generate nested plain objects and arrays of supported primitives
   - pass them as `inputs`
   - return them unchanged from guest code
   - assert exact canonical round-trip

2. Unsupported host values fail closed
   - generate or enumerate:
     - functions
     - class instances
     - `Date`
     - `Map`
     - `Set`
     - objects with custom prototypes
     - getters/setters
     - cyclic objects
     - proxies
   - assert clean JS or `MustardError` failure
   - assert the process stays alive

3. Host error sanitization shape
   - generate host errors with combinations of `name`, `message`, `code`, and
     `details`
   - assert guest-visible shape is preserved only in the documented fields

4. `Progress` misuse stays safe
   - random sequences of `resume`, `resumeError`, `cancel`, and `dump/load`
   - assert single-use semantics and safe failure

5. Cancellation interleavings
   - abort before start
   - abort during async capability wait
   - abort after suspension but before resume
   - assert only documented completion, cancellation, or limit outcomes

### Strengthen fuzzing rather than replacing it

The existing fuzz targets are good and should stay:

- `parser`
- `ir_lowering`
- `bytecode_validation`
- `bytecode_execution`
- `snapshot_load`
- `sidecar_protocol`

Recommended upgrades:

- seed corpora from:
  - `tests/node/differential.test.js`
  - `tests/node/async-runtime.test.js`
  - `tests/test262/cases/pass`
  - minimal unsupported fixtures for validator coverage
- add dictionaries for common tokens and keywords
- run long-lived fuzz jobs outside the normal unit-test path
- record and minimize any crasher corpus

## Proposed Test Layout

If this is implemented incrementally, the file layout should stay simple.

Suggested additions:

- `tests/node/property-runtime-oracle.js`
- `tests/node/property-differential.test.js`
- `tests/node/property-unsupported.test.js`
- `tests/node/property-host-boundary.test.js`
- `crates/mustard/tests/property_roundtrip.rs`
- `crates/mustard/tests/property_snapshot_roundtrip.rs`
- `crates/mustard/tests/property_generated_execution.rs`

Suggested dependency addition:

- `fast-check` in `devDependencies`

## Suggested Rollout Order

1. Upgrade the Node oracle.
   - add canonical encoding for numeric edge cases and structured outcomes

2. Add a small supported-program generator in Node.
   - keep it value-only
   - compare Node and `mustard`

3. Add an unsupported-feature generator in Node.
   - require constructor-time `Validation` failure

4. Add Rust valid-program properties.
   - round-trip bytecode and snapshots
   - budget and accounting invariants

5. Add JS property tests for the host boundary.
   - structured values
   - host errors
   - `Progress`
   - cancellation

6. Upgrade fuzz execution outside CI smoke checks.
   - keep `cargo check` in the fast path
   - add longer sanitizer-backed fuzz jobs separately

## Practical Acceptance Criteria

If this work is implemented well, the resulting property-based test strategy
should let the project make these narrower, defensible claims:

- every generated supported program in the maintained subset either matches
  Node or exposes a concrete parity bug
- every generated unsupported program is rejected during validation
- arbitrary hostile bytes at parser, loader, snapshot, and sidecar boundaries
  do not produce panics or fail-open behavior
- public host-boundary conversions either round-trip correctly or reject cleanly
- limits, cancellation, and serialized-state restoration hold under generated
  stress

That is the right testing target for `mustard`.

It directly supports the subset contract, the fail-closed rule, and the
security model without pretending that property tests alone can certify absolute
memory safety.
