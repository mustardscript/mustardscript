# LANGUAGE_GAPS_3

Re-audited on 2026-04-12 against the current repository state after the latest
language-surface implementation work landed on `main`.

This file now tracks the verified current state instead of preserving the
original speculative gap list.

Audit inputs:

- `crates/mustard/src/runtime/builtins/install.rs`
- `crates/mustard/src/runtime/builtins/primitives.rs`
- `crates/mustard/src/runtime/builtins/strings.rs`
- `crates/mustard/src/runtime/builtins/arrays.rs`
- `crates/mustard/src/runtime/builtins/collections.rs`
- `crates/mustard/src/runtime/properties.rs`
- `crates/mustard/src/parser/operators.rs`
- `crates/mustard/src/runtime/compiler/bindings.rs`
- `crates/mustard/src/runtime/conversions/operators.rs`
- `tests/node/builtins.test.js`
- `tests/node/keyed-collections.test.js`
- `tests/node/language-gaps.test.js`
- `tests/test262/harness.test.js`
- `crates/mustard/tests/builtin_surface.rs`
- `crates/mustard/tests/keyed_collections.rs`
- `crates/mustard/src/parser/tests/acceptance.rs`
- `crates/mustard/src/parser/tests/rejections.rs`

## Verifiably Complete

The following items from the original `LANGUAGE_GAPS_3` audit are now
implemented and covered:

- [x] Global `NaN`, `Infinity`, `isNaN()`, `isFinite()`, `parseInt()`, `parseFloat()`
- [x] `Number.isNaN`, `Number.isFinite`, `Number.isInteger`, `Number.isSafeInteger`, `Number.parseInt`, `Number.parseFloat`
- [x] `Number` constants: `MAX_SAFE_INTEGER`, `MIN_SAFE_INTEGER`, `EPSILON`, `MAX_VALUE`, `MIN_VALUE`, `POSITIVE_INFINITY`, `NEGATIVE_INFINITY`, `NaN`
- [x] String `trimStart()`, `trimEnd()`, `padStart()`, `padEnd()`
- [x] String `indexOf()`, `lastIndexOf()`
- [x] String `charAt()`, `at()`
- [x] String `repeat()`
- [x] String `concat()`
- [x] `Math` constants: `PI`, `E`, `LN2`, `LN10`, `LOG2E`, `LOG10E`, `SQRT2`, `SQRT1_2`
- [x] `Date.prototype.toISOString()`, `toJSON()`, and UTC accessors
- [x] Assignment operators `%=`, `**=`
- [x] Array `reverse()`, `lastIndexOf()`, `fill()`
- [x] Array `findLast()`, `findLastIndex()`, `reduceRight()`
- [x] `Map.prototype.forEach()`, `Set.prototype.forEach()`
- [x] `Error.cause` support in error constructors
- [x] `SyntaxError` constructor
- [x] Additional `Math` methods: `exp()`, `log2()`, `log10()`, `sin()`, `cos()`, `atan2()`, `hypot()`, `cbrt()`
- [x] `Map.prototype.size` / `Set.prototype.size` were confirmed non-gaps

Verification evidence:

- `cargo test -p mustard --test builtin_surface --test keyed_collections`
- `cargo test -p mustard parser`
- `node --test tests/node/builtins.test.js tests/node/keyed-collections.test.js tests/node/language-gaps.test.js tests/test262/harness.test.js`

## Still Open

### Feasible follow-up work

- [ ] Bitwise operators: `&`, `|`, `^`, `~`, `<<`, `>>`, `>>>`
- [ ] Bitwise compound assignments: `&=`, `|=`, `^=`, `<<=`, `>>=`, `>>>=`
- [ ] URI globals: `encodeURI()`, `decodeURI()`, `encodeURIComponent()`, `decodeURIComponent()`
- [ ] `structuredClone()` global for the supported plain-object / array value surface

### Intentionally deferred or outside the current contract

- [ ] Local-time `Date` accessors such as `getFullYear()`, `getMonth()`, `getDate()`, `getHours()`, `getMinutes()`, `getSeconds()`, `getMilliseconds()`
  The documented Date subset remains UTC-only.
- [ ] `delete` operator for plain objects
  Still rejected pending an explicit deletion-semantics design.
- [ ] Classes / `extends` / `super` / private fields
- [ ] Generators / `yield`
- [ ] Tagged template literals
- [ ] Symbols and `Symbol.iterator`
- [ ] `WeakMap`, `WeakSet`, `WeakRef`, `FinalizationRegistry`
- [ ] `Proxy` / `Reflect`
- [ ] Typed arrays / `ArrayBuffer` / `SharedArrayBuffer` / `Atomics`
- [ ] ES modules and dynamic `import()`
- [ ] `var`
- [ ] Full prototype inheritance and user-defined constructor `new`

## Notes

- The original version of this file became stale once `main` picked up the
  Date/Number/string helper work, sparse-array fixes, keyed-collection helper
  work, and the conservative assignment-operator expansion.
- Treat this file as the current gap tracker for the next pass rather than as a
  historical snapshot of the pre-implementation tree.
