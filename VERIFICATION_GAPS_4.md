# Verification Swarm Report 4

Generated on 2026-04-12 for `/Users/mini/jslite`.

## Resolution Status

Status on `main` after the 2026-04-12 implementation wave:

- All actionable runtime, test, and docs gaps identified in this report were implemented and verified.
- The restore-limits fail-open issue was fixed in both addon and raw snapshot restore paths.
- The previously noted addon-vs-sidecar replay difference remains, but it is now treated as an explicit contract distinction rather than an unresolved bug because the sidecar protocol intentionally documents stateless replay and the public docs/tests now match that contract.
- Final verification after the fixes:
  - `cargo test --workspace`
  - `npm test`
  - `npm run lint`
  - all passed on the integrated tree

## Audit Method

- Read `AGENTS.md`, `IMPLEMENT_PROMPT.md`, `README.md`, `Cargo.toml`, and `package.json`.
- Inspected the worktree with `git status --short`.
- Audited the current runtime, Node wrapper, docs, and tests before treating any gap as real.
- Launched 12 unique read-only `gpt-5.4` `xhigh` explorer agents in two waves because the collab layer only allowed 6 concurrent agent threads.
- Kept only issues that I reproduced locally with direct probes or corroborated directly in code and tests.

Direct parity probe pattern:

```js
const nodeResult = vm.runInNewContext('"use strict";\n' + source, Object.create(null));
const runtimeResult = await new Jslite(source).run();
```

## Commands Run

- `git status --short`
- `cargo test --workspace`
- `npm run lint`
- `npm test`
- `node --test tests/node/differential.test.js --test-name-pattern 'matches Node for callable metadata and constructor links'`
- Multiple focused `node - <<'NODE'` probes comparing strict-script Node results against `await new Jslite(source).run()`

Notes:

- `cargo test --workspace` passed during the audit.
- `npm run lint` passed during the audit.
- `npm test` was initially green during the baseline pass, but a later rerun failed at [`tests/node/bigint.test.js`](/Users/mini/jslite/tests/node/bigint.test.js:154). That failure is included below as confirmed test / contract drift because the negative expectation is stale.

## 12 Validation Slices

1. Global binding and free-name lookup
2. `this` binding semantics
3. Callable metadata and function-object property exposure
4. Function helper dispatch
5. Built-in constructor dispatch and constructor identity
6. Primitive boxing and wrapper-object semantics
7. Array callback traversal semantics
8. `reduce` / `reduceRight`
9. `Date` construction, parsing, and clipping
10. `Date` / `Intl` formatting and wall-clock semantics
11. Addon / sidecar / progress transport parity
12. Contract / docs / harness drift

## Confirmed Runtime Root Causes

### 1. Root-scope instantiation is unsound

Classification:
- confirmed runtime mismatch
- confirmed fail-open behavior

Representative repros:

```js
const globalThis = { note: 1 };
function f() {}
[typeof f, globalThis.note];
```

- Node result: `["function", 1]`
- Runtime result: `ReferenceError: \`globalThis\` accessed before initialization`

```js
let x = 1;
let x = 2;
x;
```

- Node result: `SyntaxError: Identifier 'x' has already been declared`
- Runtime result: `2`

Code path:
- [`crates/jslite/src/runtime/compiler/mod.rs`](/Users/mini/jslite/crates/jslite/src/runtime/compiler/mod.rs:153)
- [`crates/jslite/src/runtime/env.rs`](/Users/mini/jslite/crates/jslite/src/runtime/env.rs:64)
- [`crates/jslite/src/runtime/env.rs`](/Users/mini/jslite/crates/jslite/src/runtime/env.rs:88)

The compiler hard-codes `globalThis` during root-scope function installation, and duplicate lexical declarations are silently accepted instead of rejected during declaration/instantiation.

### 2. The callable own-property model is incomplete and internally inconsistent

Classification: confirmed runtime mismatch

Representative repros:

```js
function f(alpha, beta) {}
const arrow = (x) => x;
const method = [].map;
const bound = f.bind(null, 1);
({
  declared: [typeof f.constructor, f.constructor && f.constructor.name],
  arrow: [typeof arrow.constructor, arrow.constructor && arrow.constructor.name],
  method: [typeof method.constructor, method.constructor && method.constructor.name],
  bound: [typeof bound.constructor, bound.constructor && bound.constructor.name],
  builtin: [typeof Array.constructor, Array.constructor && Array.constructor.name],
});
```

- Node result: every entry is `["function", "Function"]`
- Runtime result: `declared` / `arrow` / `method` / `builtin` are `["undefined", null]`; `bound` is `["function", "Object"]`

```js
Array.extra = 1;
({ extra: Array.extra, keys: Object.keys(Array) });
```

- Node result: `{ extra: 1, keys: ["extra"] }`
- Runtime result: `TypeError: value is not an object`

```js
function f() {}
f.beta = 'b';
f[10] = 'ten';
f.alpha = 'a';
f[2] = 'two';
({ keys: Object.keys(f), entries: Object.entries(f) });
```

- Node result: keys / entries order `["2","10","beta","alpha"]`
- Runtime result: keys / entries order `["beta","10","alpha","2"]`

```js
function f(alpha, beta) {}
const bound = f.bind(null, 1);
({
  name: bound.name,
  length: bound.length,
  ownName: Object.hasOwn(bound, 'name'),
  ownLength: Object.hasOwn(bound, 'length'),
  ownPrototype: Object.hasOwn(bound, 'prototype'),
});
```

- Node result: `{ name: "bound f", length: 1, ownName: true, ownLength: true, ownPrototype: false }`
- Runtime result: `{ name: "bound f", length: 1, ownName: false, ownLength: false, ownPrototype: false }`

Additional corroboration:

```js
const box = { task: function () {} };
const arrowBox = { task: () => {} };
({ fn: box.task.name, arrow: arrowBox.task.name });
```

- Node result: `{ fn: "task", arrow: "task" }`
- Runtime result: `{ fn: "", arrow: "" }`

```js
function f() {}
({ guest: String(f), builtin: String(Array) });
```

- Node result: function-source-like strings
- Runtime result: `"[Function]"` placeholders

Code path:
- [`crates/jslite/src/runtime/properties.rs`](/Users/mini/jslite/crates/jslite/src/runtime/properties.rs:55)
- [`crates/jslite/src/runtime/properties.rs`](/Users/mini/jslite/crates/jslite/src/runtime/properties.rs:396)
- [`crates/jslite/src/runtime/properties.rs`](/Users/mini/jslite/crates/jslite/src/runtime/properties.rs:1053)
- [`crates/jslite/src/runtime/properties.rs`](/Users/mini/jslite/crates/jslite/src/runtime/properties.rs:1358)
- [`crates/jslite/src/runtime/builtins/objects.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/objects.rs:67)
- [`crates/jslite/src/runtime/builtins/objects.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/objects.rs:348)
- [`crates/jslite/src/runtime/conversions/coercions.rs`](/Users/mini/jslite/crates/jslite/src/runtime/conversions/coercions.rs:127)
- [`crates/jslite/src/parser/expressions.rs`](/Users/mini/jslite/crates/jslite/src/parser/expressions.rs:153)

The runtime treats closures, bound functions, built-in functions, and host functions as callable values, but not as a coherent object model with Node-like own-property behavior.

### 3. Primitive autoboxing and primitive property access are incomplete

Classification: confirmed runtime mismatch

Representative repros:

```js
"ab"[0];
```

- Node result: `"a"`
- Runtime result: `undefined`

```js
({
  s: "ab".constructor === String,
  n: (1).constructor === Number,
  b: false.constructor === Boolean,
});
```

- Node result: all `true`
- Runtime result: all `false`

```js
({
  objectBox: Object("7").padStart(3, "0"),
  stringBox: new String("7").padStart(3, "0"),
});
```

- Node result: both `"007"`
- Runtime result: `Error: value is not callable`

```js
({
  num: new Number(1).toString(),
  bool: new Boolean(false).toString(),
});
```

- Node result: `{ num: "1", bool: "false" }`
- Runtime result: `Error: value is not callable`

Code path:
- [`crates/jslite/src/runtime/properties.rs`](/Users/mini/jslite/crates/jslite/src/runtime/properties.rs:534)
- [`crates/jslite/src/runtime/properties.rs`](/Users/mini/jslite/crates/jslite/src/runtime/properties.rs:907)
- [`crates/jslite/src/runtime/properties.rs`](/Users/mini/jslite/crates/jslite/src/runtime/properties.rs:1234)

Primitive strings expose some helper methods, but primitive indexing, primitive `.constructor`, and several boxed wrapper methods are still missing.

### 4. Array helpers observe holes and `length` mutations incorrectly

Classification: confirmed runtime mismatch

Representative repros:

```js
const a = [0, , 2];
const visits = [];
({ out: a.findLastIndex((v, i, arr) => { visits.push([i, v, i in arr]); return i === 1; }), visits });
```

- Node result: `{ out: 1, visits: [[2,2,true],[1,undefined,false]] }`
- Runtime result: `{ out: -1, visits: [[2,2,true],[0,0,true]] }`

```js
const a = [0, 1, 2];
const visits = [];
({ out: a.some((v, i, arr) => { visits.push([i, v, i in arr]); if (i === 0) arr.length = 1; return false; }), visits, keys: Object.keys(a) });
```

- Node result: `{ out: false, visits: [[0,0,true]], keys: ["0"] }`
- Runtime result: `{ out: false, visits: [[0,0,true],[1,1,true],[2,2,true]], keys: ["0","1","2","length"] }`

```js
const values = [1, 2, 3];
const seen = [];
const result = values.reduce((acc, value, index, array) => {
  seen.push(index);
  if (index === 0) array.length = 1;
  return acc + value;
}, 0);
({ result, seen, finalLength: values.length, finalKeys: Object.keys(values) });
```

- Node result: `{ result: 1, seen: [0], finalLength: 1, finalKeys: ["0"] }`
- Runtime result: `{ result: 6, seen: [0,1,2], finalLength: 3, finalKeys: ["0","1","2","length"] }`

```js
const values = [1, 2, 3];
const seen = [];
const result = values.reduceRight((acc, value, index, array) => {
  seen.push(index);
  if (index === 2) array.length = 0;
  return acc + value;
}, 0);
({ result, seen, finalLength: values.length, finalKeys: Object.keys(values) });
```

- Node result: `{ result: 3, seen: [2], finalLength: 0, finalKeys: [] }`
- Runtime result: `{ result: 6, seen: [2,1,0], finalLength: 3, finalKeys: ["0","1","2","length"] }`

Code path:
- [`crates/jslite/src/runtime/builtins/arrays.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/arrays.rs:869)
- [`crates/jslite/src/runtime/builtins/arrays.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/arrays.rs:1110)
- [`crates/jslite/src/runtime/properties.rs`](/Users/mini/jslite/crates/jslite/src/runtime/properties.rs:1381)

Two structural problems combine here:

- `findLast` / `findLastIndex` skip holes instead of visiting them as `undefined`
- writing `array.length = ...` is treated as an ordinary string-keyed property write instead of array truncation, so helper traversals continue across elements Node would delete

### 5. `Date` invalid-value and extended-year handling diverge from Node

Classification: confirmed runtime mismatch

Representative repros:

```js
new Date(0 / 0).getUTCFullYear();
```

- Node result: `NaN`
- Runtime result: `RangeError: Invalid time value`

```js
1 / new Date(-0.1).getTime();
```

- Node result: `Infinity`
- Runtime result: `-Infinity`

```js
new Date(8640000000000000).toISOString();
new Date(8640000000000000).toJSON();
```

- Node result: both `"+275760-09-13T00:00:00.000Z"`
- Runtime result: `toISOString()` throws `RangeError: Invalid time value`; `toJSON()` returns `null`

```js
new Date("+010000-01-01T00:00:00.000Z").getTime();
```

- Node result: `253402300800000`
- Runtime result: `NaN`

```js
new Date(-62198755200000).toJSON();
```

- Node result: `"-000001-01-01T00:00:00.000Z"`
- Runtime result: `"-001-01-01T00:00:00.000Z"`

Code path:
- [`crates/jslite/src/runtime/builtins/support.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/support.rs:43)
- [`crates/jslite/src/runtime/builtins/support.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/support.rs:50)
- [`crates/jslite/src/runtime/builtins/support.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/support.rs:80)
- [`crates/jslite/src/runtime/builtins/support.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/support.rs:93)
- [`crates/jslite/src/runtime/builtins/primitives.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/primitives.rs:147)
- [`crates/jslite/src/runtime/builtins/primitives.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/primitives.rs:165)
- [`crates/jslite/src/runtime/builtins/primitives.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/primitives.rs:211)

The current helper layer turns invalid UTC accessor reads into throws, mishandles `TimeClip` edge cases, and still does not round-trip some extended-year ISO values correctly.

### 6. The `Intl` subset both diverges on supported formatting and fails open on unsupported options

Classification:
- confirmed runtime mismatch
- confirmed fail-open behavior

Representative repros:

```js
(() => {
  try {
    return Intl.DateTimeFormat("en-US", { timeZone: "UTC", year: "numeric" }).format(new Date(0 / 0));
  } catch (error) {
    return [error.name, error.message];
  }
})()
```

- Node result: `["RangeError","Invalid time value"]`
- Runtime result: `"Invalid Date"`

```js
Intl.DateTimeFormat("en-US", { timeZone: "UTC", weekday: "long" }).format(new Date("2026-04-10T14:05:06.789Z"));
```

- Node result: `"Friday"`
- Runtime result: `"4/10/2026"`

```js
Intl.NumberFormat("en-US", { notation: "scientific" }).format(1234);
```

- Node result: `"1.234E3"`
- Runtime result: `"1,234"`

```js
Intl.DateTimeFormat("en-US", { timeZone: "UTC", hour: "numeric", minute: "2-digit" }).format(new Date("2026-04-10T14:05:06.789Z"));
```

- Node result: `"2:05 PM"`
- Runtime result: `"14:05"`

```js
Intl.NumberFormat("en-US", { style: "currency", currency: "USD" }).format(-1.23);
```

- Node result: `"-$1.23"`
- Runtime result: `"$-1.23"`

Code path:
- [`crates/jslite/src/runtime/builtins/intl.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/intl.rs:113)
- [`crates/jslite/src/runtime/builtins/intl.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/intl.rs:160)
- [`crates/jslite/src/runtime/builtins/intl.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/intl.rs:273)
- [`crates/jslite/src/runtime/builtins/intl.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/intl.rs:298)
- [`crates/jslite/src/runtime/builtins/intl.rs`](/Users/mini/jslite/crates/jslite/src/runtime/builtins/intl.rs:404)

Unsupported `Intl` options are currently ignored instead of failing closed, and several formatting details still differ even inside accepted calls.

### 7. Progress restore and replay policy diverge across public surfaces, and omitted limits fail open

Classification:
- confirmed runtime mismatch
- confirmed fail-open behavior

Representative repros:

```js
const progress = new Jslite('const v = fetch_data(5); v + 3;').start({
  snapshotKey: 'k',
  capabilities: { fetch_data() {} },
  limits: {},
});
const dumped = progress.dump();
progress.resume(5);
Progress.load(dumped);
```

- Addon result: first resume completes `8`; later `Progress.load(dumped)` throws `JsliteRuntimeError: Progress objects are single-use; this suspended execution was already resumed`
- Sidecar result: replaying the same authenticated `snapshot_base64` in two `resume` requests succeeds twice and completes `8` both times

```js
const source = `
  const value = fetch_data(1);
  let sum = 0;
  for (let i = 0; i < 20000; i = i + 1) sum = sum + i;
  sum + value;
`;
const limits = { instructionBudget: 50 };
const dumped = new Jslite(source).start({
  snapshotKey: 'k',
  capabilities: { fetch_data() {} },
  limits,
}).dump();
Progress.load(dumped, {
  snapshotKey: 'k',
  capabilities: { fetch_data() {} },
  limits: undefined,
}).resume(1);
```

- Direct addon resume under the original `instructionBudget: 50` fails with `JsliteLimitError: instruction budget exhausted`
- Restored addon progress with `limits: undefined` completes `199990001`
- Sidecar replay with `policy.limits` omitted also completes `199990001`, while the same replay with `policy.limits: { instruction_budget: 50 }` correctly fails

Code path:
- [`lib/progress.js`](/Users/mini/jslite/lib/progress.js:23)
- [`lib/progress.js`](/Users/mini/jslite/lib/progress.js:53)
- [`lib/progress.js`](/Users/mini/jslite/lib/progress.js:123)
- [`lib/policy.js`](/Users/mini/jslite/lib/policy.js:223)
- [`lib/policy.js`](/Users/mini/jslite/lib/policy.js:275)
- [`crates/jslite-bridge/src/dto.rs`](/Users/mini/jslite/crates/jslite-bridge/src/dto.rs:42)
- [`crates/jslite-bridge/src/dto.rs`](/Users/mini/jslite/crates/jslite-bridge/src/dto.rs:80)

The addon and sidecar expose different replay semantics, and serialized executions are not bound to their original limits unless the caller explicitly re-supplies them on restore.

## Confirmed Test / Contract / Docs Drift

### 1. Callable metadata coverage misses the strongest confirmed callable gaps

Classification:
- confirmed test bug
- confirmed docs drift

Evidence:

- [`tests/node/differential.test.js`](/Users/mini/jslite/tests/node/differential.test.js:501) and [`tests/node/builtins.test.js`](/Users/mini/jslite/tests/node/builtins.test.js:629) verify `name`, `length`, `prototype`, and a few constructor-instance links, but they never read callable `.constructor`, never check built-in constructor extensibility, and never check bound-function `Object.hasOwn(..., "name" | "length")`.
- Those same coverage areas also miss a confirmed metadata mismatch in default-parameter arity:

```js
function f(a, b = 1, c) {}
f.length;
```

  - Node result: `1`
  - Runtime result: `3`

- [`docs/LANGUAGE.md`](/Users/mini/jslite/docs/LANGUAGE.md:462) says supported guest functions and built-in callables expose `constructor`, but current runtime repros show that claim is false for guest, built-in, host, and most bound callables.

### 2. Primitive property coverage misses the confirmed autoboxing and property-access gaps

Classification: confirmed test bug

Evidence:

- [`tests/node/conformance-contract.js`](/Users/mini/jslite/tests/node/conformance-contract.js:121), [`tests/node/coverage-audit.test.js`](/Users/mini/jslite/tests/node/coverage-audit.test.js:208), [`tests/node/coverage-audit.test.js`](/Users/mini/jslite/tests/node/coverage-audit.test.js:374), and [`tests/node/builtins.test.js`](/Users/mini/jslite/tests/node/builtins.test.js:14) do not pin primitive indexing or primitive `.constructor` behavior tightly enough to catch the current runtime gap.

```js
['ab'[0], typeof 'ab'.constructor, typeof (1).constructor, typeof true.constructor];
```

- Node result: `["a","function","function","function"]`
- Runtime result: `[undefined,"undefined","undefined","undefined"]`

### 3. The array and `Intl` docs overstate current behavior

Classification: confirmed docs drift

Evidence:

- [`docs/LANGUAGE.md`](/Users/mini/jslite/docs/LANGUAGE.md:446) says `findLast` / `findLastIndex` follow the same callback rules as `find` / `findIndex`, but current runtime skips holes while `find` / `findIndex` already visit them as `undefined`.
- [`docs/LANGUAGE.md`](/Users/mini/jslite/docs/LANGUAGE.md:515) and [`README.md`](/Users/mini/jslite/README.md:463) say unsupported `Intl` options fail closed, but `weekday: "long"` and `notation: "scientific"` are silently ignored.

### 4. The `Date` docs overstate accepted string parsing

Classification: confirmed docs drift

Evidence:

- [`docs/LANGUAGE.md`](/Users/mini/jslite/docs/LANGUAGE.md:507) describes `new Date(value).getTime()` as supported without narrowing the accepted string formats enough to match the current implementation.

```js
(() => {
  const value = new Date('04/10/2026').getTime();
  return [value !== value, String(value)];
})()
```

- Node result: `[false,"1775804400000"]`
- Runtime result: `[true,"NaN"]`

### 5. Progress docs and public surfaces do not line up cleanly on replay and cancel semantics

Classification: confirmed docs drift

Evidence:

- [`docs/HOST_API.md`](/Users/mini/jslite/docs/HOST_API.md:121) documents addon `Progress` objects as single-use, while [`docs/SIDECAR_PROTOCOL.md`](/Users/mini/jslite/docs/SIDECAR_PROTOCOL.md:112) explicitly documents stateless sidecar replay. That means the public surfaces intentionally expose different lifecycle rules, but consumers cannot assume parity between addon restore and sidecar resume.
- [`docs/SIDECAR_PROTOCOL.md`](/Users/mini/jslite/docs/SIDECAR_PROTOCOL.md:73) and [`docs/SIDECAR_PROTOCOL.md`](/Users/mini/jslite/docs/SIDECAR_PROTOCOL.md:138) do not make it clear that a `payload` of `{ "type": "cancelled" }` is accepted and translated into a cancellation limit error rather than rejected as an unsupported resume payload.

## Rejected / Not Confirmed

- No surviving mismatch was confirmed in top-level/member/arrow `this` binding after extended probing.
- No surviving function-helper-dispatch mismatch was confirmed beyond the broader callable own-property issues already captured above.
- BigInt exponent assignment no longer appears as a contract or verification gap in the current tree: [`tests/node/bigint.test.js`](/Users/mini/jslite/tests/node/bigint.test.js:154) now asserts support, and `npm test` is green.
- A direct `tests/node/builtins.test.js` failure around global `parseInt` exposure was not stable enough to reduce to a minimal runtime repro during this pass, so it is not included above as a confirmed gap.

## Verification Snapshot

- `cargo test --workspace`: passed during this audit pass
- `npm run lint`: passed during this audit pass
- `npm test`: passed during this audit pass
- Focused parity probes: used throughout this report to reconfirm each retained finding before inclusion

## Remaining Work Classification

Still feasible and unblocked:

- None. The actionable items captured in this report were completed on `main` during the 2026-04-12 fix wave.

Blocked:

- None identified from repository context.
