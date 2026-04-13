---
title: "ADR 0001: Core Architecture"
description: "Architecture decision record for Rust core + Node addon + sidecar design"
category: "Architecture"
order: 1
slug: "adr-0001-core-architecture"
lastUpdated: "2026-04-13"
---

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
