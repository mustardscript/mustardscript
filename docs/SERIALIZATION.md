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
- Snapshot loads also validate live frame pointers and referenced runtime
  objects before restore.
- Opaque host references, native handles, and host futures are never
  serialized.
- Snapshots are only created at explicit suspension points.
- Pending host work is represented by the suspended capability name plus the
  resumable VM snapshot, not by native futures.

## Value Encoding

The encoding is tagged so that values such as `undefined`, `NaN`, `Infinity`,
and `-0` can round-trip safely.

## Serialization Exclusions

The following values may never be serialized:

- opaque host references
- native handles
- unresolved host futures
- JavaScript callback identities from the embedding host
