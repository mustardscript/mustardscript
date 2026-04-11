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
- arrays of allowed values
- plain objects with string keys and allowed values

Rejected values:

- functions
- symbols
- bigint
- cycles
- class instances
- accessors
- custom prototypes
- host objects

## Capability Calls

- Capabilities are named host functions.
- Capability lookup is explicit.
- The core runtime represents capability calls as suspension points.
- In synchronous guest code, a capability call suspends immediately.
- In async guest code, a capability call produces an internal guest promise,
  queues the host request, and suspends at the next runtime checkpoint.
- `start()` returns a suspension object containing the capability name, the
  converted arguments, and a resumable snapshot.
- `resume()` accepts either a structured success value or a sanitized host
  error payload.
- The Node wrapper accepts sync or async JavaScript capability functions and
  bridges both cases by awaiting the host result before calling `resume()`.
- `limits.maxOutstandingHostCalls` bounds the combined number of queued and
  currently suspended host requests for async guest execution.

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
- `Progress.dump()` / `Progress.load()` preserve that single-use identity within
  one Node process, so cloned dumped progress objects cannot both be resumed.
- Hosts must not attempt to run nested guest execution on the same runtime
  state while another `run()`, `start()`, or `resume()` is active.

## Cancellation and Abort Propagation

- Explicit cancellation is not implemented yet in the core runtime.
- The current addon boundary is synchronous and one-shot, so it does not yet
  expose a shared cancellation hook the Rust VM can poll mid-execution.
- In-process hosts can stop awaiting a suspended execution, but that does not
  currently inject a guest-visible cancellation signal, including when guest
  async code is awaiting a host promise.
- Sidecar hosts that need forceful aborts must terminate the sidecar process.
