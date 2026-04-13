# Sidecar Protocol

This document defines the current `mustard-sidecar` process contract.

## Transport

- The transport is newline-delimited JSON over stdio.
- The host writes exactly one request object per line to stdin.
- The sidecar writes exactly one response object per line to stdout.
- Empty input lines are ignored.
- Request lines longer than 1 MiB are rejected and terminate the sidecar with a
  non-zero exit before JSON parsing continues.

## Request Methods

`mustard-sidecar` currently accepts three request shapes:

1. `compile`
2. `start`
3. `resume`

All requests include:

- `protocol_version`
- an integer `id` chosen by the host

The current protocol version is `1`.
The sidecar treats `id` as a correlation token only: it echoes the value back
verbatim and does not require request IDs to be unique.

### `compile`

```json
{
  "protocol_version": 1,
  "method": "compile",
  "id": 1,
  "source": "const value = 1; value;"
}
```

Successful responses return a base64-encoded compiled program blob.
They also return a session-local `program_id` handle that the same sidecar
process may reuse on later `start` requests.

### `start`

```json
{
  "method": "start",
  "protocol_version": 1,
  "id": 2,
  "program_id": "...",
  "options": {
    "inputs": {},
    "capabilities": ["fetch_data"]
  }
}
```

`start` requires either:

- `program_id` for a compiled program already cached in the current sidecar
  session
- or `program_base64` for a raw program blob that should be decoded directly
  and may seed that session cache

If both fields are omitted, the request fails closed. If `program_base64` is
used to seed a cached entry for a supplied `program_id`, the digest must match
or the request fails closed.

Successful responses return either:

- a completed value
- or a suspended step containing `capability`, `args`, `snapshot_base64`,
  `snapshot_id`, and a session-local `policy_id`

### `resume`

```json
{
  "method": "resume",
  "protocol_version": 1,
  "id": 3,
  "snapshot_id": "...",
  "policy_id": "...",
  "auth": {
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
`resume` now accepts either:

- the original `snapshot_base64` plus full `policy`
- or cached `snapshot_id` plus cached `policy_id` plus `auth`

The host must still reassert the authoritative restore policy before the
sidecar will inspect or resume a loaded snapshot.

- Full `policy` requests work as before: they must include `capabilities`,
  `limits`, `snapshot_id`, `snapshot_key_base64`, `snapshot_key_digest`, and
  `snapshot_token`.
- Cached `policy_id` requests reuse the `capabilities` and `limits` that were
  seeded from the original `start` request in the same sidecar session, but the
  host must still supply fresh `auth` metadata for the specific suspended
  snapshot being resumed.

The token is the lowercase hex HMAC-SHA256 of the detached `snapshot_id` under
the caller-chosen snapshot key, and the sidecar recomputes `snapshot_id` from
either the supplied raw `snapshot_base64` bytes or the cached bytes referenced
by `snapshot_id` before trusting the snapshot contents. Those fields bind
resume to trusted detached dump metadata, but hosts still need ordinary
integrity controls when snapshots are stored or transported.

## Response Shape

Every response includes:

- `protocol_version`
- `id`
- `ok`
- `result` on success
- `error` on failure

Failures are rendered as guest-safe or protocol-safe strings and do not expose
raw host capabilities or internal runtime handles.
Requests with a missing or unsupported `protocol_version` fail closed with an
explicit protocol-version error instead of attempting best-effort compatibility.

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

## Session Semantics

- The sidecar now keeps a session-local compiled-program cache keyed by
  `program_id`.
- The sidecar now also keeps a session-local suspended-snapshot cache keyed by
  `snapshot_id`.
- The sidecar keeps a session-local capability/limits cache keyed by
  `policy_id`.
- `compile` seeds that cache and returns both the opaque `program_base64` blob
  and its `program_id`.
- `start` may reference a cached `program_id` instead of resending
  `program_base64`, but those IDs only remain valid for the lifetime of the
  current sidecar process.
- Suspended `start` and `resume` responses return `snapshot_id`, so later
  `resume` requests may reference cached bytes without resending
  `snapshot_base64`.
- Suspended `start` responses also return `policy_id`, so later `resume`
  requests may reference cached capability/limits metadata without resending
  it on every hop.
- Hosts may still replay the same snapshot bytes in multiple `resume` requests
  as long as they preserve or recompute the matching detached `snapshot_id`,
  `snapshot_key_digest`, and `snapshot_token` for the supplied
  `snapshot_key_base64`.
- Replaying a snapshot re-executes from that suspension point deterministically
  under the supplied `policy`; there is still no in-sidecar single-use
  tracking.
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
  resumable blob was saved. Cached `program_id`, `snapshot_id`, and `policy_id`
  values do not survive process termination.
