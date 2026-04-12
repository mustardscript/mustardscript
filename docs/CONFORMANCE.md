# Conformance Strategy

`jslite` currently uses two active conformance buckets:

1. Node-parity programs
   These are guest programs that should evaluate the same way in `jslite` and
   Node for the documented subset. We exercise them with direct differential
   tests, generated property tests, and the curated `test262` pass manifest.
2. Validator-rejected programs
   These are syntactic or statically recognizable forms that should never reach
   runtime. We exercise them with generated validation-negative cases and the
   curated `test262` unsupported manifest.

The contract format still reserves a third `known_divergence` bucket so new
audited mismatches can be tracked explicitly if they are discovered, but there
are no current documented divergence entries in the machine-readable contract.

## Generated Coverage

The machine-readable source of truth for these outcomes lives in
`tests/node/conformance-contract.js`.

Unsupported contract entries now record both the expected phase
(`constructor`/validation or `runtime`) and a diagnostic category, so the
negative suites can assert fail-closed behavior without depending on full
message equality.

The property-based conformance generator is split intentionally:

- `SUPPORTED_PARITY_FAMILIES` splits Node-parity generation into independent
  semantic families such as control flow, exceptions, objects/arrays,
  keyed collections, async promises, and capability traces. Each family runs
  as its own property test with isolated shrinking.
- `REJECTION_FAMILIES` does the same for fail-closed coverage, so unsupported
  syntax, ambient globals, binding errors, operator rejects, runtime surface
  rejects, and missing global built-ins all shrink independently.
- `supportedProgramArbitrary` remains the mixed Node-parity source used by the
  broader mixed conformance property. Those programs must compile and
  differentially match Node.
- `unsupportedValidationCaseArbitrary` only emits programs that should fail
  during constructor-time validation with explicit diagnostics.
- `conformanceCaseArbitrary` mixes both domains and asserts that each generated
  case has only two legal outcomes: Node-equivalent execution or validation
  failure.
- `ast-conformance.js` adds a second generated layer that works on a small
  typed AST, supports bounded exhaustive enumeration, renders trace-sensitive
  programs, and feeds metamorphic rewrites from the same source AST.

When a family property fails, the test output prints the fast-check seed and
shrink path plus the minimized guest program and a canonical outcome or trace
diff instead of a raw object mismatch.

This is more useful than a naive source fuzzer because the generated programs
stay inside deliberate semantic buckets and produce canonicalizable outputs.

The contract also carries a curated rejection-regression slice for phase- and
category-sensitive cases such as ambient globals, unsupported operators, and
deferred object-model built-ins like `Object.create`, `Object.freeze`, and
`Object.seal`.

## Fixture Coverage

The curated `test262` subset complements the generated layer:

- `pass` fixtures are stable regression cases inside the Node-parity subset.
- `unsupported` fixtures are stable regression cases for explicit validator
  exclusions.

When adding coverage, prefer one of these paths:

- add a new generated family when the behavior is a broad semantic class
- add a new curated fixture when the behavior is a stable regression or an
  exact parser diagnostic category
- add an audit test when the behavior is especially regression-prone or relies
  on observable ordering, rendering, or trace details
