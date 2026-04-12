# Hardening Guide

This document tracks the hostile-input and fuzzing entry points that back Phase
10 of the roadmap.

## Verified Suites

Run the maintained hardening checks with:

```bash
scripts/run-hardening.sh
```

That runs:

- `cargo test -p jslite --test security_hostile_inputs`
- `cargo test -p jslite-sidecar --test hostile_protocol`
- `cargo check --manifest-path fuzz/Cargo.toml --bins`

## Fuzz Targets

The `fuzz/` package contains libFuzzer entry points for the main untrusted
input boundaries:

- `parser`
- `ir_lowering`
- `bytecode_validation`
- `bytecode_execution`
- `snapshot_load`
- `sidecar_protocol`

Example usage after installing `cargo-fuzz`:

```bash
cargo fuzz run parser --manifest-path fuzz/Cargo.toml
cargo fuzz run sidecar_protocol --manifest-path fuzz/Cargo.toml
```

## Denial-of-Service Audit

Current hostile-input pressure points and mitigations:

- Parser and IR lowering accept untrusted source text. Property tests cover
  arbitrary byte slices, and sidecar mode remains the recommended deployment
  boundary for adversarial inputs.
- Bytecode and snapshot loading treat serialized blobs as untrusted. The
  verified suites mutate valid blobs, fuzz loaders with arbitrary bytes, and
  assert that failures stay host-safe. The dedicated
  `crates/jslite/tests/snapshot_policy_security.rs` suite also verifies that
  loaded snapshots cannot resume until the host reasserts allowed capability
  names and explicit runtime limits.
- Bytecode execution is bounded by instruction, heap, allocation, and call-depth
  limits. The hardening suite injects over-budget workloads and checks for
  guest-safe failures.
- Sidecar protocol decoding treats every input line as hostile. The sidecar
  library path is fuzzable directly and has integration coverage for malformed,
  truncated, and semantically hostile requests.
- Cooperative cancellation is now part of the core runtime boundary. The new
  cancellation suite asserts guest-safe failures both for in-flight compute and
  for suspended async host waits. Hard-stop behavior still remains OS-process
  termination in sidecar mode when hosts need preemptive kill guarantees.

Residual risk notes:

- The parser/lowering path is still recursive. The hostile-source test runs in a
  larger-stack thread to keep the regression suite reproducible while avoiding
  false negatives from the default test-thread stack.
- In-process addon mode is still best-effort containment only. Use sidecar mode
  plus host-managed OS sandboxing for adversarial deployments.

## Kill and Cancellation Coverage

The current kill/cancellation evidence is intentionally split by deployment
mode:

- Core cooperative cancellation is covered by
  `crates/jslite/tests/cancellation.rs`, which interrupts running guest code and
  cancels suspended async host waits without letting guest `try` / `catch`
  convert the host abort into ordinary guest control flow.
- Addon-mode cancellation plumbing is covered by
  `tests/node/cancellation.test.js`, which verifies already-aborted signals,
  explicit `Progress.cancel()`, and cancellation while guest async code is
  awaiting a host promise.
- Addon-mode delayed-await behavior is still covered by
  `tests/node/coverage-audit.test.js`, which verifies that merely dropping or
  delaying the caller's immediate await does not itself inject cancellation.
- Sidecar hard-stop behavior is covered by
  `crates/jslite-sidecar/tests/protocol.rs`, which forcefully terminates a live
  sidecar process and verifies that a fresh sidecar can be started cleanly
  afterward.
- Same-thread addon compute is still cooperative rather than preemptive. If the
  host needs a hard kill while native execution is on the Node main thread,
  sidecar termination remains the stronger control.
