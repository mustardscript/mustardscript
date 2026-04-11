# jslite

`jslite` is a small, opinionated JavaScript runtime for executing a deliberately
limited subset of JavaScript inside a Node.js service with explicit host
capabilities, bounded resources, and resumable execution.

This project is **not** trying to recreate Node.js, V8, npm compatibility, or a
browser. It is trying to provide a compact execution engine for sandboxed
agent-style scripts and other constrained guest code.

## Status

`jslite` is an early-stage design and implementation project.

Two warnings belong at the top because they affect almost every technical
decision:

1. **In-process addon mode is not a hard security boundary.** It is a
   low-latency embedding mode. If the runtime has a memory-safety bug, logic
   bug, or denial-of-service failure, the host process can still be impacted.
2. **Sidecar mode is the deployment mode for stronger isolation.** For
   adversarial workloads, sidecar mode should be combined with OS-level controls
   such as process limits, sandboxing, containers, or platform-native jail
   mechanisms.

## Current Baseline

The current implementation already supports:

- parse -> validate -> IR -> bytecode -> VM execution for a synchronous subset
- `let`/`const`, functions and closures, arrays, plain objects, loops, and
  basic control flow
- `Math` and `JSON` built-ins
- explicit named host capabilities with `start()` / `resume()` suspension
- same-version compiled-program and suspension snapshot round trips
- a thin Node addon wrapper and a sidecar process that reuse the same Rust core

The current implementation does **not** yet execute:

- `throw`, `try`, `catch`, or `finally`
- `async` functions or `await`
- deterministic console callbacks
- cancellation, heap limits, call-depth limits, or outstanding-host-call limits
- deep bytecode or snapshot structural validation beyond decode and version
  checks

## Reference Docs

- [Security Model](docs/SECURITY_MODEL.md)
- [Language Contract](docs/LANGUAGE.md)
- [Host API](docs/HOST_API.md)
- [Serialization](docs/SERIALIZATION.md)
- [Limits](docs/LIMITS.md)
- [Bytecode VM Model](docs/BYTECODE.md)
- [Architecture ADRs](docs/ADRs/0001-core-architecture.md)

## Installation

`jslite` should currently be treated as a source-build-only package.

From a clean checkout:

```sh
npm install
npm test
```

That flow builds the Rust addon locally and then runs the Node integration
tests. Prebuilt binaries are intentionally deferred until the package shape is
stable.

## Project Goals

`jslite` should provide:

- A small, auditable runtime surface
- No ambient filesystem, network, environment, module, or subprocess access
- Explicit host capabilities instead of implicit globals
- Fast startup and low embedding overhead
- Precise accounting for instructions, memory, allocations, and call depth
- Deterministic or tightly specified behavior for the supported subset
- Suspension and resume at explicit host boundaries
- Same-version serialization of compiled programs and execution snapshots
- A Node.js-first embedding experience with a thin wrapper over a reusable Rust
  core

## Non-Goals

`jslite` is not intended to be:

- A secure wrapper around `node:vm`
- A general-purpose JavaScript runtime
- A compatibility layer for npm packages
- A CommonJS environment
- An ES module loader
- A DOM or browser runtime
- A JIT
- A drop-in replacement for Node.js or V8
- A place where unsupported features are partially emulated “well enough”

Unsupported features should fail closed with clear diagnostics.

## Design Principles

### 1. No Ambient Authority

Guest code starts with no access to the host outside core language semantics and
the approved built-in surface. There is no ambient `process`, `require`,
filesystem, network, environment, timers, subprocess API, or native addon
access.

Anything effectful must come from an explicit host capability.

### 2. A Small Language Contract Beats an Implicit One

The supported subset must be written down precisely. A small, explicit language
contract is better than accidental compatibility.

### 3. Safety Properties Must Be Designed Early

Instruction budgeting, cancellation, memory accounting, snapshot validation, and
host-boundary validation are not “polish.” They shape the VM, bytecode, async
model, and public API and must be designed from the beginning.

### 4. The Core Owns the Semantics

The Rust runtime owns guest semantics. The Node wrapper should be thin. Sidecar
mode should run the same core runtime with a different transport boundary.

### 5. Correctness Before Cleverness

For v1, centralized semantics and predictable behavior matter more than advanced
optimizations. Optimizations such as shapes, inline caches, and specialized
representations should be introduced only after the baseline semantics are
correct and well tested.

## Threat Model and Deployment Modes

`jslite` should document three deployment modes clearly.

### Addon Mode

- In-process Node-API addon
- Lowest latency
- Shares the host process
- Best-effort containment only
- Suitable for trusted or semi-trusted guest workloads where latency matters
  more than isolation

### Sidecar Mode

- Separate process running the same Rust core
- Structured IPC boundary
- Better crash containment
- Easier forceful termination
- Better choice for untrusted or resource-heavy workloads

### Hardened Sidecar Deployment

- Sidecar mode plus OS-level controls
- Recommended for adversarial guest code
- Examples include CPU and memory limits, restricted syscalls or sandbox
  policies, containerization, job objects, cgroups, or platform-native jail
  mechanisms

`jslite` itself is responsible for language-level containment. Production
security for untrusted inputs should assume sidecar mode plus host-managed OS
controls.

## Core Terminology

- **Guest code**: JavaScript executed by `jslite`
- **Host**: The embedding application
- **Capability**: A named host function intentionally exposed to the guest
- **Suspension point**: A boundary where guest execution pauses awaiting host
  progress
- **Snapshot**: A serialized representation of a compiled program or suspended
  execution state
- **Structured host value**: A value that may legally cross the host boundary

## Technical Choices

### Rust Core

The interpreter core should be written in Rust.

Reasons:

- Strong memory-safety baseline
- Good tooling for parsers, serialization, fuzzing, and testing
- Clean path to a Node-API addon
- Practical fit for a custom VM and capability boundary

### Node-API Native Addon First

The primary in-process embedding should be a Node-API native addon, likely via
`napi-rs`.

Reasons:

- The target embedder is a Node.js service
- Node-API keeps the host interop layer stable and relatively small
- `napi-rs` is a practical way to keep the Node binding thin while leaving the
  runtime in Rust

### Oxc Frontend

Use `oxc` as the parser frontend unless evaluation proves it to be a poor fit.

Reasons:

- It is a strong Rust-native frontend for JavaScript parsing
- It allows `jslite` to separate parsing from runtime design
- It is a better fit than building a parser from scratch before the runtime
  architecture exists

### Custom Execution Pipeline

The runtime pipeline should be:

`source -> parse -> validate -> lowered IR -> bytecode -> VM`

The explicit validation phase matters. Parsing alone is not enough because some
things should be rejected as unsupported semantics rather than syntax errors.

## Language Contract for v1

The first useful version should support a strict, intentionally limited subset of
JavaScript.

### Baseline Semantic Rules

- Guest code always runs with strict semantics
- There is no ambient module system
- There is no dynamic code loading
- Unsupported features fail with explicit diagnostics
- Diagnostics and tracebacks must not leak host paths or host internals

### Current Implemented Baseline

- Numbers, booleans, strings, `null`, and `undefined`
- Arrays and plain objects
- `let` and `const`
- Functions and closures
- Arrow functions
- `if`, `switch`, loops, `break`, and `continue`
- Common-case destructuring
- Template literals
- Optional chaining and nullish coalescing
- Host capability calls
- Suspension and resume at host boundaries
- Snapshotting at safe suspension points

### Parsed But Not Yet Executable

- `throw`
- `try`, `catch`, and `finally`
- `async` functions and `await`

### Explicitly Out of Scope for v1

- ES modules
- CommonJS
- `eval`
- `Function` constructor
- `with`
- Classes
- Generators and iterator protocol
- `for...of`
- Symbols
- `Map`, `Set`, `WeakMap`, `WeakSet`
- Typed arrays, `ArrayBuffer`, shared memory, and atomics
- `Date`
- `Intl`
- `Proxy`
- Full `RegExp` parity
- Full property descriptor semantics
- Accessors
- Full prototype semantics
- Implicit host globals such as `process`, `module`, `exports`, `global`,
  `require`, timers, or fetch-like APIs

### Important Clarification About Names Like `require`

`jslite` should not reject arbitrary identifiers named `require` or `process`.
Those names can be legitimate local bindings in JavaScript.

What `jslite` should reject is:

- module syntax and dynamic import forms
- dynamic code loading primitives such as `eval` and `Function`
- unresolved free references to forbidden ambient globals, when static
  resolution can prove they are free references

If a program defines its own local `require`, that is ordinary lexical
JavaScript and should be treated as such.

## Built-In Surface

The initial built-in surface should be conservative and explicit.

Currently implemented built-ins:

- `globalThis`
- `Object`
- `Array`
- `String`
- `Number`
- `Boolean`
- `Math`
- `JSON`
- A placeholder `console` global object

`Promise` should be considered part of the async milestone, not a separate early
promise of compatibility before the internal async runtime exists.

No default clock, random source, filesystem, network, timers, or environment
access should exist in the guest runtime. If the host wants those capabilities,
it must provide them explicitly.

## Structured Host Boundary

The host boundary should be narrowly defined and documented as its own contract.

### Allowed Structured Host Values

- `undefined`
- `null`
- booleans
- strings
- numbers, including non-finite values and `-0`
- arrays of structured host values
- plain objects with string keys and structured host values

### Rejected at the Host Boundary

- functions
- symbols
- bigint, unless and until bigint support is added deliberately
- class instances
- host objects
- dates
- regex objects
- maps, sets, typed arrays, buffers, and array buffers
- cycles
- objects with accessors or custom prototypes

This is intentionally narrower than general JavaScript values.

The sidecar wire format and snapshot format should **not** rely on plain JSON if
that would lose information such as `undefined`, `NaN`, `Infinity`, or `-0`.
Use a tagged internal encoding instead.

## Architecture

### Frontend and Validation

Responsibilities:

- Parse source text
- Reject modules and unsupported syntax with clear diagnostics
- Preserve spans for tracebacks and error reporting
- Run a validation pass that rejects forbidden dynamic forms and unsupported
  semantic constructs
- Lower valid programs into an internal IR

### IR

The IR should make control flow and semantics explicit.

Design goals:

- Explicit scopes and bindings
- Explicit function and closure boundaries
- Explicit exception regions
- Explicit host-call suspension points
- A structure that is convenient for validation and bytecode generation

### Bytecode

The bytecode should be:

- Easy to interpret
- Easy to validate
- Easy to serialize
- Instrumentable for instruction budgeting
- Private to `jslite`, not a public stable standard

Compiled programs only need to round-trip within the same `jslite` version.

### VM

The VM should initially be stack-based unless profiling proves a different choice
worth the complexity.

Responsibilities:

- Execute bytecode
- Maintain call frames and lexical environments
- Handle control flow and exceptions
- Enforce instruction budgets and cancellation checks
- Suspend and resume around host interactions
- Produce guest-safe tracebacks

### Values, Objects, and Heap

`jslite` should define an explicit internal `JsValue` type and a heap object
model with a disciplined rooting strategy.

For v1, object semantics should prioritize correctness and centralization over
aggressive optimization:

- centralized property get, set, and deletion rules
- plain-object and array behavior first
- explicit decisions around enumeration order
- explicit decisions around prototype support and deferrals

A simple dictionary-backed representation is a sensible starting point. Shapes or
hidden-class-style metadata can be added later if profiling shows a real need.
They should be treated as an optimization, not as the foundation of the v1
semantic model.

### Garbage Collection

Use a non-moving mark-sweep collector in v1.

Requirements:

- explicit root-set design
- no raw guest references crossing host boundaries without rooting
- test coverage for cyclic data
- accounting hooks for heap limits and allocation tracking

### Exceptions

Guest exceptions must be guest-facing objects with guest-safe rendering.

Requirements:

- support `throw`, `try`, `catch`, and `finally`
- standard error hierarchy for the supported subset
- tracebacks mapped to guest source spans
- no host paths, internal filenames, or Rust panic details in guest output

### Async Runtime

`jslite` should own its own async model.

Requirements:

- internal promise representation
- internal microtask queue
- explicit scheduling checkpoints
- clear ordering rules
- suspension at host boundaries
- no reentrancy into the same VM unless explicitly designed and documented

The host should not need to understand VM internals to resume guest execution.

### Serialization and Snapshots

Two formats matter:

1. **Compiled program format**
2. **Suspended execution snapshot format**

Requirements:

- same-version round trips only
- explicit version tags
- defensive validation on load
- corruption-safe failure behavior
- no raw host pointers, JS callback handles, or native references in serialized
  state

Snapshots should only be allowed at safe suspension points. If a suspended
execution depends on ongoing external work, the host must represent that work
through an explicit continuation token or equivalent resumable contract. `jslite`
must not attempt to serialize opaque host futures.

## Resource Model

Resource controls should be designed into the runtime from the beginning.

The public model should include at least:

- instruction budget
- heap byte limit
- allocation limit or allocation accounting
- call-depth limit
- maximum outstanding host calls
- cancellation support

Default limits should be explicit and documented.

Limit failures should be deterministic, guest-safe, and distinguishable from
ordinary guest exceptions.

## Public API Shape

The public Node API should stay small.

Illustrative shape:

```ts
type HostValue =
  | undefined
  | null
  | boolean
  | number
  | string
  | HostValue[]
  | { [k: string]: HostValue }

type Capability = (
  args: HostValue[],
  ctx: { name: string; signal?: AbortSignal }
) => HostValue | Promise<HostValue>

const program = await Jslite.compile(source, {
  inputs: ["x"],
})

const result = await program.run({
  inputs: { x: 1 },
  capabilities: {
    fetch_data: async ([url]) => "...",
  },
  limits: {
    instructions: 100_000,
    heapBytes: 8 << 20,
    callDepth: 256,
  },
})
```

Lower-level control should exist for advanced hosts:

- `compile(...)`
- `run(...)`
- `start(...)`
- `resume(...)`
- `dumpProgram(...)`
- `loadProgram(...)`
- `dumpSnapshot(...)`
- `loadSnapshot(...)`

The common path should be easy. The advanced path should remain explicit.

## Repository Shape

```text
jslite/
  crates/
    jslite/
    jslite-node/
    jslite-sidecar/
  docs/
    SECURITY_MODEL.md
    LANGUAGE.md
    HOST_API.md
    SERIALIZATION.md
    LIMITS.md
    ADRs/
  tests/
  examples/
```

The extra documentation matters. The risk in a project like this is not only code
complexity. It is semantic ambiguity.

## What Must Be True Before Production Use

Before `jslite` is described as production-ready for untrusted workloads, the
following should be true:

- the supported subset is written down and tested
- forbidden features fail closed
- resource limits are enforced predictably
- snapshots and compiled programs are validated before load
- guest diagnostics never leak host internals
- sidecar mode is available for stronger isolation
- kill and cancellation behavior is well defined
- the capability boundary is narrow, tested, and documented

## End State

The desired end state is a reusable Rust core plus thin Node-facing wrappers that
together provide:

- safe-by-default host embedding
- explicit and testable feature boundaries
- good diagnostics and tracebacks
- snapshot and resume across process boundaries
- predictable behavior under limits
- a practical path to production use for constrained guest code

The finished project should feel small, opinionated, and predictable. It should
optimize for sandboxed execution of constrained JavaScript, not for language
maximalism.
