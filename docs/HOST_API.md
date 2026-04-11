# Host API

## Structured Host Values

Allowed values:

- `undefined`
- `null`
- booleans
- strings
- numbers, including `NaN`, `Infinity`, and `-0`
- arrays of allowed values
- plain objects with string keys and allowed values

Rejected values:

- functions
- symbols
- bigint
- cycles
- class instances
- accessors
- custom prototypes
- host objects

## Capability Calls

- Capabilities are named host functions.
- Capability lookup is explicit.
- Capability calls may be sync or async.
- Async capability calls suspend guest execution until resumed.

## Error Sanitization

Host failures are surfaced as guest-safe errors with:

- `name`
- `message`
- optional `code`
- optional `details`

## Console Contract

- `console.log`, `console.warn`, and `console.error` are deterministic host callbacks.
- Console output is observable only through the configured host hook.

## Reentrancy

The same VM instance must not be re-entered while already executing unless that
behavior is explicitly supported by the runtime mode.
