# Security Issues 4

This file records validated findings from the fourth threat-model review.
Each issue below was:

- first identified by a file-focused `gpt-5.4` `xhigh` hunter
- independently re-verified by a second `gpt-5.4` `xhigh` verifier
- checked again with focused local repros on the current checkout

## Reviewed Targets

These were the 12 primary files selected from the threat model as the most
security-relevant review targets:

- [`native-loader.js`](native-loader.js)
- [`install.js`](install.js)
- [`lib/structured.js`](lib/structured.js)
- [`lib/policy.js`](lib/policy.js)
- [`lib/progress.js`](lib/progress.js)
- [`crates/jslite-node/src/lib.rs`](crates/jslite-node/src/lib.rs)
- [`crates/jslite-sidecar/src/lib.rs`](crates/jslite-sidecar/src/lib.rs)
- [`crates/jslite/src/runtime/serialization.rs`](crates/jslite/src/runtime/serialization.rs)
- [`crates/jslite/src/runtime/validation/snapshot.rs`](crates/jslite/src/runtime/validation/snapshot.rs)
- [`crates/jslite/src/runtime/conversions/boundary.rs`](crates/jslite/src/runtime/conversions/boundary.rs)
- [`crates/jslite/src/runtime/builtins/primitives.rs`](crates/jslite/src/runtime/builtins/primitives.rs)
- [`crates/jslite/src/runtime/vm.rs`](crates/jslite/src/runtime/vm.rs)

## Critical: forged raw snapshots can rewrite suspended capability metadata in low-level inspect/resume flows

**Status:** fixed on `codex/security-issues-4`

**Affected files**

- [`crates/jslite/src/runtime/serialization.rs`](crates/jslite/src/runtime/serialization.rs)
- [`crates/jslite/src/runtime/validation/snapshot.rs`](crates/jslite/src/runtime/validation/snapshot.rs)
- [`crates/jslite/src/runtime/validation/policy.rs`](crates/jslite/src/runtime/validation/policy.rs)
- [`crates/jslite/src/runtime/api.rs`](crates/jslite/src/runtime/api.rs)
- [`crates/jslite-bridge/src/operations.rs`](crates/jslite-bridge/src/operations.rs)

**Impact**

Raw snapshot bytes accepted by `load_snapshot()` are not bound back to the
original suspended host call. Structural validation only checks pointer
consistency, and policy validation only checks that the forged capability name
is present in the caller-supplied allowlist.

As a result, a low-level caller that uses `inspectSnapshot` / `resumeProgram`
can be steered into dispatching an attacker-chosen allowed capability with
attacker-chosen arguments.

**Validation notes**

- Local repro confirmed that replacing same-length ASCII substrings inside a
  suspended snapshot changed the inspected capability from `fetch_data` to
  `drop_table`.
- The same forged snapshot resumed successfully through the low-level native
  API and completed with the forged handler result.
- Scope note: the higher-level Node `Progress.load(...)` wrapper adds an HMAC
  tamper check, but the core/bridge/native snapshot path still accepts the
  forged metadata.

**Short repro**

```js
const native = require('./native-loader').loadNative();
const { Jslite } = require('./index.js');

const progress = new Jslite('const value = fetch_data("read_only!"); value;').start({
  capabilities: { fetch_data() {}, drop_table() {} },
});
const forged = Buffer.from(progress.snapshot);
forged.set(Buffer.from('drop_table'), forged.indexOf('fetch_data'));
forged.set(Buffer.from('drop_users'), forged.indexOf('read_only!'));

const policy = JSON.stringify({ capabilities: ['fetch_data', 'drop_table'], limits: {} });
console.log(JSON.parse(native.inspectSnapshot(forged, policy)).capability); // drop_table
```

## Critical: forged promise-combinator snapshot state can panic and abort the process on resume

**Status:** fixed on `codex/security-issues-4`

**Affected files**

- [`crates/jslite/src/runtime/serialization.rs`](crates/jslite/src/runtime/serialization.rs)
- [`crates/jslite/src/runtime/validation/snapshot.rs`](crates/jslite/src/runtime/validation/snapshot.rs)
- [`crates/jslite/src/runtime/validation/walk.rs`](crates/jslite/src/runtime/validation/walk.rs)
- [`crates/jslite/src/runtime/async_runtime/reactions.rs`](crates/jslite/src/runtime/async_runtime/reactions.rs)
- [`crates/jslite/src/runtime/api.rs`](crates/jslite/src/runtime/api.rs)

**Impact**

`load_snapshot()` accepts forged promise-combinator state whose `index` is out
of bounds for the target `PromiseDriver::{All, AllSettled, Any}` storage. The
first resumed idle step then executes unchecked indexing in
`activate_promise_combinator()`, causing a Rust panic instead of a guest-safe
serialization error.

In addon mode this aborts the embedding Node process. In sidecar mode it kills
the worker process with one hostile snapshot restore.

**Validation notes**

- Local repro confirmed that mutating one byte in a suspended `Promise.all(...)`
  snapshot from `0` to `2` survived `load_snapshot()`.
- Resuming the forged snapshot through `native.resumeProgram(...)` aborted the
  child process with `index out of bounds: the len is 2 but the index is 2`.

**Short repro**

```js
const native = require('./native-loader').loadNative();
const { Jslite } = require('./index.js');

const progress = new Jslite(`
  async function main() { return Promise.all([fetch_data(1), fetch_data(2)]); }
  main();
`).start({ capabilities: { fetch_data() {} } });

const forged = Buffer.from(progress.snapshot);
forged[1943] = 2; // current checkout repro offset
native.resumeProgram(
  forged,
  JSON.stringify({ type: 'value', value: { Number: { Finite: 10 } } }),
  JSON.stringify({ capabilities: ['fetch_data'], limits: {} }),
); // aborts
```

## High: same-process `Progress.load()` exposes authoritative replay metadata before the single-use guard fires

**Status:** fixed on `codex/security-issues-4`

**Affected files**

- [`lib/progress.js`](lib/progress.js)
- [`lib/policy.js`](lib/policy.js)
- [`lib/executor.js`](lib/executor.js)

**Impact**

The same-process single-use burn is enforced only inside `resume()`,
`resumeError()`, and `cancel()`. `Progress.load(...)` still authenticates and
inspects an already-consumed dump, then returns authoritative `capability` and
`args`.

Normal host flows dispatch on `progress.capability` / `progress.args` before
calling `resume()`, so stale consumed dumps can still duplicate external side
effects even though the later `resume()` fails as single-use.

**Validation notes**

- Local repro confirmed that a consumed dump still reloads in the same process.
- Dispatching on the reloaded `capability` / `args` duplicated the simulated
  side effect.
- The later `resume()` correctly threw `Progress objects are single-use`, which
  is too late for hosts that already acted on the authoritative metadata.

**Short repro**

```js
const { Jslite, Progress } = require('./index.js');

let sideEffects = 0;
const runtime = new Jslite('const value = fetch_data(7); value * 2;');
const first = runtime.start({ capabilities: { fetch_data() {} } });
const dumped = first.dump();
first.resume(7);

const replay = Progress.load(dumped);
sideEffects += 1; // host dispatch based on replay.capability / replay.args
try { replay.resume(7); } catch {}

console.log({ capability: replay.capability, args: replay.args, sideEffects });
```

## High: host error sanitization executes prototype and coercion hooks before fail-closed rejection

**Status:** fixed on `codex/security-issues-4`

**Affected files**

- [`lib/structured.js`](lib/structured.js)
- [`lib/runtime.js`](lib/runtime.js)
- [`lib/progress.js`](lib/progress.js)

**Impact**

`encodeResumePayloadError()` rejects some unsupported error shapes, but it does
so only after reading fallback fields through `source.name`, `source.message`,
or `String(error)`. That allows inherited getters, proxy traps, or coercion
hooks to run during host-error sanitization.

This violates the boundary rule that hostile host values should fail closed
without executing host-side hooks.

**Validation notes**

- Local repro confirmed that an inherited `message` getter ran and polluted
  `Object.prototype` before the boundary threw.
- Independent verification also confirmed that proxy `getPrototypeOf` traps and
  `Symbol.toPrimitive` hooks run before later rejection paths.
- Both public host-error paths are affected: capability failures in `run()` and
  explicit `Progress.resumeError(...)`.

**Short repro**

```js
const { Jslite } = require('./index.js');

let ran = 0;
class EvilError extends Error {}
Object.defineProperty(EvilError.prototype, 'message', {
  get() { ran += 1; Object.prototype.__jslite_pwned = true; return 'boom'; },
});

try {
  await new Jslite('fetch_data();').run({
    capabilities: { fetch_data() { throw new EvilError(); } },
  });
} catch {}

console.log({ ran, polluted: Object.prototype.__jslite_pwned === true });
```

## High: structured boundary conversions bypass instruction-budget and mid-conversion cancellation checks

**Status:** fixed on `codex/security-issues-4`

**Affected files**

- [`crates/jslite/src/runtime/conversions/boundary.rs`](crates/jslite/src/runtime/conversions/boundary.rs)
- [`crates/jslite/src/runtime/mod.rs`](crates/jslite/src/runtime/mod.rs)
- [`crates/jslite/src/runtime/vm.rs`](crates/jslite/src/runtime/vm.rs)
- [`crates/jslite/src/runtime/async_runtime/scheduler.rs`](crates/jslite/src/runtime/async_runtime/scheduler.rs)

**Impact**

Large structured inputs, resume payloads, capability arguments, and completed
results are recursively converted without `charge_native_helper_work()` and
without mid-conversion cancellation polling.

That lets hostile structured values do large amounts of CPU work under tiny
instruction budgets and pushes cancellation observation until after conversion
finishes.

**Validation notes**

- Local repro confirmed that a `600000`-element array crossed `resume()` under
  `instructionBudget: 5` and still completed successfully after ~639ms.
- A second repro confirmed that returning a `600000`-element array across the
  host boundary also succeeded under the same tiny budget after ~1295ms.
- Independent verification confirmed the same gap on the ingress capability
  marshalling path.

**Short repro**

```js
const { Jslite } = require('./index.js');

const progress = new Jslite('fetch_data(); 0;').start({
  capabilities: { fetch_data() {} },
  limits: { instructionBudget: 5, heapLimitBytes: 1_000_000_000, allocationBudget: 2_000_000 },
});

const big = Array.from({ length: 600000 }, (_, i) => i);
console.log(progress.resume(big)); // 0
```
