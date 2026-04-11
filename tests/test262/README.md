# Curated `test262` Subset

This directory contains a small, committed conformance subset used to exercise
`jslite`'s documented v1 language contract.

Rules for this subset:

- `manifest.js` is the source of truth for which fixtures are intentionally in
  scope.
- `pass` cases are expected to match both `jslite` and Node for the selected
  supported subset.
- `unsupported` cases are explicit exclusions. Each one records a concrete
  reason instead of relying on accidental gaps.
- Fixtures are organized under canonical-style `test262` paths so future
  expansion can stay deliberate.

This is intentionally a harnessable subset, not a wholesale vendoring of the
upstream `test262` repository.
