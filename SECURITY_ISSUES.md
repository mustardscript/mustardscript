# Security Issues

This file records confirmed critical findings from the threat-model review and
their current status.
Entries below were originally validated with focused local repros or
child-process crash / timeout repros. All listed findings are now fixed on
`main` as of 2026-04-11. The sections remain here as a historical record of the
original issue statements and repros.

## Critical: `__proto__` keys can rewrite the prototype of host-visible plain objects

**Status:** fixed on `main`

**Affected files**

- [`index.js`](index.js)
- [`docs/HOST_API.md`](docs/HOST_API.md)

**Impact**

The Node wrapper reconstructs structured objects with ordinary `{}` assignment.
A guest-controlled `__proto__` key is interpreted as the prototype mutator on
`Object.prototype`, not as plain data.

That means guest output can cross the boundary as a host-visible object with
attacker-controlled inherited properties. Host logic that treats returned values
as plain data objects can be steered by `in` checks, truthiness checks,
destructuring, or ad hoc authorization logic.

The same unsafe assignment pattern also affects host-to-guest encoding, so host
inputs containing `__proto__` do not round-trip as documented.

**Short repro**

```js
const { Jslite } = require('./index.js');

(async () => {
  const result = await new Jslite(
    `({ ['__proto__']: { admin: true }, user: 'alice' });`,
  ).run();

  console.log(Object.hasOwn(result, 'admin')); // false
  console.log(result.admin); // true
})();
```

**Suggested fix**

Encode and decode structured objects with a prototype-safe representation. Do
not use plain property assignment on `{}` for attacker-controlled keys.

## Critical: enumerable accessors execute instead of being rejected at the host boundary

**Status:** fixed on `main`

**Affected files**

- [`index.js`](index.js)
- [`docs/HOST_API.md`](docs/HOST_API.md)

**Impact**

The documented host boundary says accessors are rejected. The wrapper does not
enforce that. It serializes arrays with `map(...)` and objects with
`Object.entries(...)`, both of which execute enumerable getters instead of
failing closed.

That lets guest execution coerce host getter logic into running and materialize
its results as ordinary guest data. If a host capability returns an object with
lazy getters or if the host passes one as input, the guest can observe the
materialized value even though the contract says accessors should never cross
the boundary.

**Short repro**

```js
const { Jslite } = require('./index.js');

(async () => {
  let getterRuns = 0;
  const value = {};
  Object.defineProperty(value, 'secret', {
    enumerable: true,
    get() {
      getterRuns += 1;
      return 'top-secret';
    },
  });

  const result = await new Jslite('value.secret;').run({ inputs: { value } });
  console.log(result); // "top-secret"
  console.log(getterRuns); // 1
})();
```

**Suggested fix**

Inspect property descriptors before reading values and reject getter/setter
descriptors explicitly.

## Critical: guest-created cycles crash the process instead of failing closed

**Status:** fixed on `main`

**Affected files**

- [`crates/jslite/src/runtime.rs`](crates/jslite/src/runtime.rs)
- [`docs/HOST_API.md`](docs/HOST_API.md)
- [`README.md`](README.md)

**Impact**

The boundary contract says cycles are rejected. The runtime path that converts
guest values into `StructuredValue` does not implement cycle detection. Cyclic
guest arrays or objects recurse until the native stack overflows.

In addon mode this crashes the host Node process. In sidecar mode it kills the
sidecar process instead of returning a guest-safe boundary error.

The crash is reachable from:

- root result serialization
- host capability argument serialization
- `JSON.stringify(...)`

**Short repro**

```js
const { spawnSync } = require('node:child_process');

const child = spawnSync(process.execPath, ['-e', `
  const { Jslite } = require('./index.js');
  (async () => {
    await new Jslite('const o = {}; o.self = o; o;').run();
  })();
`], {
  cwd: process.cwd(),
  encoding: 'utf8',
});

console.log(child.status); // null
console.log(child.signal); // "SIGSEGV"
```

**Suggested fix**

Track visited guest heap objects while converting to `StructuredValue` and fail
closed on cycles instead of recursing indefinitely.

## Critical: regex-backed built-ins bypass instruction budgeting and cancellation

**Status:** fixed on `main`

**Affected files**

- [`crates/jslite/src/runtime.rs`](crates/jslite/src/runtime.rs)
- [`README.md`](README.md)

**Impact**

Supported regex-backed string helpers delegate into `regress::Regex` matching
without incrementing the VM instruction budget and without polling the
cancellation token while the regex engine runs.

That allows a tiny guest program to trigger catastrophic backtracking and pin
the runtime even when the host configured a very small instruction budget.

Representative affected surface includes:

- `String.prototype.search`
- `String.prototype.match`
- regex-backed `replace` / `replaceAll`
- regex-backed `split`
- `RegExp.prototype.exec`
- `RegExp.prototype.test`

**Short repro**

```js
const { spawnSync } = require('node:child_process');

const payload = `
  const { Jslite } = require('./index.js');
  (async () => {
    const text = 'a'.repeat(200) + '!';
    await new Jslite('text.search(/^(a+)+$/);').run({
      inputs: { text },
      limits: { instructionBudget: 20 },
    });
  })();
`;

const child = spawnSync(process.execPath, ['-e', payload], {
  cwd: process.cwd(),
  encoding: 'utf8',
  timeout: 2000,
  killSignal: 'SIGKILL',
});

console.log(child.error && child.error.code); // "ETIMEDOUT"
console.log(child.signal); // "SIGKILL"
```

**Suggested fix**

Do not run guest-controlled regex matching in an unmetered backtracking engine.
Either move to a regex engine with hard complexity guarantees or add a separate
metered and cancellable regex execution path.

## Critical: `Progress.load()` trusts capability metadata that is not bound to the snapshot

**Status:** fixed on `main`

**Affected files**

- [`index.js`](index.js)
- [`examples/agent-style.js`](examples/agent-style.js)
- [`docs/HOST_API.md`](docs/HOST_API.md)

**Impact**

`Progress.dump()` emits `capability`, `args`, `snapshot`, and `token`.
`Progress.load()` trusts caller-supplied `capability` and `args`, but
`resume()` ignores them and resumes using only the serialized snapshot.

That means a tampered persisted progress object can mislead the host before
`jslite` regains control:

1. the host loads persisted progress
2. the host dispatches on `progress.capability` and `progress.args`
3. the host performs the wrong side effect
4. `resume()` still resumes the original suspended runtime state

No snapshot-byte forgery is required.

**Short repro**

```js
const { Jslite, Progress } = require('./index.js');

const runtime = new Jslite('const value = fetch_data(1); value + 1;');
const first = runtime.start({
  capabilities: {
    fetch_data(value) {
      return value;
    },
  },
});

const forged = Progress.load({
  ...first.dump(),
  capability: 'drop_table',
  args: ['users'],
});

const calls = [];
const handlers = {
  fetch_data(value) {
    calls.push(['fetch_data', value]);
    return value;
  },
  drop_table(name) {
    calls.push(['drop_table', name]);
    return 1;
  },
};

const hostResult = handlers[forged.capability](...forged.args);
const completed = forged.resume(hostResult);

console.log(calls); // [ [ 'drop_table', 'users' ] ]
console.log(completed); // 2
```

**Suggested fix**

Do not trust `capability` or `args` supplied to `Progress.load()`. Derive them
from the snapshot itself during load/resume or reject loaded progress metadata
that is not structurally bound to snapshot contents.

## Critical: `Progress.load()` token spoofing bypasses the documented single-use guarantee

**Status:** fixed on `main`

**Affected files**

- [`index.js`](index.js)
- [`docs/HOST_API.md`](docs/HOST_API.md)

**Impact**

The documented contract says `Progress.dump()` / `Progress.load()` preserve
single-use identity within one Node process. The implementation enforces that
only through a JavaScript `Set` keyed by the caller-controlled `token` field.

By cloning a dumped progress object and changing `token`, an attacker can
resume the same suspended snapshot more than once and obtain duplicated
follow-on capability requests.

That is a replay vulnerability for one-shot approval or side-effecting flows.

**Short repro**

```js
const { Jslite, Progress } = require('./index.js');

const runtime = new Jslite('const token = auth(); spend(token);');
const first = runtime.start({
  capabilities: {
    auth() {},
    spend() {},
  },
});

const dumped = first.dump();
const a = Progress.load({ ...dumped, token: 'token-a' });
const b = Progress.load({ ...dumped, token: 'token-b' });

const nextA = a.resume('ALLOW');
const nextB = b.resume('ALLOW');

console.log(nextA.capability, nextA.args); // spend [ 'ALLOW' ]
console.log(nextB.capability, nextB.args); // spend [ 'ALLOW' ]
```

**Suggested fix**

Bind replay protection to immutable snapshot identity in the native layer or to
metadata derived from the snapshot itself, not to a mutable caller-supplied
token.

## Critical: forged snapshots can rewrite capability-bearing runtime state

**Status:** fixed on `main`

**Affected files**

- [`crates/jslite/src/runtime.rs`](crates/jslite/src/runtime.rs)
- [`docs/SERIALIZATION.md`](docs/SERIALIZATION.md)
- [`docs/HOST_API.md`](docs/HOST_API.md)

**Impact**

Serialized snapshots preserve capability-bearing runtime state and
`load_snapshot()` restores it without rebinding that authority to current host
policy.

At minimum, directly serialized `HostFunction(String)` values are accepted on
load and become live callable capability references again after restore. A
forged snapshot can therefore change which host capability the runtime asks for
later, even though capability access is supposed to be explicit host authority.

**Short repro**

```js
const { Jslite, Progress } = require('./index.js');

function replaceAllAscii(buffer, from, to) {
  const a = Buffer.from(from, 'utf8');
  const b = Buffer.from(to, 'utf8');
  for (let i = 0; i <= buffer.length - a.length; i++) {
    if (buffer.subarray(i, i + a.length).equals(a)) {
      b.copy(buffer, i);
      i += a.length - 1;
    }
  }
}

(async () => {
  const runtime = new Jslite(`
    const first = fetch_data(1);
    const second = fetch_data(2);
    [first, second];
  `);

  const progress = runtime.start({
    capabilities: {
      fetch_data(value) {
        return value;
      },
    },
  });

  const dumped = progress.dump();
  const snapshot = Buffer.from(dumped.snapshot);
  replaceAllAscii(snapshot, 'fetch_data', 'drop_table');

  const forged = Progress.load({ ...dumped, snapshot, token: 'forged-token' });
  const next = forged.resume(1);
  console.log(next.capability); // "drop_table"
})();
```

**Suggested fix**

Do not deserialize ambient capability references from snapshots. Rebind them at
load/resume time against a host-provided allowlist or reject snapshots that
contain live capability-bearing state.

## Critical: forged snapshots can raise runtime limits and bypass host policy

**Status:** fixed on `main`

**Affected files**

- [`crates/jslite/src/limits.rs`](crates/jslite/src/limits.rs)
- [`crates/jslite/src/runtime.rs`](crates/jslite/src/runtime.rs)
- [`index.js`](index.js)
- [`crates/jslite-sidecar/src/lib.rs`](crates/jslite-sidecar/src/lib.rs)
- [`docs/SERIALIZATION.md`](docs/SERIALIZATION.md)
- [`docs/LIMITS.md`](docs/LIMITS.md)

**Impact**

Snapshots serialize `RuntimeLimits` inside the restored runtime state, and
resume paths do not let the host reassert instruction, heap, allocation,
call-depth, or outstanding-host-call limits.

That means a forged snapshot can silently raise the policy chosen by the host
at `start()`. A hostile blob can therefore buy more CPU, memory, stack depth,
or host-call fan-out on resume than the host originally allowed.

**Short repro**

```js
const { Jslite, Progress } = require('./index.js');

function makeDump(instructionBudget) {
  const runtime = new Jslite(`
    const ready = fetch_data(1);
    let total = 0;
    for (let i = 0; i < 10000; i = i + 1) {
      total = total + 1;
    }
    total;
  `);
  return runtime.start({
    limits: { instructionBudget },
    capabilities: { fetch_data(value) { return value; } },
  }).dump();
}

const low = makeDump(50);
const high = makeDump(5_000_000);

const lowSnapshot = Buffer.from(low.snapshot);
const highSnapshot = Buffer.from(high.snapshot);
for (let i = 0; i < lowSnapshot.length; i++) {
  if (lowSnapshot[i] !== highSnapshot[i]) {
    lowSnapshot[i] = highSnapshot[i];
  }
}

try {
  Progress.load({ ...low, token: 'low-token' }).resume(1);
} catch (error) {
  console.log(error.name); // "JsliteLimitError"
}

console.log(
  Progress.load({ ...low, snapshot: lowSnapshot, token: 'forged-token' }).resume(1),
); // 10000
```

**Suggested fix**

Treat host-supplied resume policy as authoritative, not serialized limits.
Reapply or clamp limits at load/resume time, or reject snapshots whose embedded
limits are not explicitly accepted by the host.
