# Sidecar Protocol

This document defines the current `mustard-sidecar` process contract.

## Transport

The default transport is a length-prefixed binary frame over stdio:

1. `u32` little-endian header length
2. `u32` little-endian binary payload length
3. UTF-8 JSON header bytes
4. raw payload bytes

The JSON header carries ordinary protocol metadata such as `method`, `id`,
`protocol_version`, policy/auth fields, and structured guest values. Raw
program bytes and raw snapshot bytes travel in the binary payload section
instead of base64 fields.

The current protocol version is `2`.

Debug mode is still available:

- `mustard-sidecar --jsonl` switches to newline-delimited JSON over stdio
- JSONL mode preserves the older inspectable `program_base64` and
  `snapshot_base64` fields for debugging and corpus seeding

Safety limits:

- binary request frames larger than `1 MiB` fail closed before protocol parsing
- JSONL request lines larger than `1 MiB` fail closed before JSON parsing
- empty JSONL lines are ignored

## Request Methods

`mustard-sidecar` accepts three request methods:

1. `compile`
2. `start`
3. `resume`

All requests include:

- `protocol_version`
- an integer `id` chosen by the host

The sidecar treats `id` as a correlation token only: it echoes the value back
verbatim and does not require request IDs to be unique.

### `compile`

Binary-frame header:

```json
{
  "protocol_version": 2,
  "method": "compile",
  "id": 1,
  "source": "const value = 1; value;"
}
```

`compile` does not send a binary payload. Successful responses return:

- a session-local `program_id`
- raw compiled program bytes in the binary payload section

In `--jsonl` mode, the same response exposes `program_base64` instead.

### `start`

Binary-frame header using a cached program:

```json
{
  "protocol_version": 2,
  "method": "start",
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
- or raw program bytes in the binary payload, which may also seed that session
  cache

If both a `program_id` and inline program bytes are supplied, the digest must
match or the request fails closed. If neither is supplied, the request fails
closed.

Successful responses return either:

- a completed value with no binary payload
- or a suspended step containing `capability`, `args`, plus `snapshot_id` and
  `policy_id` in the JSON header and raw snapshot bytes in the binary payload

In `--jsonl` mode, suspended steps expose `snapshot_base64` instead of a raw
payload blob.

### `resume`

Binary-frame header using cached snapshot/policy state:

```json
{
  "protocol_version": 2,
  "method": "resume",
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

`resume` accepts either:

- raw snapshot bytes in the binary payload plus full `policy`
- or cached `snapshot_id` plus cached `policy_id` plus fresh `auth`

The host must still reassert the authoritative restore policy before the
sidecar will inspect or resume a loaded snapshot.

- Full `policy` requests must include `capabilities`, `limits`, `snapshot_id`,
  `snapshot_key_base64`, `snapshot_key_digest`, and `snapshot_token`.
- Cached `policy_id` requests reuse the `capabilities` and `limits` seeded from
  the original `start` request in the same sidecar session, but the host must
  still supply fresh `auth` metadata for the specific suspended snapshot being
  resumed.

The token is the lowercase hex HMAC-SHA256 of the detached `snapshot_id` under
the caller-chosen snapshot key. The sidecar recomputes `snapshot_id` from the
supplied raw snapshot bytes or from the cached bytes referenced by
`snapshot_id` before trusting the snapshot contents.

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

- The sidecar keeps a session-local compiled-program cache keyed by
  `program_id`.
- The sidecar keeps a session-local suspended-snapshot cache keyed by
  `snapshot_id`.
- The sidecar keeps a session-local capability/limits cache keyed by
  `policy_id`.
- `compile` returns both raw program bytes and the matching `program_id`.
- `start` may reference a cached `program_id` instead of resending program
  bytes, but those IDs only remain valid for the lifetime of the current
  sidecar process.
- Suspended `start` and `resume` responses return `snapshot_id`, so later
  `resume` requests may reference cached bytes without resending the snapshot
  payload.
- Suspended `start` responses also return `policy_id`, so later `resume`
  requests may reference cached capability/limits metadata without resending
  it on every hop.
- Hosts may still replay the same snapshot bytes in multiple `resume` requests
  as long as they preserve or recompute the matching detached `snapshot_id`,
  `snapshot_key_digest`, and `snapshot_token`.
- Replaying a snapshot re-executes from that suspension point deterministically
  under the supplied `policy`; there is still no in-sidecar single-use
  tracking.
- If the embedding host wants stronger single-use or anti-replay guarantees, it
  must enforce them above this protocol boundary.

## Lifecycle and Shutdown

- EOF on stdin is a clean shutdown signal. The sidecar exits successfully after
  processing all prior requests.
- Invalid request frames are fatal. The sidecar reports an error to stderr and
  exits with a non-zero status.
- The protocol is request/response only. There is no background push channel or
  heartbeat.

## Termination

- Because cooperative cancellation is not implemented yet, hosts that need a
  hard stop must terminate the sidecar process.
- The sidecar is a separate process specifically so hosts can do that without
  corrupting the embedding process.
- Hard-stop semantics are therefore OS-process semantics, not an in-band
  protocol message.
- Once a sidecar is forcefully terminated, the embedding host must treat that
  process as dead and must not reuse its stdio channel.
- Any in-flight request is lost when the process is killed. To continue work,
  the host starts a fresh sidecar and replays a previously persisted compiled
  program blob or suspension snapshot, or recompiles from source if no saved
  artifact exists. Cached `program_id`, `snapshot_id`, and `policy_id` values
  do not survive process termination.
