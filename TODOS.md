# jslite TODOs

This file tracks the phase-by-phase work needed to build `jslite`.

The goal is to keep this checklist execution-oriented. `README.md` describes the
project and its desired end state. This file describes the concrete work needed
to get there.

Testing is part of every phase. Do not defer verification work to the end.

## Locked Decisions

These decisions are no longer open and should guide implementation:

- Build through the full roadmap in `README.md`; the first milestone is an
  intermediate checkpoint, not the stopping point
- Enforce strict semantics for all guest code regardless of source text
- Start with a conservative built-in global surface:
  `globalThis`, `Math`, `JSON`, `Number`, `String`, `Boolean`, `Array`,
  `Object`, `Promise`, standard `Error` types, and a minimal deterministic
  `console`
- Reject `import`, `export`, dynamic `import()`, and `require`
- Restrict the host boundary to JSON-like values plus `undefined`; reject
  functions, cycles, class instances, and host objects that do not fit the
  supported value contract
- Allow host capabilities to return `Promise<json-like>`
- Make `run()` async and have it drive suspension and resume internally when
  guest async execution requires it
- Keep `start()` and `resume()` as explicit lower-level controls
- Prioritize plain objects and arrays with a deliberately reduced object model;
  defer prototype-heavy behavior and full `this` semantics until later
- Treat in-process execution as best-effort containment; sidecar mode is the
  stronger isolation boundary
- Target Rust stable and Node.js v24
- Serialized programs and snapshots only need to round-trip within the same
  `jslite` version

## Phase 0: Bootstrap

- [ ] Create the repository layout
- [ ] Add a Rust workspace with `crates/jslite` and `crates/jslite-node`
- [ ] Add a minimal Node package wrapper for the addon
- [ ] Add Rust unit and integration test harnesses
- [ ] Add Node integration and end-to-end test harnesses
- [ ] Add golden-file helpers for diagnostics, IR, and bytecode snapshots
- [ ] Configure formatting, linting, and CI
- [ ] Add Linux, macOS, and Windows build coverage
- [ ] Add Node.js v24 coverage in CI
- [ ] Add a minimal smoke test that loads the addon from Node
- [ ] Add a minimal end-to-end smoke test that compiles and runs guest code
- [ ] Write contribution and security guidance
- [ ] Document early source-build-only installation and defer prebuilt binaries
  until release work
- [ ] Document the core sandbox invariant: no ambient host access

Exit criteria:

- [ ] The workspace builds cleanly
- [ ] Rust and Node test harnesses both run in CI
- [ ] The Node addon loads successfully in CI
- [ ] The end-to-end smoke test passes in CI
- [ ] The repo has baseline docs and contributor guidance

## Phase 1: Parsing and Diagnostics

- [ ] Integrate `oxc` for parsing JavaScript source
- [ ] Define a supported-syntax matrix for v1
- [ ] Enforce parse-time rejection for modules, `require`, and other unsupported
  top-level forms
- [ ] Enforce strict semantics for all guest code regardless of source text
- [ ] Reject unsupported syntax explicitly and consistently
- [ ] Design source span handling for errors and tracebacks
- [ ] Define the internal IR data model
- [ ] Implement lowering from parser AST to IR
- [ ] Add parser acceptance tests for the supported subset
- [ ] Add rejection tests for unsupported syntax and unsupported runtime forms
- [ ] Add diagnostics snapshot tests with source spans
- [ ] Add golden tests for representative IR output

Exit criteria:

- [ ] `jslite` can parse supported input
- [ ] Unsupported syntax fails with clear diagnostics
- [ ] Parser, rejection, and IR golden tests pass in CI
- [ ] IR lowering is stable enough to build execution on top of it

## Phase 2: Bytecode and Minimal VM

- [ ] Design the bytecode format
- [ ] Decide and document stack-frame layout
- [ ] Implement constant loading
- [ ] Implement local variable access
- [ ] Implement arithmetic and comparison operations
- [ ] Implement branching and jump instructions
- [ ] Implement function calls and returns
- [ ] Implement lexical scope and closures
- [ ] Add a run-to-completion execution path
- [ ] Add tracebacks with guest source locations
- [ ] Add unit tests for bytecode decoding and core instruction behavior
- [ ] Add execution tests for locals, branching, calls, and closures
- [ ] Add pure-compute differential tests against Node for the supported subset

Exit criteria:

- [ ] Pure compute programs run correctly
- [ ] Closures work for representative cases
- [ ] Bytecode and VM execution tests pass in CI
- [ ] Runtime errors include useful guest-facing location data

## Phase 3: Heap, Objects, and GC

- [ ] Define the `JsValue` representation
- [ ] Implement heap allocation for strings, arrays, objects, and functions
- [ ] Design the object layout with shape metadata
- [ ] Implement the reduced object model for v1 plain objects and arrays
- [ ] Implement `Array`, `Object`, `Math`, and `JSON` within the conservative
  built-in surface
- [ ] Document the object-model deferrals around prototypes and full `this`
  semantics
- [ ] Implement property get and set semantics for supported cases
- [ ] Implement array behavior for supported cases
- [ ] Implement a non-moving mark-sweep collector
- [ ] Define the GC root set
- [ ] Add object, array, and built-in behavior tests for the supported subset
- [ ] Add GC stress tests

Exit criteria:

- [ ] Plain object and array programs run correctly
- [ ] Conservative built-ins behave correctly for supported cases
- [ ] GC collects unreachable cyclic data
- [ ] Heap, property, and GC stress tests pass in CI
- [ ] Property logic is centralized and testable

## Phase 4: Exceptions and Control Semantics

- [ ] Implement `throw`
- [ ] Implement `try`
- [ ] Implement `catch`
- [ ] Implement `finally`
- [ ] Implement VM unwind logic
- [ ] Define runtime exception types and rendering
- [ ] Implement guest-visible `Error` objects and the supported standard error
  hierarchy
- [ ] Ensure tracebacks never leak host paths or internals
- [ ] Add nested exception and unwind matrix tests
- [ ] Add diagnostics tests that verify guest-safe error rendering

Exit criteria:

- [ ] Nested exception handling works
- [ ] Guest-visible error objects behave correctly for supported cases
- [ ] Exception and diagnostics tests pass in CI
- [ ] Exception rendering is stable and host-safe

## Phase 5: Host Capabilities and Suspension

- [ ] Design the host capability registration API
- [ ] Implement named host function lookup
- [ ] Define and validate the JSON-like host boundary contract
- [ ] Implement argument conversion from guest to host
- [ ] Implement result conversion from host to guest
- [ ] Implement sync host calls
- [ ] Implement async host calls that suspend and later resume guest execution
- [ ] Sanitize host-thrown or rejected errors into guest-safe errors with
  `name`, `message`, `code`, and `details`
- [ ] Implement suspension objects for iterative execution
- [ ] Implement `start()` and resume flow
- [ ] Implement deterministic print or console callback support
- [ ] Add capability conversion tests for accepted and rejected values
- [ ] Add suspend and resume integration tests
- [ ] Add host error mapping tests
- [ ] Add deterministic console callback tests

Exit criteria:

- [ ] Host functions can be called safely and explicitly
- [ ] Iterative execution works end to end
- [ ] Capability, suspension, and error-mapping tests pass in CI
- [ ] The host does not need VM internals to resume execution

## Phase 6: Async Runtime

- [ ] Define internal promise representation
- [ ] Define the internal microtask queue
- [ ] Lower `async` functions into runtime form
- [ ] Implement `await`
- [ ] Implement async host-call suspension and resume
- [ ] Implement async `run()` that hides internal suspension and resume from the
  common Node API
- [ ] Ensure async execution composes with exceptions
- [ ] Add microtask ordering tests
- [ ] Add guest async function tests
- [ ] Add async host capability tests
- [ ] Add async differential tests against supported Node behavior

Exit criteria:

- [ ] Supported async programs run correctly
- [ ] Async host calls can suspend and resume cleanly
- [ ] Async execution and differential tests pass in CI
- [ ] Microtask behavior is predictable within the supported subset

## Phase 7: Serialization

- [ ] Serialize compiled programs
- [ ] Implement versioning for serialized formats
- [ ] Reject cross-version loads explicitly and safely
- [ ] Serialize execution snapshots at safe suspension points
- [ ] Implement load and restore APIs
- [ ] Add round-trip fixtures for compiled programs and snapshots
- [ ] Add corruption and invalid-input tests
- [ ] Add cross-process resume tests

Exit criteria:

- [ ] Compiled programs can round-trip without reparsing
- [ ] Execution snapshots can round-trip safely
- [ ] Serialization round-trip and corruption tests pass in CI
- [ ] Invalid serialized input fails safely

## Phase 8: Resource Limits and Hardening

- [ ] Add allocation counting
- [ ] Add memory accounting
- [ ] Add recursion-depth accounting
- [ ] Add execution time or instruction-budget enforcement
- [ ] Define and document reasonable default limits in the public API
- [ ] Define runtime cancellation semantics
- [ ] Add deterministic tests for each resource limit and cancellation path
- [ ] Add hostile-input tests
- [ ] Fuzz parser input
- [ ] Fuzz IR lowering
- [ ] Fuzz bytecode execution
- [ ] Fuzz snapshot loading

Exit criteria:

- [ ] Resource limits are enforced predictably
- [ ] The runtime fails safely on hostile input
- [ ] Limit, cancellation, and hostile-input tests pass in CI
- [ ] Fuzzing covers the critical boundaries

## Phase 9: Sidecar Isolation Mode

- [ ] Define the sidecar execution protocol
- [ ] Build a separate-process runner around the core runtime
- [ ] Define structured request and response messages
- [ ] Support compiled-program loading in sidecar mode
- [ ] Support snapshot resume in sidecar mode
- [ ] Define process lifecycle and termination behavior
- [ ] Add sidecar protocol and compatibility tests
- [ ] Add tests for crash containment and forceful termination

Exit criteria:

- [ ] Sidecar mode runs the same core runtime
- [ ] Process boundaries improve containment for production deployments
- [ ] Sidecar protocol and isolation tests pass in CI
- [ ] Forceful termination is possible without corrupting the host process

## Phase 10: Node Embedding Polish

- [ ] Build the `napi-rs` binding layer
- [ ] Design the high-level Node API
- [ ] Keep `run()` async while preserving explicit `start()` and `resume()`
- [ ] Implement input handling
- [ ] Implement capability registration in the Node wrapper
- [ ] Implement error conversion for syntax, runtime, resource, and snapshot errors
- [ ] Implement dump and load APIs
- [ ] Add TypeScript declarations
- [ ] Add TypeScript type tests for the public API
- [ ] Add Node integration tests
- [ ] Add packaging smoke tests for source builds on supported platforms
- [ ] Add example usage for agent-style execution

Exit criteria:

- [ ] The addon is usable from a real Node service
- [ ] TypeScript consumers get a clean typed API
- [ ] Node integration and packaging smoke tests pass in CI
- [ ] The Node wrapper stays thin and does not own core semantics

## Phase 11: Tests and Conformance

- [ ] Add unit tests for IR, bytecode, VM, GC, and serialization
- [ ] Add differential tests against Node for the supported subset
- [ ] Import a curated `test262` subset
- [ ] Exclude unsupported features explicitly
- [ ] Add regression tests for security-sensitive behaviors
- [ ] Add performance smoke benchmarks
- [ ] Audit earlier-phase test coverage and fill any gaps before release

Exit criteria:

- [ ] The supported subset is well covered
- [ ] Differential tests are stable
- [ ] Conformance, regression, and benchmark suites run cleanly
- [ ] `test262` coverage is deliberate rather than accidental

## Phase 12: Documentation and Release

- [ ] Document the supported subset clearly
- [ ] Document the capability model clearly
- [ ] Document what security guarantees are and are not provided
- [ ] Document sidecar-mode tradeoffs
- [ ] Write embedding examples
- [ ] Prepare npm publishing flow
- [ ] Add optional prebuilt-binary publishing only after the addon package shape
  is stable
- [ ] Prepare Rust crate publishing flow if needed
- [ ] Write release guidance
- [ ] Add release verification checklists for build, install, upgrade, and basic
  runtime smoke tests

Exit criteria:

- [ ] The docs match actual behavior
- [ ] Users can embed `jslite` without relying on tribal knowledge
- [ ] Release checklists are runnable and verified
- [ ] The project is publishable and maintainable

## Cross-Cutting Rules

- [ ] Keep the supported subset explicit and written down
- [ ] Write tests as features land; do not defer verification to the end
- [ ] Reject unsupported features clearly instead of partially emulating them
- [ ] Keep the public API small until real use cases justify expansion
- [ ] Avoid ambient globals and fallback host lookups
- [ ] Treat serialization and deserialization as security-sensitive boundaries
- [ ] Preserve guest-facing diagnostics without leaking host internals
- [ ] Keep the Node wrapper thin and keep runtime semantics in Rust

## First Milestone

This is an internal checkpoint, not the final stopping point.

The first milestone should prove the architecture end to end.

- [ ] Parse source
- [ ] Lower to IR
- [ ] Compile to bytecode
- [ ] Execute arithmetic, locals, functions, closures, arrays, and plain objects
- [ ] Support one host function call
- [ ] Support `start()` and `resume()`
- [ ] Expose the runtime through the Node addon
- [ ] Add end-to-end tests that cover the milestone path from parse to resume

Definition of done:

- [ ] A Node script can create a `Jslite` instance
- [ ] A supported guest program can run to completion
- [ ] A guest program can suspend on a host call and resume with a result
- [ ] The milestone end-to-end tests pass in CI
