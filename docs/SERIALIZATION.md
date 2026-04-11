# Serialization

## Formats

`jslite` serializes:

1. Compiled programs
2. Suspended execution snapshots

## Versioning

- Serialized data is versioned explicitly.
- Round-tripping is only guaranteed within the same `jslite` version.
- Cross-version loads are rejected.

## Safety Rules

- Inputs are validated before load.
- Opaque host references are never serialized.
- Pending host work must be represented as resumable metadata, not native futures.

## Value Encoding

The encoding is tagged so that values such as `undefined`, `NaN`, `Infinity`,
and `-0` can round-trip safely.
