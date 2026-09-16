---
title: "Language Contract"
description: "Describes the implemented language surface, supported value types, syntax, and built-in methods"
category: "Language & Runtime"
order: 1
slug: "language"
lastUpdated: "2026-04-13"
---

# Language Contract

This document describes the currently implemented language surface. Planned
extensions are called out explicitly instead of being implied.

## Baseline Rules

- Guest code always runs with strict semantics.
- Input has no module system; `import` and `export` syntax is rejected even
  though top-level `await` is supported.
- Unsupported features fail closed with explicit diagnostics.
- Free references to forbidden ambient globals are rejected when lexical
  resolution proves they are unresolved.
- Free `eval` and free `Function` are rejected for the same reason.
- `typeof` an unresolvable non-forbidden identifier returns `"undefined"`;
  uninitialized lexical bindings still throw `ReferenceError`. This exception
  does not suppress errors while evaluating member expressions or bypass the
  forbidden-ambient-global validation policy.
- Hosts may opt into `lenientMode` at compile time for generated snippets. In
  that mode, only a final top-level `return <expr>;` is accepted, and it is
  treated exactly like the final expression result of the script. Other
  top-level returns still fail closed.

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
- conservative `Intl.DateTimeFormat` and `Intl.NumberFormat` formatter objects
- guest functions

## Supported End-to-End Syntax

- variable declarations with `let` and `const`
- function declarations and expressions, including rest parameters and default
  parameter initializers
- `async` function declarations and expressions
- arrow functions
- `await` inside async functions and at top level
- literals, arrays, and plain-object literals with static keys, computed keys,
  method shorthand, and spread from plain objects or arrays
- `if`, `switch`, `while`, `do...while`, `for`, `for...of`,
  `for await...of`, `for...in`, `break`, and `continue`
- `return`
- `throw`, `try`, `catch`, and `finally`
- common destructuring, including conservative default initializers
- assignment to identifiers, member expressions, and conservative
  array/object destructuring targets
- sequence expressions
- member access, calls, and `new` for supported built-ins
- template literals
- optional chaining
- nullish coalescing
- update expressions
- binary `**`, `in`, and conservative `instanceof`
- named host capability calls

By default, `return` is accepted only inside function bodies. With the Node
compile option `{ lenientMode: true }`, a final root statement such as
`return answer;` is normalized to `answer;` for compatibility with generated
snippets. This does not add script-level early-return control flow: `return`
inside top-level `if`, loop, block, or any non-final root statement is still
rejected.

## Supported Function Call Surface

- non-arrow guest member calls bind the computed receiver as `this`
- arrow functions capture lexical `this` from the surrounding supported guest
  frame, including top-level script execution and ordinary guest member calls
- rest parameters are supported for function declarations, function
  expressions, and arrow functions
- `new` remains limited to the documented conservative built-in constructors

## Supported Async Surface

- async functions return guest promise values
- top-level `await` is supported by running the root script body as an internal
  async continuation only when top-level await syntax is present
- `await` suspends the current async continuation onto the runtime microtask
  queue
- `for await...of` is supported inside async functions over the documented
  iterable surface by awaiting each yielded value before the loop body runs
- host capability calls inside async guest code, including code reached from
  top-level `await`, return guest promises and still suspend through the
  existing `start()` / `resume()` boundary
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
- assignment-target headers support identifiers, members, and the same
  conservative array/object destructuring forms supported by ordinary
  assignment
- each `for...of` iteration gets a fresh lexical binding environment for
  declaration headers; assignment-target headers reuse the existing binding or
  member reference each iteration
- array iteration yields values in ascending numeric index order, treats holes
  as `undefined`, and ignores non-index properties
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
- `delete` of bindings and compound optional chains
- free `arguments`
- free `eval` and free `Function`
- free references to `process`, `module`, `exports`, `global`, `require`,
  `setTimeout`, `setInterval`, `queueMicrotask`, and `fetch`
- `with`
- classes
- generators and `yield`
- `var`, `using`, and `await using`
- `for...of` declaration headers that do not declare exactly one `let` or
  `const` binding, and declaration initializers in `for...of` / `for...in`
  headers
- `debugger`
- labeled statements
- object literal accessors

## Explicit Deferrals

- fully general Promise constructor and thenable-adoption edge cases,
  including hostile thenable cycles
- unsupported assignment operators such as the bitwise and shift assignment
  families
- full `this` semantics beyond the current basic function-call behavior
- implicit `arguments` object semantics
- legacy `var` hoisting, same-scope redeclaration, and loop interaction rules
- symbol-based custom iterable protocol support
- custom iterator authoring beyond the documented collection helpers
- module loading
- property descriptor semantics
- full prototype semantics
- accessors
- symbols
- typed arrays
- full `Date` parity beyond the documented conservative subset
- full `Intl` parity beyond the documented conservative subset
- `Proxy`

## Deliberate V1 Decisions

- `var` is intentionally out of scope for v1. The supported binding surface is
  lexical `let` / `const` only, so the runtime does not emulate function or
  global hoisting, same-scope redeclaration, or legacy loop-scoping behavior.
- `delete` removes own data properties from plain objects and arrays. Array
  index deletion creates a hole without changing length. Missing properties
  return `true`; delete/reinsert puts non-index keys at the end of enumeration.
  Static/computed references are evaluated once. A single optional member
  short-circuits without evaluating its key; compound optional chains fail closed
  until the chain IR can preserve their boundaries. Binding deletion is rejected
  during validation. Array `length` and string indices/`length` are non-configurable
  and throw `TypeError`; deletion on special objects (including `globalThis`),
  callable values, and collections remains outside the supported surface.
  See [ADR 0002](ADRs/0002-language-completion.md).
- `instanceof` is intentionally conservative in v1. The supported surface is
  constructor-instance checks for the runtime's built-in instance kinds,
  conservative primitive-wrapper objects, and `Object` checks over the
  runtime's callable values. Guest-function constructors are accepted on the
  right-hand side but currently return `false` because `new` on guest
  functions and full prototype-link semantics remain deferred.

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
- Sparse array holes are supported in literals and array storage. Reading a
  hole yields `undefined`, `Object.keys` / `Object.values` / `Object.entries`
  / `for...in` skip missing indices, array iteration treats holes as
  `undefined`, and `JSON.stringify` renders holes as `null`.
- Non-index array properties are ignored by `JSON.stringify`.
- `JSON.stringify(value, replacer, space)` supports callable replacers, ordered
  string/number property lists (including boxed values), and numeric/string
  indentation capped at ten characters. It invokes supported `toJSON` methods
  before the replacer and unboxes primitive wrappers afterwards. Replacer calls
  receive `(key, value)` with the containing object as `this`, including the root
  holder under key `""`. Keys/array length are snapshotted, but values are read
  live so callback mutations are observable.
- `JSON.parse(text, reviver)` invokes callable revivers in postorder with the
  containing object as `this`. Returning `undefined` removes a property or leaves
  an array hole; the root result can be replaced or removed. Invalid JSON throws
  guest `SyntaxError`. Non-callable revivers follow ECMAScript's ignore rule.
- JSON callbacks propagate guest exceptions and reject synchronous host
  suspension. Traversals meter work, protect live values from GC during callbacks,
  and reject nesting deeper than 128. The existing Unicode-scalar string contract
  also applies to indentation-string truncation; arbitrary UTF-16 strings are not
  silently approximated.
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
  present numeric indices in ascending order followed by custom string
  properties in insertion order.
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
- `SyntaxError`
- `EvalError`
- `URIError`
- `AggregateError`
- `Number`
- `Boolean`
- `Intl`
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
- `Array.prototype.shift`
- `Array.prototype.unshift`
- `Array.prototype.toSorted`
- `Array.prototype.toReversed`
- `Array.prototype.toSpliced`
- `Array.prototype.with`
- `Array.prototype.copyWithin`
- `Array.prototype.slice`
- `Array.prototype.splice`
- `Array.prototype.concat`
- `Array.prototype.at`
- `Array.prototype.join`
- `Array.prototype.includes`
- `Array.prototype.indexOf`
- `Array.prototype.lastIndexOf`
- `Array.prototype.reverse`
- `Array.prototype.fill`
- `Array.prototype.sort`
- `Array.prototype.values`
- `Array.prototype.keys`
- `Array.prototype.entries`
- `Array.prototype.forEach`
- `Array.prototype.map`
- `Array.prototype.filter`
- `Array.prototype.find`
- `Array.prototype.findIndex`
- `Array.prototype.findLast`
- `Array.prototype.findLastIndex`
- `Array.prototype.some`
- `Array.prototype.every`
- `Array.prototype.flat`
- `Array.prototype.flatMap`
- `Array.prototype.reduce`
- `Array.prototype.reduceRight`
- `Object.keys`
- `Object.values`
- `Object.entries`
- `Object.assign`
- `Object.fromEntries`
- `Object.hasOwn`
- `Object.groupBy`
- `Map.groupBy`
- `Map.prototype.get`
- `Map.prototype.set`
- `Map.prototype.has`
- `Map.prototype.delete`
- `Map.prototype.clear`
- `Map.prototype.size`
- `Map.prototype.entries`
- `Map.prototype.keys`
- `Map.prototype.values`
- `Map.prototype.forEach`
- `Set.prototype.add`
- `Set.prototype.has`
- `Set.prototype.delete`
- `Set.prototype.clear`
- `Set.prototype.size`
- `Set.prototype.entries`
- `Set.prototype.keys`
- `Set.prototype.values`
- `Set.prototype.forEach`
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
- `Date.prototype.toISOString`
- `Date.prototype.toJSON`
- `Date.prototype.getUTCFullYear`
- `Date.prototype.getUTCMonth`
- `Date.prototype.getUTCDate`
- `Date.prototype.getUTCHours`
- `Date.prototype.getUTCMinutes`
- `Date.prototype.getUTCSeconds`
- `Number.parseInt`
- `Number.parseFloat`
- `Number.isNaN`
- `Number.isFinite`
- `Number.isInteger`
- `Number.isSafeInteger`
- `Number.prototype.toFixed`
- `Number.prototype.toExponential`
- `Number.prototype.toPrecision`
- global `parseInt`
- global `parseFloat`
- global `isNaN`
- global `isFinite`
- global `NaN`
- global `Infinity`
- `String.prototype.trim`
- `String.prototype.trimStart`
- `String.prototype.trimEnd`
- `String.prototype.includes`
- `String.prototype.startsWith`
- `String.prototype.endsWith`
- `String.prototype.indexOf`
- `String.prototype.lastIndexOf`
- `String.prototype.charAt`
- `String.prototype.at`
- `String.prototype.slice`
- `String.prototype.substring`
- `String.prototype.toLowerCase`
- `String.prototype.toUpperCase`
- `String.prototype.repeat`
- `String.prototype.concat`
- `String.prototype.padStart`
- `String.prototype.padEnd`
- `String.prototype.split`
- `String.prototype.replace`
- `String.prototype.replaceAll`
- `String.prototype.search`
- `String.prototype.match`
- `String.prototype.matchAll`
- `Intl.DateTimeFormat`
- `Intl.NumberFormat`
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
- `Math.exp`
- `Math.log2`
- `Math.log10`
- `Math.sin`
- `Math.cos`
- `Math.atan2`
- `Math.hypot`
- `Math.cbrt`
- `Math.random`
- `JSON.stringify`
- `JSON.parse`
- `console.log` when the host provides a `console.log` callback
- `console.warn` when the host provides a `console.warn` callback
- `console.error` when the host provides a `console.error` callback

## Current Helper Constraints

- array literals plus call and constructor argument lists support spread over
  the documented iterable surface (`Array`, `String`, `Map`, `Set`, and
  supported iterator objects); array holes iterate as `undefined`, and other
  inputs fail closed with `TypeError: value is not iterable in the supported surface`
- array callback helpers snapshot the starting array length, read element values
  live by index, honor live `array.length` truncation for later visits, and
  pass `(value, index, array)` plus an optional `thisArg`
- array callback helpers currently support guest callbacks, built-in callbacks,
  and promise-valued callback results reached from an async guest boundary
- synchronous host suspensions from array callback helpers fail closed with a
  runtime `TypeError`
- `Array.from` accepts supported iterables, boxed strings, and array-like inputs.
  Array-like length is captured once and truncated/clamped; indexed values are
  read live, and absent positions become own `undefined` elements. The optional
  synchronous mapper accepts `thisArg` and bound callbacks. Inside async guest
  flows, guest map callbacks may yield promises for helpers such as `Promise.all`.
  Nullish sources, invalid mappers, lengths beyond the array range, and lengths
  exceeding heap headroom fail explicitly before oversized backing allocations.
- `Array.of` always creates a fresh guest array from its arguments and does not
  expose the special single-length constructor behavior from full JavaScript
- `Array(...)` and `new Array(...)` follow JavaScript's single-length
  constructor behavior for one numeric argument, including explicit
  `RangeError` rejection for invalid lengths
- `Array.prototype.concat` returns a fresh guest array, spreads only actual
  guest array arguments, and appends every other value as a single element
- `Array.prototype.at` truncates the requested index, supports negative offsets
  from the end, and returns `undefined` when the computed index is out of range
- `Array.prototype.lastIndexOf` mirrors the conservative equality and hole
  treatment used by `indexOf`, but searches from right to left
- `Array.prototype.reverse` mutates the original array in place, preserves hole
  positions, and returns the same array value
- `Array.prototype.fill` mutates the original array in place over the requested
  start/end range and preserves holes outside that range
- `shift`/`unshift` mutate queue ends and preserve shifted holes. `toSorted`,
  `toReversed`, `toSpliced`, and `with` return fresh dense arrays (holes become
  `undefined`) without copying extra properties. `with` supports negative indices
  and throws `RangeError` out of range. `copyWithin` mutates with overlap-safe
  copying, preserving source holes and array length. These prototype helpers,
  like the existing array methods, require actual arrays as their receivers.
- Sorting is stable, places `undefined` after defined values without passing it
  to comparators, preserves collected values through comparator mutations/GC,
  and respects the snapshotted range. Work and heap growth remain budgeted.
- `Array.prototype.splice` mutates the original array in place, returns a fresh
  guest array of removed elements, and preserves non-index array properties on
  the mutated receiver
- `Array.prototype.reduce` throws a runtime `TypeError` when called on an empty
  array without an explicit initial value
- `Array.prototype.reduceRight` follows the same callback and empty-array
  failure rules as `reduce`, but traverses present elements from right to left
- `Array.prototype.findLast` and `findLastIndex` follow the same callback
  rules as `find` / `findIndex`, including hole visits as `undefined`, but
  traverse from right to left
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
- `Object(value)` preserves existing supported object-like guest values and
  boxes primitive strings, numbers, and booleans into conservative wrapper
  objects; boxed strings expose the documented string helper surface, while
  boxed numbers expose the documented number helper surface and boxed booleans
  expose `toString()` / `valueOf()`
- supported guest functions, bound functions, and built-in callables expose
  own `name` / `length`, inherit `constructor` plus `call` / `apply` / `bind`
  from the shared callable surface, and expose the usual
  constructible-function `.prototype` property where applicable; guest
  functions and supported constructor values also support custom enumerable
  string-keyed properties, and anonymous guest functions infer names from
  supported binding and property-key writes
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
- `Map.prototype.forEach` and `Set.prototype.forEach` support synchronous guest
  callbacks plus `thisArg`, visit entries appended during active iteration, and
  fail closed for synchronous host suspensions
- `String.prototype.split`, `replace`, `replaceAll`, `search`, and `match`
  accept string-coercible patterns and real `RegExp` instances
- primitive strings expose `length`, numeric index reads, `constructor`,
  `toString()`, and `valueOf()` in the conservative helper surface; primitive
  numbers expose `constructor`, `toString()`, `valueOf()`, `toFixed()`,
  `toExponential()`, and `toPrecision()`; `Number.prototype.toString`
  supports radix arguments from 2 through 36; primitive booleans expose
  `constructor`, `toString()`, and `valueOf()`
- `String.prototype.trimStart`, `trimEnd`, `padStart`, and `padEnd` are
  supported on primitive strings and boxed `String` wrappers in the
  conservative helper surface
- `String.prototype.indexOf`, `lastIndexOf`, `charAt`, `at`, `repeat`, and
  `concat` are available on primitive strings in the conservative helper surface
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
- supported `RegExp` flags are `d`, `g`, `i`, `m`, `s`, `u`, and `y`; unsupported
  flags fail closed with a runtime `SyntaxError`
- `String.prototype.match` returns either `null`, a guest array of matched
  strings for global `RegExp` patterns, or the first-match array for
  non-global patterns, with guest-visible `index`, `input`, and optional
  `groups` properties on that result array
- `Date.now()` reads the host wall clock as integral epoch milliseconds,
  `new Date(value)` currently supports zero arguments or exactly one numeric,
  string, or existing `Date` value, supported string inputs are currently
  `YYYY-MM-DD` plus RFC3339 timestamps with `Z` or explicit numeric UTC
  offsets, `Date.prototype.getTime()` returns the stored integral epoch
  milliseconds, `toISOString()` and `toJSON()` render UTC RFC3339 timestamps
  across the full ECMAScript time-clip range with signed six-digit years when
  required, and the documented `getUTC*` accessors expose UTC
  year/month/day/hour/minute/second fields while returning `NaN` for invalid
  dates
- `Number.parseInt`, `Number.parseFloat`, `Number.isNaN`,
  `Number.isFinite`, `Number.isInteger`, and `Number.isSafeInteger` are
  available as conservative static helpers on `Number`; the corresponding
  global aliases plus `NaN` / `Infinity` are also installed on `globalThis`
- `Number.MAX_SAFE_INTEGER`, `MIN_SAFE_INTEGER`, `EPSILON`, `MAX_VALUE`,
  `MIN_VALUE`, `POSITIVE_INFINITY`, `NEGATIVE_INFINITY`, and `NaN` are exposed
  as conservative static constants on `Number`
- `Number.prototype.toFixed`, `toExponential`, and `toPrecision` support
  fraction/precision arguments in the ECMAScript 0..100 / 1..100 ranges;
  `Number.prototype.toString` supports radix arguments from 2 through 36
- `Intl.DateTimeFormat` and `Intl.NumberFormat` are available in a narrow
  deterministic subset: locale support is currently limited to `en-US`,
  `DateTimeFormat` currently accepts only `UTC` time-zone formatting plus the
  documented numeric / two-digit date-time fields, formats hour-bearing output
  with the default `en-US` 12-hour clock plus `AM` / `PM`, and rejects any
  other option keys explicitly; `NumberFormat` currently supports only
  `decimal`, `percent`, and `currency` formatting with `USD` as the only
  supported currency code and rejects any other option keys explicitly
- `Math.PI`, `E`, `LN2`, `LN10`, `LOG2E`, `LOG10E`, `SQRT2`, and `SQRT1_2`
  are available as numeric constants on `Math`
- `Math.exp`, `log2`, `log10`, `sin`, `cos`, `atan2`, `hypot`, and `cbrt`
  are available as pure numeric helpers over the supported number surface
- `Math.random()` draws host entropy and returns a finite `number` in the
  half-open range `[0, 1)`; values are intentionally nondeterministic, are not
  seedable or reproducible across runs or resumes, and are not a
  cryptographically strong API contract
- structured host arrays may be sparse; hole positions are preserved across the
  boundary in both directions up to 1,000,000 elements
- direct `Date()` calls, multi-argument `new Date(...)`, locale-specific date
  strings outside the documented `Date` parsing surface, unsupported `Intl`
  locales or options, and returning `Date` values across the structured host
  boundary all fail closed
- real `RegExp` instances support `source`, `flags`, `global`, `ignoreCase`,
  `multiline`, `dotAll`, `unicode`, `sticky`, `hasIndices`, `lastIndex`, `exec`, and `test`
- symbol-based match/replace protocol hooks and full ECMAScript `RegExp`
  parity remain deferred

## Error objects

All eight standard error constructors are available with call/new, constructor
metadata, built-in `instanceof`, optional `cause`, and `toString()` behavior.
`AggregateError(errors, message, options)` copies the supported iterable into its
`errors` array; non-iterables fail closed. `Promise.any` uses the same visible kind.
Malformed JSON raises `SyntaxError`. Error metadata (`name`, `message`, `stack`,
`cause`, `errors`) is excluded from enumerable-key helpers and JSON's default keys.

Every error captures a deterministic guest-only `stack` string at creation:
`Name: message` followed by guest function names and source-span offsets. It never
adds Rust frames, host filenames, process details or native addresses. The stack
is ordinary snapshot-preserved data, not a host `Error` object. Full mutable
prototype-chain and property-descriptor semantics remain deferred.

## Grouping

`Object.groupBy(items, callback)` and `Map.groupBy(items, callback)` consume the
supported iterable surface and call the callback with `(value, index)` and an
undefined receiver. Bound callbacks work; exceptions propagate, synchronous host
suspension rejects, and work/allocation limits apply throughout.

Object grouping coerces keys to supported property keys and returns a prototype-less
plain data object. Missing `constructor`, `toString`, and `hasOwnProperty` really are
absent; `__proto__` is ordinary data. These objects support deletion, enumeration,
spread/assignment, JSON, snapshots and the structured data boundary. The host boundary
carries their own data, not guest prototype metadata. Full prototype manipulation
remains deferred. Map grouping preserves key identity and SameValueZero semantics,
including NaN and canonical positive-zero keys. Bucket values retain input order;
Map keys retain first occurrence order, and Object keys follow own-key ordering.

### Object compatibility and explicit string conversion

`Object.is` uses SameValue equality (NaN equals itself; positive and negative zero
are distinct). `Object.hasOwn` and `Object.prototype.hasOwnProperty` accept
non-nullish primitives as well as supported objects. String indices/length are own
properties; inherited methods and Map/Set size are not. Plain objects inherit
`hasOwnProperty` and `toString`; prototype-less grouping results do not. Own values,
including an explicit `undefined`, shadow these methods.

`Object.prototype.toString.call(value)` returns the supported built-in type tag.
`Array.prototype.toString` calls the receiver's callable `join`, or falls back to
Object's type tag. Ordinary arrays join with commas; nullish values, holes, and
cyclic references contribute empty strings. Array string conversion is bounded by
work/output limits and a 128-array nesting limit (RangeError). User-defined coercion
hooks on nested elements and arbitrary prototype-chain mutation remain unsupported.

### URI encoding and decoding

The pure globals `encodeURI`, `encodeURIComponent`, `decodeURI`, and
`decodeURIComponent` implement UTF-8 percent encoding/decoding. URI variants keep
URI-reserved punctuation; component variants encode/decode it. Decoding does not
turn plus signs into spaces and preserves reserved escape spelling in `decodeURI`.
Malformed escapes, overlong UTF-8, surrogate encodings, and out-of-range code points
throw `URIError`. Work and output allocation are budgeted. These functions operate
on the existing well-formed Unicode string surface; URL and URLSearchParams remain
host APIs, not guest globals.

### Unicode character APIs and normalization

`normalize()` supports NFC (default), NFD, NFKC and NFKD; invalid form names throw
RangeError. Normalization uses pinned Unicode data in the Rust dependency and
meters decomposition buffering and combining-mark sorting. `isWellFormed()` is
true for all supported strings.

The existing string profile is **Unicode scalar based**, not arbitrary UTF-16:
`'🙂'.length` is 1, and indexing, slicing and regexp offsets count scalars. The new
`charCodeAt(index)` and `codePointAt(index)` deliberately expose ECMAScript's
**UTF-16 numeric view**: `'🙂'.charCodeAt(0)` is 55357,
`'🙂'.codePointAt(0)` is 128578, and both at index 1 return the low surrogate 56898.
Out-of-range results are NaN and undefined respectively.

`String.fromCharCode(...units)` wraps numbers to unsigned 16-bit units and accepts
valid surrogate pairs. `String.fromCodePoint(...points)` accepts scalar values up
to 0x10FFFF. Both reject lone-surrogate results with RangeError; fromCodePoint also
rejects non-integers and out-of-range values. Lone-surrogate source literals,
template fragments and property keys are validation errors, and lone-surrogate
host strings/keys are boundary TypeErrors. They are never silently replaced.


### Linear regexp character classes and match indices

`\w`, `\d`, `\b` and their complements use ECMAScript ASCII word/digit
semantics (including inside bracket classes). `\s` uses ECMAScript whitespace,
and dot excludes all four ECMAScript line terminators unless `s` is set. The `iu`
word class also includes long-s (ſ) and Kelvin sign (K); legacy `i` folding does
not incorrectly fold those characters into ASCII. Character classes are lowered
structurally through the Rust regex-syntax parser, not by text substitution.

The linear engine cannot express the special `iu` word boundary for ſ/K.
Matching a pattern containing `\b`/`\B` with `iu` against input containing
those characters therefore throws TypeError instead of returning a wrong result.
Lookaround/backreferences, inline engine flags and non-ECMAScript nested/POSIX or
set-operation class syntax remain explicit rejections. Existing non-UTF-16 regexp
semantics and other bounded engine differences are not a full ECMAScript engine.

The `d` flag adds `indices`, an array of `[start, end]` pairs (undefined for an
unmatched capture), plus `indices.groups` for named captures. Named pairs share
identity with their indexed capture pair; group dictionaries have no prototype.
Offsets use the runtime's existing **Unicode scalar** contract. `matchAll` starts
at a regexp's lastIndex without mutating it; empty exec matches leave lastIndex at
the match end. Flags are exposed in canonical order. Compilation, matching and
index construction are budgeted, and the native regexp cache/engine sizes are bounded.
