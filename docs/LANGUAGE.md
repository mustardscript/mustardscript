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
- arrow functions
- literals, arrays, and objects
- `if`, `switch`, `while`, `do...while`, `for`, `break`, and `continue`
- `return`
- common destructuring
- assignment to identifiers and member expressions
- member access, calls, and `new` for supported built-ins
- template literals
- optional chaining
- nullish coalescing
- named host capability calls

## Parsed But Not Yet Executable

- `throw`
- `try`, `catch`, and `finally`
- `async` functions
- `await`

## Rejected With Validation Diagnostics

- `import`, `export`, and dynamic `import()`
- free `eval` and free `Function`
- free references to `process`, `module`, `exports`, `global`, `require`,
  `setTimeout`, `setInterval`, `queueMicrotask`, and `fetch`
- `with`
- classes
- generators and `yield`
- `for...in` and `for...of`
- `debugger`
- labeled statements
- object spread and object-literal methods
- array spread and array holes
- bigint, regexp literals, and sequence expressions

## Explicit Deferrals

- guest-visible `Error` objects and standard error types
- deterministic console callbacks
- async runtime and promises
- full `this` semantics beyond the current basic function-call behavior
- iterator protocol support
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
- Current traceback precision is function-level span data rather than
  exact-expression locations.
- Guest-facing rendering does not include host paths, internal filenames, or
  Rust implementation details.

## Built-Ins and Global Names

- `globalThis`
- `Object`
- `Array`
- `String`
- `Number`
- `Boolean`
- `Math`
- `JSON`
- `console` as a placeholder global object with no callable methods yet

### Currently Implemented Built-In Members

- `Array.isArray`
- `Math.abs`
- `Math.max`
- `Math.min`
- `Math.floor`
- `Math.ceil`
- `Math.round`
- `JSON.stringify`
- `JSON.parse`
