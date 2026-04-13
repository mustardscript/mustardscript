# Security Issues 8

This review treated addon/in-process mode as if it needed to be a hard security
boundary, even though the repository docs explicitly do not promise that.
Targets were selected from the native loading path, the Node wrapper, the
snapshot/restore boundary, and runtime budget/accounting code. Only issues that
survived skeptical verification or direct local reproduction are listed here.

## Reviewed Targets

- `native-loader.js`
- `lib/runtime.js`
- `lib/structured.js`
- `lib/policy.js`
- `lib/progress.js`
- `lib/executor.js`
- `crates/jslite-node/src/lib.rs`
- `crates/jslite-bridge/src/operations.rs`
- `crates/jslite/src/runtime/conversions/boundary.rs`
- `crates/jslite/src/runtime/serialization.rs`
- `crates/jslite/src/runtime/validation/snapshot.rs`
- `crates/jslite/src/runtime/builtins/primitives.rs`

## Critical: `Progress.load(...)` exposes authoritative metadata before the snapshot is burned

Affected files:
- `lib/progress.js:61-69`
- `lib/progress.js:94-108`
- `lib/progress.js:187-246`
- `lib/executor.js:424-489`
- `docs/HOST_API.md:124-127`

Impact:
Two same-process `Progress.load(dumped)` calls both succeed before either one
burns the suspended snapshot. Both loaded handles expose authoritative
`progress.capability` and `progress.args`, so a host that dispatches on that
metadata can trigger the same side effect twice even though only one later
`resume()` succeeds. Under a hard-boundary assumption, that is a critical
replay/confused-restore bug for one-shot actions such as approvals, writes, or
payments.

Validation notes:
An independent verifier confirmed the code path, and I reproduced it locally.
Two loaded handles both exposed `approve(["order-7"])`; the first `resume()`
returned `"ok"` and the second failed with `Progress objects are single-use`.

Short repro:
```js
const { Jslite, Progress } = require('./index.js');
const started = new Jslite('const x = approve("order-7"); x;').start({
  snapshotKey: Buffer.from('verify-progress-load-key'),
  capabilities: { approve() {} },
});
const dumped = started.dump();
const a = Progress.load(dumped);
const b = Progress.load(dumped);
console.log(a.capability, a.args, b.capability, b.args);
console.log(a.resume('ok'));
console.log(b.resume('ok')); // throws only after metadata was already exposed twice
```

## Critical: same-isolate progress-policy caching allows restore-time policy rebinding without the victim `snapshotKey`

Affected files:
- `lib/policy.js:208-217`
- `lib/policy.js:236-299`
- `lib/progress.js:75-85`
- `lib/progress.js:126-145`
- `lib/progress.js:187-246`
- `lib/runtime.js:73-84`

Impact:
`Progress.start()` caches `{ policy, snapshotKey }` by dump token. A later
same-isolate caller can load a stolen dump with attacker-chosen
`capabilities`/`limits` while omitting `snapshotKey`; `resolveProgressLoadContext()`
falls back to the cached victim key for authentication, then returns the
attacker's freshly built policy. That lets the attacker rebind restore-time
limits and steer resumed execution while never knowing the victim secret key.
Under a hard-boundary assumption, that is a critical same-process
cross-tenant/cross-policy boundary break.

Validation notes:
An independent verifier confirmed the behavior, and I reproduced both variants
locally. `Progress.load(dumped, { capabilities, limits })` succeeded with no
`snapshotKey`; `resume(1337)` then produced a new suspension on
`write_audit([1337])`. A separate repro showed the attacker could also lower the
resume-time instruction budget enough to force `instruction budget exhausted`.

Short repro:
```js
const { Jslite, Progress } = require('./index.js');
const dumped = new Jslite(`
  const secret = fetch_data(7);
  const next = write_audit(secret);
  next;
`).start({
  snapshotKey: Buffer.from('victim-only-key'),
  capabilities: { fetch_data() {}, write_audit() {} },
  limits: { instructionBudget: 5_000_000 },
}).dump();

const hijacked = Progress.load(dumped, {
  capabilities: { fetch_data() {}, write_audit() {} },
  limits: { instructionBudget: 5_000_000 },
});
console.log(hijacked.capability, hijacked.args);
const next = hijacked.resume(1337);
console.log(next.capability, next.args); // write_audit [1337]
```

## Critical: predictable global cancellation token IDs allow cross-session cancellation in the same process

Affected files:
- `crates/jslite-node/src/lib.rs:22-25`
- `crates/jslite-node/src/lib.rs:32-35`
- `crates/jslite-node/src/lib.rs:37-48`
- `crates/jslite-node/src/lib.rs:135-161`
- `lib/cancellation.js:30-56`

Impact:
The addon stores cancellation tokens in one process-global registry and names
them `cancel-<n>` using a monotonic counter. Any same-process code that can load
the addon can guess or brute-force live token IDs and cancel or release another
caller's execution. Under a hard-boundary assumption, that is a critical
cross-session DoS/control break.

Validation notes:
An independent verifier confirmed the behavior, and I reproduced it locally with
a `worker_threads` harness. The worker created a token and entered a long native
execution; the main thread called `native.cancelCancellationToken('cancel-1')`
without learning the token through the worker API surface, and the worker died
with `Limit: execution cancelled`.

Short repro:
```js
const native = require('./native-loader').loadNative();
// In another same-process context:
native.cancelCancellationToken('cancel-1');
```

## Critical: sparse-array host-value encoding is an uncancellable same-process DoS

Affected files:
- `lib/structured.js:91-107`
- `lib/structured.js:210-219`
- `lib/runtime.js:33-42`
- `lib/cancellation.js:30-55`

Impact:
`encodeStructuredArray()` allocates `new Array(value.length)` and walks every
index from `0` to `length - 1`, including holes, before any native cancellation
token or runtime budget exists. A hostile caller can hand `run()` or `start()`
a giant sparse array and pin the Node event loop for seconds or longer even
when the `AbortSignal` is already aborted. Under a hard-boundary assumption,
that is a critical same-process availability break.

Validation notes:
An independent verifier confirmed the claim, and I reproduced it locally.
Passing a sparse array with `length = 5_000_000` under an already-aborted signal
still spent about `1113ms` in boundary marshalling before the call returned
`JsliteLimitError: execution cancelled`.

Short repro:
```js
const { Jslite } = require('./index.js');
const sparse = [];
sparse.length = 5_000_000;
const ac = new AbortController();
ac.abort();
await new Jslite('0;').run({ inputs: { sparse }, signal: ac.signal });
```

## Critical: `JSON.parse()` bare-string results bypass `heapLimitBytes`

Affected files:
- `crates/jslite/src/runtime/builtins/primitives.rs:544-557`
- `crates/jslite/src/runtime/conversions/boundary.rs:188-205`
- `crates/jslite/src/runtime/accounting.rs:377-455`

Impact:
`call_json_parse()` parses into `serde_json::Value` and the string arm of
`value_from_json_inner()` returns `Value::String(value)` directly without heap
capacity checks or allocation accounting. Bare transient strings are not counted
by the heap recomputation path, so guest code can materialize multi-megabyte
strings under a tiny `heapLimitBytes` cap and still use them through string
helpers. Under a hard-boundary assumption, that is a critical memory-limit
bypass and same-process availability risk.

Validation notes:
An independent verifier confirmed the code path, and I reproduced it locally.
With `heapLimitBytes: 50_000`, `JSON.parse(<5 MB string>).length` returned
`5000000` and `.slice(0, 1)` returned `"x"` instead of failing with
`heap limit exceeded`.

Short repro:
```js
const { Jslite } = require('./index.js');
const payload = JSON.stringify('x'.repeat(5_000_000));
const limits = {
  heapLimitBytes: 50_000,
  allocationBudget: 20_000_000,
  instructionBudget: 100_000_000,
  callDepthLimit: 1000,
  maxOutstandingHostCalls: 1,
};
console.log(await new Jslite(`JSON.parse(${JSON.stringify(payload)}).length;`).run({ limits }));
console.log(await new Jslite(`JSON.parse(${JSON.stringify(payload)}).slice(0, 1);`).run({ limits }));
```

## Critical: `Number.parseInt()` / `Number.parseFloat()` bypass native-helper metering

Affected files:
- `crates/jslite/src/runtime/builtins/primitives.rs:240-323`
- `crates/jslite/src/runtime/mod.rs:301-314`

Impact:
Both helpers do large Rust-side scans of attacker-controlled strings without
calling `charge_native_helper_work()`. That means a single builtin call can burn
substantial same-process CPU while barely advancing the guest instruction
counter. `parseInt()` also clones large scratch strings (`remainder` and
`accepted`), which amplifies transient memory usage. Under a hard-boundary
assumption, that is a critical budget/cancellation bypass and same-process DoS
primitive.

Validation notes:
I reproduced this locally. With `instructionBudget: 32` and a `10_000_000`
character input string, `Number.parseFloat(text)` completed in about `324ms`
and returned `Infinity`; `Number.parseInt(text, 10)` completed in about `316ms`
and returned `0`. Neither threw `instruction budget exhausted`.

Short repro:
```js
const { Jslite } = require('./index.js');
const text = '9'.repeat(10_000_000);
const limits = {
  instructionBudget: 32,
  heapLimitBytes: 128 * 1024 * 1024,
  allocationBudget: 2_000_000,
  callDepthLimit: 1024,
  maxOutstandingHostCalls: 8,
};
console.log(await new Jslite('[Number.parseFloat(text) === Infinity]').run({ inputs: { text }, limits }));
console.log(await new Jslite('Number.parseInt(text, 10);').run({ inputs: { text }, limits }));
```
