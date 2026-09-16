# Language completion: ranked features 1–20

Scope: implement the first twenty entries of the unified language-feature
ranking, with Rust-owned semantics, explicit limits and rejection behavior,
snapshot support, Rust/Node coverage, updated contracts, and a green PR.
Rank is priority, not dependency order. Existing behavior was audited in code,
language docs, tests, and comparative runtime probes before implementation.

## Verified milestones

- [x] 1. JSON replacer, reviver, and indentation
- [x] 2. Object and array property deletion
- [x] 3. `typeof` unresolvable identifiers, preserving TDZ and ambient policy
- [x] 4. Error constructors, parse-error types, and guest-safe stack properties
- [x] 5. `Array.from` array-like inputs
- [x] 6. Array queue, copy, and copy-within methods
- [ ] 7. Object/Map grouping
- [ ] 8. Object/Array compatibility helpers
- [ ] 9. RegExp character-class parity and match indices
- [ ] 10. URI encoding and decoding
- [ ] 11. UTC-profile Date completion
- [ ] 12. Deterministic, real en-US collation
- [ ] 13. Number and multi-currency formatting
- [ ] 14. Unicode normalization and character APIs with an explicit string contract
- [ ] 15. Bitwise/shift operators and assignments
- [ ] 16. BigInt conversion
- [ ] 17. Set algebra
- [ ] 18. Remaining Math helpers
- [ ] 19. Labeled break and continue
- [ ] 20. Promise.withResolvers and Array.fromAsync

## Verification and delivery

- [ ] `cargo test --workspace`
- [ ] `npm test`
- [ ] `npm run lint`
- [ ] PR opened; CI green on the final head commit

## Starting state

- Base: `27d4ae8` (`origin/main`); isolated worktree under `/Volumes/essd`.
- `cargo test -p mustard --lib`: 88 passed before changes.
- Existing npm lockfile disagrees with optional binding versions; the documented
  `npm install` flow is being used to reconcile it.
- GitHub CLI currently reports no repository push permission. Implementation and
  local verification do not depend on publication credentials.

## Milestone evidence

### Unresolvable `typeof`

- Added a dedicated name-resolution instruction; lexical TDZ and ambient-global
  validation remain unchanged. The instruction is stack-validated on snapshot load.
- Rust and Node language-completion tests cover missing names, parentheses, lexical
  and global bindings, TDZ, member evaluation, and forbidden ambient globals.
- Passed: `cargo test -p mustard --test language_completion`,
  `cargo test --workspace`, `npm run lint`, `npm run build`, and
  `node --test --test-reporter=dot tests/node/language-completion.test.js
  tests/node/language-gaps.test.js tests/node/exceptions.test.js
  tests/node/serialization.test.js` (23 tests). Node commands use Node 24, matching CI.

### Property deletion

- Implemented plain-object/array deletion, cache invalidation, sparse holes,
  incremental heap accounting and budgeted ordered removals. ADR 0002 records
  non-configurable and unsupported receiver/compound-chain behavior.
- Passed: `cargo test --workspace`, `npm run lint`, `npm run build`; Node
  language-completion, builtins, serialization, coverage-audit, property-differential,
  differential and Test262 suites (172 tests); deletion snapshot and hardening
  mutation-guard tests (7 tests).
- Updated stale hardening assumptions: default parameters already work, and
  snapshot load requires policy and claims single-use before resume.

### JSON options and callback safety

- Rust JSON module implements replacer functions/property lists, indentation,
  toJSON, boxed primitives, postorder revivers with true deletion, SyntaxError,
  live callback mutations, GC roots, depth/IO/work budgets and UTF-8-safe chunking.
- Fixed the shared synchronous callback runner to reject host suspension before
  the VM state can be transferred into a snapshot. Callback diagnostics survive.
- Passed: `cargo test --workspace`, `npm test` (672 Node tests passed, 1 skipped;
  type checks and 2 package-smoke tests passed), `npm run lint`. Focused tests
  include Node comparisons, callback exceptions, suspension rejection, GC pressure,
  instruction limits, Unicode IO, cycles and nesting limits.

### Error family and microtask GC roots

- Added EvalError, URIError, AggregateError, error toString/prototype metadata,
  non-enumerable error metadata, and snapshot-preserved guest-only stack strings.
- Native temporary roots no longer depend on a guest frame. GC also traces pending
  callback results/exceptions; Promise.any protects its constructed AggregateError.
- Passed: `cargo test --workspace`, `npm run lint`, `npm run build`, and the full
  Node suite (`node --test --test-reporter=dot tests/node/**/*.test.js`). Added
  constructor/Node parity, Promise.any identity, snapshot, and no-frame GC tests.

### Array-like construction and array completion

- Array.from accepts array-likes/boxed strings, captures length, reads live values,
  supports bound mapping callbacks and preflights backing-store capacity.
- Implemented shift/unshift/toSorted/toReversed/toSpliced/with/copyWithin with
  sparse/dense semantics, method metadata and prototype lookup. Sorting preserves
  stable identity, skips undefined comparator operands and handles callback mutation.
- Corrected one-argument splice; array allocation/sparse expansion is preflighted,
  and new array payloads are GC-rooted during allocation.
- Passed: `cargo test --workspace`, `npm run lint`, `npm run build`, full Node
  suite; 122 focused Node cases include direct Node comparisons, errors, snapshots
  and billion-element capacity rejection. Updated the superseded array-like
  rejection in builtin_surface to retain nullish-input rejection coverage.
