# Verification Gaps 2

Audit date: April 12, 2026.

This swarm reran the current-branch verification baseline, validated candidates
with six `gpt-5.4` `xhigh` explorer agents, and kept only issues that were
locally reproduced or directly corroborated in code.

## Audit Method

- Read:
  - `AGENTS.md`
  - `IMPLEMENT_PROMPT.md`
  - `README.md`
  - `Cargo.toml`
  - `package.json`
- Inspected the worktree with `git status --short --branch`.
  - Dirty before the audit and intentionally left untouched:
    - `TESTING_GAPS.md`
    - `SECURITY_ISSUES_5.md`
- Audited the current implementation in:
  - `index.js`
  - `lib/runtime.js`
  - `lib/progress.js`
  - `tests/node/runtime-oracle.js`
  - `tests/node/differential.test.js`
  - `tests/node/builtins.test.js`
  - `tests/node/language-gaps.test.js`
  - `crates/jslite/src/runtime/{vm.rs,mod.rs,env.rs,properties.rs,builtins/*.rs,conversions/*.rs}`
  - `docs/LANGUAGE.md`
  - `docs/CONFORMANCE.md`
  - `docs/HOST_API.md`
- Baseline commands run:
  - `cargo test --workspace`
  - `npm test`
  - `npm run lint`
  - `npm run test:conformance`
  - `npm run test:use-cases`
  - `npm run test:hardening`
- Additional focused verification run:
  - `node --test --test-name-pattern='aligns globalThis|exposes callable metadata and constructor links|matches supported Array, Object, and primitive-wrapper constructor semantics|keeps Array.prototype.reduce callback this undefined|reduce callback this and Date millisecond truncation|conservative instanceof checks' tests/node/builtins.test.js tests/node/differential.test.js tests/node/language-gaps.test.js`
  - direct `node` probes comparing:

```js
vm.runInNewContext('"use strict";\n' + source, Object.create(null))
```

    against:

```js
await new Jslite(source).run()
```

- Validation slices used:
  1. Global environment and `this`
  2. Callable / function-object parity
  3. Constructor and boxing semantics
  4. Array callback parity with focus on `reduce`
  5. `Date` and time semantics
  6. Failing tests, contract drift, and docs drift

## Confirmed Runtime Gaps

### 1. Top-level function declarations do not become `globalThis` properties

Accepted behavior:

`jslite` currently accepts top-level function declarations. In strict-script
Node, those bindings are also visible as properties on the real global object.

Representative repro:

```js
function declared() { return 1; }
({
  same: globalThis.declared === declared,
  inGlobal: 'declared' in globalThis,
  type: typeof globalThis.declared,
});
```

Node result:

```js
{ same: true, inGlobal: true, type: "function" }
```

`jslite` result:

```js
{ same: false, inGlobal: false, type: "undefined" }
```

Code path:

- `crates/jslite/src/runtime/vm.rs:5-20` runs the root script in a fresh child
  environment of `globals`
- `crates/jslite/src/runtime/compiler/mod.rs:118-132` emits top-level
  `FunctionDeclaration`s into that child scope
- `crates/jslite/src/runtime/env.rs:64-85` only mirrors bindings installed
  through `define_global()` onto the actual global object

The same root cause also reproduces with `async function declared() {}`.

### 2. Supported callables do not expose `.constructor`

Accepted behavior:

The runtime accepts guest and built-in callables, and `docs/LANGUAGE.md:434-436`
currently says supported callables expose `constructor`.

Representative repro:

```js
function declared() {}
({
  declaredType: typeof declared.constructor,
  builtinType: typeof Array.constructor,
  declaredIn: 'constructor' in declared,
  builtinIn: 'constructor' in Array,
});
```

Node result:

```js
{ declaredType: "function", builtinType: "function", declaredIn: true, builtinIn: true }
```

`jslite` result:

```js
{ declaredType: "undefined", builtinType: "undefined", declaredIn: false, builtinIn: false }
```

Code path:

- `crates/jslite/src/runtime/properties.rs:46-70` `closure_own_property()`
  handles `name`, `length`, and `prototype`, but not `constructor`
- `crates/jslite/src/runtime/properties.rs:328-380`
  `builtin_function_own_property()` also omits `constructor`
- `crates/jslite/src/runtime/properties.rs:520-523` makes closure/built-in
  `'in'` checks depend on those same helpers, so both property access and
  `'constructor' in callable` fail

### 3. Anonymous function values in object-literal properties lose inferred names

Accepted behavior:

Anonymous function and arrow values assigned to an object-literal property name
should pick up that property name as their `name`.

Representative repro:

```js
const box = { task: function () {} };
const arrowBox = { task: () => {} };
({ fn: box.task.name, arrow: arrowBox.task.name });
```

Node result:

```js
{ fn: "task", arrow: "task" }
```

`jslite` result:

```js
{ fn: "", arrow: "" }
```

Code path:

- `crates/jslite/src/parser/expressions.rs:153-159` only infers a name for
  method syntax (`task() {}`), not for `task: function () {}` or
  `task: () => {}`
- the lowered runtime then preserves the empty-name callable unchanged

### 4. Callable string coercion collapses to `[Function]`

Accepted behavior:

Accepted callable values are already observable through `String(...)`, and on
the supported subset the current runtime accepts those programs but returns a
lossy placeholder instead of Node's function-source-like strings.

Representative repro:

```js
function f() {}
({ guest: String(f), builtin: String(Array) });
```

Node result:

```js
{ guest: "function f() {}", builtin: "function Array() { [native code] }" }
```

`jslite` result:

```js
{ guest: "[Function]", builtin: "[Function]" }
```

Code path:

- `crates/jslite/src/runtime/conversions/coercions.rs:133-135` maps every
  `Value::Closure`, `Value::BuiltinFunction`, and `Value::HostFunction` to the
  fixed string `"[Function]"`

### 5. Primitive autoboxed property reads are incomplete

Accepted behavior:

The runtime already exposes conservative primitive-wrapper behavior elsewhere
(`Object("ab")`, `new String`, `new Number`, `new Boolean`), so primitive
property reads that the evaluator already accepts should behave consistently
instead of silently collapsing to `undefined`.

Representative repro:

```js
({
  indexMatches: "ab"[0] === "a",
  stringCtorType: typeof "ab".constructor,
  numberCtorType: typeof (1).constructor,
  boolCtorType: typeof true.constructor,
});
```

Node result:

```js
{
  indexMatches: true,
  stringCtorType: "function",
  numberCtorType: "function",
  boolCtorType: "function"
}
```

`jslite` result:

```js
{
  indexMatches: false,
  stringCtorType: "undefined",
  numberCtorType: "undefined",
  boolCtorType: "undefined"
}
```

Code path:

- `crates/jslite/src/runtime/properties.rs:898-917` only exposes `length` and
  string helper methods on primitive `Value::String`, not indexed characters or
  `constructor`
- `crates/jslite/src/runtime/properties.rs:922` falls through to `undefined`
  for primitive numbers and booleans instead of conservative wrapper-style
  property reads

### 6. The accepted `Date` surface diverges from Node in method exposure and string parsing

Accepted behavior:

`jslite` accepts `new Date(value)` and `Date` instances, so supported Date
properties should remain method-shaped. The current docs also say
`new Date(value)` supports exactly one numeric, string, or existing `Date`
argument, but the implementation only parses RFC3339 strings successfully.

Representative repro A:

```js
const date = new Date(5);
let call;
try {
  call = date.valueOf();
} catch (error) {
  call = error.name + ": " + error.message;
}
({ type: typeof date.valueOf, call });
```

Node result:

```js
{ type: "function", call: 5 }
```

`jslite` result:

```js
{ type: "number", call: "Error: value is not callable" }
```

Representative repro B:

```js
const value = new Date("2026-04-10").getTime();
({ isNaN: value !== value, value });
```

Node result:

```js
{ isNaN: false, value: 1775779200000 }
```

`jslite` result:

```js
{ isNaN: true, value: NaN }
```

Code path:

- `crates/jslite/src/runtime/properties.rs:712-716` returns
  `Value::Number(date.timestamp_ms)` for `valueOf` instead of a callable method
- `crates/jslite/src/runtime/builtins/support.rs:43-46` parses date strings
  only with `Rfc3339`
- `crates/jslite/src/runtime/builtins/primitives.rs:143-149` funnels every
  string `new Date(value)` argument through that RFC3339-only parser

Note:

The older fractional-millisecond bug from `VERIFICATION_GAPS.md` is resolved;
current RFC3339 inputs truncate to integral milliseconds correctly.

## Confirmed Test / Contract / Docs Drift

### 1. `docs/LANGUAGE.md` still describes `reduce` like the other array callbacks

File refs:

- `docs/LANGUAGE.md:398-399`
- `crates/jslite/src/runtime/builtins/arrays.rs:892-930`
- `tests/node/builtins.test.js:765-780`
- `tests/node/differential.test.js:572-590`

Observed behavior:

The doc says array callback helpers pass `(value, index, array)` plus optional
`thisArg`.

Confirmed current runtime behavior:

`reduce` correctly passes `(accumulator, value, index, array)` and keeps
`this === undefined`, matching Node.

Representative repro:

```js
const seed = { tag: "seed" };
[1].reduce(function (acc, value, index, array) {
  return {
    thisType: typeof this,
    same: this === acc,
    accTag: acc.tag,
    value,
    index,
    arrayLength: array.length,
  };
}, seed);
```

Node result:

```json
{"thisType":"undefined","same":false,"accTag":"seed","value":1,"index":0,"arrayLength":1}
```

`jslite` result: identical

Classification: standalone docs drift

This is the only docs-only finding from the swarm. The callable `.constructor`
and generic `Date` string entries above are not repeated here because those are
live runtime mismatches, not just stale documentation.

No standalone conformance-contract or harness drift was confirmed. The nearby
green tests above still pass while the runtime gaps survive, so this run found
coverage holes rather than a broken presubmit lane.

In particular, the existing passing callable/constructor tests cover instance
constructor links such as `[].constructor === Array`, `({}).constructor ===
Object`, and similar built-in instance cases. They do not currently cover
callable `.constructor`, primitive autoboxed property reads like `"ab"[0]`,
or the `Date.prototype.valueOf` / non-RFC3339 string paths documented above.

## Investigated And Rejected

The following historical or suspected issues were rechecked and did not hold on
the current branch:

- the older `globalThis` identity, top-level `this`, and arrow lexical `this`
  mismatches from `VERIFICATION_GAPS.md`
- the older `Array(3)` / `new Array(3)` single-length constructor bug
- the older `Object(value)` / wrapper-constructor regressions
- the older `reduce` callback `this` bug
- the older fractional-`Date` millisecond truncation bug
- conformance-contract, coverage-audit, rejection-harness, and `test262`
  drift in the docs/contract slice
