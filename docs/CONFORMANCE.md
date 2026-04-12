# Conformance Strategy

`jslite` currently needs three distinct conformance buckets instead of one:

1. Node-parity programs
   These are guest programs that should evaluate the same way in `jslite` and
   Node for the documented subset. We exercise them with direct differential
   tests, generated property tests, and the curated `test262` pass manifest.
2. Validator-rejected programs
   These are syntactic or statically recognizable forms that should never reach
   runtime. We exercise them with generated validation-negative cases and the
   curated `test262` unsupported manifest.
3. Documented divergences
   A few currently documented surfaces intentionally do not match Node today,
   such as sorted-key object enumeration and `JSON.stringify` rendering. Those
   cases are covered by targeted audit tests and remain explicit blockers to a
   universal “Node parity or validation rejection” claim.

## Generated Coverage

The property-based conformance generator is split intentionally:

- `supportedProgramArbitrary` only emits programs from the current Node-parity
  subset. Those programs must compile and differentially match Node.
- `unsupportedValidationCaseArbitrary` only emits programs that should fail
  during constructor-time validation with explicit diagnostics.
- `conformanceCaseArbitrary` mixes both domains and asserts that each generated
  case has only two legal outcomes: Node-equivalent execution or validation
  failure.

This is more useful than a naive source fuzzer because the generated programs
stay inside deliberate semantic buckets and produce canonicalizable outputs.

## Fixture Coverage

The curated `test262` subset complements the generated layer:

- `pass` fixtures are stable regression cases inside the Node-parity subset.
- `unsupported` fixtures are stable regression cases for explicit validator
  exclusions.

When adding coverage, prefer one of these paths:

- add a new generated family when the behavior is a broad semantic class
- add a new curated fixture when the behavior is a stable regression or an
  exact parser diagnostic category
- add an audit test when the current behavior is intentionally documented but
  not Node-parity
