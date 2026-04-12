# Curated `test262` Subset

This directory contains a curated, committed conformance subset used to
exercise `jslite`'s documented v1 language contract.

Rules for this subset:

- `manifest.js` is the source of truth for which fixtures are intentionally in
  scope.
- `pass` cases are expected to match both `jslite` and Node for the selected
  Node-parity subset.
- pass fixtures belong under `cases/pass/**`; if a fixture is meant to pin a
  specific machine-readable contract entry, `manifest.js` records that with a
  `contractId`.
- `unsupported` cases are explicit exclusions. Each one records a concrete
  reason instead of relying on accidental gaps.
- Fixtures are organized under canonical-style `test262` paths so future
  expansion can stay deliberate.
- This directory complements the generated property harness described in
  [docs/CONFORMANCE.md](../../docs/CONFORMANCE.md); it is the deterministic
  regression layer, not the only conformance layer.

This is intentionally a harnessable subset, not a wholesale vendoring of the
upstream `test262` repository.
