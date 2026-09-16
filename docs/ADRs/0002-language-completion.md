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
