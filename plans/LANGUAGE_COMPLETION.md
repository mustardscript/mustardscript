# Language completion: ranked features 1–20

Scope: implement the first twenty entries of the unified language-feature
ranking, with Rust-owned semantics, explicit limits and rejection behavior,
snapshot support, Rust/Node coverage, updated contracts, and a green PR.
Rank is priority, not dependency order. Existing behavior was audited in code,
language docs, tests, and comparative runtime probes before implementation.

## Verified milestones

- [ ] 1. JSON replacer, reviver, and indentation
- [ ] 2. Object and array property deletion
- [x] 3. `typeof` unresolvable identifiers, preserving TDZ and ambient policy
- [ ] 4. Error constructors, parse-error types, and guest-safe stack properties
- [ ] 5. `Array.from` array-like inputs
- [ ] 6. Array queue, copy, and copy-within methods
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
