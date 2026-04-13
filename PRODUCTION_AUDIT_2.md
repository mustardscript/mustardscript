# Production Audit 2

Generated on 2026-04-12 for `/Users/mini/jslite`.

## Verdict

`jslite` is materially stronger than the earlier audit baseline: `cargo test --workspace`,
`npm test`, and `npm run lint` all passed in this tree, and several previously
reported snapshot/cancellation issues are already fixed.

That said, the repository is still not production-ready for hostile or
high-value workloads. This pass found no live `CRITICAL` issues, but it did find
multiple `HIGH` issues across snapshot validation, sidecar transport, and
release/install integrity.

## Verification Run

- `cargo test --workspace`
- `npm test`
- `npm run lint`
- `npm pack --dry-run --json --silent`
- Focused `node` repro for guest structured export behavior

## Findings

### 1. `HIGH` Async snapshot state is not fully validated before restore

- Evidence:
  - Snapshot validation walks `runtime.frames`, heap values, promises, and host
    call metadata in
    [crates/jslite/src/runtime/validation/snapshot.rs](/Users/mini/jslite/crates/jslite/src/runtime/validation/snapshot.rs:3),
    but does not walk `AsyncContinuation.frames` or microtask frame payloads.
  - The walker for those async frames already exists in
    [crates/jslite/src/runtime/validation/walk.rs](/Users/mini/jslite/crates/jslite/src/runtime/validation/walk.rs:139).
  - Async continuation and microtask state are serialized in
    [crates/jslite/src/runtime/state.rs](/Users/mini/jslite/crates/jslite/src/runtime/state.rs:434).
  - Docs currently claim snapshot loads validate live frame pointers and promise
    state before restore in
    [docs/SERIALIZATION.md](/Users/mini/jslite/docs/SERIALIZATION.md:26).
- Why this matters:
  - A tampered or corrupt snapshot can carry malformed async frame graphs past
    the restore gate and fail only later when an async continuation or microtask
    is activated.
- Coverage gap:
  - I did not find a malformed-async-snapshot rejection test proving those
    async frame paths are rejected up front.

### 2. `HIGH` The live sidecar transport accepts unbounded request lines

- Evidence:
  - The sidecar main loop reads `stdin.lock().lines()` without any byte cap in
    [crates/jslite-sidecar/src/main.rs](/Users/mini/jslite/crates/jslite-sidecar/src/main.rs:6).
  - The protocol parser itself is line-oriented JSON in
    [crates/jslite-sidecar/src/lib.rs](/Users/mini/jslite/crates/jslite-sidecar/src/lib.rs:94).
- Why this matters:
  - One oversized line can force a large allocation or OOM before the
    fail-closed JSON/protocol validation runs.
- Coverage gap:
  - The hostile protocol tests and fuzzing focus on the parser/library path, not
    the transport reader that accumulates the line first.

### 3. `HIGH` Source installs build a debug native addon by default

- Evidence:
  - The install path runs `cargo build` without `--release` in
    [install.ts](/Users/mini/jslite/install.ts:95).
  - The install regression test locks in that exact argument list in
    [tests/node/security-install.test.js](/Users/mini/jslite/tests/node/security-install.test.js:119).
  - The README and release guide both describe source-build install as the
    default/baseline path in [README.md](/Users/mini/jslite/README.md:85) and
    [docs/RELEASE.md](/Users/mini/jslite/docs/RELEASE.md:8).
- Why this matters:
  - Production consumers who take the documented baseline path get a larger,
    slower debug-profile addon unless they separately install a prebuilt.

### 4. `HIGH` The baseline npm source-build path is not reproducible

- Evidence:
  - The package ships Rust sources and manifests via
    [package.json](/Users/mini/jslite/package.json:7), but does not ship
    `Cargo.lock`.
  - `npm pack --dry-run --json --silent` for the current tree confirmed the
    tarball contents omit `Cargo.lock`.
  - The repo documents source-build installation as the default verified path in
    [README.md](/Users/mini/jslite/README.md:87) and
    [docs/RELEASE.md](/Users/mini/jslite/docs/RELEASE.md:9).
- Why this matters:
  - `npm install` on the packed artifact can resolve newer semver-compatible
    Rust dependencies than the ones maintainers actually tested, which weakens
    release integrity on the primary install path.

### 5. `HIGH` Sidecar protocol has no version/compatibility handshake

- Evidence:
  - The request surface only defines `compile`, `start`, and `resume` in
    [crates/jslite-sidecar/src/lib.rs](/Users/mini/jslite/crates/jslite-sidecar/src/lib.rs:8).
  - The protocol doc defines the current request/response shape but no version
    field or negotiation path in
    [docs/SIDECAR_PROTOCOL.md](/Users/mini/jslite/docs/SIDECAR_PROTOCOL.md:12).
- Why this matters:
  - Independent host/sidecar rollout can degrade into opaque request failures or
    semantic drift instead of a clean incompatibility check.
- Coverage gap:
  - Current equivalence tests prove same-repo behavior, not mixed-version host
    and sidecar interoperability.

### 6. `HIGH` Release verification does not prove the shipped package can build and run sidecar mode

- Evidence:
  - The npm package intentionally ships sidecar and bridge sources in
    [package.json](/Users/mini/jslite/package.json:14).
  - The release verifier checks packed file presence and Rust package manifests,
    but not a consumer-installed sidecar launch path, in
    [scripts/release-verify.ts](/Users/mini/jslite/scripts/release-verify.ts:42).
  - The package smoke test only exercises addon usage through
    `require('@keppoai/jslite')` in
    [tests/package-smoke.test.js](/Users/mini/jslite/tests/package-smoke.test.js:162).
- Why this matters:
  - A release can pass current verification while a consumer-installed package
    still cannot build or launch the sidecar path.

### 7. `MEDIUM` Guest structured export is fail-open for many non-plain objects

- Evidence:
  - Structured export rejects `Date`, `Map`, `Set`, functions, and `BigInt`, but
    otherwise serializes `Value::Object` by copying own properties in
    [crates/jslite/src/runtime/conversions/boundary.rs](/Users/mini/jslite/crates/jslite/src/runtime/conversions/boundary.rs:126).
  - The host API contract says only plain objects and arrays are allowed across
    the boundary and that class instances, custom prototypes, and host objects
    are rejected in [docs/HOST_API.md](/Users/mini/jslite/docs/HOST_API.md:6).
  - Direct repro in this tree returned `[{},{},{}]` for:
    - `new Jslite('new Number(1);').run()`
    - `new Jslite('/a/;').run()`
    - `new Jslite('fetch_data(new Number(1));').start(...).args[0]`
- Why this matters:
  - The runtime silently widens the export surface and erases semantics instead
    of failing closed per the documented contract.

### 8. `MEDIUM` Optional prebuilt verification is only partial

- Evidence:
  - The root package metadata has `napi.targets` but no root
    `optionalDependencies` wiring in
    [package.json](/Users/mini/jslite/package.json:27).
  - The prebuilt smoke test manually installs both the root tarball and a
    host-specific prebuilt tarball in
    [tests/package-smoke.test.js](/Users/mini/jslite/tests/package-smoke.test.js:201).
  - The release guide describes this as a separate optional flow in
    [docs/RELEASE.md](/Users/mini/jslite/docs/RELEASE.md:52).
- Why this matters:
  - The repo proves a staged two-tarball scenario, not the final publish-time
    auto-resolution path a public consumer would actually rely on.

### 9. `MEDIUM` The published package still lacks a `LICENSE` file

- Evidence:
  - The repository root currently has no `LICENSE`, `LICENSE.md`, or
    `LICENSE.txt`.
  - `npm pack --dry-run --json --silent` for this tree confirmed the tarball
    does not contain a license file.
  - The package metadata still declares `MIT` in
    [package.json](/Users/mini/jslite/package.json:73).
- Why this matters:
  - Public open-source distribution should ship the actual license text, not
    only metadata, because many legal/compliance reviews require the file.

### 10. `MEDIUM` `SECURITY.md` still does not provide a real private disclosure path

- Evidence:
  - The security policy says only “Report security issues privately to the
    maintainers” in [SECURITY.md](/Users/mini/jslite/SECURITY.md:17).
- Why this matters:
  - There is no security email, advisory workflow, supported-version policy, or
    response expectation for an external reporter.

## Already Improved Since The Earlier Audit

- `cargo test --workspace`, `npm test`, and `npm run lint` all passed in this
  tree.
- Progress/snapshot replay handling is stricter now: same-process
  `Progress.load(...)` requires explicit restore policy and rejects consumed
  dumps before exposing metadata.
- Cancellation token IDs are now random rather than predictable.
- The host boundary now caps array length at 1,000,000 elements.

## Recommendation

Do not market the current tree as production-ready yet.

Before that claim, address findings 1 through 6 at minimum. Findings 7 through
10 should also be fixed before a public npm release intended for production
consumption.
