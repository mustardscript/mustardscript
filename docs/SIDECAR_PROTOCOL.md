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
The sidecar treats `id` as a correlation token only: it echoes the value back
verbatim and does not require request IDs to be unique.

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
  "policy": {
    "capabilities": ["fetch_data"],
    "limits": {},
    "snapshot_id": "...",
    "snapshot_key_base64": "...",
    "snapshot_key_digest": "...",
    "snapshot_token": "..."
  },
  "payload": {
    "type": "value",
    "value": { "Number": { "Finite": 1.0 } }
  }
}
```

`payload.type` is `value`, `error`, or `cancelled`.
`policy` is required. The host must reassert the allowed capability names and
authoritative runtime limits before the sidecar will inspect or resume a loaded
snapshot. The `limits` field must be present even when the host intentionally
wants default limits and therefore sends `{}`. `snapshot_id`,
`snapshot_key_base64`, `snapshot_key_digest`, and `snapshot_token` are also
required for loaded snapshots. The token is the lowercase hex HMAC-SHA256 of
the detached `snapshot_id` under the caller-chosen snapshot key, and the
sidecar recomputes `snapshot_id` from the raw `snapshot_base64` bytes before
trusting the snapshot contents. Those fields bind resume to trusted detached
dump metadata, but hosts still need ordinary integrity controls when snapshots
are stored or transported.

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

## Stateless Request Semantics

- The sidecar keeps no server-side session, program handle, or snapshot handle
  between requests.
- `program_base64` and `snapshot_base64` are host-managed opaque blobs. The host
  may reuse the same compiled program bytes for multiple `start` requests and
  may replay the same snapshot bytes in multiple `resume` requests as long as
  it preserves or recomputes the matching detached `snapshot_id`,
  `snapshot_key_digest`, and `snapshot_token` for the supplied
  `snapshot_key_base64`.
- Replaying a snapshot re-executes from that suspension point deterministically
  under the supplied `policy`; there is no in-sidecar single-use tracking.
- If the embedding host wants stronger single-use or anti-replay guarantees, it
  must enforce them above this protocol boundary.

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
- Hard-stop semantics are therefore OS-process semantics, not an in-band
  protocol message. Hosts may use their platform kill primitive, job control,
  container stop, or equivalent.
- Once a sidecar is forcefully terminated, the embedding host must treat that
  process as dead and must not reuse its stdio channel.
- Any in-flight request is lost when the process is killed. To continue work,
  the host starts a fresh sidecar and replays a previously persisted
  compiled-program blob or suspension snapshot, or recompiles from source if no
  resumable blob was saved.
