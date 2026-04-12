# Host API

This document describes the current host boundary and the near-term rules the
runtime is expected to preserve as it grows.

## Structured Host Values

Allowed values:

- `undefined`
- `null`
- booleans
- strings
- numbers, including `NaN`, `Infinity`, and `-0`
- arrays of allowed values, including sparse arrays with preserved hole
  positions
- plain objects with string keys and allowed values

Rejected values:

- functions
- symbols
- guest `BigInt` values and host bigints
- `Map` and `Set`
- proxies and other trap-bearing wrapper objects
- cycles
- guest-exported arrays or plain objects with shared references
- class instances
- accessors
- custom prototypes
- host objects

Guest `Map` and `Set` values are runtime-internal heap objects. They may appear
inside guest execution, snapshots, and guest-visible data structures, but they
do not cross the structured host boundary in either addon mode or sidecar mode.
Guest `BigInt` values now follow the same rule: they are available inside guest
execution and snapshots, but results, inputs, capability arguments, and resume
payloads still reject them at the structured boundary.
Structured host-boundary traversal is depth-limited and fails closed with a
typed error instead of recursing until the JS or Rust stack overflows.
Structured values are trees, not identity graphs. Guest results and suspended
capability arguments therefore fail closed on shared guest arrays or plain
objects instead of alias-expanding them during export.

## Capability Calls

- Capabilities are named host functions.
- Capability lookup is explicit.
- The core runtime represents capability calls as suspension points.
- In synchronous guest code, a capability call suspends immediately.
- In async guest code, a capability call produces an internal guest promise,
  queues the host request, and suspends at the next runtime checkpoint.
- `start()` returns a suspension object containing the capability name, the
  converted arguments, and a resumable snapshot.
- Snapshots loaded from bytes must be rebound to explicit host policy before
  their capability metadata is trusted or resume is allowed to continue.
- Raw native `inspectSnapshot(...)` / `resumeProgram(...)` flows also require a
  `limits` field plus `snapshot_id`, `snapshot_key_base64`,
  `snapshot_key_digest`, and matching `snapshot_token` inside the snapshot
  policy JSON. The token is the HMAC-SHA256 of the detached `snapshot_id`
  under the caller-chosen snapshot key, and restore recomputes `snapshot_id`
  from the raw snapshot bytes before inspection or resume. Those fields bind
  raw restore to trusted detached dump metadata, but hosts still need ordinary
  integrity controls when storing or transporting snapshots. Passing `{}` is
  the explicit way to request default runtime limits during raw restore.
- `resume()` accepts either a structured success value or a sanitized host
  error payload. Raw native and sidecar resume transport also accepts an
  explicit `cancelled` payload shape for host-driven cancellation.
- `Progress.cancel()` injects an explicit cooperative cancellation failure into
  a suspended execution instead of resuming it with a host value.
- The Node wrapper accepts sync JavaScript capability functions and real
  `Promise`-returning async handlers; it does not adopt arbitrary thenables or
  proxy-backed handler registries.
- `run()` and `start()` accept an optional `AbortSignal`, and
  `resume()` / `resumeError()` accept an optional `{ signal }` object for the
  resumed compute segment.
- `limits.maxOutstandingHostCalls` bounds the combined number of queued and
  currently suspended host requests for async guest execution.
- If hosts want dumped progress blobs to survive a fresh process boundary, they
  must provide the same `snapshotKey` on `start()`/`run()` and on
  `Progress.load(...)`.

## Error Sanitization

Host failures cross the boundary with:

- `name`
- `message`
- optional `code`
- optional `details`

Resumed host failures re-enter guest execution as guest-visible error objects
using those fields. Guest `try` / `catch` can inspect `name`, `message`,
optional `code`, and optional `details`, and uncaught failures render with the
same guest-safe summary.
The Node wrapper only trusts own data properties for those fields. Inherited
getters, proxy traps, coercion hooks, and accessor-backed `details` fail closed
instead of executing during host-error sanitization. If `name` or `message` are
missing, the sanitized fallback is `Error` plus an empty message. `code` is
accepted only as an own string data property; missing or non-string `code`
values are dropped instead of being coerced.
The Node wrapper rethrows core failures as typed JavaScript errors:
`JsliteParseError`, `JsliteValidationError`, `JsliteRuntimeError`,
`JsliteLimitError`, and `JsliteSerializationError`. The original native error is
preserved as the JavaScript `cause`.
When those failures resume into guest execution, the runtime also renders a
guest-only traceback with guest function names and source spans.

## Console Contract

- A `console` global object exists so the global name is reserved.
- `console.log`, `console.warn`, and `console.error` are exposed only when the
  host provides the matching callback explicitly.
- Console callbacks receive the same structured guest values as ordinary host
  capabilities.
- Guest-visible console calls always evaluate to `undefined`, regardless of what
  the host callback returns.
- If the host does not provide a callback, the corresponding console method is
  absent and guest calls fail as ordinary guest runtime errors.

## Reentrancy

- Execution is single-threaded and non-reentrant.
- The Node `Progress` wrapper is single-use and rejects repeated
  `resume()`/`resumeError()` calls for the same suspended snapshot.
- `Progress.load(...)` also rejects already-consumed same-process dumps before
  exposing authoritative `progress.capability` / `progress.args`, so stale
  blobs cannot be replayed into duplicated side effects after the real resume
  already happened.
- Consumed progress tokens stay burned for the lifetime of the current process,
  including across `worker_threads` and duplicate physical package/addon copies
  in the same PID, so unrelated same-process progress churn cannot make an old
  dumped snapshot replayable again.
- `Progress.dump()` includes detached `snapshot_id`, `snapshot_key_digest`, and
  `token` metadata authenticated by the configured `snapshotKey`, and
  `Progress.load()` verifies that bundle before trusting the dumped snapshot
  bytes.
- In the Node wrapper, `Progress.load(...)` only reuses cached policy
  automatically when the dumped token is still present in the same-process
  cache. Fresh-process restores, or same-process restores after cache eviction,
  must pass explicit `capabilities`, `limits`, and `snapshotKey` so the host
  reasserts both authority and resource policy before dispatching on
  `progress.capability` / `progress.args`. `limits` must be present as a plain
  object even when the caller intentionally wants default limits and therefore
  passes `{}`.
- Hosts must not attempt to run nested guest execution on the same runtime
  state while another `run()`, `start()`, or `resume()` is active.

## Cancellation and Abort Propagation

- The Rust core now exposes a pollable cooperative cancellation token and checks
  it before each instruction dispatch, before idle microtask or queued-host-call
  checkpoints, on every `resume()` entry, and inside long-running native helper
  loops such as `Array.prototype.sort()` and `Object.keys()`.
- Cancellation fails as a top-level guest-safe limit error with the message
  `execution cancelled`. It is host authority, not guest control flow, so guest
  `try` / `catch` does not intercept it.
- In the Node wrapper, hosts use `AbortSignal` to cancel active compute segments
  and `Progress.cancel()` to abort a currently suspended execution.
- Cancelling a suspended async host wait stops guest execution immediately, but
  it does not force-stop the host promise or capability handler that was already
  started.
- The in-process addon still runs on the Node main thread, so a same-thread
  `AbortSignal` cannot preempt synchronous guest compute until control returns
  to the event loop. Hosts that need stronger kill guarantees should still use
  sidecar mode plus OS-level termination controls.
- Cancellation handles are runtime-only state. Serialized snapshots do not
  preserve them, so hosts resuming a loaded snapshot must provide a fresh
  cancellation signal or token if they want later compute to remain cancellable.
