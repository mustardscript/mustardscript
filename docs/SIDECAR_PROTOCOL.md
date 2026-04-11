# Sidecar Protocol

This document defines the current `jslite-sidecar` process contract.

## Transport

- The transport is newline-delimited JSON over stdio.
- The host writes exactly one request object per line to stdin.
- The sidecar writes exactly one response object per line to stdout.
- Empty input lines are ignored.

## Request Methods

`jslite-sidecar` currently accepts three request shapes:

1. `compile`
2. `start`
3. `resume`

All requests include an integer `id` chosen by the host.

### `compile`

```json
{ "method": "compile", "id": 1, "source": "const value = 1; value;" }
```

Successful responses return a base64-encoded compiled program blob.

### `start`

```json
{
  "method": "start",
  "id": 2,
  "program_base64": "...",
  "options": {
    "inputs": {},
    "capabilities": ["fetch_data"]
  }
}
```

Successful responses return either:

- a completed value
- or a suspended step containing `capability`, `args`, and `snapshot_base64`

### `resume`

```json
{
  "method": "resume",
  "id": 3,
  "snapshot_base64": "...",
  "payload": {
    "type": "value",
    "value": { "Number": { "Finite": 1.0 } }
  }
}
```

`payload.type` is either `value` or `error`.

## Response Shape

Every response includes:

- `id`
- `ok`
- `result` on success
- `error` on failure

Failures are rendered as guest-safe or protocol-safe strings and do not expose
raw host capabilities or internal runtime handles.

## Capability Proxy Model

The sidecar does not execute host capabilities itself.

Instead:

1. the guest hits a host boundary inside the shared Rust core
2. the sidecar returns a suspended step with a capability name and structured
   arguments
3. the embedding host performs the actual host work locally
4. the host sends either a structured `resume` value or a sanitized `resume`
   error back to the sidecar

That means capability proxying happens through suspension and resume, not by
shipping host callbacks or JavaScript functions into the sidecar process.

## Lifecycle and Shutdown

- EOF on stdin is a clean shutdown signal. The sidecar exits successfully after
  processing all prior requests.
- Invalid request lines are fatal. The sidecar reports an error to stderr and
  exits with a non-zero status.
- The protocol is request/response only. There is no background push channel or
  heartbeat yet.

## Termination

- Because cooperative cancellation is not implemented yet, hosts that need a
  hard stop must terminate the sidecar process.
- The sidecar is a separate process specifically so hosts can do that without
  corrupting the embedding process.
