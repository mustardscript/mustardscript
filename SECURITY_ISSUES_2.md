# Security Issues 2

This file records confirmed findings from the second threat-model review.
Entries below were validated with focused local repros on the current checkout
as of 2026-04-11 and are believed to still be present on `main`.

Update on `codex/security-issues-2`: every finding below remains confirmed on
`main` at `aa61959`, and each item is fixed in this worktree branch with
targeted tests.

## Critical: `MUSTARD_NATIVE_LIBRARY_PATH` and `NAPI_RS_NATIVE_LIBRARY_PATH` allow arbitrary JavaScript execution at import time

**Status:** confirmed on `main`; fixed on `codex/security-issues-2`

**Affected files**

- [`index.js`](index.js)
- [`native-loader.js`](native-loader.js)

**Impact**

Importing `mustardscript` can execute arbitrary attacker-controlled
JavaScript before any native API validation happens.

The override path is passed directly into `require(...)`, so the payload does
not need to be a real native `.node` binary. Any resolvable JavaScript file or
module path runs in the host process as soon as the package is imported.

**Short repro**

```sh
tmpdir="$(mktemp -d)"
cat > "$tmpdir/payload.js" <<'EOF'
const fs = require('node:fs');
fs.writeFileSync(process.env.SENTINEL_PATH, 'owned');
module.exports = {};
EOF

SENTINEL_PATH="$tmpdir/sentinel.txt" \
MUSTARD_NATIVE_LIBRARY_PATH="$tmpdir/payload.js" \
node -e "try { require('./index.js'); } catch {}"

cat "$tmpdir/sentinel.txt" # owned
```

**Suggested fix**

Do not pass arbitrary override paths to plain `require(...)`. If override paths
remain supported, constrain them to validated native-addon artifacts and reject
JavaScript/module resolution.

## Critical: a fake optional prebuilt package can hijack addon loading and skip the source build

**Status:** confirmed on `main`; fixed on `codex/security-issues-2`

**Affected files**

- [`package.json`](package.json)
- [`install.js`](install.js)
- [`native-loader.js`](native-loader.js)

**Impact**

A malicious package with the expected optional-prebuilt package name becomes
trusted executable code.

`install.js` treats the mere presence of the expected package name as a reason
to skip the source build, and `native-loader.js` later does a plain
`require(prebuilt.packageName)` with no verification that the package is
actually the sanctioned native artifact or even a native addon at all.

That means a shadowed package with a JavaScript `main` can run arbitrary code
at import time.

**Short repro**

```sh
npm pack
tmpdir="$(mktemp -d)"
cd "$tmpdir"
npm init -y >/dev/null
npm install --ignore-scripts /Users/mini/mustard/keppoai-mustard-0.1.0.tgz >/dev/null

fake="mustardscript-darwin-arm64" # use the host-matching package name
mkdir -p "node_modules/$fake"
cat > "node_modules/$fake/package.json" <<EOF
{ "name": "$fake", "version": "0.0.0", "main": "index.js" }
EOF
cat > "node_modules/$fake/index.js" <<'EOF'
const fs = require('node:fs');
fs.writeFileSync(process.env.SENTINEL_PATH, 'fake-prebuilt-ran');
module.exports = {};
EOF

SENTINEL_PATH="$tmpdir/sentinel.txt" node -e "try { require('mustardscript'); } catch {}"
cat "$tmpdir/sentinel.txt" # fake-prebuilt-ran
```

**Suggested fix**

Treat prebuilt resolution as a verified artifact-loading step, not a plain npm
module lookup. Refuse JavaScript fallbacks for prebuilt package loading.

## High: proxy-backed host values bypass the plain-data boundary and execute traps during serialization

**Status:** confirmed on `main`; fixed on `codex/security-issues-2`

**Affected files**

- [`index.js`](index.js)

**Impact**

Proxy objects can cross the documented plain-data boundary while running host
traps during:

- input serialization
- capability registration
- capability result serialization
- `resumeError(...).details` serialization

That means attacker-controlled JavaScript values can execute host-side trap code
at the boundary even though the contract says only plain objects and arrays may
cross.

**Short repro**

```js
const { Mustard } = require('./index.js');

(async () => {
  const events = [];
  const value = new Proxy({}, {
    getPrototypeOf() { events.push('getPrototypeOf'); return Object.prototype; },
    ownKeys() { events.push('ownKeys'); return ['answer']; },
    getOwnPropertyDescriptor() {
      events.push('gopd');
      return { enumerable: true, configurable: true, value: 42, writable: true };
    },
  });

  const result = await new Mustard('value.answer;').run({ inputs: { value } });
  console.log(result);  // 42
  console.log(events);  // [ 'getPrototypeOf', 'ownKeys', 'gopd' ]
})();
```

The same class of issue also reproduces with `run({ capabilities: proxy })`.

**Suggested fix**

Reject proxies and other exotic host objects explicitly before boundary
serialization, or move validation into a mechanism that cannot be steered by JS
proxy traps.

## High: cyclic host values still recurse to raw `RangeError` instead of a fail-closed boundary error

**Status:** confirmed on `main`; fixed on `codex/security-issues-2`

**Affected files**

- [`index.js`](index.js)

**Impact**

Self-referential host values passed as:

- inputs
- capability results
- `resumeError(...).details`

still recurse until the wrapper throws `RangeError: Maximum call stack size
exceeded`.

That bypasses the documented structured-boundary rejection path and can take
down a host process if the error is uncaught.

**Short repro**

```js
const { Mustard } = require('./index.js');

(async () => {
  const value = {};
  value.self = value;
  await new Mustard('value;').run({ inputs: { value } });
})();
```

Current result:

```txt
RangeError: Maximum call stack size exceeded
```

**Suggested fix**

Add visited-set cycle detection to the JavaScript wrapper encoder and convert
cycles into a typed boundary rejection.

## Critical: the `Progress` single-use replay guard can be bypassed by mutating serialized snapshot nonce bytes

**Status:** confirmed on `main`; fixed on `codex/security-issues-2`

**Affected files**

- [`index.js`](index.js)
- [`crates/mustard/src/runtime/state.rs`](crates/mustard/src/runtime/state.rs)
- [`crates/mustard/src/runtime/vm.rs`](crates/mustard/src/runtime/vm.rs)
- [`crates/mustard/src/runtime/serialization.rs`](crates/mustard/src/runtime/serialization.rs)

**Impact**

The Node replay guard is keyed to `sha256(snapshot bytes)`, not to canonical
snapshot semantics. The serialized snapshot contains a `snapshot_nonce` field
that changes the bytes without changing the suspended work.

By flipping a nonce byte, an attacker can obtain a different snapshot hash for
the same suspended execution and resume it again after the original `Progress`
object was already consumed.

That is a replay vulnerability for one-shot approval or side-effecting flows.

**Short repro**

```js
const { Mustard, Progress } = require('./index.js');

const progress = new Mustard('fetch_data(1);').start({
  capabilities: { fetch_data() {} },
});

const dumped = progress.dump();
progress.resume(9); // consumes the original snapshot

const mutated = Buffer.from(dumped.snapshot);
mutated[1546] ^= 1; // local repro on this checkout: nonce byte

const restored = Progress.load(
  { snapshot: mutated },
  { capabilities: { fetch_data() {} }, limits: {} },
);

console.log(restored.resume(9)); // 9
```

**Suggested fix**

Bind replay protection to authenticated or canonicalized snapshot identity, or
remove non-semantic serialized fields from the replay identity calculation.

## Critical: forged snapshots can lower the serialized instruction counter and bypass post-load budget enforcement

**Status:** confirmed on `main`; fixed on `codex/security-issues-2`

**Affected files**

- [`crates/mustard/src/runtime/state.rs`](crates/mustard/src/runtime/state.rs)
- [`crates/mustard/src/runtime/accounting.rs`](crates/mustard/src/runtime/accounting.rs)
- [`crates/mustard/src/runtime/mod.rs`](crates/mustard/src/runtime/mod.rs)

**Impact**

Snapshots serialize `instruction_counter`, but snapshot load only recomputes
heap and allocation accounting. It trusts the serialized instruction counter
when applying the host's post-load budget check.

That means a forged snapshot can lower the stored counter and resume under a
budget that should have already been exhausted.

**Short repro**

```js
const { Mustard, Progress } = require('./index.js');

const progress = new Mustard('fetch_data(1);').start({
  capabilities: { fetch_data() {} },
});

const original = progress.dump().snapshot;

function tryLoad(snapshot) {
  try {
    const restored = Progress.load(
      { snapshot },
      { capabilities: { fetch_data() {} }, limits: { instructionBudget: 3 } },
    );
    return { ok: true, result: restored.resume(9) };
  } catch (error) {
    return { ok: false, name: error.name, message: error.message };
  }
}

const mutated = Buffer.from(original);
mutated[1554] = 0; // local repro on this checkout: counter byte

console.log(tryLoad(original)); // limit error
console.log(tryLoad(mutated));  // succeeds
```

**Suggested fix**

Recompute or authenticate serialized instruction counters on load instead of
trusting the stored value.

## High: native helper loops bypass instruction budgeting and cancellation

**Status:** confirmed on `main`; fixed on `codex/security-issues-2`

**Affected files**

- [`crates/mustard/src/runtime/builtins/arrays.rs`](crates/mustard/src/runtime/builtins/arrays.rs)
- [`crates/mustard/src/runtime/builtins/objects.rs`](crates/mustard/src/runtime/builtins/objects.rs)
- [`crates/mustard/src/runtime/mod.rs`](crates/mustard/src/runtime/mod.rs)
- [`crates/mustard/src/runtime/vm.rs`](crates/mustard/src/runtime/vm.rs)

**Impact**

Long-running native helper loops can monopolize the host thread even when the
guest has an extremely small instruction budget or the host abort signal is
already scheduled.

Verified examples on the current tree:

- `Array.prototype.sort()` on a large descending host array
- `Object.keys()` on a large host object

The current `sort()` implementation is especially dangerous because it is an
insertion sort, so descending input drives quadratic work.

**Short repro**

```js
const { Mustard } = require('./index.js');

(async () => {
  const big = Array.from({ length: 12000 }, (_, i) => 12000 - i);
  const started = Date.now();
  const result = await new Mustard('big.sort(); 1;').run({
    inputs: { big },
    limits: { instructionBudget: 20 },
  });
  console.log({ result, ms: Date.now() - started });
})();
```

Observed locally on this checkout:

- completed successfully
- returned `1`
- took about `4.3s`
- did not fail with `instruction budget exhausted`

A second repro also showed `Object.keys(big).length` succeeding under
`instructionBudget: 20` on a `40000`-key object.

**Suggested fix**

Add explicit metering and cancellation polling inside native helper loops, or
move these helpers onto execution paths that cannot monopolize the process.

## Medium: process-global `Progress` caches leak host memory outside guest limits

**Status:** confirmed on `main`; fixed on `codex/security-issues-2`

**Affected files**

- [`index.js`](index.js)

**Impact**

The wrapper keeps global process-lifetime caches for:

- used snapshot hashes
- remembered snapshot policies

Those caches are never evicted. A hostile client that can repeatedly create
unique suspensions can grow the host's JavaScript heap outside guest runtime
limits.

This is a host-memory DoS surface, not a guest-heap violation inside the Rust
runtime.

**Short repro**

```sh
node --expose-gc - <<'EOF'
const { Mustard } = require('./index.js');
if (global.gc) global.gc();
const before = process.memoryUsage();
for (let i = 0; i < 50000; i++) {
  new Mustard(`fetch_data(${i});`).start({ capabilities: { fetch_data() {} } });
}
if (global.gc) global.gc();
const after = process.memoryUsage();
console.log({
  beforeHeapMb: Math.round(before.heapUsed / 1024 / 1024),
  afterHeapMb: Math.round(after.heapUsed / 1024 / 1024),
});
EOF
```

Observed locally on this checkout:

- heap usage grew by about `13 MiB` after forced GC
- RSS grew by about `84 MiB`

**Suggested fix**

Add bounded eviction or explicit lifecycle cleanup for process-global snapshot
identity and policy caches.
