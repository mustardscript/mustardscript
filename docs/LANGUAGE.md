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
- guest functions

## Supported End-to-End Syntax

- variable declarations with `let` and `const`
- function declarations and expressions
- `async` function declarations and expressions
- arrow functions
- `await` inside async functions
- literals, arrays, and objects
- `if`, `switch`, `while`, `do...while`, `for`, array `for...of`, `break`,
  and `continue`
- `return`
- `throw`, `try`, `catch`, and `finally`
- common destructuring
- assignment to identifiers and member expressions
- member access, calls, and `new` for supported built-ins
- template literals
- optional chaining
- nullish coalescing
- named host capability calls

## Supported Async Surface

- async functions return guest promise values
- `await` suspends the current async continuation onto the runtime microtask
  queue
- host capability calls inside async guest code return guest promises and still
  suspend through the existing `start()` / `resume()` boundary
- `Promise.resolve(...)` and `Promise.reject(...)` are available
- unsupported Promise construction and instance methods still fail closed at
  runtime

## Supported Iteration Surface

- `for...of` is currently supported only when the header declares exactly one
  `let` or `const` binding pattern
- only guest arrays are iterable in the current surface
- header patterns can use the same identifier, array, and object destructuring
  forms already supported elsewhere in the runtime
- each `for...of` iteration gets a fresh lexical binding environment for the
  loop header bindings
- array iteration yields values in ascending numeric index order and ignores
  non-index properties
- active array iterators observe the live backing array length, so elements
  appended before exhaustion are visited in order
- array iterator helper methods such as `values()`, `keys()`, and `entries()`
  are still absent from the built-in surface and therefore fail closed when
  called
- unsupported iterable inputs such as strings, plain objects, promises, and
  custom iterables throw a runtime `TypeError`
- abrupt completion from `break`, `continue`, `return`, or `throw` discards the
  internal array iterator state with no user-visible iterator-close hook because
  generators and custom iterators remain deferred

## Rejected With Validation Diagnostics

- `import`, `export`, and dynamic `import()`
- `delete`
- free `eval` and free `Function`
- free references to `process`, `module`, `exports`, `global`, `require`,
  `setTimeout`, `setInterval`, `queueMicrotask`, and `fetch`
- `with`
- classes
- generators and `yield`
- `for...in`
- `for await...of`
- `for...of` forms that do not declare exactly one `let` or `const` binding
- `debugger`
- labeled statements
- object spread and object-literal methods
- array spread and array holes
- bigint, regexp literals, and sequence expressions

## Explicit Deferrals

- full Promise constructor semantics and promise instance methods
- full `this` semantics beyond the current basic function-call behavior
- non-array iterable protocol support
- public iterator-producing APIs and custom iterator authoring
- module loading
- property descriptor semantics
- full prototype semantics
- accessors
- symbols
- typed arrays
- `Date`
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

- The only currently supported observable property-order surface is
  `JSON.stringify` plus array `for...of`.
- `JSON.stringify` on plain objects renders string keys in sorted key order.
- `JSON.stringify` on arrays renders elements in ascending numeric index order.
- Non-index array properties are ignored by `JSON.stringify`.
- Array `for...of` yields values in ascending numeric index order.
- Enumeration APIs such as `Object.keys` and `for...in` remain unsupported.

## Built-Ins and Global Names

- `globalThis`
- `Object`
- `Array`
- `Promise`
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
- `Promise.resolve`
- `Promise.reject`
- `Math.abs`
- `Math.max`
- `Math.min`
- `Math.floor`
- `Math.ceil`
- `Math.round`
- `JSON.stringify`
- `JSON.parse`
- `console.log` when the host provides a `console.log` callback
- `console.warn` when the host provides a `console.warn` callback
- `console.error` when the host provides a `console.error` callback
