# Serialization

This document covers the serialized forms that already exist in the runtime and
the safety rules they are expected to follow.

## Formats

`jslite` serializes:

1. Compiled programs
2. Suspended execution snapshots

## Compiled-Program Format Goals

- preserve lowered bytecode without reparsing source
- remain private to `jslite` rather than becoming a public stable bytecode
- round-trip only within the same `jslite` version
- fail safely on corrupt or unsupported input

## Versioning

- Serialized data is versioned explicitly.
- Round-tripping is only guaranteed within the same `jslite` version.
- Cross-version loads are rejected.

## Safety Rules

- The current loader validates the outer format by decoding the tagged payload
  and checking the serialized version.
- Compiled-program loads validate root function ids, closure targets, jump
  targets, and stack/scope discipline before execution.
- Snapshot loads also validate live frame pointers, referenced runtime objects,
  iterator references, promise references, and queued host-call state before
  restore.
- Loaded snapshots are inert until the host rebinds explicit resume policy.
  Restores fail closed if the host does not reassert allowed capability names
  and authoritative runtime limits.
- Public `ExecutionSnapshot` serde deserialization reuses the same validation,
  accounting recomputation, and explicit-policy gate as `load_snapshot()`, so
  callers cannot bypass restore checks by deserializing the public type
  directly.
- Opaque host references, native handles, and host futures are never
  serialized.
- Snapshots are only created at explicit suspension points.
- Pending host work is represented by suspended or queued capability metadata
  plus the resumable VM snapshot and internal promise state, not by native
  futures.
- Active array `for...of` iterators are serialized as internal runtime state
  that preserves their source array reference and next index, so resumed loops
  continue from the next unvisited element.
- `Map` and `Set` values are serialized only inside internal runtime snapshots,
  where their insertion-ordered entry lists and referenced guest values are
  preserved after validation.
- In the Node wrapper, `start()` and `Progress.dump()` happen before any async
  capability promise is awaited, so JavaScript `Promise` objects never enter the
  serialized snapshot.
- In the Node wrapper, `Progress.dump()` includes detached `snapshot_id`,
  `snapshot_key_digest`, and `token` metadata authenticated by the configured
  `snapshotKey`. `Progress.load(...)` verifies that bundle before it inspects
  or resumes a dumped snapshot.
- `Progress.load(...)` always requires explicit `capabilities` or `console`,
  explicit `limits` as an object (use `{}` for defaults), and the original
  `snapshotKey` before inspection or resume. Consumed same-process dumps stay
  single-use across `worker_threads` and duplicate package copies in the same
  PID.

## Value Encoding

The encoding is tagged so that values such as `undefined`, `NaN`, `Infinity`,
and `-0` can round-trip safely.

## Serialization Exclusions

The following values may never be serialized:

- opaque host references
- native handles
- unresolved host futures
- JavaScript callback identities from the embedding host

`Map` and `Set` are intentionally absent from `StructuredValue`, sidecar wire
messages, and host-call payloads even though validated snapshots may preserve
them as internal runtime objects.
