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
- `start()` returns a suspension object containing the capability name, the
  converted arguments, and a resumable snapshot.
- `resume()` accepts either a structured success value or a sanitized host
  error payload.
- The Node wrapper accepts sync or async JavaScript capability functions and
  bridges both cases by awaiting the host result before calling `resume()`.

## Error Sanitization

Host failures cross the boundary with:

- `name`
- `message`
- optional `code`
- optional `details`

The current runtime renders resumed host failures as guest-safe runtime errors
using those fields. Guest-visible `Error` objects are not implemented yet.
The Node wrapper rethrows core failures as typed JavaScript errors:
`JsliteParseError`, `JsliteValidationError`, `JsliteRuntimeError`,
`JsliteLimitError`, and `JsliteSerializationError`. The original native error is
preserved as the JavaScript `cause`.
When those failures resume into guest execution, the runtime also renders a
guest-only traceback with guest function names and source spans.

## Console Contract

- A `console` global object exists so the global name is reserved.
- Deterministic `console.log`, `console.warn`, and `console.error` callbacks are
  a planned later milestone and are not implemented yet.

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
  currently inject a guest-visible cancellation signal.
- Sidecar hosts that need forceful aborts must terminate the sidecar process.
