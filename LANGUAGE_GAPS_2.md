# LANGUAGE_GAPS_2

This is a second-pass language-surface audit based on the current repository
state, not just the docs.

Audit inputs:

- `README.md`
- `USE_CASE_GAPS.md`
- `USE_CASE_GAPS_2.md`
- `crates/mustard/src/parser/tests/acceptance.rs`
- `crates/mustard/src/parser/tests/rejections.rs`
- `crates/mustard/src/runtime/builtins/install.rs`
- `crates/mustard/src/runtime/builtins/arrays.rs`
- `crates/mustard/src/runtime/builtins/objects.rs`
- `crates/mustard/src/runtime/builtins/primitives.rs`
- `crates/mustard/src/runtime/builtins/strings.rs`
- `crates/mustard/src/runtime/properties.rs`
- `tests/node/language-gaps.test.js`
- `tests/node/builtins.test.js`
- `tests/node/use-cases.test.js`
- `tests/test262/manifest.js`

## Summary

The important repo fact is that `mustard` is no longer missing many of the
obvious subset features older gap lists used to cite. The current code already
implements default parameters, destructuring defaults, destructuring assignment,
array/object spread, `for...in`, `for await...of`, `instanceof`, keyed
collections, promises, `RegExp`, sparse arrays, and a fairly usable array /
string / object helper set.

Also important: the realistic programmatic tool-call gallery is currently all
green (`USE_CASE_GAPS.md` reports `24/24` passing). So the gaps below are best
understood as compatibility wins for future agent-written code, not evidence
that the current product target is failing.

## Ranking

### High confidence

These fit the project goals well: they improve ordinary agent/tool-calling code
without obviously breaking the repo's "small, auditable subset" constraint.

| Gap | Why it should be added | Code evidence |
| --- | --- | --- |
| Wider `Date` surface: `toISOString()`, `toJSON()`, UTC field accessors, and likely a few more constructor forms | The README explicitly positions `mustard` for freshness checks, SLA logic, and resumable workflows. Current `Date` support is enough for timestamp math, but not for the next common step: formatting dates back into stable strings or extracting UTC components without a host round-trip. | `crates/mustard/src/runtime/builtins/primitives.rs` only implements `Date.now()`, zero/one-arg construction, and `getTime()`. `crates/mustard/src/runtime/properties.rs` only exposes `getTime`, `valueOf`, and `constructor` on `Date` objects. |
| `Number` parsing/predicate helpers: `Number.parseInt`, `Number.parseFloat`, `Number.isNaN`, `Number.isFinite` | Tool-calling code routinely receives numbers as strings from host tools, JSON, logs, CSV-ish inputs, or scraped text. These helpers are low-complexity, high-utility, and keep parsing in guest code instead of forcing extra host capabilities. | `crates/mustard/src/runtime/builtins/install.rs` exposes only the `Number` constructor. `crates/mustard/src/runtime/properties.rs` gives `Number` no static helpers beyond constructor metadata. |
| Small string-formatting helpers: `trimStart`, `trimEnd`, `padStart`, `padEnd` | This is common glue code for IDs, report formatting, fixed-width outputs, and normalization. It is materially useful for agent-written code and much cheaper than broader semantic work like classes or modules. | `crates/mustard/src/runtime/builtins/strings.rs` implements `trim`, `includes`, `startsWith`, `endsWith`, `slice`, `substring`, case conversion, `split`, `replace`, `replaceAll`, `search`, `match`, and `matchAll`, but not these helpers. `crates/mustard/src/runtime/properties.rs` also does not expose them on strings. |

### Medium confidence

These are real gaps in the code and would help realistic workloads, but either
their value is narrower or their semantic cost is noticeably higher.

| Gap | Why it may be worth adding | Code evidence |
| --- | --- | --- |
| Additional array reducers/search helpers: `reduceRight`, `findLast`, `findLastIndex` | The current array surface is already good, so these are incremental rather than foundational. They would reduce friction for generated code and are still far cheaper than widening the object model. | `crates/mustard/src/runtime/builtins/arrays.rs` and `crates/mustard/src/runtime/properties.rs` expose `map`, `filter`, `find`, `findIndex`, `some`, `every`, `flat`, `flatMap`, and `reduce`, but not the reverse-direction helpers. |
| `Intl` subset, likely starting with `Intl.DateTimeFormat` and `Intl.NumberFormat` | `USE_CASE_GAPS_2.md` is directionally right that locale-aware formatting matters for real operations and finance workflows. The problem is size: even a conservative `Intl` surface is much larger and trickier than the helper gaps above. | There is no `Intl` global installed in `crates/mustard/src/runtime/builtins/install.rs`, and `tests/test262/manifest.js` / existing docs treat it as absent. |
| Async iteration / generator support beyond the current conservative `for await...of` | Streaming and paged-cursor workloads are a believable next step for the product. But this is no longer "just add a helper"; it affects parser validation, IR, runtime iteration state, and snapshot semantics. | `crates/mustard/src/parser/tests/rejections.rs` and `tests/test262/manifest.js` keep generators/yield unsupported. `crates/mustard/src/runtime/properties.rs` only creates iterators for arrays, strings, `Map`, `Set`, and existing iterator helper objects. |

### Low confidence

These are definitely current gaps, but they cut against the repo's explicit
product boundaries or would expand semantics so much that they no longer look
like obvious next additions.

| Gap | Why confidence is low | Code evidence |
| --- | --- | --- |
| Classes, user-defined constructor instances, and broader prototype inheritance | This is a major compatibility win for generic JS, but it directly conflicts with the repo's conservative object model. Adding it would ripple through `new`, `instanceof`, property lookup, object layout, and diagnostics. | `tests/node/language-gaps.test.js` only supports conservative `instanceof`; guest constructors still produce `false`. `crates/mustard/src/runtime/builtins/objects.rs` explicitly rejects `Object.create`, and `tests/test262/manifest.js` marks classes unsupported. |
| Symbol-based iteration and custom iterables | This would unlock more generic JS patterns, but it also drags in `Symbol`, iterator protocol semantics, and broader protocol surface that the runtime has deliberately avoided. | `crates/mustard/src/runtime/properties.rs` hardcodes the iterable set in `create_iterator(...)`. `tests/node/conformance-contract.js` contains explicit unsupported cases for `Symbol`. |
| Modules and guest-side imports | This is one of the biggest raw compatibility wins for agent-written code, but it is badly misaligned with the current project goals. The repo repeatedly frames `mustard` as script-only with explicit host capabilities rather than package/module loading. | `crates/mustard/src/parser/tests/rejections.rs` rejects module syntax. `tests/test262/manifest.js` marks `import` and dynamic `import()` unsupported. The README's non-goals explicitly exclude module loaders and npm compatibility. |
| Typed arrays / `ArrayBuffer` / binary payloads | Real workloads do hit binary data, but the current runtime and host boundary are intentionally structured around JSON-like values. Adding binary types would be substantial work and still would not fully solve the broader "native object boundary" issue by itself. | No binary globals are installed in `crates/mustard/src/runtime/builtins/install.rs`. `USE_CASE_GAPS_2.md` correctly identifies byte-oriented workflows, but the current structured boundary and builtin surface do not support them. |

## Notable non-gaps

These are the main items that older gap lists would be wrong to keep promoting
as current priorities:

- default parameters and default destructuring
- destructuring assignment
- array spread and spread arguments over the supported iterable surface
- object spread for plain objects and arrays
- `for...in`
- `for await...of` over the supported iterable surface
- sparse array holes
- `instanceof` on the documented conservative surface
- `Map` / `Set` constructors plus iteration helpers
- guest promises, Promise combinators, and promise instance methods
- `RegExp` plus the documented string interop helpers

Those all have direct implementation and test coverage in the current tree.

## Recommended order

If the goal is to close the highest-value gaps without diluting the product
boundary, the next additions I would prioritize are:

1. Broaden `Date` just enough for formatting and UTC extraction.
2. Add `Number` parsing/predicate statics.
3. Add the small string-formatting helpers.
4. Re-evaluate `Intl` only after the narrower helper work lands.
