# LANGUAGE_GAPS

This is an audit of the biggest gaps between the current `jslite` surface and
the kind of modern Node.js / TypeScript code that MCP-style codemode agents
commonly generate.

This document is based on:

- [README.md](/Users/mini/jslite/README.md)
- [docs/LANGUAGE.md](/Users/mini/jslite/docs/LANGUAGE.md)
- [docs/HOST_API.md](/Users/mini/jslite/docs/HOST_API.md)
- [crates/jslite/src/parser/mod.rs](/Users/mini/jslite/crates/jslite/src/parser/mod.rs)
- [crates/jslite/src/runtime/mod.rs](/Users/mini/jslite/crates/jslite/src/runtime/mod.rs)
- targeted runtime probes run on 2026-04-11

When a gap says "rejected", that means parse/validation fails before execution.
When it says "fail closed", that means the code parses but still dies at
runtime with a guest-safe error.

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
- There is no ambient browser/Web runtime. Generated code that expects `fetch`,
  `Request`, `Response`, `Headers`, `URL`, `URLSearchParams`, `TextEncoder`,
  `TextDecoder`, `WebSocket`, or DOM APIs will not run unless rewritten as host
  capabilities.
- There are no ambient timers or scheduling globals. `setTimeout`,
  `setInterval`, and `queueMicrotask` are not available.
- There is no ambient process or environment surface. Generated code that uses
  `process`, `process.env`, `global`, or `Buffer` is outside the supported
  contract.

### Syntax And Language Forms

- `class`, `extends`, and `super` are unsupported. This removes a large share
  of the object-oriented code codemode agents typically emit.
- User-defined constructor calls are unsupported even without classes.
  `new Foo()` currently fails closed unless `Foo` is one of the conservative
  built-in constructors.
- Private fields are unsupported.
- Object literal methods are unsupported. Generated code like
  `{ save() { ... } }` is rejected.
- Computed object literal keys are unsupported. Generated code like
  `{ [name]: value }` is rejected.
- Object spread is unsupported. Patterns like `{ ...defaults, override: 1 }`
  are rejected.
- Array spread is unsupported. Patterns like `[...items, extra]` are rejected.
- Spread arguments are unsupported. Calls like `fn(...args)` are rejected.
- Rest parameters are effectively unsupported in the current implementation.
  `const f = (...args) => args.length` currently reaches runtime and then fails
  because `args` is never wired up.
- Default parameters and default destructuring are only partial. They parse, but
  if the default path actually needs to execute, runtime currently throws
  `default pattern initialization at runtime requires compiled evaluation support`.
- Destructuring assignment is unsupported. Binding destructuring works in some
  places, but generated code like `[a, b] = arr` or `({ x } = obj)` is rejected.
- `var` is unsupported. Only `let` and `const` are accepted.
- Update expressions are unsupported. Common generated forms like `i++`,
  `++count`, and `index--` are rejected.
- `delete` is unsupported. Generated object cleanup like `delete obj.temp`
  cannot be used.
- `with` is unsupported.
- `for...in` is unsupported.
- `for await...of` is unsupported.
- `for...of` only supports arrays today, and only when the loop header declares
  exactly one `let` or `const` binding pattern.
- Generators and `yield` are unsupported.
- Tagged template literals are unsupported.
- `RegExp` literals are unsupported.
- `BigInt` literals are unsupported.
- Sequence expressions are unsupported.
- Array holes in literals are unsupported.
- Labeled statements are unsupported.
- `debugger` statements are unsupported.
- Meta properties are unsupported. Generated code using `new.target` or
  `import.meta` will fail.
- TypeScript syntax is not supported. Typical generated output containing type
  annotations, assertions, `satisfies`, generic instantiations, or non-null
  assertions does not run.
- JSX / TSX is not supported.

### Operators And Expression Surface

- Binary operators are limited. Common generated operators such as `**`,
  `instanceof`, `in`, bitwise operators, and shift operators are unsupported.
- Assignment operators are limited. Only `=`, `+=`, `-=`, `*=`, `/=`, and `??=`
  are implemented. Generated `||=`, `&&=`, `%=`, `**=`, and bitwise assignment
  forms are unsupported.
- Unary operators are limited. Generated code using `~value` is unsupported.

### Built-Ins And Standard Library Surface

- Arrays are extremely bare. Beyond numeric indexing, `length`, and
  `Array.isArray`, codemode-generated helpers like `push`, `pop`, `map`,
  `filter`, `reduce`, `find`, `some`, `every`, `slice`, `splice`, `concat`,
  `join`, `sort`, `includes`, `at`, `flat`, `flatMap`, `from`, and `of` are
  absent and typically fail with `value is not callable`.
- Strings are also bare. Aside from `length` and `String(...)`, generated calls
  like `split`, `trim`, `includes`, `startsWith`, `endsWith`, `slice`,
  `substring`, `replace`, `match`, `toLowerCase`, and `toUpperCase` are absent.
- Objects have plain property get/set, but almost none of the standard helper
  surface. `Object.keys`, `Object.values`, `Object.entries`, `Object.assign`,
  `Object.fromEntries`, `Object.hasOwn`, `Object.freeze`, `Object.seal`,
  `Object.create`, and descriptor APIs are absent.
- Promise support is intentionally narrow. `new Promise(...)` is unsupported.
- Promise instance methods are absent. Generated `.then(...)`, `.catch(...)`,
  and `.finally(...)` chains fail closed.
- Promise combinators are absent. Generated `Promise.all`, `Promise.race`,
  `Promise.any`, and `Promise.allSettled` do not exist.
- `Map` support is partial only. `new Map()` works, but iterable constructor
  inputs like `new Map(entries)` are unsupported.
- `Map` iteration helpers are absent. Generated `map.entries()`, `map.keys()`,
  `map.values()`, `for (const [k, v] of map)`, and `Array.from(map)` patterns
  do not work.
- `Set` support is partial only. `new Set()` works, but iterable constructor
  inputs like `new Set(values)` are unsupported.
- `Set` iteration helpers are absent. Generated `set.values()`, `set.keys()`,
  `for (const value of set)`, and `Array.from(set)` patterns do not work.
- Custom string properties on `Map` and `Set` are unsupported.
- `Math` is minimal. Only `abs`, `max`, `min`, `floor`, `ceil`, and `round`
  exist. Generated uses of `Math.random`, `Math.pow`, `Math.sqrt`,
  `Math.trunc`, `Math.sign`, `Math.log`, and similar helpers are unsupported.
- `Date` is absent. Generated code using `new Date()`, `Date.now()`, or date
  formatting helpers will not run.
- `Intl` is absent.
- `RegExp` is effectively absent. Literal syntax is rejected and there is no
  supported `RegExp` built-in surface for typical generated matching code.
- `Symbol` is absent.
- `WeakMap` and `WeakSet` are absent.
- Typed arrays, `ArrayBuffer`, `SharedArrayBuffer`, and `Atomics` are absent.
- `Proxy` and `Reflect` are absent.
- `console` is partial only. `console.log`, `console.warn`, and
  `console.error` exist only if the host explicitly supplies callbacks.
- Other `console` methods such as `info`, `debug`, `trace`, `dir`, and `table`
  are absent.
- Error objects are intentionally narrow. Generated code that expects subclassed
  errors, rich prototype behavior, or `error.stack` semantics is outside the
  current surface.

### Iteration, Collections, And Protocols

- The general iterable protocol is not implemented. AI-generated code that
  assumes "anything iterable" works with `for...of`, spread, or collection
  constructors will break.
- Strings are not iterable in `for...of`.
- Plain objects are not iterable in `for...of`.
- `Map` and `Set` are not iterable in `for...of`.
- Custom iterables and `Symbol.iterator`-based patterns are unsupported because
  symbols and public iterator APIs are deferred.
- Public iterator-producing array helpers like `values()`, `keys()`, and
  `entries()` are absent.

### Function And `this` Semantics

- The implicit `arguments` object is absent. Generated legacy-style code like
  `function f() { return arguments.length; }` fails at runtime.
- `this` semantics are incomplete. Basic free-function strict-mode behavior
  exists, but member-call `this` for guest closures is not wired like normal
  JavaScript, so generated patterns like `obj.method()` are unsafe.
- Full prototype semantics are deferred, so codegen that relies on prototype
  inheritance, `instanceof`, or method dispatch through prototypes is outside
  the supported contract.
- Accessors are unsupported, so generated getter/setter objects and classes are
  out of scope.

### Host Boundary And Interop Gaps

- The host boundary only accepts structured values: `undefined`, `null`,
  booleans, strings, numbers, arrays, and plain objects.
- Functions cannot cross the host boundary. Generated callback-based APIs or
  higher-order host interactions have to be redesigned.
- Symbols cannot cross the host boundary.
- BigInts cannot cross the host boundary.
- `Map` and `Set` cannot cross the host boundary in either direction.
- Class instances, custom prototypes, accessors, and host objects cannot cross
  the host boundary.
- Cyclic data cannot cross the host boundary.
- Generated code that assumes it can pass Dates, Buffers, typed arrays,
  streams, Errors with prototypes, or other host-native objects through a
  capability call will not work.

## Partial Support And Semantic Footguns

- Default-value patterns are a particularly sharp edge: code can compile and
  look supported, then fail only when the default branch is exercised.
- Rest parameters are another sharp edge: the parser accepts them, but the
  runtime currently does not bind the rest name.
- Member-call `this` is another sharp edge: `obj.fn()` can parse cleanly and
  still behave unlike JavaScript because guest closure calls currently ignore
  the computed receiver.
- `JSON.stringify` does not currently match normal JavaScript object key order.
  The documented and tested behavior sorts plain-object keys.
- `JSON.stringify` number formatting is also not byte-for-byte JS-compatible.
  Current tests expect outputs like `1.0` rather than `1`.
- Call-depth limits are exposed in the public API, but [docs/LIMITS.md](/Users/mini/jslite/docs/LIMITS.md)
  explicitly says they are not enforced yet. Generated recursive code can
  therefore run into a different failure profile than the public limit shape
  suggests.

## What This Means For MCP-Style Code Generation

- A generic codemode agent that targets "Node.js" or "TypeScript" will usually
  emit modules, imports, platform APIs, rich built-ins, classes, promise
  combinators, and collection helpers. `jslite` does not support that baseline.
- The current sweet spot is much narrower: script-style guest code, explicit
  host capabilities, JSON-like structured values, plain objects and arrays,
  limited `Map` / `Set`, limited built-ins, and explicit fail-closed behavior
  outside that subset.
- If `jslite` is meant to execute broader AI-generated code without manual
  rewrites, the largest compatibility wins would come from module/loading
  strategy, richer built-in method surfaces, iterable protocol support, more
  complete function parameter semantics, and a clearly expanded platform API
  story.
