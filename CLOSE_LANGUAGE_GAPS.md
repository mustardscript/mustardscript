# Close Language Gaps

This plan audits the requested language-gap list against the current
repository state and turns only the verified remaining gaps into implementation
work.

Audit sources:

- `README.md`
- `docs/LANGUAGE.md`
- `crates/jslite/src/parser.rs`
- `crates/jslite/src/runtime/compiler`
- `crates/jslite/src/runtime/env.rs`
- `crates/jslite/src/runtime/builtins`
- `tests/node/basic.test.js`
- `tests/node/async-runtime.test.js`
- `tests/node/iteration.test.js`
- targeted runtime probes run on 2026-04-11

## Requested Items: Audit Status

### Already implemented

- [x] Rest parameters work for function declarations, function expressions, and
  arrow functions.
- [x] `for...of` already works over arrays, strings, `Map`, `Set`, and the
  supported iterator helper objects.
- [x] `for...of` now accepts identifier and member assignment-target headers in
  addition to the existing single-binding `let` / `const` declaration surface.
- [x] Array helpers already implemented: `push`, `pop`, `map`, `filter`,
  `reduce`, `find`, `some`, `every`, `slice`, `join`, `sort`, `includes`, and
  `Array.from`.
- [x] String helpers already implemented: `split`, `trim`, `includes`,
  `startsWith`, `endsWith`, `slice`, `substring`, `replace`, `match`,
  `toLowerCase`, and `toUpperCase`.
- [x] Object helpers already implemented: `Object.keys`, `Object.values`,
  `Object.entries`, `Object.fromEntries`, and `Object.hasOwn`.
- [x] Promise instance methods `.then(...)`, `.catch(...)`, and
  `.finally(...)` already work.
- [x] Iterable constructor inputs like `new Set(values)` and `new Map(entries)`
  already work for the supported iterable surface.
- [x] `Math.pow`, `Math.sqrt`, `Math.trunc`, and `Math.sign` already work.
- [x] `Math.log` already works.
- [x] `Promise.all`, `Promise.race`, `Promise.any`, and
  `Promise.allSettled` already work.
- [x] `Array.of`, `Array.prototype.concat`, and `Array.prototype.at` already
  work.
- [x] `Object.assign` already works for the supported plain-object and array
  helper surface.
- [x] Object literals already support computed keys, method shorthand, and
  spread over the documented plain-object and array source surface.
- [x] `Array.prototype.splice`, `Array.prototype.flat`, and
  `Array.prototype.flatMap` already work for the supported array-only surface.
- [x] Sequence expressions and binary `**` now work end to end.
- [x] `Object.create`, `Object.freeze`, and `Object.seal` now fail closed with
  explicit guest-safe runtime errors.

### Still missing or narrower than requested

- [x] Object spread now works for plain-object and array sources, skips
  `null` / `undefined`, and fails closed for other source values.
- [x] Array spread is rejected during validation.
- [x] Spread arguments are rejected during validation.
- [x] Default parameters are rejected during validation.
- [x] Default destructuring is rejected during validation.
- [x] Destructuring assignment is rejected during validation.
- [x] `var` is rejected; only `let` and `const` are supported.
- [x] Update expressions are rejected during validation.
- [x] `delete` is rejected during validation.
- [x] `for...in` now works for plain objects and arrays with the documented
  conservative key-order and header surface.
- [x] `for await...of` now works inside async functions over the documented
  sync-iterable surface by awaiting each yielded value before the loop body.
- [x] Array holes in literals are rejected during validation.
- [x] `instanceof` is rejected as an unsupported binary operator.
- [x] Math helper gaps remaining: `Math.random`.

### Missing but not in the original requested list

- [x] Computed object literal keys are supported end to end.
- [x] Object literal methods are supported end to end.
- [x] Object literal spread is supported end to end for the documented
  plain-object and array source surface.
- [x] Conservative `in` support exists for the runtime's currently exposed
  property surface without widening prototype or descriptor semantics.
- [x] Logical assignment operators `||=` and `&&=` are rejected during
  validation.
- [x] Additional unsupported compound assignments `%=`, `**=`, and bitwise
  assignment operators are rejected during validation.

### Stale assumptions corrected by this audit

- [x] Default parameters and default destructuring no longer parse and then
  fail later at runtime; they now fail closed at validation.
- [x] Rest parameters are no longer a runtime hole.
- [x] `for...of` is no longer arrays-only.

## Execution Order

### Phase 1: Low-risk library additions

- [x] Add `Array.of`, `Array.prototype.concat`, `Array.prototype.at`, and
  `Math.log`.
- [x] Add `Math.random` with an explicit documented nondeterminism policy.
- [x] Add `Object.assign` without widening the host boundary or prototype
  surface.
- [x] Decide whether unsupported static `Object` helpers should fail with an
  explicit guest-safe runtime error instead of surfacing as missing properties.
- [x] Add differential tests and guest-safe failure tests for each new helper.
- [x] Update `docs/LANGUAGE.md` and `LANGUAGE_GAPS.md` after each helper
  cluster lands.

### Phase 2: Mid-risk array and object helpers

- [x] Add `Array.prototype.splice` with correct in-place mutation and
  return-array behavior.
- [x] Add `Array.prototype.flat` for the supported array-only surface.
- [x] Add `Array.prototype.flatMap` on top of the chosen `flat` depth rules.

### Phase 3: Syntax and compiler gaps

- [x] Add IR, compiler, and runtime support for `**`.
- [x] Add IR, compiler, and runtime support for `in` if it stays in scope for
  the compatibility target.
- [x] Add IR, compiler, and runtime support for sequence expressions.
- [ ] Decide and implement sparse-array semantics so array holes can exist
  consistently across literals, property access, helper methods, JSON, and the
  host boundary.
- [ ] Add lowering and runtime expansion for array spread and spread arguments
  using the currently supported iterable surface.
- [x] Add lowering and runtime expansion for object spread using the documented
  plain-object enumeration rules.
- [x] Add lowering for computed object literal keys and object literal methods
  if they stay in scope for the compatibility target.
- [ ] Add destructuring assignment lowering for identifier and member targets.
- [ ] Add default parameter and default destructuring evaluation in
  function-parameter scope.
- [ ] Add update-expression lowering and semantics for prefix and postfix
  `++` / `--`.
- [ ] Add logical assignment operators `||=` and `&&=` if they stay in scope
  for the compatibility target.
- [x] Add validation, diagnostics, and differential tests for the full syntax
  cluster above.

### Phase 4: Iteration and control-flow expansion

- [x] Broaden `for...of` headers beyond exactly one declared `let` / `const`
  binding by supporting identifier and member assignment-target headers.
- [x] Decide the supported `for...in` surface for plain objects and arrays,
  including enumeration order and inherited-property behavior.
- [x] Decide the minimum `for await...of` surface: reuse the existing
  synchronous iterable inputs (`Array`, `String`, `Map`, `Set`, and supported
  iterator helper objects) inside async functions, await each yielded value,
  and continue to defer custom async-iterator protocol support.
- [x] Implement the chosen `for await...of` surface with snapshot and resume
  coverage where suspension is possible.

### Phase 5: Hard semantic gaps that need design first

- [x] Decide whether `var` is being added at all. If yes, implement function
  and global hoisting, redeclaration, and loop-scoping rules.
- [x] Decide conservative `delete` semantics for plain objects and arrays, and
  document whether descriptor-level configurability remains deferred.
- [x] Decide the minimum prototype and constructor model needed for
  `instanceof`.
- [ ] Implement `instanceof` only after the prototype model is explicit and
  tested.

### Phase 6: Explicit rejections and non-goals

- [x] Explicitly reject `Object.create` with a guest-safe runtime error instead
  of leaving it absent as an undefined property.
- [x] Explicitly reject `Object.freeze` with a guest-safe runtime error instead
  of leaving it absent as an undefined property.
- [x] Explicitly reject `Object.seal` with a guest-safe runtime error instead
  of leaving it absent as an undefined property.
- [x] Document these three helpers as intentional non-goals for the current
  contract until prototype and descriptor semantics are deliberately expanded.
- [x] Add tests that assert the rejection message for each helper.

## Verification Matrix

- [x] Parser-facing syntax work adds positive coverage plus rejection snapshots
  for nearby unsupported forms.
- [ ] Runtime built-in work adds direct Node tests for success paths.
- [ ] Runtime built-in work adds guest-safe failure tests for wrong receivers,
  wrong arity, wrong callback types, and unsupported host-suspension cases.
- [x] Semantics-sensitive work adds differential tests against Node for the
  supported subset.
- [x] Loop and iterator work adds snapshot and resume coverage when suspension
  can occur mid-iteration.
- [x] Any new unsupported decision is documented in `docs/LANGUAGE.md` and
  reflected in `LANGUAGE_GAPS.md`.

## Explicit Deferrals / Non-Goals For This Plan

- [x] Full prototype semantics remain out of scope.
- [x] Descriptor-level object semantics remain out of scope.
- [x] Symbol-based iterable protocol support remains out of scope.
- [x] `Object.create`, `Object.freeze`, and `Object.seal` are treated as
  explicit rejections, not implementation targets, in the current plan.

## Done Criteria For Each Checklist Item

- [ ] Parser and validator behavior is explicit and fail-closed for unsupported
  edge cases.
- [ ] IR, compiler, and runtime support exists end to end.
- [ ] Rust and Node tests cover success paths plus guest-safe failure behavior.
- [ ] `docs/LANGUAGE.md` and `LANGUAGE_GAPS.md` are updated.
- [ ] The gap is removed from this file only after verification.
