---
title: "Runtime Value Model"
description: "Internal guest-value heap model, GC semantics, and value representation"
category: "Language & Runtime"
order: 2
slug: "runtime-model"
lastUpdated: "2026-04-13"
---

# Runtime Value Model

This document defines the current internal guest-value and heap model used by
the Rust runtime.

It is intentionally internal. None of these types are a public host API or a
stable serialized contract by themselves.

## Internal Guest Values

The runtime represents guest values with an internal `Value` enum:

- immediate scalars: `Undefined`, `Null`, `Bool`, `Number`, and `String`
- heap handles: `Object`, `Array`, `Map`, `Set`, internal `Iterator`,
  `Closure`, and `Promise`
- callable built-ins: `BuiltinFunction`
- explicit host entry points: `HostFunction`

The public host boundary does not expose `Value` directly. Host input, host
output, sidecar messages, and snapshot-adjacent APIs use `StructuredValue`
instead.
`StructuredValue` is tree-shaped: it preserves nested data, holes, and special
numeric tags, but it does not preserve guest object identity or alias graphs.

## Heap Objects and Handles

Heap-backed state is stored indirectly through slotmap keys:

- `ObjectKey` points to `PlainObject`
- `ArrayKey` points to `ArrayObject`
- `MapKey` points to `MapObject`
- `SetKey` points to `SetObject`
- `IteratorKey` points to `IteratorObject`
- `ClosureKey` points to `Closure`
- `PromiseKey` points to `PromiseObject`
- `EnvKey` points to lexical environments
- `CellKey` points to mutable or immutable bindings

These handles are runtime-internal identities. Raw keys must never cross the
host boundary, appear in structured values, or be treated as stable serialized
references outside `mustard`.

## Rooting and Reachability Rules

`mustard` now uses a non-moving mark-sweep collector over the existing slotmap
heaps. Heap handles remain stable for live allocations; collection only removes
unreachable entries and never relocates them.

The collector walks an explicit root set:

- the globals environment is always a root
- active call frames are roots
- each frame roots its current environment, scope stack, and operand stack
- async frames also root their backing promise object when present
- frame exception state also roots guest values and environments:
  pending exceptions, pending return/throw completions, and saved handler
  environments
- internal microtask jobs root saved async continuations and their settled
  values or rejections
- queued or suspended host requests root the guest promise they will settle
- environments root their cells
- cells root the `Value` they contain
- objects, arrays, maps, sets, and promises root their contained values
- iterators root the array they are currently traversing
- closures root their captured environment chain
- validated suspended snapshots restore the same runtime graph and therefore the
  same root categories after load

Practical rules that follow from this:

- no raw guest references may cross into JavaScript host code
- no raw guest references may cross the sidecar boundary
- no raw guest references may be embedded in structured host values
- snapshots may only persist validated runtime handles owned by `mustard`

Collection currently runs at allocation-safe execution boundaries and resume
points. That keeps the collector precise without requiring a moving handle
layer or conservative native-stack scanning.

## Internal Iterators

The current iteration milestone introduces one internal iterator kind:

- array iterators used by `for...of`

These iterator objects are heap-allocated and snapshot-safe, but they are still
runtime-internal:

- they are never exposed through `StructuredValue`
- they are not guest-authorable
- they currently preserve only an array reference and the next numeric index
- they allow suspended `for...of` loops to resume without rebuilding loop state
- they do not implement user-visible iterator closing because generators and
  custom iterators are still deferred

## Plain Objects, Arrays, Maps, Sets, and Shapes

Plain objects currently use `IndexMap<String, Value>`. Arrays use a dedicated
element vector plus string-keyed extra properties. `Map` and `Set` use
dedicated insertion-ordered entry vectors with SameValueZero key and membership
semantics.

For keyed collections:

- `Map` updates preserve an existing key's insertion position; duplicate Set
  adds do not change order.
- Deletion leaves a tombstone, so live iterators can still observe later adds.
  Sets compact at 64 or more slots when at least half are dead. Every live Set
  cursor is rebased to its live-prefix position, including `forEach` and
  set-algebra callback cursors. Compaction never restarts traversal or revisits
  surviving entries. Fully exhausted iterators remain exhausted.
- `clear` empties the collection and advances its clear epoch. Non-exhausted
  iterators can observe subsequent additions; snapshots preserve their cursors.
- Set insertion reserves heap growth before changing storage or lookup indexes,
  so a pressure-triggered GC sees consistent cached accounting and failed
  allocations leave the collection unchanged.
- Iterable constructors, `for...of`, and `entries`/`keys`/`values` iterators use
  these same insertion-order guarantees.

Plain-object shape metadata and inline caches are optimizations only: property
get/set/delete semantics remain centralized, and shape changes invalidate caches.

## Boundary Separation

The runtime keeps three layers distinct:

- internal guest values: `Value`
- host-safe transferred values: `StructuredValue`
- serialized runtime state: validated compiled programs and snapshots

That separation is part of the safety model. Guest semantics stay in Rust, and
the Node wrapper stays limited to marshaling, capability dispatch, and error
normalization.
