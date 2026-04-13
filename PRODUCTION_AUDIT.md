# Production Audit

Generated on 2026-04-12 for `/Users/mini/jslite`.

## Verdict

`jslite` is not ready yet for production workloads that need strong limit enforcement, replay-safe progress handling, or public open-source release hygiene.

The broad test matrix is mostly strong:

- `cargo test --workspace` passed
- `npm test` passed
- `npm run lint` failed

The repository also still describes itself as an early-stage project in [`README.md`](README.md#status).

## Audit Scope

- Read `AGENTS.md`, `IMPLEMENT_PROMPT.md`, `README.md`, `Cargo.toml`, `package.json`, `TODOS.md`, core security docs, release docs, and CI workflows.
- Inspected the Rust core, Node wrapper, sidecar, serialization, structured-boundary, and progress/snapshot paths.
- Ran focused local reproductions for the findings below.
- Did not rerun the full fuzz/hardening script during this audit.

## Findings

### 1. `JSON.parse()` bare-string results bypass `heapLimitBytes`

- Importance: `critical`
- Affected code:
  - `crates/jslite/src/runtime/conversions/boundary.rs:188-200`
  - `crates/jslite/src/runtime/accounting.rs`
- Why this matters:
  - The runtime documents heap-byte limits as a core containment guarantee, but parsed JSON strings are created as `Value::String(value)` without heap-capacity or allocation accounting.
  - A guest can materialize multi-megabyte strings under a tiny heap cap and continue to use them.
- Confirmed evidence:
  - With `heapLimitBytes: 50_000`, `JSON.parse(<5 MB string>).length` returned `5000000`.
  - `JSON.parse(<5 MB string>).slice(0, 1)` returned `"x"`.
- Verification gap:
  - Existing limit tests cover `JSON.parse()` instruction budgeting and `JSON.stringify()` heap behavior, but not heap accounting for parsed bare strings:
    - `tests/node/limits.test.js:95-235`
    - `crates/jslite/tests/cancellation.rs:239-316`
- Fix before production:
  - Account JSON string construction the same way other heap-resident strings are accounted.
  - Add Rust and Node regression tests for large parsed strings under tight heap limits.

### 2. `Number.parseInt()` and `Number.parseFloat()` bypass native-helper metering

- Importance: `critical`
- Affected code:
  - `crates/jslite/src/runtime/builtins/primitives.rs:240-323`
- Why this matters:
  - Both helpers scan attacker-controlled strings in Rust without calling `charge_native_helper_work()`.
  - A single builtin call can consume substantial CPU while barely advancing the guest instruction counter and without observing cancellation checkpoints.
- Confirmed evidence:
  - With `instructionBudget: 32` and a 10,000,000-character input:
    - `Number.parseFloat(text) === Infinity` completed in about `205ms`
    - `Number.parseInt(text, 10) === 0` completed in about `346ms`
  - Neither call failed with `instruction budget exhausted`.
- Verification gap:
  - Current tests cover parse semantics, not budget/cancellation behavior for these helpers.
- Fix before production:
  - Make these helpers meter work in chunks and observe cancellation.
  - Add dedicated Node and Rust tests for both budget exhaustion and cancellation.

### 3. `Progress.load(...)` exposes authoritative metadata before the snapshot is actually burned

- Importance: `high`
- Affected code:
  - `lib/progress.js:61-69`
  - `lib/progress.js:94-108`
  - `lib/progress.js:187-246`
- Contract drift:
  - `docs/HOST_API.md:124-129` says `Progress.load(...)` rejects already-consumed dumps before exposing authoritative `progress.capability` and `progress.args`.
  - The implementation only burns the snapshot inside `#claimSnapshot()`, which runs later during `resume()`, `resumeError()`, or `cancel()`.
- Confirmed evidence:
  - Two same-process `Progress.load(dumped)` calls both succeeded and both exposed:
    - `capability: "approve"`
    - `args: ["order-7"]`
  - The first `resume("ok")` completed.
  - The second only failed when `resume("ok")` tried to claim the snapshot.
- Test drift:
  - The current tests encode this buggy behavior instead of rejecting it:
    - `tests/node/security-progress-load.test.js:111-117`
    - `tests/hardening/mutation-guards.test.js:111-121`
- Why this matters:
  - Hosts often dispatch side effects based on `progress.capability` / `progress.args` before calling `resume()`.
  - In that shape, duplicate loads can trigger duplicate approvals, writes, or queue actions even though only one later resume succeeds.
- Fix before production:
  - Claim/burn the snapshot before exposing metadata, or make load/dispatch/resume atomic.
  - Update docs and tests to match the fail-closed behavior.

### 4. Same-process `Progress.load(...)` allows policy rebinding without the caller providing `snapshotKey`

- Importance: `high`
- Affected code:
  - `lib/policy.js:250-317`
  - `lib/progress.js:227-245`
- Contract drift:
  - `docs/HOST_API.md:138-145`, `docs/SERIALIZATION.md:61-67`, and `SECURITY_THREAT_MODEL.md:320-332` describe same-process cache reuse as reusing cached policy for the same snapshot, not as authenticating with the cached key and then accepting attacker-chosen replacement `capabilities` / `limits`.
- Root cause:
  - When `Progress.load(state, options)` is called without `options.snapshotKey`, `resolveProgressLoadContext()` falls back to the cached same-process key at `lib/policy.js:298-301`.
  - It then returns a new policy from `createExecutionPolicy({ ...options, limits }).policy` at `lib/policy.js:314-316`.
- Confirmed evidence:
  - A dump created with a private `snapshotKey` could be loaded in the same process with no `snapshotKey` supplied.
  - The resumed execution accepted attacker-supplied policy and advanced to `write_audit([1337])`.
- Why this matters:
  - Same-process code that can read a dumped progress object can steer restore-time authority and limits without possessing the original key.
  - That breaks the intended meaning of explicit restore policy and weakens multi-tenant or plugin-heavy hosts.
- Fix before production:
  - If `options` are provided, require an explicit `snapshotKey`.
  - If the cache is used, reuse the cached policy verbatim and reject policy overrides.
  - Add regression tests for “options without snapshotKey” and “attempted capability/limit rebinding”.

### 5. Sparse-array host-value encoding is pre-runtime, uncancellable, and blocks the Node event loop

- Importance: `high`
- Affected code:
  - `lib/structured.js:91-107`
  - `lib/structured.js:210-219`
  - `lib/runtime.js:25-42`
  - `lib/cancellation.js:24-48`
- Why this matters:
  - Host input marshalling walks every array index, including holes, before native execution starts.
  - That work happens before runtime instruction limits apply and before the native cancellation token is passed into Rust.
  - The issue affects both addon mode and sidecar mode because the wrapper performs this work before IPC.
- Confirmed evidence:
  - With an already-aborted `AbortSignal` and a sparse array of length `2,000,000`, `run()` still spent about `491ms` before returning `JsliteLimitError: execution cancelled`.
- Verification gap:
  - Boundary tests cover cycles, proxies, and in-runtime budget checks, but not pre-native sparse-array denial of service.
- Fix before production:
  - Add host-boundary length caps or chunked/cancellable traversal.
  - Prefer moving boundary conversion into Rust if the runtime must enforce budgets on it.
  - Add regression coverage for already-aborted signals and giant sparse inputs.

### 6. Cancellation token IDs are predictable and process-global

- Importance: `medium`
- Affected code:
  - `crates/jslite-node/src/lib.rs:22-35`
  - `crates/jslite-node/src/lib.rs:135-161`
- Confirmed evidence:
  - Fresh tokens were issued as `cancel-1`, `cancel-2`, `cancel-3`.
- Why this matters:
  - Any same-process JavaScript that can load the addon can guess active token IDs and call `cancelCancellationToken()` or `releaseCancellationToken()` on another execution.
  - This is not a guest escape, but it is weak isolation for multi-tenant hosts, plugin ecosystems, or mixed-trust same-process code.
- Fix before production:
  - Use unguessable random IDs, or avoid public string token IDs entirely and keep cancellation handles opaque to userland.

### 7. The repository and packed npm tarball do not contain a `LICENSE` file

- Importance: `medium`
- Affected repo/package:
  - Root repository has no `LICENSE`, `LICENSE.md`, or `LICENSE.txt`
  - `npm pack --dry-run --json --silent` did not include a license file
- Why this matters:
  - `package.json` and `Cargo.toml` say `MIT`, but public open-source distribution should include the actual license text.
  - Many legal/compliance review pipelines expect the file, not only metadata.
- Fix before production/open-source release:
  - Add a canonical MIT `LICENSE` file at the repository root and ensure it is shipped in the npm package.

### 8. `SECURITY.md` does not provide an actual private disclosure channel

- Importance: `medium`
- Affected doc:
  - `SECURITY.md:17-19`
- Why this matters:
  - The file says to report security issues privately to the maintainers, but it gives no email address, advisory workflow, or other private route.
  - That is weak for an open-source project that expects production use.
- Fix before production/open-source release:
  - Add a monitored security email or GitHub Security Advisory instructions.
  - State expected response handling and supported disclosure path clearly.

### 9. The repository currently fails its own lint gate

- Importance: `low`
- Confirmed evidence:
  - `npm run lint` failed at `cargo fmt --all --check`.
  - The reported formatting drift was in:
    - `crates/jslite/src/parser/scope.rs`
    - `crates/jslite/src/parser/tests/rejections.rs`
    - `crates/jslite/src/runtime/vm.rs`
- Why this matters:
  - Release verification currently is not clean in the audited environment.
  - Even if this is only formatting drift, it blocks the documented verification path.
- Fix before release:
  - Format the affected files and rerun `npm run lint`.
  - If this is a rustfmt version skew issue, pin the toolchain/rustfmt version explicitly.

## Production Recommendation

Do not position the current tree as production-ready for adversarial or high-value workloads until findings 1 through 5 are fixed and covered by regression tests.

For public open-source release, findings 7 through 9 should also be addressed so the package is legally and operationally publishable with a credible disclosure path and a clean verification story.

## Commands Run

- `git status --short --branch`
- `cargo test --workspace`
- `npm test`
- `npm run lint`
- Focused `node - <<'NODE' ... NODE` reproductions for:
  - `Progress.load(...)` replay/metadata exposure
  - same-process policy rebinding without `snapshotKey`
  - sparse-array cancelled input handling
  - `JSON.parse()` heap-limit bypass
  - `Number.parseInt()` / `Number.parseFloat()` budget bypass
  - predictable cancellation token IDs
