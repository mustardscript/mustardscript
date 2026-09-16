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
