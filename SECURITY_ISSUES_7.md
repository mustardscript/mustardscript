# Security Issues 7

This review focused on sandbox escapes that could affect other guest executions
or host capabilities.

Method:

- Selected 13 primary trust-boundary files from the repository threat model.
- Ran one `gpt-5.4` `xhigh` hunter per target file.
- Collapsed duplicate reports and excluded issues already documented in earlier
  `SECURITY_ISSUES*.md` files.
- Ran separate skeptical verifier agents for the surviving issues.
- Reproduced the surviving issues locally with focused read-only repros on
  2026-04-12 before recording them here.

## Reviewed Targets

- [`lib/structured.js`](lib/structured.js)
- [`lib/policy.js`](lib/policy.js)
- [`lib/progress.js`](lib/progress.js)
- [`lib/runtime.js`](lib/runtime.js)
- [`lib/executor.js`](lib/executor.js)
- [`crates/mustard-node/src/lib.rs`](crates/mustard-node/src/lib.rs)
- [`crates/mustard-sidecar/src/lib.rs`](crates/mustard-sidecar/src/lib.rs)
- [`crates/mustard-bridge/src/operations.rs`](crates/mustard-bridge/src/operations.rs)
- [`crates/mustard/src/runtime/api.rs`](crates/mustard/src/runtime/api.rs)
- [`crates/mustard/src/runtime/conversions/boundary.rs`](crates/mustard/src/runtime/conversions/boundary.rs)
- [`crates/mustard/src/runtime/serialization.rs`](crates/mustard/src/runtime/serialization.rs)
- [`crates/mustard/src/runtime/validation/snapshot.rs`](crates/mustard/src/runtime/validation/snapshot.rs)
- [`crates/mustard/src/runtime/vm.rs`](crates/mustard/src/runtime/vm.rs)

## Critical: restore-time policy rebinding skips closure-owned capability state

**Affected files**

- [`crates/mustard/src/runtime/validation/snapshot.rs`](crates/mustard/src/runtime/validation/snapshot.rs)
- [`crates/mustard/src/runtime/validation/policy.rs`](crates/mustard/src/runtime/validation/policy.rs)
- [`crates/mustard/src/runtime/validation/walk.rs`](crates/mustard/src/runtime/validation/walk.rs)
- [`crates/mustard/src/runtime/state.rs`](crates/mustard/src/runtime/state.rs)
- [`crates/mustard/src/runtime/properties.rs`](crates/mustard/src/runtime/properties.rs)
- [`crates/mustard/src/runtime/api.rs`](crates/mustard/src/runtime/api.rs)
- [`crates/mustard/src/runtime/serialization.rs`](crates/mustard/src/runtime/serialization.rs)

**Impact**

Forged snapshot bytes can stash an unauthorized `HostFunction("drop_table")`
inside `Closure.properties` while the visible suspended call still uses an
allowed capability such as `fetch_data`.

`load_snapshot(...)` and restore-time policy rebinding accept the snapshot
because snapshot validation and the restore-policy walk do not descend into
closure-owned values. After a benign `resume(...)`, guest code reads the forged
closure property and suspends on the hidden unauthorized capability. This lets
restored guest code escape the host's narrowed allowlist and steer host effects
outside the approved capability set.

**Validation notes**

- A skeptical verifier confirmed that `walk_heap_values()` and the policy walk
  omit `Closure.this_value`, `Closure.prototype`, and `Closure.properties`.
- A local throwaway Rust harness against the public `mustard` crate reproduced
  the bypass on the current checkout. Output:
  `{"hit":6,"capability":"drop_table","args":[String("boom")]}`
- Existing targeted coverage still passes:
  `cargo test -q -p mustard --test snapshot_policy_security`
  The current suite covers obvious suspended-call rewrites, but not
  closure-owned capability state.

**Short repro**

```js
function helper() {}
helper.backdoor = fetch_data;
const value = fetch_data(1);
helper.backdoor('boom');
```

Suspend on the first `fetch_data`, mutate the closure-carried `fetch_data`
bytes in the serialized snapshot to `drop_table`, then restore under policy
`{ capabilities: ['fetch_data'] }`. The current checkout resumes and next
suspends on `drop_table('boom')`.

## Critical: `MustardExecutor` does not bind stored progress to the owning job

**Affected files**

- [`lib/executor.js`](lib/executor.js)
- [`lib/progress.js`](lib/progress.js)
- [`lib/policy.js`](lib/policy.js)
- [`MUSTARD_EXECUTOR.md`](MUSTARD_EXECUTOR.md)

**Impact**

`MustardExecutor` uses one executor-wide `snapshotKey`, capability set, and
limit set. `_resumeWaitingJob(jobId)` blindly loads whatever blob
`store.loadProgress(jobId)` returns and never checks that the blob actually
belongs to that job.

If a stale or tampered store returns job A's still-live waiting snapshot for
job B, the executor dispatches A's capability and arguments, then writes the
resumed result back under job B. That lets one guest/job interfere with another
job's capability dispatch, cancellation path, and final result. In multi-tenant
hosts, the side effects of host capabilities can be misattributed across
tenants or workflows.

**Validation notes**

- A skeptical verifier confirmed the cross-job mixup.
- A local custom-store repro showed the victim job completing from the
  attacker's snapshot while the attacker job later failed single-use:
  `{"victim":{"state":"completed","result":"ATTACKER"},"attacker":{"state":"failed","error":{"name":"MustardRuntimeError","message":"Progress objects are single-use; this suspended execution was already resumed"}}}`
- Existing targeted coverage still passes:
  `node --test tests/node/executor.test.js`
  The current executor suite does not cover swapped waiting snapshots.

**Short repro**

- Use a custom executor store that captures attacker progress, returns it from
  `loadProgress('victim')`, and delays `loadProgress('attacker')` until the
  victim consumes it.
- Run two jobs under one executor with a shared `snapshotKey`.
- Current result: the victim job completes from the attacker's snapshot, and
  the attacker job then fails single-use.

## High: loading a second physical addon copy bypasses process-lifetime `Progress` single-use burn

**Affected files**

- [`lib/progress.js`](lib/progress.js)
- [`crates/mustard-node/src/lib.rs`](crates/mustard-node/src/lib.rs)
- [`docs/HOST_API.md`](docs/HOST_API.md)

**Impact**

The Node wrapper documents that consumed progress tokens stay burned for the
lifetime of the current process, including across `worker_threads`.

In practice, the burn set lives inside a `OnceLock<Mutex<HashSet<String>>>`
inside one loaded addon image. Loading a second physical copy of the addon in
the same PID creates a fresh burn registry. A dump consumed in package copy A
can therefore be loaded and resumed again in package copy B, replaying one-shot
host capability effects inside the same process.

**Validation notes**

- A skeptical verifier confirmed the bypass and traced it to the addon-local
  `used_progress_snapshots()` registry.
- A local repro copied `index.js`, `native-loader.js`, `lib/`, and the built
  `.node` file into a temp directory, required that temp package as copy B, and
  successfully resumed an already-consumed dump. Output:
  `{"firstResult":8,"sameCopyError":"Progress objects are single-use; this suspended execution was already resumed","secondCapability":"fetch_data","secondArgs":[4],"secondResult":8}`
- Existing targeted coverage still passes:
  `node --test tests/node/security-progress-load.test.js`
  The current suite covers same-copy reuse, `worker_threads`, churn, and
  cross-process restore, but not a second package copy in the same PID.

**Short repro**

- In package copy A, create and consume a `Progress` dump.
- Confirm copy A rejects reloading the same dump as single-use.
- Require a second physical package copy from a temp directory in the same PID
  and call `Progress.load(dumped, { snapshotKey, capabilities, limits }).resume(...)`.
- Current result: copy A throws the documented single-use error, while copy B
  resumes the already-consumed dump successfully.
