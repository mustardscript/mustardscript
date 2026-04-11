# jslite TODOs

This file tracks the concrete work required to build `jslite`.

`README.md` explains the goals, threat model, and architecture. This file turns
that into executable milestones with exit criteria.

Testing is part of every phase. No feature is done until the tests, docs, and
failure behavior for that feature exist.

## Non-Negotiable Rules

- Keep the supported subset explicit and written down
- Fail closed on unsupported features
- Treat in-process addon mode as best-effort containment, not as a hard security
  boundary
- Treat snapshots, compiled bytecode, and sidecar messages as untrusted input
- Keep the Node wrapper thin and keep guest semantics in Rust
- Do not serialize opaque host references, native handles, or unresolved host
  futures
- Do not make optimizations a prerequisite for correctness
- Prefer centralized semantics over scattered fast paths in early phases
- A phase is not complete until its exit criteria pass

## Locked Decisions

These decisions are closed unless a later ADR explicitly changes them:

- The runtime core is written in Rust
- The primary in-process embedder is a Node-API addon, likely through `napi-rs`
- The parser frontend is `oxc` unless evaluation proves it unworkable
- Guest code always runs with strict semantics
- There is no ambient module system, no `eval`, and no `Function` constructor
- The built-in surface starts conservative and explicit
- `run()` is async in the Node API
- `start()` and `resume()` remain available as lower-level controls
- Sidecar mode runs the same core runtime behind a structured IPC boundary
- Compiled programs and snapshots only need to round-trip within the same
  `jslite` version
- Resource accounting is a core design concern, not a late-stage add-on

## Required Design Docs Before Major Implementation

Create and maintain these docs early:

- [x] `docs/SECURITY_MODEL.md`
- [x] `docs/LANGUAGE.md`
- [x] `docs/HOST_API.md`
- [x] `docs/SERIALIZATION.md`
- [x] `docs/LIMITS.md`
- [x] `docs/ADRs/` for irreversible decisions

Minimum content:

### `SECURITY_MODEL.md`

- [x] Define threat assumptions
- [x] Define what addon mode does and does not guarantee
- [x] Define what sidecar mode improves
- [x] Define what requires host-managed OS sandboxing
- [x] Define which failures are considered security issues

### `LANGUAGE.md`

- [x] Define the supported syntax matrix
- [x] Define supported runtime semantics
- [x] Define forbidden forms and their diagnostics
- [x] Define built-ins and global names
- [x] Define semantic deferrals such as prototypes, descriptors, iterators, and
  `this`

### `HOST_API.md`

- [x] Define the structured host value contract
- [x] Define sync and async capability calls
- [x] Define host error sanitization
- [x] Define console or print behavior
- [x] Define reentrancy rules
- [x] Define cancellation and abort propagation

### `SERIALIZATION.md`

- [x] Define compiled-program format goals
- [x] Define snapshot safety rules
- [x] Define versioning and validation requirements
- [x] Define how non-JSON values are tagged and preserved
- [x] Define what may never be serialized

### `LIMITS.md`

- [x] Define instruction budgeting
- [x] Define heap accounting and limits
- [x] Define call-depth limits
- [x] Define outstanding-host-call limits
- [x] Define cancellation semantics
- [x] Define default public limits

Exit criteria:

- [x] Each doc exists with concrete decisions, not placeholders
- [x] The docs are internally consistent with `README.md`
- [x] The docs are referenced from the repository root

## Phase 0: Repository Bootstrap

- [x] Create repository layout
- [x] Add Rust workspace with `crates/jslite`, `crates/jslite-node`, and
  `crates/jslite-sidecar`
- [x] Add minimal Node package wrapper for the addon
- [x] Add Rust unit and integration test harnesses
- [x] Add Node integration and end-to-end test harnesses
- [x] Add golden-file helpers for diagnostics, IR, and bytecode snapshots
- [x] Configure formatting, linting, and CI
- [ ] Add Linux, macOS, and Windows build coverage
- [x] Add Node.js target coverage in CI
- [x] Add minimal smoke test that loads the addon from Node
- [x] Add minimal end-to-end smoke test that compiles and runs guest code
- [x] Write contribution, security, and disclosure guidance
- [x] Document source-build-only installation first
- [x] Link the core sandbox invariant from the root docs

Exit criteria:

- [x] Workspace builds cleanly
- [x] Rust and Node test harnesses both run in CI
- [x] Addon loads successfully in CI
- [x] End-to-end smoke test passes in CI
- [x] Baseline docs and contributor guidance exist

## Phase 1: Parsing, Validation, and Diagnostics

- [x] Integrate `oxc` for JavaScript parsing
- [x] Decide whether `jslite` accepts scripts only or a tightly defined script
  subset with module syntax rejected
- [x] Define and publish the supported-syntax matrix for v1
- [x] Implement a validation pass after parsing
- [x] Reject `import`, `export`, and dynamic `import()`
- [x] Reject `eval` and `Function` constructor use
- [x] Define how unresolved free references to forbidden ambient globals are
  diagnosed
- [ ] Preserve source spans for errors and tracebacks
- [x] Define the internal IR data model
- [x] Lower parser AST to IR
- [x] Add parser acceptance tests for the supported subset
- [x] Add rejection tests for unsupported syntax and forbidden forms
- [x] Add diagnostics snapshot tests with source spans
- [x] Add golden tests for representative IR output

Exit criteria:

- [x] Supported input parses and validates
- [x] Unsupported input fails with clear diagnostics
- [x] IR lowering is stable enough to build execution on top
- [x] Parser, validation, diagnostics, and IR tests pass

## Phase 2: Minimal VM With Limits From Day One

- [x] Design the bytecode format
- [x] Document frame layout and operand model
- [x] Implement bytecode validation
- [x] Implement constant loading
- [x] Implement local variable access
- [x] Implement arithmetic and comparison operations
- [x] Implement branching and jumps
- [x] Implement function calls and returns
- [x] Implement lexical scope and closures
- [x] Add a run-to-completion execution path
- [x] Add instruction-budget accounting
- [ ] Add cancellation checks at defined execution points
- [ ] Add tracebacks with guest source locations
- [x] Add unit tests for bytecode decoding and instruction behavior
- [x] Add execution tests for locals, branching, calls, and closures
- [ ] Add pure-compute differential tests against Node for the supported subset

Exit criteria:

- [x] Pure compute programs run correctly
- [x] Closures work for representative cases
- [ ] Instruction budget and cancellation behave predictably
- [x] Bytecode validation and VM tests pass
- [ ] Runtime errors include useful guest-facing location data

## Phase 3: Heap, Plain Objects, Arrays, and GC

- [ ] Define `JsValue`
- [ ] Define rooting and handle rules
- [x] Implement heap allocation for strings, arrays, objects, and functions
- [x] Start with a centralized plain-object and array semantic layer
- [x] Choose an initial object storage strategy that optimizes for correctness
  first
- [ ] If shapes are used, document them as an optimization layer rather than a
  semantic dependency
- [ ] Define enumeration order for supported cases
- [x] Implement property get and set semantics for supported cases
- [ ] Decide whether property deletion is supported in v1 and document it
- [x] Implement array behavior for supported cases
- [x] Implement `Object`, `Array`, `Math`, and `JSON`
- [ ] Add heap accounting hooks
- [ ] Implement a non-moving mark-sweep collector
- [ ] Define and test the GC root set
- [x] Add object, array, and built-in behavior tests
- [ ] Add GC stress tests

Exit criteria:

- [ ] Plain-object and array programs run correctly
- [ ] Conservative built-ins behave correctly for supported cases
- [ ] Heap limits and allocation accounting are wired into the runtime
- [ ] GC collects unreachable cyclic data
- [ ] Heap, property, and GC tests pass

## Phase 4: Exceptions and Guest-Safe Errors

- [ ] Implement `throw`
- [ ] Implement `try`, `catch`, and `finally`
- [ ] Implement VM unwind logic
- [ ] Define runtime exception types and rendering
- [ ] Implement guest-visible `Error` objects and supported standard errors
- [ ] Ensure tracebacks never leak host paths or host internals
- [ ] Decide what stack information is guest-visible
- [ ] Add nested exception and unwind matrix tests
- [ ] Add diagnostics tests for guest-safe rendering

Exit criteria:

- [ ] Nested exception handling works
- [ ] Guest-visible errors behave correctly for supported cases
- [ ] Exception rendering is stable and host-safe
- [ ] Exception and diagnostics tests pass

## Phase 5: Host Capabilities and Suspension

- [x] Design capability registration in the core runtime
- [x] Define the structured host value contract precisely
- [x] Decide how numbers such as `NaN`, `Infinity`, and `-0` cross the boundary
- [x] Implement argument conversion from guest to host
- [x] Implement result conversion from host to guest
- [x] Reject disallowed values with clear guest-safe errors
- [x] Implement named capability lookup
- [x] Implement sync host calls
- [x] Implement async host calls that suspend guest execution
- [x] Sanitize host-thrown or rejected errors into guest-safe errors with
  `name`, `message`, `code`, and `details`
- [ ] Define and implement non-reentrant execution rules
- [x] Implement suspension objects for iterative execution
- [x] Implement `start()` and `resume()` flow
- [ ] Implement deterministic console or print callback support
- [x] Add conversion tests for accepted and rejected values
- [x] Add suspension and resume integration tests
- [x] Add host error mapping tests
- [ ] Add deterministic console callback tests

Exit criteria:

- [x] Host functions can be called explicitly and safely
- [x] Disallowed boundary values fail clearly
- [x] Iterative execution works end to end
- [x] Non-reentrant behavior is enforced or clearly documented
- [x] Capability, suspension, and error-mapping tests pass

## Phase 6: Async Runtime and Promise Semantics

- [ ] Define internal promise representation
- [ ] Define internal microtask queue
- [ ] Define microtask checkpoints and ordering rules
- [ ] Lower `async` functions into runtime form
- [ ] Implement `await`
- [ ] Implement async host-call suspension and resume
- [ ] Finalize `Promise` in the built-in surface
- [ ] Ensure async execution composes correctly with exceptions
- [ ] Define behavior for cancellation while guest code is awaiting a host
  promise
- [ ] Define maximum outstanding host calls
- [ ] Add microtask ordering tests
- [ ] Add guest async function tests
- [ ] Add async host capability tests
- [ ] Add async differential tests against supported Node behavior

Exit criteria:

- [ ] Supported async programs run correctly
- [ ] Async host calls suspend and resume cleanly
- [ ] Microtask behavior is predictable within the supported subset
- [ ] Async execution and differential tests pass

## Phase 7: Serialization and Safe Snapshotting

- [x] Serialize compiled programs
- [x] Implement explicit versioning for serialized formats
- [x] Validate serialized inputs before load
- [x] Reject cross-version loads explicitly
- [x] Define a tagged encoding for values that plain JSON cannot preserve
- [x] Serialize execution snapshots at safe suspension points only
- [x] Define what suspended external work looks like in a snapshot
- [x] Represent pending host work through continuation tokens or equivalent
  resumable metadata
- [ ] Ensure opaque host futures are never serialized
- [x] Implement load and restore APIs
- [x] Add round-trip fixtures for compiled programs and snapshots
- [x] Add corruption and invalid-input tests
- [x] Add cross-process resume tests

Exit criteria:

- [x] Compiled programs round-trip without reparsing
- [x] Execution snapshots round-trip safely at supported suspension points
- [x] Invalid or corrupted serialized input fails safely
- [x] Serialization tests pass

## Phase 8: Sidecar Protocol and Isolation

- [x] Define the sidecar protocol
- [x] Define structured request and response messages
- [x] Decide whether the transport is stdio, sockets, or both
- [x] Build a separate-process runner around the core runtime
- [x] Support compiled-program loading in sidecar mode
- [x] Support snapshot resume in sidecar mode
- [ ] Define lifecycle, shutdown, and termination behavior
- [ ] Define kill semantics for stuck or over-budget executions
- [ ] Decide how host capabilities are proxied across the sidecar boundary
- [x] Document how sidecar mode interacts with OS-level sandboxing
- [x] Add sidecar protocol and compatibility tests
- [ ] Add crash-containment and forceful-termination tests

Exit criteria:

- [ ] Sidecar mode runs the same core runtime
- [ ] Sidecar protocol is documented and tested
- [ ] Forceful termination is possible without corrupting the host process
- [ ] Isolation tests pass

## Phase 9: Node Binding and Packaging

- [x] Build the `napi-rs` binding layer
- [x] Design the high-level Node API around the core contract
- [x] Keep `run()` async while preserving explicit `start()` and `resume()`
- [x] Implement input handling
- [x] Implement capability registration in the Node wrapper
- [ ] Implement error conversion for syntax, runtime, resource, and snapshot
  errors
- [x] Implement program dump/load APIs
- [ ] Implement snapshot dump/load APIs
- [ ] Add TypeScript declarations
- [ ] Add TypeScript type tests for the public API
- [x] Add Node integration tests
- [ ] Add packaging smoke tests for source builds on supported platforms
- [ ] Add example usage for agent-style execution
- [x] Defer prebuilt binaries until the package shape is stable

Exit criteria:

- [ ] Addon is usable from a real Node service
- [ ] TypeScript consumers get a clean typed API
- [ ] Node integration and packaging smoke tests pass
- [ ] Node wrapper remains thin and does not own core semantics

## Phase 10: Security Hardening, Fuzzing, and Hostile Inputs

- [ ] Add hostile-input test suites
- [ ] Add parser fuzzing
- [ ] Add IR lowering fuzzing
- [ ] Add bytecode validation fuzzing
- [ ] Add bytecode execution fuzzing
- [ ] Add snapshot-load fuzzing
- [ ] Add sidecar-protocol fuzzing
- [ ] Add regression tests for security-sensitive behaviors
- [ ] Add fault-injection tests for cancellation, limits, and corrupted state
- [ ] Audit denial-of-service failure modes
- [ ] Verify resource failures stay guest-safe

Exit criteria:

- [ ] Critical boundaries are fuzzed
- [ ] Hostile-input failures are safe and reproducible
- [ ] Security regressions are covered by tests
- [ ] Hardening suites pass or continuous fuzzing infrastructure

## Phase 11: Conformance, Benchmarking, and Coverage Audit

- [ ] Add unit tests for IR, bytecode, VM, GC, capabilities, async, and
  serialization
- [ ] Expand differential tests against Node for the supported subset
- [ ] Import a curated `test262` subset
- [ ] Exclude unsupported features explicitly
- [ ] Add performance smoke benchmarks
- [ ] Define startup, memory, and suspension overhead budgets
- [ ] Audit earlier-phase test coverage
- [ ] Fill coverage gaps before release

Exit criteria:

- [ ] Supported subset is well covered
- [ ] Differential tests are stable
- [ ] `test262` coverage is deliberate rather than accidental
- [ ] Benchmark and coverage results are available to maintainers

## Phase 12: Documentation and Release

- [ ] Ensure `README.md` matches actual behavior
- [ ] Publish the supported subset clearly
- [ ] Publish the capability model clearly
- [ ] Publish security guarantees and non-guarantees clearly
- [ ] Publish sidecar-mode tradeoffs clearly
- [ ] Write embedding examples
- [ ] Prepare npm publishing flow
- [ ] Add optional prebuilt-binary publishing only after package shape is stable
- [ ] Prepare Rust crate publishing flow if needed
- [ ] Write release guidance
- [ ] Add release verification checklists for build, install, upgrade, and basic
  runtime smoke tests

Exit criteria:

- [ ] Docs match the implementation
- [ ] Users can embed `jslite` without tribal knowledge
- [ ] Release checklists are runnable and verified
- [ ] Project is publishable and maintainable

## First Real Milestone

This should prove the architecture end to end without overpromising security or
language breadth.

- [x] Parse source
- [x] Validate supported and forbidden forms
- [x] Lower to IR
- [x] Compile to bytecode
- [x] Execute arithmetic, locals, functions, and closures
- [x] Execute plain arrays and plain objects for supported cases
- [x] Support one named host capability
- [x] Support suspension with `start()` and `resume()`
- [x] Enforce an instruction budget
- [x] Expose the runtime through the Node addon
- [x] Add end-to-end tests that cover parse through resume

Definition of done:

- [x] A Node script can compile a program
- [x] A supported guest program can run to completion
- [x] A guest program can suspend on a host call and resume with a result
- [x] Over-budget execution fails predictably
- [x] Milestone tests pass

## Production-Readiness Gate

Before claiming the project is ready for untrusted guest workloads:

- [x] Sidecar mode exists and is tested
- [x] Security model is published
- [x] Limits are enabled by default
- [x] Serialization validation is enabled
- [x] Host errors are sanitized
- [ ] Guest diagnostics do not leak host internals
- [ ] Kill and cancellation behavior are documented and tested
- [x] Supported subset and unsupported subset are both explicit
