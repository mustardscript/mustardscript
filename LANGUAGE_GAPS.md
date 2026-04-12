# LANGUAGE_GAPS

This is an audit of the biggest gaps between the current `jslite` surface and
the kind of modern Node.js / TypeScript code that MCP-style codemode agents
commonly generate.

This document is based on:

- `README.md`
- `docs/LANGUAGE.md`
- `docs/HOST_API.md`
- `crates/jslite/src/parser.rs`
- `crates/jslite/src/runtime`
- `tests/node/basic.test.js`
- `tests/node/async-runtime.test.js`
- `tests/node/iteration.test.js`
- targeted runtime probes run on 2026-04-11

When a gap says "rejected", that means parse or validation fails before
execution. When it says "fail closed", that means the code parses but still
dies at runtime with a guest-safe error.

## Already Landed Since Earlier Audits

The runtime already supports several surfaces that older audits used to list as
missing:

- rest parameters
- `for...of` over arrays, strings, `Map`, `Set`, and supported iterator helper
  objects
- `BigInt` literals plus exact integer arithmetic inside the guest runtime
- `RegExp` literals, `RegExp(...)`, `new RegExp(...)`, `exec`, `test`, and the
  documented string and regexp interop helpers
- `new Promise(executor)`, promise instance methods, and promise combinators
- iterable `Map` and `Set` constructors plus `entries()` / `keys()` /
  `values()`
- conservative array callback helpers plus `Array.from`
- conservative string helpers such as `split`, `replace`, `match`,
  `toLowerCase`, and `toUpperCase`
- `Object.keys`, `Object.values`, `Object.entries`, `Object.fromEntries`, and
  `Object.hasOwn`
- `Math.pow`, `Math.sqrt`, `Math.trunc`, and `Math.sign`
- `Date.now()` and `new Date(value).getTime()`

## Highest-Probability Gaps For AI-Generated Code

### Modules, Packages, And Platform APIs

- ES module syntax is unsupported. Typical generated code like
  `import { z } from 'zod'` or `export async function run()` is rejected.
- CommonJS is unsupported. Free `require`, `module`, and `exports` are rejected
  or unavailable, so generated code like `const fs = require('node:fs')` does
  not run.
- Dynamic `import()` is explicitly rejected.
- Dynamic code loading is unsupported. Free `eval(...)` and `new Function(...)`
  are rejected or unavailable.
- There is no package resolution or npm compatibility layer. Even if a parser
  accepted an import string, `jslite` still has no module loader.
- There is no ambient Node standard library. Generated references to `fs`,
  `path`, `os`, `crypto`, `stream`, `events`, `url`, `buffer`,
  `child_process`, and similar APIs are outside the runtime surface.
- There is no ambient browser or Web runtime. Generated code that expects
  `fetch`, `Request`, `Response`, `Headers`, `URL`, `URLSearchParams`,
  `TextEncoder`, `TextDecoder`, `WebSocket`, or DOM APIs will not run unless
  rewritten as host capabilities.
- There are no ambient timers or scheduling globals. `setTimeout`,
  `setInterval`, and `queueMicrotask` are not available.
- There is no ambient process or environment surface. Generated code that uses
  `process`, `process.env`, `global`, or `Buffer` is outside the supported
  contract.

### Syntax And Language Forms

- `class`, `extends`, and `super` are unsupported.
- User-defined constructor calls are unsupported even without classes.
  `new Foo()` fails closed unless `Foo` is one of the conservative built-in
  constructors.
- Private fields are unsupported.
- Object literal accessors are unsupported.
- Array spread is supported for arrays, strings, `Map`, `Set`, and supported
  iterator objects. Other inputs fail closed at runtime with the documented
  iterable-surface `TypeError`.
- Spread arguments are supported for the same documented iterable surface and
  fail closed with the same runtime `TypeError` on unsupported inputs.
- Default parameters are supported for function declarations, expressions, and
  arrow functions.
- Default destructuring is supported in parameter scope, declarations, and
  `catch` bindings.
- Destructuring assignment is supported for identifiers, members, and the
  documented loop-header assignment targets.
- `var` is rejected during validation. This is a deliberate v1 contract
  decision: the runtime supports only lexical `let` / `const` bindings and
  does not emulate hoisting or legacy redeclaration.
- Update expressions are supported for identifiers and member expressions.
- `delete` is rejected during validation. Plain-object and array deletion stay
  intentionally unsupported until own-property absence, sparse-array behavior,
  and descriptor/configurability semantics are chosen explicitly.
- `with` is unsupported.
- `for...in` now works for plain objects and arrays only, using the same key
  order as `Object.keys(...)` and the same header surface as the documented
  `for...of` subset.
- `for await...of` is supported for the documented synchronous iterable
  surface by awaiting each yielded value inside async functions.
- `for...of` supports the documented iterable surface plus single-binding
  `let` / `const` declaration headers plus identifier, member, and
  conservative destructuring assignment-target headers.
- Generators and `yield` are unsupported.
- Tagged template literals are unsupported.
- Sparse array holes are now supported across literals, property access, the
  documented helper surface, JSON, and structured host-boundary round trips.
- Labeled statements are unsupported.
- `debugger` statements are unsupported.
- Meta properties such as `new.target` and `import.meta` are unsupported.
- TypeScript syntax is not supported. Typical generated output containing type
  annotations, assertions, `satisfies`, generic instantiations, or non-null
  assertions does not run.
- JSX and TSX are not supported.

### Operators And Expression Surface

- Binary operators are still limited. `**`, conservative `in`, and
  conservative `instanceof` now exist, but bitwise operators and shift
  operators are still rejected during validation.
- Assignment operators are limited. Only `=`, `+=`, `-=`, `*=`, `/=`, `||=`,
  `&&=`, and `??=` are implemented. Generated `%=`, `**=`, and bitwise
  assignment forms are still rejected during validation.
- Unary operators are limited. Generated code using `~value` is unsupported.

### Built-Ins And Standard Library Surface

- Arrays are no longer bare. The current surface already includes `push`,
  `pop`, `slice`, `splice`, `join`, `includes`, `indexOf`, `sort`, `values`,
  `keys`, `entries`, `forEach`, `map`, `filter`, `find`, `findIndex`, `some`,
  `every`, `flat`, `flatMap`, `reduce`, and `Array.from`.
- Strings are no longer bare. The current surface includes `trim`, `includes`,
  `startsWith`, `endsWith`, `slice`, `substring`, `toLowerCase`,
  `toUpperCase`, `split`, `replace`, `replaceAll`, `search`, `match`, and
  `matchAll`.
- Objects already support `Object.keys`, `Object.values`, `Object.entries`,
  `Object.assign`, `Object.fromEntries`, and `Object.hasOwn`.
- Object helpers still missing from the documented surface include
  `Object.freeze`, `Object.seal`, `Object.create`, and descriptor APIs.
- `Object.create`, `Object.freeze`, and `Object.seal` now fail closed with
  explicit runtime `TypeError`s because prototype and descriptor semantics are
  still deferred.
- Promise support is no longer intentionally narrow. `new Promise(...)`,
  `.then(...)`, `.catch(...)`, `.finally(...)`, `Promise.resolve(...)`,
  `Promise.reject(...)`, `Promise.all(...)`, `Promise.race(...)`,
  `Promise.any(...)`, and `Promise.allSettled(...)` are implemented for the
  documented subset.
- Full ECMAScript promise parity is still out of scope. Hostile thenable
  cycles, exotic Promise subclassing, and the rest of the edge-case matrix are
  not the target surface.
- `Map` and `Set` support is broader than older audits claimed. Iterable
  constructors plus `entries()`, `keys()`, `values()`, `get`, `set`, `add`,
  `has`, `delete`, `clear`, and `size` are implemented.
- Custom string properties on `Map` and `Set` are still unsupported.
- `Math` is broader than older audits claimed. `abs`, `max`, `min`, `floor`,
  `ceil`, `round`, `pow`, `sqrt`, `trunc`, and `sign` exist.
- `Math.random` now exists but is intentionally narrow: it returns a finite
  host-generated `number` in `[0, 1)`, makes no reproducibility guarantees,
  and is not a cryptographic API contract.
- Most of the wider `Math` surface is still unsupported.
- `Date` is partial rather than absent. `Date.now()` plus
  `new Date(value).getTime()` work, but broader constructor overloads and full
  instance method parity are deferred.
- `Intl` is absent.
- `RegExp` is partial rather than absent. Literal syntax, constructor support,
  `exec`, `test`, and documented string-helper integration work, but full
  ECMAScript `RegExp` parity remains deferred.
- `Symbol` is absent.
- `WeakMap` and `WeakSet` are absent.
- Typed arrays, `ArrayBuffer`, `SharedArrayBuffer`, and `Atomics` are absent.
- `Proxy` and `Reflect` are absent.
- `console` is partial only. `console.log`, `console.warn`, and
  `console.error` exist only if the host explicitly supplies callbacks.
- Other `console` methods such as `info`, `debug`, `trace`, `dir`, and `table`
  are absent.
- Error objects are intentionally narrow. Generated code that expects rich
  prototype behavior or exact `error.stack` parity is outside the current
  surface.

### Iteration, Collections, And Protocols

- The full symbol-based iterable protocol is not implemented.
- Arrays, strings, `Map`, `Set`, and the documented iterator helper objects are
  iterable in the current surface.
- Spread syntax is still only partially supported: array spread and spread
  arguments work for the documented iterable surface, object spread works for
  documented plain-object and array sources, and custom symbol-based iterables
  remain unsupported.
- Plain objects are not iterable in `for...of`.
- Custom iterables and `Symbol.iterator`-based patterns are unsupported because
  symbols and the public iterator protocol remain deferred.

### Function And `this` Semantics

- The implicit `arguments` object is absent.
- Member calls for non-arrow guest functions now bind the computed receiver as
  `this`.
- Arrow functions capture lexical `this` from the surrounding supported guest
  frame, but this does not imply support for `call`, `apply`, `bind`, or full
  prototype-heavy function semantics.
- Full prototype inheritance semantics are still deferred, so codegen that
  relies on user-defined prototype chains or method dispatch through
  prototypes is outside the supported contract.
- The remaining prototype deferral is deliberate rather than accidental:
  `jslite` now exposes enough constructor/prototype identity for conservative
  `instanceof`, but not the full JavaScript prototype model.
- Accessors are unsupported, so generated getter and setter objects and classes
  are out of scope.

### Host Boundary And Interop Gaps

- The host boundary only accepts structured values: `undefined`, `null`,
  booleans, strings, numbers, arrays, and plain objects. Structured arrays may
  now be sparse, and hole positions round-trip across the boundary.
- Functions cannot cross the host boundary.
- Symbols cannot cross the host boundary.
- BigInts cannot cross the host boundary.
- `Map` and `Set` cannot cross the host boundary in either direction.
- `Date`, `RegExp`, class instances, custom prototypes, accessors, and host
  objects cannot cross the host boundary.
- Cyclic data cannot cross the host boundary.
- Generated code that assumes it can pass Dates, Buffers, typed arrays,
  streams, Errors with prototypes, or other host-native objects through a
  capability call will not work.

## Partial Support And Semantic Footguns

- Default parameters and default destructuring are now part of the supported
  surface. The older validation-reject and runtime-only fallback notes from
  earlier audits no longer apply.
- `var` is not a temporary parser gap. The v1 contract deliberately keeps only
  lexical bindings, so legacy hoisting and redeclaration behavior remain out of
  scope.
- `delete` is not a temporary parser gap. Plain-object and array deletion stay
  rejected until the runtime deliberately chooses an absence/sparse-array model;
  supported `Map.prototype.delete` and `Set.prototype.delete` methods are a
  separate collection API surface.
- `for...of` is narrower than full JavaScript: declaration headers still must
  declare exactly one `let` or `const` binding, and `for...in` is still
  limited to plain objects and arrays.
- Object spread is narrower than full JavaScript: plain-object and array
  sources work, `null` / `undefined` are skipped, and other source values fail
  closed instead of following full boxing and coercion rules.
- `in` intentionally checks only the runtime's currently exposed property
  surface. It does not introduce full prototype walking, descriptor semantics,
  or a reflective `globalThis` mirror of every global binding.
- `instanceof` is now supported conservatively for the documented
  constructors, wrapper objects, and callable `Object` checks without implying
  full class or descriptor semantics.
- Array callback helpers and `Array.from` mapping fail closed when a callback
  would cause a synchronous host suspension.
- `JSON.stringify` on plain objects now follows JavaScript own-property order:
  array-index keys first in numeric order, then remaining string keys in
  insertion order.
- `Object.keys`, `Object.values`, and `Object.entries` on plain objects follow
  the same JavaScript own-property order as `JSON.stringify`.

## What This Means For MCP-Style Code Generation

- A generic codemode agent that targets "Node.js" or "TypeScript" will still
  emit modules, imports, platform APIs, classes, prototype-heavy code, wider
  built-in surfaces, and unsupported operators. `jslite` does not support that
  baseline.
- The current sweet spot is still narrower: script-style guest code, explicit
  host capabilities, JSON-like structured values, plain objects and arrays,
  supported keyed collections, conservative built-ins, async guest promises,
  and explicit fail-closed behavior outside that subset.
- If `jslite` is meant to execute broader AI-generated code without manual
  rewrites, the biggest compatibility wins from here are richer array and
  object built-ins, broader loop forms, wider platform API shims, and any
  future work on full prototype inheritance or class-related semantics.
