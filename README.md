# MustardScript

`MustardScript` is a small, opinionated JavaScript runtime for executing a deliberately
limited subset of JavaScript inside a Node.js service with explicit host
capabilities, bounded resources, and resumable execution.

This project is **not** trying to recreate Node.js, V8, npm compatibility, or a
browser. It is trying to provide a compact execution engine for sandboxed
agent-style scripts and other constrained guest code.

## Status

`MustardScript` is an early-stage design and implementation project.

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

- parse -> validate -> IR -> bytecode -> VM execution for the supported subset
- `let`/`const`, functions and closures, rest parameters, arrays, plain
  objects, loops, and basic control flow
- array spread and spread arguments over arrays, strings, `Map`, `Set`, and
  supported iterator objects
- `for...of` plus async `for await...of` over arrays, strings, `Map`, `Set`,
  and supported iterator objects, with the documented loop-header surface,
  destructuring bindings, and snapshot-safe iterator state
- `Map` and `Set` with supported iterable constructors, SameValueZero key and
  membership semantics, insertion-order-preserving storage, and iterator
  helpers
- `async` functions, `await`, guest promises, `new Promise(...)`, Promise
  combinators and instance methods, basic thenable adoption, and internal
  microtask scheduling for the supported subset
- guest-internal `BigInt` literals with exact-integer arithmetic,
  comparison, keyed-collection membership, and string/property-key coercion
- conservative array, string, object, and Math helper methods, including
  callback-driven array helpers, iterable normalization helpers, and
  string-pattern search/replacement helpers
- a conservative `Date` subset with UTC formatting/access helpers plus
  `Date.now()` and `new Date(value).getTime()` for realistic SLA and
  freshness checks
- narrow `Intl.DateTimeFormat` and `Intl.NumberFormat` support for explicit
  `en-US` / `UTC` formatting without widening the ambient runtime surface
- `throw`, `try`/`catch`/`finally`, and guest-visible `Error` objects
- `Math` and `JSON` built-ins
- explicit named host capabilities with `start()` / `resume()` suspension,
  including async guest fan-out across host capability calls
- deterministic `console.log` / `console.warn` / `console.error` callbacks when
  the host provides them explicitly
- instruction, call-depth, heap-byte, allocation-count, and
  outstanding-host-call budgets with guest-safe limit errors
- cooperative cancellation for running compute, suspended progress objects, and
  guest async waits on host promises
- guest-safe runtime and limit errors with guest function-span tracebacks
- same-version compiled-program and suspension snapshot round trips
- a thin Node addon wrapper and a sidecar process that reuse the same Rust core

## Reference Docs

- [Security Threat Model](docs/SECURITY_THREAT_MODEL.md)
- [Security Model](docs/SECURITY_MODEL.md)
- [Use Case Gaps](docs/USE_CASE_GAPS.md)
- [Language Contract](docs/LANGUAGE.md)
- [Host API](docs/HOST_API.md)
- [Serialization](docs/SERIALIZATION.md)
- [Limits](docs/LIMITS.md)
- [Conformance Strategy](docs/CONFORMANCE.md)
- [Bytecode VM Model](docs/BYTECODE.md)
- [Runtime Value Model](docs/RUNTIME_MODEL.md)
- [Sidecar Protocol](docs/SIDECAR_PROTOCOL.md)
- [Benchmarking Notes and Comparison Plan](benchmarks/README.md)
- [Release Guide](docs/RELEASE.md)
- [Architecture ADRs](docs/ADRs/0001-core-architecture.md)

## Installation

The release package name is `mustardscript`. The default and fully verified
path is still source-build installation from a clean checkout or packed source
tarball, where `npm install` compiles the native addon locally. Optional
prebuilt binaries now have a separate release flow for the documented target
matrix, but the loader now only accepts validated `.node` artifacts from the
expected optional package layout. Source-build fallback remains the baseline
path, now ships `Cargo.lock`, and builds the addon in release mode. It still
requires a Rust toolchain plus Node.js on the target machine.

From a clean checkout:

```sh
npm install
npm test
```

That flow builds the Rust addon locally and then runs the Node and packaging
smoke tests. Prebuilt binaries are intentionally deferred until the package
shape is stable.

Release verification and publish guidance live in
[docs/RELEASE.md](docs/RELEASE.md).
Maintainers can run `npm run verify:release` to execute the current release
verification flow end to end.

## Maintainer Helpers

Maintainers can run `npm run ralph-loop -- <plan.md>` to repeatedly invoke
`codex exec` with `gpt-5.4` and `model_reasoning_effort="xhigh"` until the plan
marks itself with `[PLAN HAS BEEN COMPLETED]` or `[BLOCKED]`. Use
`--max-iterations <n>` to cap the loop when needed.

## Website Playground

The repository now includes an experimental website playground in
[`website/`](website) that compares:

- `MustardScript` guest code running in the browser via a dedicated
  `wasm32-unknown-unknown` build of the Rust core
- vanilla JavaScript running client-side inside a sandboxed iframe

The website build copies a raw `.wasm` artifact into
`website/public/mustard-playground.wasm` through
`website/scripts/build-playground-wasm.mjs`. The iframe path is a demo-only
comparison surface, not a hardened sandbox. It intentionally has no ambient
host authority beyond the fixed scenario helper set, but a synchronous infinite
loop in browser JavaScript can still block the page event loop before the
cooperative timeout/reset logic runs.

The current release-mode `.wasm` artifact is also large, roughly 71 MB before
HTTP compression, so the browser playground should be treated as an
experimental demo target until bundle-size work lands.

## Agent-Style Example

See [examples/agent-style.ts](examples/agent-style.ts) for a minimal host loop
that starts guest execution, persists a suspended `Progress`, reloads it, and
resumes with a host result.

For more realistic guest programs shaped like programmatic tool-calling
workloads, see [examples/programmatic-tool-calls](examples/programmatic-tool-calls).

## Primary Use Cases

`MustardScript` is primarily aimed at agent runtimes that need to execute small,
bounded guest programs with explicit host tools instead of exposing a large
ambient runtime.

- Server-side "code mode" workloads where an agent writes code against a typed
  SDK or a compact `search()` / `execute()` tool surface instead of loading a
  large API into model context
- Programmatic tool-calling workloads where an agent fans out across many host
  tools, reduces large intermediate results in code, and only returns the final
  answer to the model
- Resumable host-mediated workflows where execution must pause at explicit host
  capability boundaries, persist state, and resume later

These are the workload shapes described by
[Cloudflare's Code Mode](https://blog.cloudflare.com/code-mode-mcp/) and
[Anthropic's Programmatic Tool Calling](https://www.anthropic.com/engineering/advanced-tool-use).
They are a better fit for `MustardScript` than trying to match general-purpose
JavaScript runtime behavior.

## Project Goals

`MustardScript` should provide:

- A small, auditable runtime surface
- No ambient filesystem, network, environment, module, or subprocess access
- Explicit host capabilities instead of implicit globals
- Fast startup and low embedding overhead
- Good cold-start, memory, and host-call overhead for code-mode and
  programmatic tool-calling workloads
- Precise accounting for instructions, memory, allocations, and call depth
- Deterministic or tightly specified behavior for the supported subset
- Suspension and resume at explicit host boundaries
- Same-version serialization of compiled programs and execution snapshots
- A Node.js-first embedding experience with a thin wrapper over a reusable Rust
  core

## Non-Goals

`MustardScript` is not intended to be:

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

`MustardScript` should document three deployment modes clearly.

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

`MustardScript` itself is responsible for language-level containment. Production
security for untrusted inputs should assume sidecar mode plus host-managed OS
controls.

## Core Terminology

- **Guest code**: JavaScript executed by `MustardScript`
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
- It allows `mustard` to separate parsing from runtime design
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
- Arrays, plain objects, `Map`, and `Set`
- `let` and `const`
- Functions and closures
- `async` functions and `await`
- Arrow functions
- `if`, `switch`, loops, `break`, and `continue`
- `throw`, `try`, `catch`, and `finally`
- Common-case destructuring
- Template literals
- Optional chaining and nullish coalescing
- Internal guest promises and microtask checkpoints for the supported subset
- Host capability calls
- Suspension and resume at host boundaries
- Snapshotting at safe suspension points

### Still Deferred Within The Async Surface

- fully general Promise constructor and thenable-adoption edge cases,
  including hostile thenable cycles
- synchronous host suspensions from Promise executors or adopted thenables

### Explicitly Out of Scope for v1

- ES modules
- CommonJS
- `eval`
- `Function` constructor
- `with`
- Classes
- Generators and custom iterator authoring
- symbol-based custom iterable protocols outside the documented built-ins
- Symbols
- `WeakMap`, `WeakSet`
- Typed arrays, `ArrayBuffer`, shared memory, and atomics
- Full `Date` parity beyond the documented conservative subset
- Full `Intl` parity beyond the documented conservative subset
- `Proxy`
- Full `RegExp` parity
- Full property descriptor semantics
- Accessors
- Full prototype semantics
- Implicit host globals such as `process`, `module`, `exports`, `global`,
  `require`, timers, or fetch-like APIs

### Important Clarification About Names Like `require`

`mustard` should not reject arbitrary identifiers named `require` or `process`.
Those names can be legitimate local bindings in JavaScript.

What `mustard` should reject is:

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
- `Map`
- `Set`
- `Promise`
- `RegExp`
- `Date`
- `String`
- `Error`
- `TypeError`
- `ReferenceError`
- `RangeError`
- `Number`
- `Boolean`
- `Intl`
- `Math`
- `JSON`
- A placeholder `console` global object

Current Promise support is intentionally narrow:

- async functions return internal guest promises
- `new Promise(executor)` is supported when `executor` is callable and
  completes synchronously from the runtime's perspective
- `Promise.resolve(...)`, `Promise.reject(...)`, `Promise.all(...)`,
  `Promise.race(...)`, `Promise.any(...)`, and `Promise.allSettled(...)` are
  supported
- promise instance methods `then(...)`, `catch(...)`, and `finally(...)` are
  supported
- promise resolution and `await` adopt guest promises plus guest object or
  array thenables whose `.then` property is callable
- Promise executor and thenable resolve/reject functions keep first-settlement
  semantics; later resolve/reject calls and post-settlement throws do not
  override the settled result
- `Promise.any(...)` rejects with a guest-visible `AggregateError` carrying an
  `errors` array when every input rejects
- async Promise executors and async adopted `.then` handlers reject with an
  explicit `TypeError`
- synchronous host suspensions from Promise executors and adopted thenables
  still fail closed

Current BigInt support is intentionally conservative:

- guest code supports `123n` literals plus exact-integer `+`, `-`, `*`, `/`,
  `%`, truthiness, `typeof`, string coercion, and property-key coercion
- `Map` and `Set` treat `BigInt` values as stable guest keys using the same
  equality surface as other guest values
- mixed `BigInt` / `Number` arithmetic and relational comparisons fail closed
- `Number(1n)` and unary `+1n` fail closed instead of implicitly coercing
- `JSON.stringify(...)` rejects `BigInt` values with an explicit `TypeError`
- `BigInt` values remain guest-internal and still cannot cross the structured
  host boundary

Current built-in helper support is intentionally conservative:

- arrays support `push`, `pop`, `slice`, `join`, `includes`, `indexOf`,
  `values`, `keys`, `entries`, `forEach`, `map`, `filter`, `find`,
  `findIndex`, `findLast`, `findLastIndex`, `some`, `every`, `reduce`, and
  `reduceRight`
- strings support `trim`, `trimStart`, `trimEnd`, `includes`, `startsWith`,
  `endsWith`, `slice`, `substring`, `toLowerCase`, `toUpperCase`,
  `padStart`, `padEnd`, `split`, `replace`, `replaceAll`, `search`, and
  `match`
- `Array(...)` and `new Array(...)` follow JavaScript's single-length
  constructor behavior for one numeric argument and reject invalid lengths
  with `RangeError`
- `Object(value)` preserves supported object-like guest values and boxes
  primitive strings, numbers, and booleans into conservative wrapper objects
- `Object.keys`, `Object.values`, `Object.entries`, and `Object.hasOwn`
  support plain objects, arrays, supported callables, and conservative boxed
  strings
- `Math.pow`, `Math.sqrt`, `Math.trunc`, and `Math.sign` are supported
- `Date.now()`, `new Date(value).getTime()`, `Date.prototype.toISOString()`,
  `Date.prototype.toJSON()`, and the documented UTC field accessors are
  supported
- `Number.parseInt`, `Number.parseFloat`, `Number.isNaN`, and
  `Number.isFinite` are supported
- `Intl.DateTimeFormat` and `Intl.NumberFormat` are available in a narrow
  `en-US` / `UTC` subset with explicit fail-closed behavior for unsupported
  locales and options
- array callback helpers currently support guest callbacks, built-in callbacks,
  and async host callbacks reached from an async guest boundary; synchronous
  host suspensions from those helpers fail closed
- string pattern helpers accept string-coercible patterns and real `RegExp`
  instances, including callback replacements for `replace` and `replaceAll`
- string replacement callbacks are synchronous-only; host suspensions fail
  closed, and only `g`, `i`, `m`, `s`, `u`, and `y` flags are supported
- full `RegExp` parity and symbol-based match/replace protocol hooks remain
  unsupported
- descriptor/prototype helpers remain unsupported
- proxy-backed host values, accessor-backed handler registries, and cyclic host
  values fail closed at the JavaScript wrapper boundary before guest execution

Current function-call support is intentionally narrow:

- non-arrow guest member calls bind the computed receiver as `this`
- arrow functions capture lexical `this` from the surrounding supported guest
  frame
- rest parameters are supported for functions and arrow functions
- default parameters and conservative default destructuring are supported
- implicit free `arguments` is rejected with a validation diagnostic
- `new` remains limited to the documented conservative built-in constructors

Current legacy-binding and prototype-related exclusions are deliberate:

- `var` is intentionally not part of the v1 contract. The runtime keeps only
  lexical `let` / `const` bindings and does not emulate function/global
  hoisting or legacy redeclaration rules.
- the `delete` operator is intentionally unavailable for plain objects and
  arrays. Supporting it would require explicit rules for own-property absence,
  sparse arrays, and descriptor/configurability semantics; until then guest
  code must rebuild values instead. This does not affect the supported
  `Map.prototype.delete` and `Set.prototype.delete` methods.
- full prototype inheritance remains unavailable, but conservative
  `instanceof` checks work for the documented built-in constructors,
  primitive-wrapper objects, and `Object` checks over supported callables.

Current keyed-collection support is intentionally narrow:

- `new Map()` and `new Set()` accept the supported iterable surface
- `Map` supports `get`, `set`, `has`, `delete`, `clear`, and `size`
- `Set` supports `add`, `has`, `delete`, `clear`, and `size`
- `Map` keys and `Set` membership use SameValueZero semantics, so `NaN`
  matches `NaN` and `-0` is treated the same as `0`
- `Map` and `Set` preserve first-in insertion order internally
- public collection iterator helpers `entries()`, `keys()`, and `values()`
  return guest iterator objects with `.next()`
- `for...of` supports arrays, strings, `Map`, `Set`, and iterator objects
  produced by the supported helper surface

The guest runtime intentionally remains narrow at the host boundary: it does
not expose filesystem, network, timers, or environment access by default. The
documented built-ins `Date.now()` and `Math.random()` are the current
exceptions, both intentionally nondeterministic and not reproducible across
runs or resumes. If the host wants broader capabilities, it must provide them
explicitly.

## Structured Host Boundary

The host boundary should be narrowly defined and documented as its own contract.

### Allowed Structured Host Values

- `undefined`
- `null`
- booleans
- strings
- numbers, including non-finite values and `-0`
- arrays of structured host values, including sparse arrays with preserved hole
  positions up to 1,000,000 elements
- plain objects with string keys and structured host values

### Rejected at the Host Boundary

- functions
- symbols
- guest `BigInt` values and host bigints; the current `BigInt` surface remains
  guest-internal only
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
- Private to `mustard`, not a public stable standard

Compiled programs only need to round-trip within the same `mustard` version.

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

`mustard` should define an explicit internal `JsValue` type and a heap object
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

`mustard` owns its own async model.

Current behavior:

- internal promise representation for async guest execution
- internal microtask queue with explicit checkpoint draining
- host-boundary suspension and resume for async guest capability calls
- enforced maximum outstanding host calls for async guest fan-out
- no reentrancy into the same VM unless explicitly designed and documented
- cooperative cancellation now fails top-level execution, including while guest
  async code is awaiting host promises
- same-thread addon `AbortSignal` delivery remains cooperative rather than
  preemptive

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
through an explicit continuation token or equivalent resumable contract. `mustard`
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
import { ExecutionContext, Mustard } from 'mustardscript'

type HostValue =
  | undefined
  | null
  | boolean
  | number
  | string
  | HostValue[]
  | { [k: string]: HostValue }

type Capability = (...args: HostValue[]) => HostValue | Promise<HostValue>

const program = new Mustard(source, {
  inputs: ['x'],
})

const context = new ExecutionContext({
  capabilities: {
    fetch_data: async (url) => '...',
  },
  limits: {
    instructionBudget: 100_000,
    heapLimitBytes: 8 << 20,
    callDepthLimit: 256,
  },
  snapshotKey: 'host-chosen-snapshot-key',
})

const result = await program.run({
  context,
  inputs: { x: 1 },
})
```

`ExecutionContext` is optional, but it is the intended steady-state path when a
host will reuse the same capabilities, limits, and snapshot key across many
`run()` / `start()` / `Progress.load()` calls.

Lower-level control should exist for advanced hosts:

- `new Mustard(...)`
- `new ExecutionContext(...)`
- `Mustard.validateProgram(...)`
- `run(...)`
- `start(...)`
- `progress.resume(...)`
- `progress.resumeError(...)`
- `dump()`
- `Mustard.load(...)`
- `progress.dump()`
- `Progress.load(...)`

For hosts managing a large backlog of resumable jobs, the Node wrapper also
exports `MustardExecutor` plus `InMemoryMustardExecutorStore` as a thin
queue-oriented layer over `start()` / `Progress.dump()` / `Progress.load()`.
The design and invariants for that layer are documented in
[docs/MUSTARD_EXECUTOR.md](docs/MUSTARD_EXECUTOR.md).

Native failures are surfaced in Node as typed JavaScript errors:
`MustardParseError`, `MustardValidationError`, `MustardRuntimeError`,
`MustardLimitError`, and `MustardSerializationError`.

`Mustard.validateProgram(source)` checks that a guest program parses, stays
inside the supported language subset, and lowers to an executable compiled
program. It does not prove that a later `run()` or `start()` call will succeed
with a particular host policy, input set, capability map, or runtime limit.

`Progress.load(...)` always requires explicit restore authority: the host must
pass either a reusable `ExecutionContext` or explicit `capabilities` /
`console`, explicit `limits` as an object (use `{}` for default runtime
limits), and the original `snapshotKey`. The dumped token authenticates the
snapshot bytes before any loaded capability metadata is trusted, current dumps
also carry authenticated suspended metadata so `Progress.load(...)` usually
avoids native re-inspection, legacy dumps still fall back to inspection, and
same-process dumps stay single-use.

The common path should be easy. The advanced path should remain explicit.

## Repository Shape

```text
mustard/
  crates/
    mustard/
    mustard-node/
    mustard-sidecar/
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

Before `mustard` is described as production-ready for untrusted workloads, the
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
