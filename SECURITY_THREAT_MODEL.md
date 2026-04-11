# Security Threat Model

This document is the project-level threat model for `jslite`. It complements
the normative security contract in [docs/SECURITY_MODEL.md](docs/SECURITY_MODEL.md)
and the hostile-input verification notes in [docs/HARDENING.md](docs/HARDENING.md).

The goal here is to describe what `jslite` is trying to protect, what it does
not protect, where the trust boundaries are, and which concrete attacks the
current design is meant to resist.

## System Summary

`jslite` has four security-relevant components:

- `crates/jslite`: the Rust parser, validator, compiler, VM, heap, serializer,
  and runtime limits.
- `crates/jslite-node`: the Node-API addon that exposes compile/start/resume
  primitives to JavaScript.
- `index.js` plus `native-loader.js`: the thin JavaScript wrapper that validates
  host values, normalizes errors, manages cancellation tokens, and loads the
  native addon.
- `crates/jslite-sidecar`: the separate-process runner that exposes the same
  Rust core over newline-delimited JSON on stdio.

Security-sensitive data that crosses those components includes:

- guest source text
- compiled-program blobs
- suspended execution snapshots
- capability names and structured arguments
- structured host return values and sanitized host errors
- cancellation and kill signals

## Security Goals

`jslite` is trying to provide these properties:

1. Untrusted guest code does not get ambient host authority by default.
2. Unsupported syntax, values, and serialized state fail closed.
3. Guest-visible errors and tracebacks do not expose host paths, native
   handles, or Rust internals.
4. Resource usage is bounded by default and over-budget execution fails
   predictably.
5. Hosts can choose a stronger deployment boundary by moving execution into the
   sidecar process.

## Non-Goals

`jslite` does not claim the following:

- Addon mode is not a hard isolation boundary.
- Sidecar mode is not an OS sandbox by itself.
- Host capability implementations are not sandboxed by `jslite`.
- The project does not authenticate or encrypt sidecar transport traffic.
  Today the supported transport is local stdio, so confidentiality and peer
  authentication depend on the embedding host.
- The project does not defend against a compromised package supply chain,
  malicious Rust toolchain, or malicious native addon selected via privileged
  environment variables such as `JSLITE_NATIVE_LIBRARY_PATH` or
  `NAPI_RS_NATIVE_LIBRARY_PATH`.

## Assets And What Matters

The highest-value assets in this repository are:

- Host process integrity and availability.
- The explicit capability surface exposed by the embedding application.
- Host data intentionally passed into guest code.
- Serialized programs and snapshots that may be stored and resumed later.
- The sidecar process boundary when hosts use it for stronger containment.
- Guest-safe diagnostics, which must not leak deployment details.

## Trust Assumptions

The current design assumes:

- Guest source, compiled blobs, snapshots, and sidecar request lines may be
  malformed or hostile.
- The embedding host is trusted to choose deployment mode, provide capability
  implementations, and decide which inputs are safe to expose.
- Capability implementations may fail, but their failures must be sanitized
  before they become guest-visible.
- Strong process isolation, syscall restrictions, filesystem restrictions,
  network restrictions, and hard kill guarantees are host responsibilities.
- The environment used to install or load the addon is trusted. If an attacker
  controls native-loader override environment variables or the package
  resolution path, they can redirect the process to arbitrary native code.

## Deployment Modes

### Addon Mode

- The guest runs in the Node process through the native addon.
- This gives the lowest latency and the smallest integration surface.
- It does not protect the host from memory-safety bugs, native crashes, or
  same-process denial of service.

### Sidecar Mode

- The same Rust core runs in a separate process.
- This improves crash containment and gives the host an external kill
  primitive.
- It does not remove the need for OS-level sandboxing if the workload is
  adversarial.

### Hardened Sidecar Mode

- Sidecar mode plus host-managed OS controls such as cgroups, job objects,
  containers, seccomp, jail mechanisms, uid separation, filesystem restrictions,
  and network restrictions.
- This is the intended deployment shape for hostile guest workloads.

## Trust Boundaries

### 1. Guest Source Boundary

Untrusted source enters through the parser and validator in `crates/jslite`.

Main risks:

- parser crashes or panics
- unsupported syntax executing accidentally instead of being rejected
- guest access to implicit authority through language features or ambient names

Current controls:

- explicit parse -> validate -> IR -> bytecode pipeline
- supported-subset validation before execution
- documented forbidden forms and explicit non-goals in `README.md` and
  `docs/LANGUAGE.md`
- hostile-source tests and parser/lowering fuzz targets

Residual risk:

- the parser/lowering path is still recursive, so sidecar mode remains the
  recommended boundary for adversarial inputs

### 2. Structured Host Boundary

The host boundary is intentionally narrower than JavaScript itself.

Main risks:

- host objects or callbacks crossing into guest code
- guest values exposing runtime-internal handles back to the host
- prototype, accessor, cycle, or collection edge cases bypassing the value
  contract

Current controls:

- `StructuredValue` only permits `undefined`, `null`, booleans, strings,
  numbers, arrays, and plain objects
- the JavaScript wrapper rejects non-plain host objects before they enter the
  addon
- the Rust core rejects guest functions and guest `Map`/`Set` values at the
  boundary
- boundary behavior is covered by Node property tests and keyed-collection
  tests

Residual risk:

- capability handlers remain trusted code and can reintroduce authority or leak
  secrets if they return more than intended
- cyclic host inputs are outside the intended contract and currently fail in
  the JavaScript wrapper via recursion overflow rather than a dedicated
  structured-boundary diagnostic

### 3. Serialization Boundary

Compiled programs and snapshots are attacker-controlled input once they are
stored or transmitted.

Main risks:

- deserialization crashes or unsafe state restoration
- cross-version confusion
- corrupted snapshots resuming with invalid frame, heap, iterator, or promise
  state

Current controls:

- explicit format versioning
- validation on load before execution or resume
- rejection of cross-version loads
- snapshots only created at explicit suspension points
- native handles, host futures, and callback identities are excluded from the
  serialized form
- hostile-input tests mutate valid program and snapshot blobs
- dedicated fuzz targets cover snapshot loading and bytecode validation/execution

Residual risk:

- serialized blobs should still be treated as untrusted content and stored with
  ordinary host integrity controls

### 4. Native Addon Boundary

The Node process crosses from JavaScript into Rust through `crates/jslite-node`
and the dynamic loader in `native-loader.js`.

Main risks:

- in-process native memory-safety or logic bugs affecting the host
- arbitrary native library loading through privileged environment overrides
- host starvation because same-thread compute is cooperative rather than
  preemptive

Current controls:

- the Node layer stays thin and leaves guest semantics in Rust
- cancellation tokens exist for cooperative stop points
- typed error normalization avoids exposing raw native details to callers
- the package includes source-build and package-smoke verification

Residual risk:

- any successful exploit in the native addon runs in the host process
- if the host allows attacker control of the native loader override path, addon
  loading is equivalent to arbitrary native code execution

### 5. Sidecar Protocol Boundary

`crates/jslite-sidecar` accepts newline-delimited JSON requests on stdio.

Main risks:

- malformed or hostile protocol messages
- confused-deputy behavior if sidecar were to execute capabilities directly
- denial of service through stuck executions or protocol corruption

Current controls:

- structured `compile`, `start`, and `resume` request shapes only
- the sidecar never executes host capabilities itself; it only returns
  suspension metadata
- invalid lines fail closed
- hostile-protocol tests mutate requests and assert host-safe failures
- separate-process lifecycle allows host-enforced termination

Residual risk:

- protocol traffic has no built-in auth, replay protection, or confidentiality
- a hostile client that can write to the sidecar stdio stream can still drive
  sidecar work within the permissions of that process

### 6. Diagnostics Boundary

Errors cross from Rust to Node and sometimes back into guest execution.

Main risks:

- leaking host file paths, Rust module names, or internal state in diagnostics
- letting host cancellation appear as ordinary guest control flow

Current controls:

- host failures are sanitized into `name`, `message`, optional `code`, and
  optional structured `details`
- guest tracebacks use guest function names and source spans
- cancellation is treated as host authority and is not catchable by guest
  `try` / `catch`
- hostile-input and Node tests assert that messages stay guest-safe

Residual risk:

- capability implementations still control the sanitized message content they
  provide, so the embedding host must avoid placing secrets in host error
  messages or `details`

## Threat Scenarios

### Guest Escape To Host Authority

Scenario:

- guest code tries to use `eval`, `Function`, imports, unsupported ambient
  globals, or unsupported runtime features to escape the language contract

Mitigations:

- validation rejects forbidden forms
- the built-in surface is explicit and intentionally small
- capability access is opt-in and name-based

Failure condition considered security-relevant:

- guest code reaches host authority that was not intentionally exposed

### Boundary Smuggling

Scenario:

- the host passes functions, class instances, accessors, cycles, or native
  handles into guest execution
- guest code tries to return runtime-internal values such as guest functions or
  keyed collections through the structured boundary

Mitigations:

- wrapper-side value shape checks
- Rust-side structured conversion checks
- tests for unsupported boundary values and keyed-collection rejection

Failure condition considered security-relevant:

- unsupported values cross the boundary in a way that changes authority or
  exposes host internals

### Deserialization Bugs

Scenario:

- an attacker supplies a malformed compiled-program blob or snapshot to trigger
  a panic, invalid state restore, or execution of unvalidated bytecode

Mitigations:

- versioned formats
- load-time validation
- fuzzing and mutation-based hostile-input tests

Failure condition considered security-relevant:

- malformed serialized input bypasses validation or restores unsafe runtime
  state

### Resource-Exhaustion Attacks

Scenario:

- guest code loops forever, recurses deeply, allocates aggressively, or fans out
  across many host calls

Mitigations:

- default instruction, heap, allocation, call-depth, and outstanding-host-call
  limits
- cooperative cancellation checks at defined runtime checkpoints
- sidecar kill semantics for hard stops

Current default limits:

- instruction budget: `1_000_000`
- heap limit: `8 MiB`
- allocation budget: `250_000`
- call-depth limit: `256`
- max outstanding host calls: `128`

Residual risk:

- addon-mode compute on the Node main thread is still cooperative, so a hard
  preemptive stop requires sidecar mode

### Diagnostic And Data Leakage

Scenario:

- runtime, parser, or host failures leak local paths, Rust source details, or
  sensitive capability data into guest-visible errors

Mitigations:

- typed guest-safe errors
- guest-only tracebacks
- tests that reject host path fragments in error output

Failure condition considered security-relevant:

- guest-visible diagnostics expose deployment details or host-only state that
  are outside the documented contract

### Sidecar Abuse

Scenario:

- a caller sends malformed protocol input, drives repeated expensive work, or
  relies on protocol confusion to gain direct access to host capabilities

Mitigations:

- narrow protocol surface
- no direct capability execution inside the sidecar
- host-controlled process lifecycle

Residual risk:

- if the sidecar is exposed to an untrusted peer without an outer transport or
  process boundary, that peer can still consume resources or invoke any work
  the embedding host permits

## Operational Guidance

For low-latency trusted or semi-trusted workloads:

- addon mode is acceptable if the host is willing to accept same-process risk
- keep the capability surface minimal
- keep limits enabled

For untrusted workloads:

- prefer sidecar mode
- run the sidecar with OS-level CPU, memory, filesystem, and network controls
- run the sidecar under a dedicated low-privilege identity
- treat snapshots and compiled blobs as untrusted data
- normalize or reject cyclic host input objects before passing them to `jslite`
- keep capability implementations small and auditable
- avoid propagating sensitive host exception data into sanitized guest errors
- do not allow attacker control over native-loader override environment
  variables

## Evidence In This Repository

The current security story is backed by concrete tests and fuzz targets:

- [docs/HARDENING.md](docs/HARDENING.md)
- [crates/jslite/tests/security_hostile_inputs.rs](crates/jslite/tests/security_hostile_inputs.rs)
- [crates/jslite-sidecar/tests/hostile_protocol.rs](crates/jslite-sidecar/tests/hostile_protocol.rs)
- [crates/jslite-sidecar/tests/protocol.rs](crates/jslite-sidecar/tests/protocol.rs)
- [tests/node/property-boundary.test.js](tests/node/property-boundary.test.js)
- [tests/node/cancellation.test.js](tests/node/cancellation.test.js)
- [tests/node/keyed-collections.test.js](tests/node/keyed-collections.test.js)
- `fuzz/parser`
- `fuzz/ir_lowering`
- `fuzz/bytecode_validation`
- `fuzz/bytecode_execution`
- `fuzz/snapshot_load`
- `fuzz/sidecar_protocol`

## What Counts As A Security Issue

The following should be treated as security bugs for this project:

- guest access to forbidden ambient authority
- unsafe boundary crossings that expose host objects, native handles, or raw
  runtime identities
- deserialization bugs in compiled-program or snapshot loading
- failures of limits, cancellation, or sidecar termination guarantees relative
  to the documented contract
- guest-visible diagnostics that leak host-only internals

Report those issues using the process in [SECURITY.md](SECURITY.md).
