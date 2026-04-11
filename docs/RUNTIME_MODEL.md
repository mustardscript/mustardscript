# Runtime Value Model

This document defines the current internal guest-value and heap model used by
the Rust runtime.

It is intentionally internal. None of these types are a public host API or a
stable serialized contract by themselves.

## Internal Guest Values

The runtime represents guest values with an internal `Value` enum:

- immediate scalars: `Undefined`, `Null`, `Bool`, `Number`, and `String`
- heap handles: `Object`, `Array`, `Closure`, and `Promise`
- callable built-ins: `BuiltinFunction`
- explicit host entry points: `HostFunction`

The public host boundary does not expose `Value` directly. Host input, host
output, sidecar messages, and snapshot-adjacent APIs use `StructuredValue`
instead.

## Heap Objects and Handles

Heap-backed state is stored indirectly through slotmap keys:

- `ObjectKey` points to `PlainObject`
- `ArrayKey` points to `ArrayObject`
- `ClosureKey` points to `Closure`
- `PromiseKey` points to `PromiseObject`
- `EnvKey` points to lexical environments
- `CellKey` points to mutable or immutable bindings

These handles are runtime-internal identities. Raw keys must never cross the
host boundary, appear in structured values, or be treated as stable serialized
references outside `jslite`.

## Rooting and Reachability Rules

`jslite` now uses a non-moving mark-sweep collector over the existing slotmap
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
- objects, arrays, and promises root their contained values
- closures root their captured environment chain
- validated suspended snapshots restore the same runtime graph and therefore the
  same root categories after load

Practical rules that follow from this:

- no raw guest references may cross into JavaScript host code
- no raw guest references may cross the sidecar boundary
- no raw guest references may be embedded in structured host values
- snapshots may only persist validated runtime handles owned by `jslite`

Collection currently runs at allocation-safe execution boundaries and resume
points. That keeps the collector precise without requiring a moving handle
layer or conservative native-stack scanning.

## Plain Objects, Arrays, and Shapes

Plain objects currently use `IndexMap<String, Value>`. Arrays use a dedicated
element vector plus string-keyed extra properties.

There is no shape or hidden-class layer today. If shapes are introduced later,
they are an optimization only:

- shapes must not define guest semantics
- shape absence must not change correctness
- property get/set behavior must remain centralized

## Boundary Separation

The runtime keeps three layers distinct:

- internal guest values: `Value`
- host-safe transferred values: `StructuredValue`
- serialized runtime state: validated compiled programs and snapshots

That separation is part of the safety model. Guest semantics stay in Rust, and
the Node wrapper stays limited to marshaling, capability dispatch, and error
normalization.
