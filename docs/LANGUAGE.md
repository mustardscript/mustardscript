# Language Contract

This document describes the currently implemented language surface. Planned
extensions are called out explicitly instead of being implied.

## Baseline Rules

- Guest code always runs with strict semantics.
- Input is script-only; module syntax is rejected.
- Unsupported features fail closed with explicit diagnostics.
- Free references to forbidden ambient globals are rejected when lexical
  resolution proves they are unresolved.
- Free `eval` and free `Function` are rejected for the same reason.

## Supported Value Types

- `undefined`
- `null`
- booleans
- numbers
- strings
- arrays
- plain objects
- `Map`
- `Set`
- conservative `Date` objects
- guest functions

## Supported End-to-End Syntax

- variable declarations with `let` and `const`
- function declarations and expressions, including rest parameters
- `async` function declarations and expressions
- arrow functions
- `await` inside async functions
- literals, arrays, and plain-object literals with static keys, computed keys,
  method shorthand, and spread from plain objects or arrays
- `if`, `switch`, `while`, `do...while`, `for`, array `for...of`, `break`,
  and `continue`
- `return`
- `throw`, `try`, `catch`, and `finally`
- common destructuring
- assignment to identifiers and member expressions
- sequence expressions
- member access, calls, and `new` for supported built-ins
- template literals
- optional chaining
- nullish coalescing
- binary `**` and `in`
- named host capability calls

## Supported Function Call Surface

- non-arrow guest member calls bind the computed receiver as `this`
- arrow functions are supported, but this does not imply full lexical or
  dynamic `this` parity beyond the currently documented subset
- rest parameters are supported for function declarations, function
  expressions, and arrow functions
- `new` remains limited to the documented conservative built-in constructors

## Supported Async Surface

- async functions return guest promise values
- `await` suspends the current async continuation onto the runtime microtask
  queue
- host capability calls inside async guest code return guest promises and still
  suspend through the existing `start()` / `resume()` boundary
- `new Promise(executor)` is available when `executor` is callable and
  completes synchronously from the runtime's perspective
- `Promise.resolve(...)`, `Promise.reject(...)`, `Promise.all(...)`,
  `Promise.race(...)`, `Promise.any(...)`, and `Promise.allSettled(...)` are
  available
- promise instance methods `then(...)`, `catch(...)`, and `finally(...)` are
  available
- promise resolution and `await` adopt guest promises plus guest object or
  array thenables with callable `.then` properties
- Promise executor and thenable resolve/reject functions keep first-settlement
  semantics; later resolve/reject calls and post-settlement throws do not
  override the settled result
- `Promise.any(...)` rejects with a guest-visible `AggregateError` object whose
  `errors` property preserves rejection reasons in iteration order
- async Promise executors and async adopted `.then` handlers reject with an
  explicit `TypeError`
- synchronous host suspensions from Promise executors and adopted thenables
  still fail closed

## Supported BigInt Surface

- guest code supports `BigInt` literals such as `123n`
- exact-integer `+`, `-`, `*`, `/`, and `%` are supported when both operands
  are `BigInt`
- exact-integer `**` is supported when both operands are `BigInt` and the
  exponent is a non-negative `BigInt`
- `typeof value` reports `"bigint"` for guest `BigInt` values
- `BigInt` truthiness, string coercion, and property-key coercion are
  supported
- `Map` and `Set` membership support guest `BigInt` keys
- mixed `BigInt` / `Number` arithmetic and relational comparisons fail closed
- unary `+1n`, `Number(1n)`, and `JSON.stringify(...)` of `BigInt` values fail
  closed with explicit runtime errors
- guest `BigInt` values remain guest-internal and cannot cross the structured
  host boundary

## Supported Iteration Surface

- `for...of` supports either exactly one `let` / `const` binding declaration
  or an identifier/member assignment target in the loop header
- `for...in` supports the same header forms as the documented `for...of`
  surface, but only when the right-hand side is a plain object or array
- arrays, strings, `Map`, `Set`, and guest iterator objects from the supported
  helper surface are iterable in the current surface
- declaration headers can use the same identifier, array, and object
  destructuring forms already supported elsewhere in the runtime
- assignment-target headers support identifier and member targets only because
  destructuring assignment remains unsupported
- each `for...of` iteration gets a fresh lexical binding environment for
  declaration headers; assignment-target headers reuse the existing binding or
  member reference each iteration
- array iteration yields values in ascending numeric index order and ignores
  non-index properties
- active array iterators observe the live backing array length, so elements
  appended before exhaustion are visited in order
- strings iterate by Unicode scalar values and preserve source order
- `Map` default iteration yields `[key, value]` entry pairs in insertion order
- `Set` default iteration yields values in insertion order
- public iterator helper methods `values()`, `keys()`, and `entries()` are
  available on arrays and keyed collections, and produced iterator objects
  expose a guest-visible `.next()` method
- unsupported iterable inputs such as plain objects, promises, and custom
  symbol-based iterables throw a runtime `TypeError`
- abrupt completion from `break`, `continue`, `return`, or `throw` discards the
  internal iterator state with no user-visible iterator-close hook because
  generators and custom iterator authoring remain deferred

## Supported Keyed Collection Surface

- `new Map(iterable)` and `new Set(iterable)` accept the supported iterable
  surface
- `Map` supports `get`, `set`, `has`, `delete`, `clear`, and `size`
- `Set` supports `add`, `has`, `delete`, `clear`, and `size`
- `Map` keys and `Set` membership use SameValueZero semantics:
  `NaN` matches `NaN`, `-0` and `0` address the same entry, strings compare by
  string contents, and heap values compare by guest identity
- `Map` and `Set` preserve first-in insertion order internally; updating an
  existing entry does not move it, `delete` removes the entry, and `clear`
  empties the collection
- `Map.prototype.entries`, `Map.prototype.keys`, `Map.prototype.values`,
  `Set.prototype.entries`, `Set.prototype.keys`, and `Set.prototype.values`
  return guest iterator objects that preserve insertion order
- `Map(iterable)` expects each produced item to be a guest array pair and uses
  the first two elements as `[key, value]`
- custom string properties on `Map` and `Set` instances are currently
  unsupported and fail closed

## Rejected With Validation Diagnostics

- `import`, `export`, and dynamic `import()`
- `delete`
- default parameters
- default destructuring
- free `arguments`
- free `eval` and free `Function`
- free references to `process`, `module`, `exports`, `global`, `require`,
  `setTimeout`, `setInterval`, `queueMicrotask`, and `fetch`
- `with`
- classes
- generators and `yield`
- `for await...of`
- `for...of` declaration headers that do not declare exactly one `let` or
  `const` binding, declaration initializers in `for...of` / `for...in`
  headers, and destructuring assignment targets
- `debugger`
- labeled statements
- object literal accessors
- array spread and array holes

## Explicit Deferrals

- fully general Promise constructor and thenable-adoption edge cases,
  including hostile thenable cycles
- unsupported assignment operators such as `**=`, `||=`, `&&=`, and the
  bitwise and shift assignment families
- full `this` semantics beyond the current basic function-call behavior
- implicit `arguments` object semantics
- default parameter evaluation
- symbol-based custom iterable protocol support
- custom iterator authoring beyond the documented collection helpers
- module loading
- property descriptor semantics
- full prototype semantics
- accessors
- symbols
- typed arrays
- full `Date` parity beyond the documented conservative subset
- `Intl`
- `Proxy`

## Diagnostics and Tracebacks

- Parse and validation failures preserve guest source spans.
- Runtime and limit failures render guest-safe tracebacks using guest function
  names and guest source spans.
- Guest `throw` / `catch` preserves the thrown guest value. Built-in error
  constructors create guest objects with `name` and `message`, and sanitized
  host failures may also expose `code` and `details`.
- Guest-visible stack information is limited to guest function names and guest
  source spans.
- Current traceback precision is function-level span data rather than
  exact-expression locations.
- Guest-facing rendering does not include host paths, internal filenames, or
  Rust implementation details.

## Observable Ordering

- The currently supported observable property-order surface is
  `JSON.stringify`, `for...of` over the documented iterable surface, `Map` /
  `Set` iteration helpers, and the supported `Object.keys` / `Object.values` /
  `Object.entries` helpers.
- `JSON.stringify` on plain objects follows JavaScript own-property order:
  array-index keys in ascending numeric order, then remaining string keys in
  insertion order.
- `JSON.stringify` on arrays renders elements in ascending numeric index order.
- Non-index array properties are ignored by `JSON.stringify`.
- `JSON.stringify` omits object properties whose values are `undefined` or
  callable, serializes those values as `null` inside arrays, returns
  `undefined` for top-level `undefined` or callable inputs, serializes
  non-finite numbers as `null`, and renders supported `Date` values as UTC
  RFC3339 timestamps.
- Array `for...of` yields values in ascending numeric index order.
- String iteration yields characters in source order.
- `Map` iteration and `Map.prototype.entries` / `keys` / `values` preserve
  insertion order.
- `Set` iteration and `Set.prototype.entries` / `keys` / `values` preserve
  insertion order.
- `Object.keys`, `Object.values`, and `Object.entries` on plain objects follow
  the same JavaScript own-property order as `JSON.stringify`.
- `Object.keys`, `Object.values`, and `Object.entries` on arrays enumerate
  numeric indices in ascending order followed by custom string properties in
  insertion order.
- Canonical array-index keys are the standard JavaScript string forms such as
  `"0"` and `"10"`; non-canonical numeric-looking keys such as `"01"` and
  `"4294967295"` remain ordinary string properties.
- `for...in` over plain objects and arrays uses the same documented key order
  as `Object.keys(...)`; other right-hand sides fail closed with the same
  runtime `TypeError` surface as the supported `Object` helpers.

## Built-Ins and Global Names

- `globalThis`
- `Object`
- `Array`
- `Map`
- `Set`
- `Promise`
- `RegExp`
- `Date`
- `String`
- `Error`
- `TypeError`
- `ReferenceError`
- `RangeError`
- `Number`
- `Boolean`
- `Math`
- `JSON`
- `console` with deterministic `log`, `warn`, and `error` methods when the host
  explicitly provides those callbacks

### Currently Implemented Built-In Members

- `Array.isArray`
- `Array.from`
- `Array.of`
- `Array.prototype.push`
- `Array.prototype.pop`
- `Array.prototype.slice`
- `Array.prototype.splice`
- `Array.prototype.concat`
- `Array.prototype.at`
- `Array.prototype.join`
- `Array.prototype.includes`
- `Array.prototype.indexOf`
- `Array.prototype.sort`
- `Array.prototype.values`
- `Array.prototype.keys`
- `Array.prototype.entries`
- `Array.prototype.forEach`
- `Array.prototype.map`
- `Array.prototype.filter`
- `Array.prototype.find`
- `Array.prototype.findIndex`
- `Array.prototype.some`
- `Array.prototype.every`
- `Array.prototype.flat`
- `Array.prototype.flatMap`
- `Array.prototype.reduce`
- `Object.keys`
- `Object.values`
- `Object.entries`
- `Object.assign`
- `Object.fromEntries`
- `Object.hasOwn`
- `Map.prototype.get`
- `Map.prototype.set`
- `Map.prototype.has`
- `Map.prototype.delete`
- `Map.prototype.clear`
- `Map.prototype.size`
- `Map.prototype.entries`
- `Map.prototype.keys`
- `Map.prototype.values`
- `Set.prototype.add`
- `Set.prototype.has`
- `Set.prototype.delete`
- `Set.prototype.clear`
- `Set.prototype.size`
- `Set.prototype.entries`
- `Set.prototype.keys`
- `Set.prototype.values`
- `Promise.resolve`
- `Promise.reject`
- `Promise.all`
- `Promise.race`
- `Promise.any`
- `Promise.allSettled`
- `Promise.prototype.then`
- `Promise.prototype.catch`
- `Promise.prototype.finally`
- `RegExp.prototype.exec`
- `RegExp.prototype.test`
- `Date.now`
- `Date.prototype.getTime`
- `String.prototype.trim`
- `String.prototype.includes`
- `String.prototype.startsWith`
- `String.prototype.endsWith`
- `String.prototype.slice`
- `String.prototype.substring`
- `String.prototype.toLowerCase`
- `String.prototype.toUpperCase`
- `String.prototype.split`
- `String.prototype.replace`
- `String.prototype.replaceAll`
- `String.prototype.search`
- `String.prototype.match`
- `String.prototype.matchAll`
- `Math.abs`
- `Math.max`
- `Math.min`
- `Math.floor`
- `Math.ceil`
- `Math.round`
- `Math.pow`
- `Math.sqrt`
- `Math.trunc`
- `Math.sign`
- `Math.log`
- `Math.random`
- `JSON.stringify`
- `JSON.parse`
- `console.log` when the host provides a `console.log` callback
- `console.warn` when the host provides a `console.warn` callback
- `console.error` when the host provides a `console.error` callback

## Current Helper Constraints

- array callback helpers snapshot the starting array length, read element values
  live by index, and pass `(value, index, array)` plus an optional `thisArg`
- array callback helpers currently support guest callbacks, built-in callbacks,
  and promise-valued callback results reached from an async guest boundary
- synchronous host suspensions from array callback helpers fail closed with a
  runtime `TypeError`
- `Array.from` accepts the supported iterable surface and an optional
  synchronous map function plus `thisArg`; inside async guest flows, guest map
  callbacks may yield promise values for downstream helpers such as `Promise.all`
- `Array.of` always creates a fresh guest array from its arguments and does not
  expose the special single-length constructor behavior from full JavaScript
- `Array.prototype.concat` returns a fresh guest array, spreads only actual
  guest array arguments, and appends every other value as a single element
- `Array.prototype.at` truncates the requested index, supports negative offsets
  from the end, and returns `undefined` when the computed index is out of range
- `Array.prototype.splice` mutates the original array in place, returns a fresh
  guest array of removed elements, and preserves non-index array properties on
  the mutated receiver
- `Array.prototype.reduce` throws a runtime `TypeError` when called on an empty
  array without an explicit initial value
- `Array.prototype.flat` defaults to depth `1` when the argument is omitted or
  `undefined`, truncates other depth values to integers, and flattens only
  actual guest arrays
- `Array.prototype.flatMap` uses the same callback rules as the other array
  callback helpers and flattens only one returned guest-array layer
- `Array.prototype.sort` sorts in place, returns the original array value, and
  accepts either the default string ordering or a synchronous comparator
- `Object.assign` mutates and returns the original target, copies enumerable
  properties from later plain-object or array sources in the runtime's
  documented helper enumeration order, and skips `null` / `undefined` sources
- object literal spread uses the same plain-object and array source surface as
  `Object.assign`, always targets a fresh plain object, skips `null` /
  `undefined` sources, and throws a runtime `TypeError` for other source
  values
- `Object.fromEntries` accepts the supported iterable surface and expects each
  produced item to be a guest array pair
- binary `in` checks the runtime's currently exposed property surface without
  introducing user-defined prototype lookup or descriptor semantics
- for plain objects and built-in object records, `in` sees stored string-keyed
  properties plus the explicitly documented virtual members already exposed by
  the runtime
- for arrays, `in` recognizes in-bounds numeric indices, `length`, custom
  string properties, and the documented helper methods
- for `Map`, `Set`, guest iterators, promises, and the supported constructor
  functions, `in` recognizes only the members that the runtime already exposes
  directly on those values
- primitive right-hand sides for `in` fail closed with a guest-safe runtime
  `TypeError`
- `Object.create` is exposed only as an explicit runtime `TypeError` because
  full prototype semantics remain deferred
- `Object.freeze` and `Object.seal` are exposed only as explicit runtime
  `TypeError`s because property descriptor semantics remain deferred
- `String.prototype.split`, `replace`, `replaceAll`, `search`, and `match`
  accept string-coercible patterns and real `RegExp` instances
- `String.prototype.matchAll` returns a guest iterator over match-result arrays;
  `RegExp` inputs must be global
- callback replacements for `replace` / `replaceAll` are supported for guest
  callables, built-ins, and other synchronous guest-callable values
- string replacement callbacks are synchronous-only; host suspensions fail
  closed with a runtime `TypeError`
- replacement strings support `$$`, `$&`, ``$` ``, `$'`, `$1`...`$99`, and
  `$<name>` template expansion for `RegExp` matches
- `String.prototype.replaceAll` requires a global `RegExp` when the search
  value is a `RegExp`
- supported `RegExp` flags are `g`, `i`, `m`, `s`, `u`, and `y`; unsupported
  flags fail closed with a runtime `SyntaxError`
- `String.prototype.match` returns either `null`, a guest array of matched
  strings for global `RegExp` patterns, or the first-match array for
  non-global patterns, with guest-visible `index`, `input`, and optional
  `groups` properties on that result array
- `Date.now()` reads the host wall clock, `new Date(value)` currently supports
  zero arguments or exactly one numeric, string, or existing `Date` value, and
  `Date.prototype.getTime()` returns the stored epoch milliseconds
- `Math.random()` draws host entropy and returns a finite `number` in the
  half-open range `[0, 1)`; values are intentionally nondeterministic, are not
  seedable or reproducible across runs or resumes, and are not a
  cryptographically strong API contract
- direct `Date()` calls, multi-argument `new Date(...)`, and returning `Date`
  values across the structured host boundary all fail closed
- real `RegExp` instances support `source`, `flags`, `global`, `ignoreCase`,
  `multiline`, `dotAll`, `unicode`, `sticky`, `lastIndex`, `exec`, and `test`
- symbol-based match/replace protocol hooks and full ECMAScript `RegExp`
  parity remain deferred
