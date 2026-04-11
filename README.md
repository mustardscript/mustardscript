# jslite

`jslite` is a sandboxed JavaScript interpreter for running untrusted,
LLM-generated, or user-provided JavaScript inside a Node.js service without
granting ambient access to the host.

The project is intentionally closer to Monty than to Node's built-in execution
features. The goal is not to run arbitrary npm code or recreate Node.js inside a
sandbox. The goal is to execute a deliberately limited subset of JavaScript with
explicit host capabilities, strong resource controls, and resumable execution.

## Project Goals

`jslite` should provide:

- A small, auditable runtime surface
- No ambient filesystem, network, environment, or subprocess access
- Fast startup and low embedding overhead
- Explicit host capabilities instead of implicit globals
- Resource accounting for time, memory, allocations, and recursion depth
- Iterative execution with suspend and resume at host boundaries
- Serialization of compiled programs and execution snapshots
- A Node.js-first embedding experience

## What jslite Is Not

`jslite` is not intended to be:

- A secure wrapper around `node:vm`
- A general-purpose JavaScript runtime
- A compatibility layer for npm packages
- A CommonJS environment
- A DOM or browser runtime
- A JIT
- A drop-in replacement for Node.js or V8

## Primary Technical Decisions

### Rust Core

The interpreter core should be written in Rust.

Reasons:

- Strong memory-safety baseline
- Good parser and tooling ecosystem
- Practical path to a Node addon via Node-API
- Good fit for a custom VM, serialization, and host capability layer

### Native Addon First

The primary embedding should be a Node-API native addon, likely via `napi-rs`.

Reasons:

- The only target embedder is a Node.js service
- Native addons keep host interop simple and efficient
- Node-API offers a more stable interface than raw V8 bindings

### Optional Sidecar Isolation

`jslite` should support two execution modes:

1. In-process addon mode for low latency
2. Sidecar-process mode for stronger fault isolation

The runtime core should remain the same in both modes.

### Custom Interpreter

`jslite` should use a custom execution pipeline:

`source -> parser AST -> lowered IR -> jslite bytecode -> VM`

Reasons:

- Explicit control over supported semantics
- Precise resource accounting
- Clean suspension and resume boundaries
- Easier serialization than a general-purpose engine wrapper

### Oxc Frontend

Use `oxc` as the parser frontend unless evaluation proves it to be a poor fit.

### Tracing GC

Use a non-moving mark-sweep collector in v1 instead of refcounting.

Reasons:

- JavaScript object graphs are naturally cyclic
- Closures, prototypes, promises, and exception objects create cycles routinely
- Tracing GC is a more natural baseline for JS semantics

## Security Model

The core security rule is simple:

Guest code gets no ambient authority.

That means guest code must not receive direct access to:

- `process`
- `require`
- Node built-ins
- Filesystem
- Network
- Environment variables
- Subprocesses
- Native addons
- Shared memory primitives

Anything outside pure language semantics must be exposed through explicit host
capabilities.

## Architecture

### Frontend

The frontend parses JavaScript source and lowers it into an internal IR that is
stable, explicit, and decoupled from parser internals.

Responsibilities:

- Parse source text
- Reject unsupported syntax with good diagnostics
- Preserve spans for tracebacks and error reporting
- Normalize syntax sugar before bytecode generation

### IR

The IR should make control flow and semantics explicit.

Design goals:

- Explicit scopes and bindings
- Explicit function and closure boundaries
- Explicit exception regions
- Explicit async suspension points

### Bytecode

The runtime should compile IR into compact bytecode that is:

- Easy to interpret
- Easy to serialize
- Easy to cost-account
- Stable enough for snapshots and cached compilation

### VM

The VM should initially be stack-based unless profiling shows a strong need to
change direction.

Responsibilities:

- Execute bytecode
- Maintain call frames and lexical environments
- Handle exceptions
- Enforce resource limits
- Suspend and resume around host interactions

### Value and Object Model

The runtime should define an explicit internal `JsValue` type and a heap-allocated
object system with shape metadata, prototype pointers, and interned property
names where practical.

This should be designed deliberately from the start. In JavaScript, object and
property semantics are central enough that ad hoc maps scattered across the VM
will become both a correctness problem and a performance problem.

### Async Model

`jslite` should own its own async model.

That means:

- Internal promise state
- Internal microtask queue
- Explicit suspension at host boundaries
- Stable resume objects for host-driven continuation

The host should not need to understand VM internals to resume guest execution.

### Capability Interface

The host interface should be narrow and explicit:

- Named host functions exposed by the embedder
- Structured argument and result conversion
- Sync and async host call support
- No implicit fallback lookup path

## v1 Feature Boundary

The first useful version should support a strict, intentionally limited subset
of JavaScript:

- Strict-mode execution only
- Numbers, booleans, strings, `null`, `undefined`
- Arrays and plain objects
- `let` and `const`
- Functions and closures
- Arrow functions
- `if`, `switch`, loops, `break`, `continue`
- `try`, `catch`, `finally`, `throw`
- Destructuring for common cases
- Template literals
- Optional chaining and nullish coalescing if lowering cost is acceptable
- `async` functions and `await`
- Host function calls through an explicit capability table
- Deterministic print or console-style output via host callback
- Snapshotting at host-call suspension points

Likely deferred:

- Classes
- Generators
- `for...of`
- Symbols
- Full property descriptor semantics
- Accessors
- `Map`, `Set`, typed arrays
- `Date`
- Full `RegExp` parity

## End State

The desired end state is a reusable Rust core and a thin Node-facing wrapper
that together provide:

- Fast parse and execution for short scripts
- Safe-by-default host embedding
- Explicit and testable feature boundaries
- Good diagnostics and tracebacks
- Snapshot and resume across process boundaries
- A realistic path to production use for agent code execution

The finished project should feel small, opinionated, and predictable. It should
optimize for sandboxed execution of constrained JavaScript, not for language
maximalism.

## Likely Public API Shape

The initial Node-facing API should stay small:

```ts
const j = new Jslite(code, { inputs: ['x'] })

const result = j.run({
  inputs: { x: 1 },
  capabilities: {
    fetch_data: async (url: string) => '...',
  },
})
```

Expected concepts:

- `new Jslite(code, options?)`
- `run(options?)`
- `start(options?)`
- `resume(...)` on progress objects
- `dump()` and `load(...)`

## Initial Repository Shape

```text
jslite/
  crates/
    jslite/
    jslite-node/
  tests/
  examples/
```

The repository should mirror Monty's separation between a reusable Rust core and
a thin embedding layer.
