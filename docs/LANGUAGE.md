# Language Contract

## Baseline Rules

- Guest code always runs with strict semantics.
- `import`, `export`, dynamic `import()`, `eval`, and `Function` are rejected.
- Unsupported features fail with explicit diagnostics.

## Supported Value Types

- `undefined`
- `null`
- booleans
- numbers
- strings
- arrays
- plain objects
- guest functions

## Supported Syntax

- variable declarations with `let` and `const`
- function declarations and expressions
- arrow functions
- literals, arrays, and objects
- `if`, `switch`, `while`, `for`, `break`, and `continue`
- `try`, `catch`, `finally`, and `throw`
- common destructuring
- optional chaining
- nullish coalescing
- `async` and `await`

## Explicit Deferrals

- classes
- generators
- module loading
- full prototype semantics
- accessors
- symbols
- typed arrays
- `Date`
- `Intl`
- `Proxy`

## Built-Ins

- `globalThis`
- `Object`
- `Array`
- `String`
- `Number`
- `Boolean`
- `Math`
- `JSON`
- `Promise`
- standard `Error` types
- deterministic `console`
