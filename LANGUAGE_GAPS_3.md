# LANGUAGE_GAPS_3

Third-pass language-surface audit based on reading the implementation code,
not documentation claims. Every gap listed below was verified by reading the
actual Rust source under `crates/jslite/src/`.

Audit inputs:

- `crates/jslite/src/runtime/builtins/install.rs` — builtin function dispatch
  and global installation
- `crates/jslite/src/runtime/properties.rs` — property resolution for all
  value types
- `crates/jslite/src/runtime/builtins/primitives.rs` — Math, JSON, Date,
  Number, String, Boolean constructors
- `crates/jslite/src/runtime/builtins/strings.rs` — string method
  implementations
- `crates/jslite/src/runtime/builtins/arrays.rs` — array method
  implementations
- `crates/jslite/src/runtime/builtins/collections.rs` — Map/Set constructors
  and methods
- `crates/jslite/src/runtime/conversions/operators.rs` — binary, unary, and
  update operator implementations
- `crates/jslite/src/runtime/conversions/coercions.rs` — type coercions
- `crates/jslite/src/runtime/conversions/errors.rs` — error object construction
- `crates/jslite/src/parser/operators.rs` — operator lowering and rejections
- `crates/jslite/src/parser/statements.rs` — statement lowering and rejections
- `crates/jslite/src/parser/expressions.rs` — expression lowering and rejections
- `crates/jslite/src/parser/scope.rs` — scope tracking and forbidden free
  references
- targeted code searches across the full `crates/jslite/src/runtime/` tree

## What this audit focuses on

The previous `LANGUAGE_GAPS_2.md` correctly identified the biggest categories.
This audit drills deeper into the implementation to find concrete,
smaller-scope gaps that are **actionable within the project's existing product
boundary** — the kind of things that would make realistic agent-written guest
code work more often without expanding the semantic model.

## High confidence — should be added

These are low-complexity, high-utility gaps where the implementation is clearly
missing, the feature fits cleanly within the existing runtime model, and
realistic agent/tool-calling code regularly uses them.

| # | Gap | Why it should be added | Code evidence |
|---|-----|------------------------|---------------|
| 1 | **Global `NaN`, `Infinity`, `isNaN()`, `isFinite()`, `parseInt()`, `parseFloat()`** | These are among the most commonly used globals in JavaScript. `parseInt` and `parseFloat` are routinely used for parsing numbers from string data returned by host tools. `isNaN` and `isFinite` are standard guard checks. None of these are ambient platform APIs — they are core ECMAScript globals. Their absence means `parseInt("42")` silently produces `undefined` or throws, which is a surprising failure for any JS author. | `install_builtins()` in `install.rs:159-328` installs `globalThis`, constructors, `Math`, `JSON`, and `console` — but no `NaN`, `Infinity`, `isNaN`, `isFinite`, `parseInt`, or `parseFloat` globals. No `BuiltinFunction` variants exist for them. `Number.isNaN`, `Number.isFinite`, `Number.parseInt`, `Number.parseFloat`, `Number.isInteger`, `Number.isSafeInteger` are also absent from `builtin_function_own_property` in `properties.rs:343-380`. |
| 2 | **`Number` static helpers: `Number.isNaN`, `Number.isFinite`, `Number.isInteger`, `Number.isSafeInteger`, `Number.parseInt`, `Number.parseFloat`** | These are the modern, non-coercing versions of the global helpers. Agent-generated code frequently uses `Number.isNaN(x)` or `Number.isFinite(x)` for input validation. `Number.isInteger` is common for checking array lengths, pagination offsets, and retry counts. | `builtin_function_own_property` for `NumberCtor` falls through to `None` — no static methods are exposed at all. The `Number` constructor only supports `Number(value)` coercion via `call_number_ctor`. |
| 3 | **`Number` constants: `Number.MAX_SAFE_INTEGER`, `Number.MIN_SAFE_INTEGER`, `Number.EPSILON`, `Number.MAX_VALUE`, `Number.MIN_VALUE`, `Number.POSITIVE_INFINITY`, `Number.NEGATIVE_INFINITY`, `Number.NaN`** | These constants are used for bound checks, precision guards, and safe-integer validation. Their absence forces agent code to hard-code magic numbers or do workarounds. Zero implementation cost — they are just static property returns. | Same as above: `builtin_function_own_property` for `NumberCtor` returns `None` for all keys except `name`, `length`, and `prototype`. |
| 4 | **String `trimStart()`, `trimEnd()`, `padStart()`, `padEnd()`** | `trimStart`/`trimEnd` are commonly used for cleaning structured text from host tools. `padStart` is ubiquitous for formatting IDs, hex values, timestamps, fixed-width output, and zero-padded numbers. These are simple pure-string operations with no semantic cost. | `get_property` for `Value::String` in `properties.rs:898-918` only resolves `trim`, `includes`, `startsWith`, `endsWith`, `slice`, `substring`, `toLowerCase`, `toUpperCase`, `split`, `replace`, `replaceAll`, `search`, `match`, `matchAll`. No `trimStart`, `trimEnd`, `padStart`, or `padEnd`. No corresponding `BuiltinFunction` variants or implementations exist. |
| 5 | **String `indexOf()`, `lastIndexOf()`** | `String.prototype.indexOf` is one of the most commonly used string methods in JavaScript. Agent code uses it constantly for finding substrings, checking position of delimiters, and text processing. `lastIndexOf` is used for finding file extensions, last occurrences in paths, etc. Arrays already have `indexOf` — the string counterpart is a glaring omission. | Not present in the string property resolution in `properties.rs:898-918`. No `StringIndexOf` or `StringLastIndexOf` in `BuiltinFunction`. Not in `strings.rs`. |
| 6 | **String `charAt()`, `at()`** | `charAt(i)` is the most common way to access individual characters in JS. `at()` is the modern equivalent with negative indexing. Arrays already support `at()` — string `at()` is the natural counterpart. | Not present in string property resolution or `BuiltinFunction` enum. |
| 7 | **String `repeat()`** | Commonly used for generating padding, separator lines, indentation in formatted output. Simple pure function with no semantic implications. | Not present in string property resolution or implementations. |
| 8 | **String `concat()`** | While `+` works for string concatenation, `concat()` is sometimes generated by code generators and is part of the basic string API. Very low cost to add. | Not present in string property resolution. |
| 9 | **`Math` constants: `Math.PI`, `Math.E`, `Math.LN2`, `Math.LN10`, `Math.LOG2E`, `Math.LOG10E`, `Math.SQRT2`, `Math.SQRT1_2`** | The `Math` object is already installed with 12 methods. The constants are static numeric properties that cost nothing to add and are used in any code doing geometric calculations, signal processing, or unit conversions. | `install_builtins` creates the `Math` object with only function entries (`abs`, `max`, `min`, `floor`, `ceil`, `round`, `pow`, `sqrt`, `trunc`, `sign`, `log`, `random`). No numeric constant entries. |
| 10 | **Wider `Date` surface: `toISOString()`, `toJSON()`, `getFullYear()`, `getMonth()`, `getDate()`, `getHours()`, `getMinutes()`, `getSeconds()`, `getMilliseconds()`, `getUTCFullYear()`, etc.** | The README positions jslite for freshness checks and SLA logic. The current `Date` surface is `Date.now()`, `new Date(value)`, and `getTime()`. Agent code that receives timestamps from host tools needs to extract components (year, month, day) or format them as ISO strings. Without these, every date formatting operation requires a host round-trip. `toISOString()` is particularly important since `JSON.stringify` already internally formats dates as ISO strings (see `json_stringify_date` in `primitives.rs:458-487`) — the formatting logic exists but isn't exposed as a guest method. | `get_property` for `ObjectKind::Date` in `properties.rs:712-722` only resolves `getTime`, `valueOf`, and `constructor`. `call_builtin` only dispatches `DateNow` and `DateGetTime`. The `json_stringify_date` function in `primitives.rs` already does ISO formatting internally but is not exposed as `Date.prototype.toISOString()`. |
| 11 | **Bitwise operators: `&`, `|`, `^`, `~`, `<<`, `>>`, `>>>`** | Bitwise operations are used in hash functions, flag manipulation, color processing, binary protocol handling, and performance-oriented integer math. Agent code that processes binary formats, implements simple hashing, or manipulates bitmask flags hits this. The parser currently falls through to `unsupported binary operator in v1` for all bitwise/shift operators. | `lower_binary_op` in `operators.rs:42-68` only maps arithmetic, comparison, `in`, and `instanceof` operators. `BitwiseAnd`, `BitwiseOr`, `BitwiseXor`, `ShiftLeft`, `ShiftRight`, `UnsignedShiftRight` all hit the `_ =>` unsupported branch. `lower_unary_op` catches `~` as unsupported. `lower_assign_op` only supports `=`, `+=`, `-=`, `*=`, `/=`, `||=`, `&&=`, `??=` — bitwise assignment forms like `|=`, `&=`, `^=`, `<<=`, `>>=`, `>>>=` are unsupported. |
| 12 | **Assignment operators: `%=`, `**=`** | These are the only non-bitwise compound assignment operators that are missing. `%=` is used in modular arithmetic (cycling indices, round-robin). `**=` is less common but still a gap since `**` itself is already supported. | `lower_assign_op` in `operators.rs:83-102` returns `None` for `Remainder` and `Exponential` assignment operators, falling through to the unsupported branch. |

## Medium confidence — worth considering

These are real gaps that would help realistic workloads, but either have higher
implementation cost, narrower applicability, or need more careful design
decisions.

| # | Gap | Why it may be worth adding | Code evidence |
|---|-----|----------------------------|---------------|
| 13 | **Array `reverse()`, `lastIndexOf()`, `fill()`** | `reverse()` is common for display ordering. `lastIndexOf()` is the counterpart to the already-supported `indexOf()`. `fill()` is used for initializing arrays. These are simple, well-defined operations. | Not present in `BuiltinFunction` enum, not in `arrays.rs`, not in the array property resolution in `properties.rs:813-838`. |
| 14 | **Array `findLast()`, `findLastIndex()`, `reduceRight()`** | Reverse-direction search and reduction helpers. The forward versions (`find`, `findIndex`, `reduce`) are already implemented. These are incremental additions to an already-good array surface. | Not present in any of the array-related source files. |
| 15 | **`Map.prototype.forEach()`, `Set.prototype.forEach()`** | `forEach` is already implemented for arrays. The `Map` and `Set` versions are common in agent code for iterating collections without `for...of`. The iteration infrastructure already exists. | `collections.rs` has no `forEach` method. `properties.rs` Map/Set resolution at lines 846-875 does not include `forEach`. The `BuiltinFunction` enum has no `MapForEach` or `SetForEach` variants. |
| 16 | **`Error.cause` support in error constructors** | ES2022 `Error.cause` is the standard way to chain errors. Agent code catching and re-throwing errors with `new Error("msg", { cause: originalError })` is increasingly common. The error constructor in `errors.rs:4-28` currently only accepts a message argument. | `make_error_object` in `errors.rs:4-28` creates error objects with `name`, `message`, and optionally `code`/`details` (from host errors), but does not inspect a second options argument for `cause`. The built-in `ErrorCtor` dispatch in `install.rs:116` passes `args` and a name string, with no options parsing. |
| 17 | **`SyntaxError` constructor** | The runtime internally generates `SyntaxError` messages (e.g., for invalid RegExp flags in `regexp.rs`), but there is no `SyntaxError` global constructor for guest code to use. All four other standard error types are exposed. | `install_builtins` registers `Error`, `TypeError`, `ReferenceError`, and `RangeError` constructors but not `SyntaxError`. No `SyntaxErrorCtor` variant in `BuiltinFunction`. |
| 18 | **Additional `Math` methods: `Math.exp()`, `Math.log2()`, `Math.log10()`, `Math.sin()`, `Math.cos()`, `Math.atan2()`, `Math.hypot()`, `Math.cbrt()`** | The current `Math` surface has 12 methods. The trigonometric and exponential functions are used in scoring algorithms, distance calculations, statistical analysis, and ML-adjacent code. These are pure numeric functions backed by Rust's `f64` methods. | No `MathExp`, `MathLog2`, `MathLog10`, `MathSin`, `MathCos`, `MathTan`, `MathAtan2`, `MathHypot`, `MathCbrt` variants exist in `BuiltinFunction`. The `Math` object in `install_builtins` only includes the 12 currently supported methods. |
| 19 | **`Map.prototype.size` and `Set.prototype.size` as actual properties, not just method-style access** | These already work via `get_property` returning `Value::Number(...)` for the key `"size"` — this is correct behavior. Flagging as a non-gap for clarity. However, `Map.prototype.forEach` and `Set.prototype.forEach` are real gaps (see #15). | `size` works correctly: `properties.rs:848` and `properties.rs:865` return `Value::Number(...)` for `"size"` on Map and Set. |
| 20 | **`URI` encoding/decoding globals: `encodeURI()`, `decodeURI()`, `encodeURIComponent()`, `decodeURIComponent()`** | Agent code that builds URLs for host capability calls, parses query strings, or constructs API request paths uses these constantly. These are core ECMAScript globals, not platform APIs. | Not installed as globals in `install_builtins`. No `BuiltinFunction` variants. Not in the forbidden-free-reference list in `parser/mod.rs`. |
| 21 | **`structuredClone()` global** | Agent code frequently uses `structuredClone()` for deep-copying objects before mutation. In the jslite context, this would only need to handle the supported value surface (primitives, plain objects, arrays). It is a common enough pattern that its absence causes friction. | Not installed as a global. No `BuiltinFunction` variant. Would need to handle the same structured-value surface that the host boundary already handles. |
| 22 | **`delete` operator for plain objects** | Agent code commonly does `delete obj.sensitive` before passing data to a host capability, or strips keys from response objects before aggregation. The current parser rejects all uses of `delete` during validation. A scoped version limited to plain-object own properties would avoid the harder descriptor and sparse-array questions while covering the most common use case. | `operators.rs:32-33` rejects `delete` unconditionally. The runtime's plain-object model uses `IndexMap<String, Value>` in `properties.rs`, so removing a key is mechanically trivial — the design cost is deciding the absence/enumeration semantics and whether array deletion is also in scope. |

## Low confidence — real gaps but cut against project boundaries

These are definitively missing features confirmed by reading the code, but they
either conflict with explicit v1 product boundaries, require substantial
semantic expansion, or have a questionable cost-benefit ratio for the target
workload.

| # | Gap | Why confidence is low | Code evidence |
|---|-----|----------------------|---------------|
| 22 | **Classes, `extends`, `super`, private fields** | Explicitly rejected in the parser with dedicated messages. Would require expanding the object model, `new`, `instanceof`, property lookup, and prototype chains. Directly conflicts with the "conservative object model" design decision. | `expressions.rs:437-438` rejects class expressions. `statements.rs:224` rejects class declarations. `expressions.rs:350-351` and `expressions.rs:411` reject private fields. `expressions.rs:406` rejects `super`. |
| 23 | **Generators and `yield`** | Explicitly rejected in the parser. Would require new runtime state machinery for suspended generator frames, affecting IR, bytecode, VM, and snapshot serialization. | `expressions.rs:10-11` rejects generator functions. `expressions.rs:426` rejects `yield`. |
| 24 | **Tagged template literals** | Explicitly rejected in the parser. Used for styled-components, GraphQL queries, SQL templating — patterns that are mostly irrelevant to agent tool-calling code. | `expressions.rs:430-431` rejects tagged templates. |
| 25 | **Symbols and `Symbol.iterator`** | Would unlock custom iterables and protocol-based patterns but requires a new value type, well-known symbol infrastructure, and expanded property lookup. Deliberately excluded from v1. | No `Symbol` global in `install_builtins`. `create_iterator` in `properties.rs:536-570` hardcodes the iterable set. |
| 26 | **`WeakMap`, `WeakSet`, `WeakRef`, `FinalizationRegistry`** | These require GC integration and weak-reference semantics. Explicitly out of scope for v1. | No globals installed, no `BuiltinFunction` variants. |
| 27 | **`Proxy` and `Reflect`** | Would fundamentally change the property access model. Explicitly out of scope. | Not installed. The validation layer in `policy.rs` rejects proxy-backed host values at the boundary. |
| 28 | **Typed arrays, `ArrayBuffer`, `SharedArrayBuffer`, `Atomics`** | Would require a binary value type, boundary changes, and memory model work. Explicitly out of scope for v1. | Not installed. |
| 29 | **ES modules (`import`/`export`) and dynamic `import()`** | Explicitly rejected in the parser and listed as a non-goal in the README. The project is deliberately script-only with host capabilities. | `statements.rs:213-214` rejects module syntax. `expressions.rs:394-395` rejects dynamic `import()`. |
| 30 | **`var` declarations** | Deliberately excluded from v1. The runtime only supports lexical `let`/`const` bindings. | `scope.rs:36-41` rejects `var` with a diagnostic. |
| 31 | **Full prototype inheritance and user-defined constructor `new`** | The runtime uses a flat property model without prototype chains. Would require fundamental redesign of property lookup, `Object.create`, `Object.getPrototypeOf`, and inheritance semantics. | `Object.create` is exposed as an explicit `TypeError` in `install.rs:42`. `instanceof` on guest closures returns `false` per `operators.rs:236`. |

## Notable non-gaps (things that work despite what you might expect)

These items are implemented and working in the current tree. They should not be
listed as gaps in future audits:

- Ternary/conditional expressions (`a ? b : c`) — `Conditional` variant exists in IR
- Sequence/comma expressions (`a, b, c`) — `Sequence` variant exists in IR
- Optional chaining (`?.`) and nullish coalescing (`??`)
- Template literals (non-tagged)
- `for...in` on plain objects and arrays
- `for await...of` over the supported iterable surface
- `BigInt` literals and exact-integer arithmetic
- `RegExp` constructor, `exec`, `test`, and string interop
- Sparse array holes across literals, access, helpers, JSON, and host boundary
- `AggregateError` — generated internally by `Promise.any` rejection
- `Object.assign` and `Object.fromEntries`
- `Array.from`, `Array.of`, `Array.isArray`
- `Array.prototype.splice`, `concat`, `at`, `sort`, `flat`, `flatMap`
- `JSON.stringify` and `JSON.parse` with full cycle detection
- `Math.random()` and `Math.log()`
- Conservative `instanceof` for all built-in constructors
- `in` operator for the supported property surface
- Default parameters and default destructuring
- Rest parameters and spread over the supported iterable surface
- Computed property keys in object literals (`{ [expr]: value }`)
- Object literal method shorthand

## Recommended priority order

If the goal is to close the highest-value gaps while staying inside the product
boundary:

1. **Globals**: `NaN`, `Infinity`, `isNaN`, `isFinite`, `parseInt`, `parseFloat` (#1)
2. **Number statics**: `Number.isNaN`, `Number.isFinite`, `Number.isInteger`, `Number.isSafeInteger`, `Number.parseInt`, `Number.parseFloat` (#2)
3. **Number constants**: `MAX_SAFE_INTEGER`, `MIN_SAFE_INTEGER`, `EPSILON`, etc. (#3)
4. **Math constants**: `Math.PI`, `Math.E`, etc. (#9)
5. **String helpers**: `indexOf`, `lastIndexOf`, `charAt`, `at`, `repeat`, `trimStart`, `trimEnd`, `padStart`, `padEnd`, `concat` (#4-8)
6. **Assignment operators**: `%=`, `**=` (#12)
7. **Date surface expansion**: `toISOString`, `toJSON`, UTC accessors (#10)
8. **Bitwise operators**: `&`, `|`, `^`, `~`, `<<`, `>>`, `>>>` (#11)
9. **Array reverse helpers**: `reverse`, `lastIndexOf`, `fill` (#13)
10. **Collection forEach**: `Map.prototype.forEach`, `Set.prototype.forEach` (#15)
11. **URI globals**: `encodeURIComponent`, `decodeURIComponent`, etc. (#20)
12. **Error.cause** and **SyntaxError constructor** (#16-17)
13. **Math expansion**: `exp`, `log2`, `log10`, `sin`, `cos`, `atan2`, `hypot` (#18)
14. **`structuredClone`** global (#21)
15. **Array search helpers**: `findLast`, `findLastIndex`, `reduceRight` (#14)
