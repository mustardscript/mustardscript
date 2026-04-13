# ADR 0001: Core Architecture

## Status

Accepted

## Decision

`mustard` uses a Rust core with an explicit pipeline:

`source -> parse -> validate -> lowered IR -> bytecode -> VM`

The primary in-process embedder is a Node-API addon. Sidecar mode runs the same
core runtime in a separate process behind a structured protocol.

## Consequences

- Semantics stay centralized in Rust.
- The Node layer remains thin.
- Sidecar mode can reuse the same compiled programs and snapshots.
