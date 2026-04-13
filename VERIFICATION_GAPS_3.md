# Verification Swarm Report 3

Generated on 2026-04-12 for `/Users/mini/mustard`.

## Audit Method

- Read `AGENTS.md`, `IMPLEMENT_PROMPT.md`, `README.md`, `Cargo.toml`, and `package.json`.
- Checked the worktree with `git status --short`.
- Audited the current runtime, Node wrapper, docs, and tests before treating any gap as real.
- Established the repository baseline first.
- Launched exactly 6 read-only `gpt-5.4` `xhigh` explorer agents across fixed verification slices.
- Reproduced every surviving runtime issue locally with direct side-by-side probes before including it here.

Direct probe pattern used for parity checks:

```js
const nodeResult = vm.runInNewContext('"use strict";\n' + source, Object.create(null));
const runtimeResult = await new Mustard(source).run();
```

## Commands Run

- `git status --short`
- `cargo test --workspace`
- `npm test`
- `npm run lint`
- `npm run test:conformance`
- `npm run test:use-cases`
- Multiple narrow `node - <<'NODE'` probes using `node:vm` for strict-script Node results and `await new Mustard(source).run()` for runtime results
- Narrow existing-suite checks used to validate test gaps:
  - `node --test tests/node/builtins.test.js --test-name-pattern 'sparse array holes'`
  - `node --test tests/node/builtins.test.js --test-name-pattern 'globalThis'`
  - `node --test tests/node/builtins.test.js --test-name-pattern 'primitive-wrapper constructor semantics|built-in error constructors'`

All listed repo verification commands passed during this audit.

## Validation Slices

1. Global environment and `this`
2. Callable / function-object parity
3. Constructor and boxing semantics
4. Array callback parity with focus on `reduce`
5. `Date` and time semantics
6. Failing tests, contract drift, and docs drift

## Confirmed Runtime Root Causes

### 1. Sparse-array hole visitation is wrong for `find` and `findIndex`

Classification: confirmed runtime mismatch

Accepted behavior:
- On accepted sparse arrays, `find` and `findIndex` should visit holes as `undefined`.

Representative repros:

```js
const seen = [];
[, 2].find((value, index) => {
  seen[seen.length] = index;
  return index === 0;
});
seen;
```

- Node result: `[0]`
- Runtime result: `[1]`

```js
[, 2].findIndex((value) => value === undefined);
```

- Node result: `0`
- Runtime result: `-1`

Code path explaining it:
- `crates/mustard/src/runtime/builtins/arrays.rs:705`
- `crates/mustard/src/runtime/builtins/arrays.rs:745`

Both helpers currently skip indices when `array_has_index(...)` is false instead of invoking the callback with `undefined`.

### 2. The `Date` model is internally inconsistent and diverges on accepted inputs

Classification:
- confirmed runtime mismatch
- confirmed fail-open behavior

Accepted behavior:
- The documented `Date` subset should expose the documented prototype surface and preserve conservative but coherent `Date` semantics on accepted inputs.

Representative repros:

```js
typeof Date.prototype.getTime
```

- Node result: `"function"`
- Runtime result: `"undefined"`

```js
new Date("1970-01-01").getTime()
```

- Node result: `0`
- Runtime result: `NaN`

```js
new Date(8640000000000001).getTime()
```

- Node result: `NaN`
- Runtime result: `8640000000000001`

```js
new Date(5).valueOf()
```

- Node result: `5`
- Runtime result: `MustardRuntimeError: Error: value is not callable`

Code path explaining it:
- `crates/mustard/src/runtime/properties.rs:399`
- `crates/mustard/src/runtime/properties.rs:714`
- `crates/mustard/src/runtime/builtins/support.rs:43`
- `crates/mustard/src/runtime/builtins/primitives.rs:117`
- `crates/mustard/src/runtime/builtins/primitives.rs:145`

The runtime only special-cases `getTime` on `Date` instances, parses strings through strict RFC3339 only, does not apply Node-style `TimeClip` rejection for out-of-range numerics, and exposes `valueOf` as a raw number instead of a callable method.

### 3. Top-level function declarations are lexical-only instead of real global bindings

Classification: confirmed runtime mismatch

Accepted behavior:
- In the strict Node script context used for parity (`vm.runInNewContext('"use strict"; ... )`), top-level function declarations are reachable via `globalThis` and top-level `this`, and rebinding `globalThis.name` changes the binding.

Representative repro:

```js
function declared() { return 1; }
({
  inGlobal: "declared" in globalThis,
  viaThisType: typeof this.declared,
  same: globalThis.declared === declared,
  afterType: (globalThis.declared = 9, typeof declared),
  afterGlobal: globalThis.declared,
});
```

- Node result: `{"inGlobal":true,"viaThisType":"function","same":true,"afterType":"number","afterGlobal":9}`
- Runtime result: `{"inGlobal":false,"viaThisType":"undefined","same":false,"afterType":"function","afterGlobal":9}`

Code path explaining it:
- `crates/mustard/src/runtime/compiler/mod.rs:118`
- `crates/mustard/src/runtime/env.rs:64`
- `crates/mustard/src/runtime/env.rs:221`

Top-level function declarations are compiled into the root lexical environment, but they are only mirrored onto the global object when initialization happens in `env == self.globals`.

### 4. Callable metadata and prototype behavior diverge from both Node and the documented callable contract

Classification:
- confirmed runtime mismatch
- confirmed docs drift

Accepted behavior:
- The docs say supported guest functions and built-in callables expose `name`, `length`, `constructor`, and the usual constructible-function `.prototype` property.
- Non-constructible callables should not expose a constructible-function `.prototype`.

Representative repros:

```js
function declared() {}
const builtin = [].map;
({
  declaredType: typeof declared.constructor,
  declaredIn: 'constructor' in declared,
  arrayType: typeof Array.constructor,
  arrayIn: 'constructor' in Array,
  builtinType: typeof builtin.constructor,
  builtinIn: 'constructor' in builtin,
});
```

- Node result: `{"declaredType":"function","declaredIn":true,"arrayType":"function","arrayIn":true,"builtinType":"function","builtinIn":true}`
- Runtime result: `{"declaredType":"undefined","declaredIn":false,"arrayType":"undefined","arrayIn":false,"builtinType":"undefined","builtinIn":false}`

```js
function add(delta) { return this.base + delta; }
const builtin = [].map;
({
  guest: [typeof add.call, typeof add.apply, typeof add.bind],
  builtin: [typeof builtin.call, typeof builtin.apply, typeof builtin.bind],
  ctor: [typeof Array.call, typeof Array.apply, typeof Array.bind],
});
```

- Node result: `{"guest":["function","function","function"],"builtin":["function","function","function"],"ctor":["function","function","function"]}`
- Runtime result: `{"guest":["undefined","undefined","undefined"],"builtin":["undefined","undefined","undefined"],"ctor":["undefined","undefined","undefined"]}`

```js
async function asyncFn() {}
const method = ({ m() {} }).m;
({
  asyncType: typeof asyncFn.prototype,
  asyncOwn: Object.hasOwn(asyncFn, 'prototype'),
  methodType: typeof method.prototype,
  methodOwn: Object.hasOwn(method, 'prototype'),
});
```

- Node result: `{"asyncType":"undefined","asyncOwn":false,"methodType":"undefined","methodOwn":false}`
- Runtime result: `{"asyncType":"object","asyncOwn":true,"methodType":"object","methodOwn":true}`

```js
function f(a, b = 1, c) {}
f.length;
```

- Node result: `1`
- Runtime result: `3`

Code path explaining it:
- `crates/mustard/src/runtime/properties.rs:35`
- `crates/mustard/src/runtime/properties.rs:46`
- `crates/mustard/src/runtime/properties.rs:333`
- `crates/mustard/src/runtime/properties.rs:520`
- `crates/mustard/src/parser/patterns.rs:54`

The runtime does not expose `constructor` or any `Function.prototype`-style helpers on callables, gives non-constructible guest callables a `.prototype`, and computes `.length` from lowered temp parameters instead of stopping at the first defaulted source parameter.

### 5. Primitive autoboxing and boxed string wrappers are incomplete

Classification: confirmed runtime mismatch

Accepted behavior:
- Primitive property reads should conservatively autobox enough to preserve indexed string reads and constructor identity checks.
- Boxed string wrappers should expose the documented `String.prototype` method surface.

Representative repros:

```js
({
  idx: "ab"[0],
  strCtor: "ab".constructor === String,
  numCtor: (1).constructor === Number,
  boolCtor: true.constructor === Boolean,
});
```

- Node result: `{"idx":"a","strCtor":true,"numCtor":true,"boolCtor":true}`
- Runtime result: `{"idx":undefined,"strCtor":false,"numCtor":false,"boolCtor":false}`

```js
Object("  ab  ").trim();
```

- Node result: `"ab"`
- Runtime result: `MustardRuntimeError: Error: value is not callable`

Code path explaining it:
- `crates/mustard/src/runtime/properties.rs:743`
- `crates/mustard/src/runtime/properties.rs:898`
- `crates/mustard/src/runtime/builtins/objects.rs:219`
- `crates/mustard/src/runtime/builtins/strings.rs:4`

Primitive string reads currently expose direct string helper names but not indexed characters or primitive constructor identity, and boxed string wrapper objects do not receive the same method surface as primitive strings.

### 6. Error construction semantics diverge, and options are silently accepted then ignored

Classification:
- confirmed runtime mismatch
- confirmed fail-open behavior

Accepted behavior:
- Supported `Error` constructors should preserve conservative but coherent constructor identity and message coercion.
- Unsupported constructor options should fail closed rather than being silently dropped.

Representative repros:

```js
new Error(undefined).message;
```

- Node result: `""`
- Runtime result: `"undefined"`

```js
[new Error("x").constructor === Error, new TypeError("x").constructor === TypeError];
```

- Node result: `[true, true]`
- Runtime result: `[false, false]`

```js
new Error("boom", { cause: 1 }).cause;
```

- Node result: `1`
- Runtime result: `undefined`

Code path explaining it:
- `crates/mustard/src/runtime/conversions/errors.rs:11`
- `crates/mustard/src/runtime/properties.rs:781`
- `crates/mustard/src/runtime/builtins/primitives.rs:157`

`make_error_object(...)` stringifies an explicit `undefined` message instead of preserving the empty-message behavior, error instances map `constructor` back to `Object`, and the second `Error` argument is accepted without diagnostics even though the documented surface does not include options handling.

## Confirmed Test / Contract / Docs Drift

### A. `??=` is implemented and passing, but it is under-documented and missing from the machine-readable contract

Classification:
- confirmed docs drift
- confirmed test bug

Evidence:
- `docs/LANGUAGE.md:44`
- `docs/CONFORMANCE.md:20`
- `tests/node/conformance-contract.js:365`
- `tests/node/differential.test.js:108`
- `crates/mustard/tests/runtime_correctness.rs:8`

Representative repro:

```js
let left;
left ??= 4;
const record = { present: 5, missing: undefined };
record.present ??= 8;
record.missing ??= 9;
[left, record.present, record.missing];
```

- Node result: `[4, 5, 9]`
- Runtime result: `[4, 5, 9]`

The implementation and passing tests already prove support, but the docs list `??` while omitting `??=`, and the curated conformance contract only has entries for `||=` and `&&=`.

### B. The curated `test262` manifest labels passing parity fixtures under `cases/unsupported`

Classification: confirmed test bug

Evidence:
- `tests/test262/README.md:10`
- `tests/test262/manifest.js:11`
- `tests/test262/manifest.js:16`
- `tests/test262/manifest.js:197`

Validated command:

```sh
node -e "const m=require('./tests/test262/manifest.js'); console.log(m.pass.filter(e=>e.file.includes('cases/unsupported')).map(({id,file})=>({id,file})))"
```

Confirmed pass fixtures currently stored under `cases/unsupported`:
- `language/expressions/assignment/logical-or.js`
- `language/expressions/assignment/logical-and.js`
- `language/expressions/array/spread/basic.js`

### C. Existing sparse-array parity coverage misses the live `find` / `findIndex` hole bug

Classification: confirmed test bug

Evidence:
- `tests/node/builtins.test.js:213`
- `tests/node/differential.test.js:231`
- `tests/node/property-generators.js:618`
- `tests/node/coverage-audit.test.js:42`

Validated narrow suite:
- `node --test tests/node/builtins.test.js --test-name-pattern 'sparse array holes'`

That suite passes today, but it only covers `forEach`, `map`, `includes`, `indexOf`, enumeration, and JSON behavior for holes. It does not exercise `find` or `findIndex`.

### D. Existing `globalThis` parity tests miss top-level function declarations on the global object

Classification: confirmed test bug

Evidence:
- `tests/node/builtins.test.js:571`
- `tests/node/differential.test.js:28`

Validated narrow suite:
- `node --test tests/node/builtins.test.js --test-name-pattern 'globalThis'`

The current test only checks top-level `this`, `globalThis`, direct name lookup, and arrow lexical `this`. It does not check whether top-level function declarations are reflected onto `globalThis`.

### E. The array-callback docs over-generalize `reduce`

Classification: confirmed docs drift

Evidence:
- `docs/LANGUAGE.md:398`
- `crates/mustard/src/runtime/builtins/arrays.rs:892`
- `tests/node/builtins.test.js:765`

Representative repro:

```js
[1].reduce(function (acc, value, index, array) {
  return { thisType: typeof this, acc, value, index, arrayLength: array.length };
}, 7, { ignored: true });
```

- Node result: `{"thisType":"undefined","acc":7,"value":1,"index":0,"arrayLength":1}`
- Runtime result: same

The shared callback-helper docs say helpers pass `(value, index, array)` plus optional `thisArg`, but `reduce` actually passes `(accumulator, value, index, array)` and does not take a `thisArg`.

### F. The `Date` docs overstate accepted string support and understate current `undefined` handling

Classification: confirmed docs drift

Evidence:
- `docs/LANGUAGE.md:477`
- `README.md:452`
- `crates/mustard/src/runtime/builtins/support.rs:43`
- `crates/mustard/src/runtime/builtins/primitives.rs:150`

Representative repros:

```js
new Date("1970-01-01").getTime()
```

- Node result: `0`
- Runtime result: `NaN`

```js
new Date(undefined).getTime()
```

- Node result: `NaN`
- Runtime result: `NaN`

The docs currently say `new Date(value)` accepts a `string`, but the implementation only parses strict RFC3339. They also exclude `undefined` from the accepted argument set even though the runtime accepts it and produces `NaN`.

### G. The callable docs claim `constructor` exposure that the runtime does not provide

Classification: confirmed docs drift

Evidence:
- `docs/LANGUAGE.md:434`
- `crates/mustard/src/runtime/properties.rs:46`
- `crates/mustard/src/runtime/properties.rs:333`
- `crates/mustard/src/runtime/properties.rs:520`

Representative repro:

```js
function declared() {}
typeof declared.constructor;
```

- Node result: `"function"`
- Runtime result: `"undefined"`

## Rejected / Not Confirmed

- `Array.prototype.reduce` runtime parity itself was not confirmed as broken. Direct probes for initial seeding, leading-hole seeding, length snapshotting, live future-index reads, and shrink-during-iteration matched Node.
- No separate runtime mismatch was confirmed for `new Date(undefined)`. The issue there is documentation drift, not an observable Node/runtime divergence.
- No additional root cause was accepted solely from stale planning artifacts or unchecked checklist items. Every entry above was validated against live code, tests, and direct probes.
