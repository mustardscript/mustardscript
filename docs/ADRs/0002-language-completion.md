# ADR 0002: Language completion within the sandbox object model

## Status

Accepted for the ranked language-completion work.

## Property deletion

The earlier blanket deletion deferral is superseded for plain objects and arrays.
Plain-object entries are configurable own data properties. Arrays have configurable
own indices and extra data properties; their length is non-configurable. Removing
an index leaves an existing sparse-array hole, never shifting or shrinking the array.
JSON renders holes as null, enumeration skips them, and the existing host/snapshot
sparse-array contracts are unchanged. Reinserted non-index keys enumerate last.

Computed keys and receivers are evaluated once. Optional-member deletion skips key
evaluation on nullish receivers. Compound optional-chain deletion is rejected until
the existing chain IR can preserve short-circuit boundaries exactly. Deleting a
binding is a validation error in the strict-mode profile; deleting non-configurable
array/string properties throws TypeError. Special objects, globals, and callable
property deletion fail closed rather than inventing descriptor/prototype semantics.

Mutation materializes shaped objects to invalidate property caches and updates heap
accounting. Ordered removals are charged proportionally to their work. The new
bytecode instruction is validated when loading serialized programs.

## Prototype-less grouping results

Object.groupBy uses a distinct prototype-less data-object kind, not an ordinary
object with inherited fallback methods. This implements observable absence for
constructor/toString and treats __proto__ as own data. The kind is preserved in
guest snapshots and is otherwise handled like a plain object for data operations.
It does not enable arbitrary prototype mutation. The structured host boundary
continues to transport own data only. See the normative algorithms for
[Object.groupBy](https://tc39.es/ecma262/multipage/fundamental-objects.html#sec-object.groupby)
and [Map.groupBy](https://tc39.es/ecma262/multipage/keyed-collections.html#sec-map.groupby).

## Explicit Unicode string profile

Retain the existing well-formed Unicode scalar string representation and its
scalar-based length, indexing, slicing, iteration and regexp offsets. Migrating
all strings, keys, serialization and host transport to arbitrary UTF-16 would be
an incompatible object/boundary redesign, not a helper addition. New `charCodeAt`
and `codePointAt` expose an explicit UTF-16 *numeric view*, using ECMAScript unit
offsets; `fromCharCode` accepts complete UTF-16 sequences (including surrogate
pairs) and `fromCodePoint` accepts Unicode scalar values. Attempts to create lone
surrogate strings throw RangeError; source literals/templates/keys containing
lone surrogates are validation rejects. UTF-8 Node boundary encoding rejects them
before a lossy conversion; Rust transport already requires valid Unicode.

`isWellFormed` is therefore true for every supported string. JSON parsing and URI
decoding retain their SyntaxError/URIError failures for malformed encoded text.
The parser must not expose Oxc's internal lone-surrogate escape representation.
NFC/NFD/NFKC/NFKD use the maintained
[unicode-normalization implementation](https://docs.rs/unicode-normalization/latest/unicode_normalization/trait.UnicodeNormalization.html),
with decomposition-buffer allocation and sorting work preflighted against limits.


## Linear regexp profile and indices

Keep the Rust regex engine; this milestone does not add a backtracking fallback.
Lower character classes through regex-syntax into explicit scalar sets, preserving
ASCII word/digit semantics and flag-sensitive case folding. The unrepresentable
`iu` long-s/Kelvin-sign word-boundary case rejects on affected input. Match indices
count scalars, matching existing string/regexp offsets, not the separate UTF-16
numeric character view. Named indices share guest array identity and survive
snapshots; no new host-boundary aliasing permission is introduced.


## Fixed UTC local Date semantics

Declare UTC as the guest runtime's local time zone. This supersedes the earlier
local Date API deferral without adding host-zone dependencies or time-zone data.
Zone-less ISO input uses UTC; local and UTC component constructors/getters/setters
therefore agree. Pure checked calendar arithmetic extends the existing proleptic
Gregorian/time-clip implementation. Numeric-field locale rendering reuses the
existing en-US/UTC Intl policy. Display strings use fixed English names and UTC.
This does **not** make the clock deterministic: Date.now and zero-argument Date
continue to consume the existing host clock. Host-boundary Date rejection stays.
See [Date algorithms](https://tc39.es/ecma262/multipage/numbers-and-dates.html#sec-date-objects).

## Collation data and binary-size tradeoff

Use ICU4X `icu_collator` 2.3.1 with exact-pinned 2.3.0 normalization, collation
and locale-fallback data for en-US string comparison. The maintained Unicode
collator is preferable to an approximate hand-written comparator. This knowingly
adds compiled data to the binary; no system locale, host callback, networking or
locale-dependent process state is involved. Data changes require an explicit
version update and parity review. Snapshot values contain only the builtin
identity, not engine state. Native buffer capacity and combining-run work are
bounded before comparison.

Sensitivity maps to ICU strength/case-level as documented by
[ICU4X](https://docs.rs/icu_collator/latest/icu_collator/options/struct.CollatorOptions.html).
The supported options implement the sorting portion of
[ECMA-402 localeCompare](https://tc39.es/ecma402/#sec-string.prototype.localecompare);
unsupported locale/search profiles reject rather than silently approximate.

## Number formatting and currency data

Use compact tables derived from Unicode CLDR 48.2.1, pinned at commit
`26a79cb42bfcc90def764102aa2af126d9ef3108`, with Unicode-3.0 notices included in
Rust and npm distributions. Sources are the standard (not cash) fraction digits
in [currencyData.json](https://github.com/unicode-org/cldr-json/blob/26a79cb42bfcc90def764102aa2af126d9ef3108/cldr-json/cldr-core/supplemental/currencyData.json)
and default English symbols in [currencies.json](https://github.com/unicode-org/cldr-json/blob/26a79cb42bfcc90def764102aa2af126d9ef3108/cldr-json/cldr-numbers-full/main/en/currencies.json).
Do not derive precision from a currency symbol or silently assume every currency
has two decimals. Well-formed unknown codes retain the ECMA-402 two-digit fallback.

The bounded binary64 formatter rounds its shortest decimal representation with
half-expand and shifts percent in decimal, avoiding Rust's binary half-even
formatting and intermediate percent overflow. This follows the supported Number
portion of [ECMA-402 number formatting](https://tc39.es/ecma402/#sec-formatnumeric).
It intentionally does not add arbitrary-precision Intl string/BigInt inputs or
new Intl option families. Formatter fields keep their serialized layout, while
snapshot validation now bounds digit counts and checks currency configuration.

## Native callback and JSON stack safety

Guest frame depth and per-document JSON depth do not compose into a native stack
bound: a callback can start another nested traversal while its caller's traversal
remains on the stack. Use one runtime-local native-depth budget for both paths,
with weighted callback entries and refundable JSON traversal entries (see
[Limits](../LIMITS.md)). Do not serialize this transient native-stack state.
The already-pinned `stacker` dependency also supplies guarded stack segments on
native targets, so debug and instrumented frames and 1 MiB host threads have
enough headroom to reach the budget check. Keep recursive crash regressions in
subprocesses and run them in debug, release, and hardening verification.

## Pre-parse delimiter nesting

The CI-equivalent sidecar fuzz smoke exposed stack exhaustion inside Oxc on deeply
nested source before runtime limits could apply. Oxc 0.124 does not expose a
nesting option (nor does the audited 0.150 parser); enabling its benchmark-only
lexer backdoor would bypass safety invariants. Use the public, pinned SWC 44.0.0
lexer for a 64-level delimiter/template preflight, retaining Oxc for parsing
and language validation. Scanning errors fail closed; punctuation in lexical
literals/comments cannot hide or consume code nesting. Preserve the discovered
request family in protocol tests and both relevant fuzz corpora, and replay the
actual saved crash under ASan before claiming the fix verified. RESS 0.11.7 was
rejected during validation after fuzzing exposed an infinite loop on malformed
template escapes; that input is also preserved as a regression and corpus seed.

The hardening runner also bounds each fuzz input to five seconds by default
(`MUSTARD_FUZZ_TIMEOUT_SECONDS`), so a hung helper produces a failing saved input
instead of outliving the whole smoke-test budget. This tightens the checks.
