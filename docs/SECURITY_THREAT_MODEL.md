---
title: "Security Threat Model"
description: "Project-level threat model, security objectives, and mitigation strategies"
category: "Security"
order: 2
slug: "security-threat-model"
lastUpdated: "2026-04-13"
---

# Security Threat Model

This document is the project-level threat model for `mustard`. It complements:

- [SECURITY_MODEL.md](SECURITY_MODEL.md) for the short normative contract
- [HOST_API.md](HOST_API.md) for the host boundary
- [SERIALIZATION.md](SERIALIZATION.md) for compiled-program and snapshot rules
- [LIMITS.md](LIMITS.md) for runtime budgets and cancellation
- [SIDECAR_PROTOCOL.md](SIDECAR_PROTOCOL.md) for the current sidecar wire
  contract
- [HARDENING.md](HARDENING.md) for hostile-input and fuzzing evidence
- [../SECURITY.md](../SECURITY.md) for disclosure and reporting

The purpose of this file is not to restate every API detail. It is to make the
security posture explicit: what `mustard` is defending, what it is not
defending, which inputs are untrusted, where the trust boundaries are, and what
failure classes count as security issues.

## Executive Summary

- `mustard` is a language-level containment runtime, not a general-purpose OS
  sandbox.
- Addon mode is a low-latency embedding path, not a hard isolation boundary.
- Sidecar mode adds a process boundary and better killability, but it is still
  not an OS sandbox and it does not provide transport authentication,
  confidentiality, or replay protection by itself.
- Guest source, compiled-program blobs, snapshots, progress dumps, sidecar
  request lines, host-provided structured values, and sanitized host errors are
  all treated as potentially hostile input.
- Authority is supposed to enter guest execution only through explicit named
  host capabilities and a narrow structured-value contract.
- Restore and resume are fail-closed operations: loaded snapshots are inert
  until current host policy is applied, except for same-process `Progress.load`
  where the Node wrapper may reuse cached policy derived from the exact same
  snapshot bytes.
- Resource limits and cancellation are real runtime controls, but they are
  cooperative. Hard availability guarantees for adversarial workloads still
  require sidecar mode plus host-managed OS controls.

## Security Objectives

`mustard` is trying to preserve these properties:

1. Guest code does not receive ambient authority by default.
2. Unsupported syntax, unsupported runtime features, unsupported host values,
   malformed serialized state, and malformed sidecar protocol messages fail
   closed.
3. Capability authority is explicit, name-based, and host-provided.
4. Loaded snapshots cannot silently widen capability authority or runtime
   limits relative to the host policy applied at inspection or resume time.
5. Guest-visible diagnostics do not leak host file paths, Rust module details,
   raw native handles, or other host-only internals.
6. The structured host boundary does not execute host getters, mutate host
   prototypes, or smuggle opaque host identities into guest execution.
7. Runtime budgets and cancellation produce predictable guest-safe failures at
   documented checkpoints.
8. Hosts can move execution into a sidecar process for stronger crash
   containment and operational kill control.

## Explicit Non-Goals

`mustard` does not claim any of the following:

- Addon mode is not a hard boundary against memory-safety bugs, native crashes,
  or same-process denial of service.
- Sidecar mode is not a complete sandbox. It does not by itself provide
  syscall filtering, filesystem restrictions, network restrictions, uid
  separation, memory cgroups, CPU quotas, wall-clock deadlines, transport
  authentication, or encrypted IPC.
- Host capability implementations are not sandboxed by `mustard`.
- `AbortSignal`, `Progress.cancel()`, or sidecar process termination do not
  roll back host side effects that already happened.
- The project does not provide cryptographic replay protection for snapshots,
  progress dumps, or sidecar requests.
- The project does not defend against a compromised package supply chain,
  malicious optional prebuilt package, malicious Rust toolchain, or malicious
  native library selected through trusted-environment override paths.

## Adversaries And Trust Assumptions

### Adversary classes

The design explicitly considers these attacker shapes:

- a hostile guest author or model that can supply arbitrary JavaScript source
- an attacker who can supply compiled-program blobs or suspended snapshots
- an attacker who can write malformed or semantically hostile sidecar requests
- an attacker who can influence host input values, capability results, or
  sanitized host error payloads
- a careless or over-privileged host integrator who exposes too much authority,
  trusts the wrong metadata, or deploys addon mode where sidecar mode is
  required
- a local attacker who can influence package resolution, optional prebuilt
  resolution, or native-loader override environment variables

### Trust assumptions

The current design assumes:

- the embedding host and its capability code are trusted computing base
  components, even if they may be buggy
- the install/build/runtime environment used to resolve and load native code is
  trusted
- production isolation for hostile workloads comes from host-managed OS
  controls layered around sidecar mode
- hosts decide which capability names are exposed, which values are safe to
  hand to guest code, and which side effects are acceptable
- persisted snapshots and compiled blobs may be tampered with after creation
  and must be treated as untrusted on every load

## System Context

`mustard` has four security-relevant implementation components:

- `crates/mustard`: parser, validator, compiler, VM, heap, serializer, limits,
  snapshot inspection, and resume policy enforcement
- `crates/mustard-node`: the Node-API addon
- `index.js` plus `native-loader.js`: the JavaScript wrapper for capability
  registration, structured-value encoding/decoding, cancellation plumbing, and
  progress helpers
- `crates/mustard-sidecar`: the separate-process request/response runner over
  newline-delimited JSON on stdio

Security-sensitive data that crosses those components includes:

- guest source text
- compiled-program bytes
- suspension snapshot bytes
- capability names and structured arguments
- structured host return values
- sanitized host errors, including optional structured `details`
- cancellation signals
- progress dumps and sidecar request/response metadata

## Deployment Modes And What They Mean

### Addon mode

- Runs in the Node process through the native addon.
- Gives the lowest latency and simplest embedding shape.
- Does not isolate the host process from native memory corruption, logic bugs,
  stack exhaustion, or same-process starvation.
- Best treated as best-effort containment for trusted or semi-trusted guest
  workloads.

### Sidecar mode

- Runs the same Rust core in a separate process.
- Improves crash containment and gives the host an external kill primitive.
- Helps when the host needs to terminate runtime execution without killing the
  embedding process.
- Still does not prevent resource abuse at the OS level unless the host adds
  sandboxing, quotas, and supervision around the sidecar.

### Hardened sidecar mode

- Sidecar mode plus host-managed CPU, memory, filesystem, network, identity,
  and process controls.
- This is the intended deployment posture for hostile guest workloads.
- Examples include cgroups, job objects, seccomp, containers, jail
  mechanisms, low-privilege service users, restricted mounts, and explicit
  request deadlines.

## Assets And Security-Relevant Invariants

The most important assets and invariants are:

- host process integrity and availability
- the explicit capability surface exposed by the embedding host
- host data intentionally passed to guest code
- same-version compiled-program and snapshot state
- authoritative host policy for capability allowlists and runtime limits
- guest-safe diagnostics
- process-local single-use progress identity in the Node wrapper

The core invariants are:

- there is no ambient filesystem, network, environment, module, or subprocess
  authority in guest code
- unsupported features fail closed instead of falling back to accidental host
  behavior
- structured host values are plain data, not executable host objects
- restore policy comes from the current host, not from serialized snapshots

## Trust Boundaries

### 1. Source Text And Parse/Validate/Lower Boundary

Untrusted source enters through the parser, validator, and lowering pipeline in
`crates/mustard`.

Main threats:

- parser crashes, panics, stack exhaustion, or denial of service
- unsupported syntax executing accidentally instead of being rejected
- forbidden forms such as `import`, `export`, dynamic `import()`, `eval`, or
  `Function` gaining authority

Current controls:

- explicit parse -> validate -> lower -> bytecode pipeline
- documented supported subset and forbidden forms in [LANGUAGE.md](LANGUAGE.md)
- hostile-source tests and fuzz targets for parser and IR lowering

Residual risk:

- parse/lower behavior is still a hostile-input surface
- the pipeline is still recursive in places, so sidecar mode remains the safer
  boundary for adversarial source inputs

### 2. Structured Host-Value Boundary

The structured host boundary is intentionally narrower than JavaScript.

Allowed values:

- `undefined`
- `null`
- booleans
- strings
- numbers, including `NaN`, `Infinity`, and `-0`
- arrays of allowed values
- plain objects with string keys and allowed values

Rejected values and shapes:

- functions
- symbols
- bigint
- `Map` and `Set`
- cycles
- accessors
- custom prototypes and class instances
- array holes
- opaque host objects and native identities

Important boundary rules:

- `__proto__` is treated as plain data, not as a prototype mutator
- getters and setters must be rejected without executing them
- the same structured encoding rules apply to host inputs, capability results,
  progress arguments, sidecar payloads, and sanitized host `error.details`

Main threats:

- host objects or callbacks crossing into guest code
- guest values exposing runtime-internal identities back to the host
- prototype-pollution or accessor-triggering attacks at the boundary
- cycles or unexpected object shapes causing crashes instead of typed failures

Current controls:

- JavaScript wrapper checks plain-object and array constraints before crossing
  into the addon
- Rust-side conversion and serialization validation reject unsupported boundary
  values
- dedicated Node tests cover `__proto__`, accessors, array holes, unsupported
  host values, and progress round trips

Residual risk:

- capability handlers remain trusted code and can still leak secrets if they
  choose to return them
- exotic capability-registration objects are outside the structured-value
  contract; hosts should treat `options.capabilities` and `options.console` as
  trusted plain configuration, not as attacker-controlled proxy objects

### 3. Capability Registration, Suspension, And Resume Boundary

Capabilities are the only intended authority-bearing bridge from guest code to
host behavior.

Main threats:

- guest code invoking capability names that were not intentionally exposed
- the host dispatching on stale or forged progress metadata
- replaying a suspended execution in a way that duplicates side effects
- treating guest cancellation as transactional rollback of host work

Current controls:

- capability lookup is explicit and name-based
- host calls suspend guest execution rather than moving host callbacks into the
  runtime core
- `Progress.load(...)` derives authoritative `capability` and `args` from an
  authenticated suspended manifest on current dumps, falls back to snapshot
  inspection for legacy dumps, and never trusts caller-edited top-level dump
  metadata
- fresh-process `Progress.load(...)` requires explicit host `capabilities` and
  `limits` before restore metadata is trusted
- `Progress` single-use within one Node process is keyed to the snapshot hash,
  not to the caller-supplied `token`

Residual risk:

- the single-use guarantee is process-local and in-memory; it is not
  cryptographic replay protection and does not survive process restarts
- hosts that need one-shot approvals or externally visible idempotency must
  enforce that themselves
- `AbortSignal` and `Progress.cancel()` stop guest execution, but they do not
  force-stop already-running host callbacks or undo side effects that already
  happened

### 4. Compiled-Program And Snapshot Serialization Boundary

Compiled programs and snapshots are attacker-controlled input once they are
stored, transmitted, or reloaded.

Main threats:

- malformed bytecode or snapshots causing crashes, panics, or invalid restore
- snapshots widening capability authority or runtime limits
- cross-version confusion
- replay or tampering of serialized state

Current controls:

- explicit serialization versioning
- bytecode and snapshot validation on load
- loaded snapshots remain inert until current host policy is applied
- snapshot inspection and resume reassert allowed capability names and current
  authoritative runtime limits
- cross-version loads are rejected
- snapshots are created only at explicit suspension points
- native handles, unresolved host futures, and host callback identities are
  excluded from serialized state

Important nuance:

- same-process `Progress.load(...)` may reuse cached policy for the exact same
  snapshot bytes
- fresh-process restore still requires explicit host policy before inspection
  or resume
- validated snapshots may contain runtime-internal `Map`, `Set`, iterator, and
  promise state, but those values are still not part of the structured host
  boundary

Residual risk:

- serialized bytes should still be stored and transported with ordinary host
  integrity controls
- snapshot bytes are not authenticated, encrypted, or anti-replay protected by
  `mustard`

### 5. Runtime Limits And Cancellation Boundary

Resource controls are part of the runtime contract, but they are scoped to
guest execution semantics, not whole-process containment.

Main threats:

- unbounded compute, deep recursion, or excessive allocation
- async fan-out across too many host calls
- guest-controlled workloads bypassing metering or cancellation checkpoints

Current controls:

- instruction budget
- heap-byte budget
- allocation-count budget
- call-depth budget
- outstanding-host-call budget
- cooperative cancellation checks before each instruction, at resume entry, and
  at async runtime checkpoints
- sidecar process termination for stronger operational stop semantics

Residual risk:

- these controls do not provide an RSS cap, CPU quota, wall-clock timeout, or
  kernel-enforced preemption
- addon-mode compute on the Node main thread is still cooperative
- sidecar kill terminates the runtime process, but it does not roll back host
  side effects or stop already-started host work running in the embedding host

### 6. Native Addon, Package, And Build Boundary

The native loading path is part of the trust model.

Loader precedence today is:

1. `MUSTARD_NATIVE_LIBRARY_PATH` or `NAPI_RS_NATIVE_LIBRARY_PATH`
2. any local `.node` file under the installed package tree
3. the matching optional prebuilt package, when present

Main threats:

- attacker-controlled environment variables redirecting the process to arbitrary
  native code
- unexpected `.node` artifacts in the installed package tree
- malicious or substituted optional prebuilt packages
- compromised build environments during source-build installation

Current controls:

- the root package is intended to stay source-build-first
- release verification checks the published package shape and the prebuilt
  matrix
- package-smoke tests and release verification assert the expected install/load
  flow

Residual risk:

- native-loader override variables are full native-code execution overrides, not
  just a soft preference
- a stray `.node` file in the installed package tree is equivalent to shipping
  arbitrary native code
- source builds are not a provenance guarantee if the operator does not trust
  the npm environment, build toolchain, and local Rust toolchain

### 7. Sidecar Protocol Boundary

`crates/mustard-sidecar` exposes a narrow request/response protocol over local
stdio.

Main threats:

- malformed or hostile protocol messages
- replay or request confusion
- a hostile peer driving work within the permissions of the sidecar process
- operators assuming the process boundary implies transport security

Current controls:

- narrow `compile`, `start`, and `resume` request shapes
- guest-capability execution stays in the embedding host, not in the sidecar
- invalid lines fail closed
- hostile-protocol tests and dedicated fuzz targets cover malformed input

Residual risk:

- stdio traffic is not authenticated, encrypted, or replay protected
- sidecar request `id` values are correlation metadata only, not security
  tokens
- any peer that can write to the sidecar input stream can request work within
  that process's permissions
- killing the sidecar is not a rollback mechanism for host work already
  dispatched outside the sidecar

### 8. Diagnostics Boundary

Errors cross between guest execution, Rust, Node, and host capability code.

Main threats:

- leaking host paths, Rust source details, or deployment internals
- exposing raw host exception objects or native details to guest code
- capability handlers accidentally placing secrets into sanitized host errors

Current controls:

- host failures are normalized to `name`, `message`, optional `code`, and
  optional structured `details`
- guest tracebacks use guest function names and source spans
- hostile-input and Node tests assert host-safe message behavior

Residual risk:

- capability implementations still control the strings and structured details
  they choose to expose
- hosts must avoid putting secrets, credentials, internal URLs, or raw upstream
  objects into sanitized error payloads

## Representative Threat Scenarios

### Ambient-authority escape

Scenario:

- guest code attempts to use unsupported syntax, forbidden globals, or runtime
  gaps to reach host authority that was not intentionally exposed

Expected outcome:

- reject at parse/validation/lowering time, or fail as a guest-safe runtime
  error

### Boundary smuggling and prototype/accessor attacks

Scenario:

- the host passes functions, class instances, proxies, getters, cycles, array
  holes, or prototype-sensitive objects across the boundary
- guest code returns structured data intended to poison host object handling

Expected outcome:

- reject unsupported shapes without executing host getters or mutating host
  prototypes

### Snapshot forgery or policy widening

Scenario:

- an attacker tampers with a compiled program or snapshot so that restore
  changes capability names, queued host work, or runtime limits

Expected outcome:

- validation or snapshot-policy application rejects the restore before resume

### Replay or stale progress metadata

Scenario:

- a host stores `Progress.dump()` output and later trusts the stored
  `capability`, `args`, or `token` fields as authorization-bearing metadata

Expected outcome:

- hosts must treat the dump as an untrusted storage envelope and derive current
  authoritative metadata from `Progress.load(...)` or snapshot inspection under
  current policy

### Resource-exhaustion attack

Scenario:

- guest code consumes too many instructions, allocations, stack frames, or
  outstanding host-call slots, or tries to exploit an unmetered runtime path

Expected outcome:

- guest-safe limit failure at the documented checkpoint, or sidecar termination
  when the host chooses an external hard stop

### Diagnostic leakage

Scenario:

- parser failures, runtime errors, or host capability failures accidentally
  expose host filesystem paths, internal Rust details, or sensitive service
  data

Expected outcome:

- guest-safe diagnostic rendering without host-only internals

## Operational Guidance

### For trusted or semi-trusted workloads

- addon mode is acceptable when low latency matters more than hard isolation
- keep the capability surface minimal and auditable
- keep limits enabled
- treat capability handlers as part of the trusted computing base

### For hostile workloads

- prefer sidecar mode
- place the sidecar under OS-level CPU, memory, filesystem, network, and
  identity controls
- run the sidecar under a dedicated low-privilege identity
- treat all compiled-program blobs, snapshots, progress dumps, and sidecar
  messages as untrusted data
- reassert capabilities and limits on every fresh-process restore
- enforce host-level idempotency or single-use semantics when capability calls
  can trigger side effects

### For packaging and install security

- clear or tightly control `MUSTARD_NATIVE_LIBRARY_PATH` and
  `NAPI_RS_NATIVE_LIBRARY_PATH` in production
- treat optional prebuilt packages as a trust decision, not just a performance
  convenience
- treat the root npm package shape as security-sensitive: unexpected `.node`
  artifacts are security-relevant
- run `npm run verify:release` as part of release integrity, not only release
  hygiene

## Evidence In This Repository

The current security story is backed by tests, fuzz targets, and hardening
documentation:

- [docs/HARDENING.md](docs/HARDENING.md)
- [crates/mustard/tests/security_hostile_inputs.rs](crates/mustard/tests/security_hostile_inputs.rs)
- [crates/mustard/tests/snapshot_policy_security.rs](crates/mustard/tests/snapshot_policy_security.rs)
- [crates/mustard-sidecar/tests/hostile_protocol.rs](crates/mustard-sidecar/tests/hostile_protocol.rs)
- [tests/node/security-host-boundary.test.js](tests/node/security-host-boundary.test.js)
- [tests/node/security-progress-load.test.js](tests/node/security-progress-load.test.js)
- [tests/node/property-boundary.test.js](tests/node/property-boundary.test.js)
- [tests/node/cancellation.test.js](tests/node/cancellation.test.js)
- `fuzz/fuzz_targets/parser.rs`
- `fuzz/fuzz_targets/ir_lowering.rs`
- `fuzz/fuzz_targets/bytecode_validation.rs`
- `fuzz/fuzz_targets/bytecode_execution.rs`
- `fuzz/fuzz_targets/snapshot_load.rs`
- `fuzz/fuzz_targets/sidecar_protocol.rs`

Historical security reviews informed this threat model, but this document and
the normative security docs are the maintained source of truth.

## What Counts As A Security Issue

The following are security bugs for this project:

- guest access to forbidden ambient authority
- unsafe host-boundary crossings that execute accessors, mutate prototypes,
  expose host objects, or leak runtime-internal identities
- deserialization bugs in compiled-program or snapshot loading
- snapshot restores that widen capability authority or runtime limits relative
  to the applied host policy
- failures of documented limit, cancellation, or process-termination behavior
  that materially weaken the published contract
- guest-visible diagnostics that leak host-only internals
- security-relevant package or native-loader behavior that causes the project to
  load unexpected native code contrary to the documented package contract

Report those issues using [SECURITY.md](SECURITY.md).
