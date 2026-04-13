# Verification Gaps

Status: resolved on the current branch as of April 12, 2026. This file is
retained as the audit input that drove the fixes below; the listed gaps were
confirmed when the audit was written and then closed in the follow-up changes.

This document records the currently confirmed gaps where `mustard` either:

- does not match Node.js execution for behavior it currently accepts, or
- does not fail closed and instead returns the wrong observable result.

Audit date: April 12, 2026.

Resolution verification after the fixes:

- `cargo test --workspace`
- `npm test`
- `npm run lint`
- `npm run test:conformance`

## Method

- Audited the current implementation before writing anything:
  - `crates/mustard/src/runtime`
  - `crates/mustard-node/src/lib.rs`
  - `index.js`
  - `lib/runtime.js`
  - `lib/structured.js`
  - `lib/progress.js`
  - `docs/LANGUAGE.md`
  - `LANGUAGE_GAPS.md`
  - `tests/node/*`
- Ran the primary repo checks:
  - `cargo test --workspace`
  - `npm test`
  - `npm run lint`
- Ran additional verification:
  - `npm run test:conformance`
  - `npm run test:test262`
  - `npm run test:use-cases`
  - `npm run test:hardening`
- Ran targeted Node-vs-`mustard` differential probes.
- Validated the audit with 6 independent `gpt-5.4` `xhigh` explorer agents, each owning one slice:
  - global environment and `this`
  - callable / function-object parity
  - constructor and boxing semantics
  - array callback / `reduce` semantics
  - `Date` semantics
  - failing tests, contract drift, and docs drift

For the targeted differential probes, the Node baseline was strict-script execution via:

```js
vm.runInNewContext('"use strict";\n' + source, Object.create(null))
```

The `mustard` baseline was:

```js
await new Mustard(source).run()
```

## Confirmed Runtime Gaps

### 1. `globalThis` is not the real global object

Relevant implementation paths:

- `crates/mustard/src/runtime/builtins/install.rs:166-239`
- `crates/mustard/src/runtime/env.rs:8-21`
- `crates/mustard/src/runtime/env.rs:49-78`
- `crates/mustard/src/runtime/properties.rs:341-395`
- `crates/mustard/src/runtime/properties.rs:557-620`
- `crates/mustard/src/runtime/vm.rs:708-712`

Confirmed mismatches:

- `globalThis.globalThis === globalThis`
  - Node: `true`
  - `mustard`: `false`
- `globalThis.Object === Object`
  - Node: `true`
  - `mustard`: `false`
- `'Object' in globalThis`
  - Node: `true`
  - `mustard`: `false`
- `typeof globalThis.Object`
  - Node: `"function"`
  - `mustard`: `"undefined"`
- `globalThis.answer = 7; ({ prop: globalThis.answer, lookup: answer })`
  - Node: `{ prop: 7, lookup: 7 }`
  - `mustard`: `ReferenceError: \`answer\` is not defined`

This means global bindings currently live in the lexical environment, while the exposed `globalThis` object is a separate object with different contents. Some probes fail closed, but others silently return the wrong value.

### 2. Top-level `this` and arrow lexical `this` do not match Node

Relevant implementation paths:

- `crates/mustard/src/runtime/vm.rs:688-724`
- `crates/mustard/src/runtime/vm.rs:708-712`
- `crates/mustard/src/runtime/compiler/expressions.rs`

Confirmed mismatches:

- `this === globalThis`
  - Node: `true`
  - `mustard`: `false`
- `const obj = { value: 3, method() { return (() => this.value)(); } }; obj.method();`
  - Node: `3`
  - `mustard`: `TypeError: cannot read properties of nullish value`

The root cause is that arrow frames currently force `this` to `undefined` instead of capturing lexical `this`, and the top-level execution model does not expose the real global object as `this`.

### 3. Callables are invocable values, not JavaScript objects

Relevant implementation paths:

- `crates/mustard/src/runtime/state.rs:25-40`
- `crates/mustard/src/runtime/properties.rs:341-555`
- `crates/mustard/src/runtime/properties.rs:557-620`
- `crates/mustard/src/runtime/conversions/operators.rs:163-216`
- `crates/mustard/src/runtime/builtins/install.rs:173-240`

Confirmed mismatches for guest functions:

- `const fn = function named(a, b) {}; ({ name: fn.name, length: fn.length, prototypeType: typeof fn.prototype, instanceofObject: fn instanceof Object })`
  - Node: `{ name: "named", length: 2, prototypeType: "object", instanceofObject: true }`
  - `mustard`: `{ name: undefined, length: undefined, prototypeType: "undefined", instanceofObject: false }`
- `const fn = (a, b) => {}; ({ name: fn.name, length: fn.length, instanceofObject: fn instanceof Object })`
  - Node: `{ name: "fn", length: 2, instanceofObject: true }`
  - `mustard`: `{ name: undefined, length: undefined, instanceofObject: false }`
- `function f() {} f.answer = 1; f.answer`
  - Node: `1`
  - `mustard`: `TypeError: value is not an object`
- `Object.keys(function f() {})`
  - Node: `[]`
  - `mustard`: `TypeError: Object helpers currently only support plain objects and arrays`

Confirmed mismatches for built-in constructors and extracted methods:

- `({ name: Array.name, length: Array.length, prototypeType: typeof Array.prototype, instanceofObject: Array instanceof Object })`
  - Node: `{ name: "Array", length: 1, prototypeType: "object", instanceofObject: true }`
  - `mustard`: `{ name: undefined, length: undefined, prototypeType: "undefined", instanceofObject: false }`
- `Object.hasOwn(Array, "length")`
  - Node: `true`
  - `mustard`: throws the same `Object helpers currently only support plain objects and arrays` error
- `const method = [].map; ({ name: method.name, length: method.length, instanceofObject: method instanceof Object })`
  - Node: `{ name: "map", length: 1, instanceofObject: true }`
  - `mustard`: `{ name: undefined, length: undefined, instanceofObject: false }`

Confirmed missing constructor links:

- `[].constructor === Array`
- `({}).constructor === Object`
- `new Date(0).constructor === Date`
- `/a/.constructor === RegExp`
- `Promise.resolve(1).constructor === Promise`

All of those are `true` in Node and `false` in `mustard`.

This is not one isolated bug. The current runtime model separates closures and built-ins from the ordinary object system, so function metadata, property storage, helper interoperability, and prototype-adjacent observables all break together.

### 4. Constructor and boxing semantics fail open

Relevant implementation paths:

- `crates/mustard/src/runtime/builtins/install.rs:10-47`
- `crates/mustard/src/runtime/builtins/install.rs:127-148`
- `crates/mustard/src/runtime/builtins/arrays.rs:24-30`
- `crates/mustard/src/runtime/builtins/primitives.rs:111-145`
- `crates/mustard/src/runtime/builtins/primitives.rs:160-176`
- `crates/mustard/src/runtime/vm.rs:759-772`
- `crates/mustard/src/runtime/conversions/operators.rs:163-216`

Confirmed `Array` / `new Array` mismatches:

- `Array(3)` and `new Array(3)`
  - Node: holey array of length `3`
  - `mustard`: one-element array `[3]`
- `new Array(-1)`, `new Array(2.5)`, and `new Array(4294967296)`
  - Node: `RangeError: Invalid array length`
  - `mustard`: one-element arrays containing `-1`, `2.5`, and `4294967296`
- `Object.keys(new Array(3))`
  - Node: `[]`
  - `mustard`: `["0"]`
- `JSON.stringify(new Array(3))`
  - Node: `"[null,null,null]"`
  - `mustard`: `"[3]"`

Confirmed `Object(value)` mismatches:

- `Object(array) === array`
  - Node: `true`
  - `mustard`: `false`
- `Object(new Map([[1, 2]])) === map`
  - Node: `true`
  - `mustard`: `false`
- `Array.isArray(Object(array))`
  - Node: `true`
  - `mustard`: `false`
- `String(Object("ab"))`
  - Node: `"ab"`
  - `mustard`: `"[object Object]"`
- `String(Object(1))`
  - Node: `"1"`
  - `mustard`: `"[object Object]"`
- `Object("ab")` in `mustard` has no boxed-string surface:
  - `'length' in boxed` is `false`
  - `boxed[0] === undefined` is `true`

Confirmed wrapper-constructor mismatches:

- `typeof new Number(1)`
  - Node: `"object"`
  - `mustard`: `"number"`
- `typeof new String("ab")`
  - Node: `"object"`
  - `mustard`: `"string"`
- `typeof new Boolean(false)`
  - Node: `"object"`
  - `mustard`: `"boolean"`
- `!!new Boolean(false)`
  - Node: `true`
  - `mustard`: `false`
- `new Number(1) instanceof Number`
- `new String("ab") instanceof String`
- `new Boolean(false) instanceof Boolean`
  - Node: `true` for all three
  - `mustard`: `TypeError: right-hand side of instanceof must be a supported constructor`

These paths accept programs that should either behave like Node or reject explicitly. Today they silently produce the wrong values or the wrong object kinds.

### 5. `Array.prototype.reduce` binds the initial accumulator as callback `this`

Relevant implementation paths:

- `crates/mustard/src/runtime/builtins/arrays.rs:497-505`
- `crates/mustard/src/runtime/builtins/arrays.rs:873-928`

Confirmed mismatches:

- `const seed = { tag: "seed" }; [1].reduce(function (acc) { return { same: this === acc, thisTag: this && this.tag, accTag: acc.tag }; }, seed);`
  - Node: `{ same: false, accTag: "seed" }`
  - `mustard`: `{ same: true, thisTag: "seed", accTag: "seed" }`
- `const seed = { tag: "seed" }; const result = [1].reduce(function (acc) { this.extra = 1; return acc; }, seed); ({ result, seed });`
  - Node: `{ result: { tag: "seed" }, seed: { tag: "seed" } }`
  - `mustard`: `{ result: { tag: "seed", extra: 1 }, seed: { tag: "seed", extra: 1 } }`

The shared array-callback helper always reads `args[1]` as `thisArg`, but `reduce` also uses `args[1]` as the initial accumulator. That leaks a wrong observable `this` value and allows mutation through the wrong alias.

### 6. `Date` currently accepts and preserves fractional milliseconds

Relevant implementation paths:

- `crates/mustard/src/runtime/builtins/support.rs:36-46`
- `crates/mustard/src/runtime/builtins/primitives.rs:111-145`
- `crates/mustard/src/runtime/builtins/install.rs:127-133`

Confirmed mismatches:

- `const value = Date.now(); [value, value === Math.trunc(value)]`
  - Node: integer epoch milliseconds
  - `mustard`: fractional milliseconds, second element `false`
- `const value = new Date().getTime(); [value, value === Math.trunc(value)]`
  - Node: integer epoch milliseconds
  - `mustard`: fractional milliseconds, second element `false`
- `const value = new Date(1.9).getTime(); [value, value === Math.trunc(value)]`
  - Node: `[1, true]`
  - `mustard`: `[1.9, false]`
- `const value = new Date(-1.9).getTime(); [value, value === Math.trunc(value)]`
  - Node: `[-1, true]`
  - `mustard`: `[-1.9, false]`
- `const value = new Date("2026-04-10T14:00:00.123456789Z").getTime(); [value, value === Math.trunc(value)]`
  - Node: `[1775829600123, true]`
  - `mustard`: `[1775829600123.4568, false]`
- `const value = new Date("2026-04-10T14:00:00.1239Z").getTime(); [value, value === Math.trunc(value)]`
  - Node: `[1775829600123, true]`
  - `mustard`: `[1775829600123.9, false]`

This is accepted, documented surface. It is neither Node-parity nor fail-closed.

## Runtime Gap Pattern

The confirmed runtime gaps cluster into a small number of root causes:

- the runtime does not model the real global object and lexical global bindings as the same surface
- callables are not ordinary objects
- constructor behavior is approximated instead of matching Node or rejecting
- one helper path (`reduce`) reuses the wrong callback contract
- `Date` timestamps keep fractional millisecond precision instead of clipping to integer milliseconds

Most additional user-visible examples are symptoms of those same root causes.

## Verification Contract Drift

These are real repository gaps, but they are not new runtime correctness bugs. They are stale tests, stale conformance metadata, or stale docs.

### 1. `coverage-audit` has stale manual coverage mappings

Relevant paths:

- `tests/node/coverage-audit.test.js:529-550`
- `tests/node/conformance-contract.js`
- `tests/node/language-gaps.test.js`
- `tests/node/differential.test.js`

Confirmed issue:

- The audit requires every `COVERAGE.EXISTING` / `COVERAGE.AUDIT` entry to appear in `MANUAL_CONFORMANCE_BUCKETS`.
- The mapping is missing these current parity/manual entries:
  - `validation.default-parameters`
  - `validation.default-destructuring`
  - `validation.destructuring-assignment`
  - `validation.update-expressions`
  - `validation.unsupported-binary`
  - `validation.instanceof-guest-function`

This is why `npm run test:conformance` currently fails.

### 2. `conformance-contract.js` still marks a parity feature as a rejection regression

Relevant paths:

- `tests/node/conformance-contract.js:536-700`
- `tests/node/conformance-contract.js:725-747`
- `tests/node/rejection-contract.test.js`
- `crates/mustard/src/runtime/conversions/operators.rs:163-216`

Confirmed issue:

- `validation.instanceof-guest-function` is currently a parity feature.
- The same file still assigns rejection metadata to it and still includes it in curated rejection regressions.
- That makes `tests/node/rejection-contract.test.js` attempt to run rejection assertions for an entry that no longer has a rejection source shape.

This is a contract/test bug, not a runtime rejection bug.

### 3. `property-progress-lifecycle.test.js` has a harness bug

Relevant paths:

- `tests/node/property-progress-lifecycle.test.js:224-269`
- `lib/progress.js:26-31`
- `lib/progress.js:141-146`
- `docs/HOST_API.md`
- `tests/node/security-progress-load.test.js`

Confirmed issue:

- The harness processes `load-same` and `load-explicit` before checking whether the progress token has already been consumed.
- The minimized failing sequence is `resume -> load-same`.
- The runtime correctly rejects the second use as single-use, but the property harness currently expects success.

This is a test bug, not a wrapper/runtime bug.

### 4. `LANGUAGE_GAPS.md` is stale

Relevant paths:

- `LANGUAGE_GAPS.md:85-125`
- `LANGUAGE_GAPS.md:231-260`
- `docs/LANGUAGE.md:40-52`
- `docs/LANGUAGE.md:262-272`
- `tests/node/language-gaps.test.js`
- `tests/node/coverage-audit.test.js`

Confirmed drift:

- It still says default parameters are rejected.
- It still says default destructuring is rejected.
- It still says destructuring assignment is rejected.
- It still says update expressions are rejected.
- It still says `instanceof` remains intentionally rejected.
- It still says plain-object `JSON.stringify` / `Object.keys` order does not match JavaScript and is sorted.

Those claims no longer match the implementation, the passing tests, or `docs/LANGUAGE.md`.

## Current Verification Status

Commands run during this audit:

- `cargo test --workspace` -> pass
- `npm run lint` -> pass
- `npm run test:test262` -> pass
- `npm run test:use-cases` -> pass
- `npm run test:hardening` -> pass
- `npm test` -> fail
- `npm run test:conformance` -> fail

Current failing test areas:

- `tests/node/coverage-audit.test.js`
- `tests/node/rejection-contract.test.js`
- `tests/node/property-progress-lifecycle.test.js`

No additional confirmed runtime bug surfaced from the contract-drift slice beyond the runtime gaps already listed above.
