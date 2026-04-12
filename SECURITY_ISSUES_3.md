# Security Issues 3

This file records validated findings from the third threat-model review.
Each issue below was:

- first identified by a file-focused `gpt-5.4` `xhigh` sub-agent
- independently re-verified by a second `gpt-5.4` `xhigh` sub-agent
- checked again with a focused local repro on the current checkout on 2026-04-11

## Reviewed Targets

These were the 12 primary files selected from the threat model as the most
security-relevant review targets:

- [`native-loader.js`](native-loader.js)
- [`install.js`](install.js)
- [`lib/structured.js`](lib/structured.js)
- [`lib/policy.js`](lib/policy.js)
- [`lib/progress.js`](lib/progress.js)
- [`lib/runtime.js`](lib/runtime.js)
- [`crates/jslite-node/src/lib.rs`](crates/jslite-node/src/lib.rs)
- [`crates/jslite-sidecar/src/lib.rs`](crates/jslite-sidecar/src/lib.rs)
- [`crates/jslite/src/runtime/serialization.rs`](crates/jslite/src/runtime/serialization.rs)
- [`crates/jslite/src/runtime/validation.rs`](crates/jslite/src/runtime/validation.rs)
- [`crates/jslite/src/runtime/builtins/primitives.rs`](crates/jslite/src/runtime/builtins/primitives.rs)
- [`crates/jslite/src/runtime/conversions/boundary.rs`](crates/jslite/src/runtime/conversions/boundary.rs)

## Critical: unexpected top-level `.node` files are import-reachable native code

**Affected files**

- [`native-loader.js`](native-loader.js)
- [`index.js`](index.js)

**Impact**

Importing `@keppoai/jslite` can load an unexpected native addon purely because a
top-level filename under the package root or `crates/jslite-node` ends with
`.node`.

`loadNative()` does not restrict the local fallback to the expected
`index.<abi>.node` artifact. It scans the package root and `crates/jslite-node`
and queues every top-level `.node` file it finds, then `require(...)`s those
paths until one loads.

That means a hostile local package shape can make import-time native code
execution happen from an unexpected file like `evil.node`.

**Validation**

Local repro confirmed that:

- copying the existing addon to `<tmp>/evil.node`
- calling `loadNative({ searchRoot: tmp, env: {} })`

causes the loader to attempt `evil.node` directly.

I also confirmed the same behavior for `<tmp>/crates/jslite-node/evil.node`.

**Short repro**

```js
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { loadNative } = require('./native-loader');

const root = fs.mkdtempSync(path.join(os.tmpdir(), 'jslite-rogue-node-'));
fs.copyFileSync(
  require.resolve('./index.darwin-arm64.node'),
  path.join(root, 'evil.node'),
);

loadNative({ searchRoot: root, env: {} });
```

## Critical: `install.js` executes an ancestor-resolved `@napi-rs/cli`

**Affected files**

- [`package.json`](package.json)
- [`install.js`](install.js)

**Impact**

The package install lifecycle runs `node install.js`. On the source-build path,
`install.js` resolves `@napi-rs/cli/package.json` with a bare
`require.resolve(...)` and then executes the resolved `dist/cli.js` with
`execFileSync(...)`.

That resolution is not pinned to a package-local copy. Node will walk ancestor
`node_modules` directories from the installed `@keppoai/jslite` location, so an
ancestor-controlled `@napi-rs/cli` becomes trusted executable code during
`npm install`.

This is install-time arbitrary JavaScript execution with developer or CI
privileges.

**Validation**

Local repro confirmed that a fake ancestor `@napi-rs/cli/dist/cli.js` was
resolved and executed, and wrote a sentinel file containing the exact build
argv:

```json
["build","--platform","--manifest-path","crates/jslite-node/Cargo.toml","--js-package-name","@keppoai/jslite","--output-dir",".","--no-js"]
```

**Short repro**

```js
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const root = fs.mkdtempSync(path.join(os.tmpdir(), 'jslite-install-ancestor-'));
const pkgRoot = path.join(root, 'node_modules', '@keppoai', 'jslite');
const fakeCliRoot = path.join(root, 'node_modules', '@napi-rs', 'cli');

fs.mkdirSync(pkgRoot, { recursive: true });
fs.mkdirSync(path.join(fakeCliRoot, 'dist'), { recursive: true });
fs.copyFileSync('install.js', path.join(pkgRoot, 'install.js'));
fs.copyFileSync('native-loader.js', path.join(pkgRoot, 'native-loader.js'));
fs.writeFileSync(
  path.join(fakeCliRoot, 'package.json'),
  JSON.stringify({ name: '@napi-rs/cli', version: '0.0.0' }),
);
fs.writeFileSync(
  path.join(fakeCliRoot, 'dist', 'cli.js'),
  'require("node:fs").writeFileSync(process.env.SENTINEL_PATH, JSON.stringify(process.argv.slice(2)));',
);

const sentinel = path.join(root, 'sentinel.txt');
spawnSync(process.execPath, [path.join(pkgRoot, 'install.js')], {
  cwd: pkgRoot,
  env: { ...process.env, SENTINEL_PATH: sentinel },
});
```

## Critical: `Progress` single-use replay can be bypassed after cache churn

**Affected files**

- [`lib/progress.js`](lib/progress.js)
- [`lib/policy.js`](lib/policy.js)

**Impact**

`Progress` single-use enforcement is only backed by a bounded process-local
token set. Once enough other resumes happen, the oldest consumed token is
evicted from `USED_PROGRESS_SNAPSHOTS`, and the same dumped snapshot can be
loaded and resumed again.

With explicit `snapshotKey`, `capabilities`, and `limits`, `Progress.load(...)`
will still authenticate and accept the old blob after eviction.

That breaks the documented same-process single-use behavior and enables replay
of one-shot approvals, charges, grants, or other side-effecting flows.

**Validation**

Local repro confirmed the same dumped snapshot could be resumed twice:

```json
{"first":11,"replayed":22}
```

An independent verifier also confirmed the replay still succeeds when churn
exceeds `USED_PROGRESS_SNAPSHOT_CACHE_LIMIT`.

**Short repro**

```js
const { Jslite, Progress } = require('./index.js');

const SNAPSHOT_KEY = 'review-snapshot-key';
const runtime = new Jslite('approve(seed);');
const original = runtime.start({
  inputs: { seed: 1 },
  capabilities: { approve(value) { return value; } },
  snapshotKey: SNAPSHOT_KEY,
});

const dumped = original.dump();
original.resume(11);

for (let i = 0; i < 4096; i += 1) {
  const progress = runtime.start({
    inputs: { seed: i + 2 },
    capabilities: { approve(value) { return value; } },
    snapshotKey: SNAPSHOT_KEY,
  });
  progress.resume(i);
}

const replayed = Progress.load(dumped, {
  capabilities: { approve(value) { return value; } },
  limits: {},
  snapshotKey: SNAPSHOT_KEY,
}).resume(22);

console.log(replayed); // 22
```

## Critical: `JSON.parse` and `JSON.stringify` bypass metering, cancellation, and direct-return heap accounting

**Affected files**

- [`crates/jslite/src/runtime/builtins/primitives.rs`](crates/jslite/src/runtime/builtins/primitives.rs)
- [`crates/jslite/src/runtime/conversions/boundary.rs`](crates/jslite/src/runtime/conversions/boundary.rs)
- [`crates/jslite/src/runtime/mod.rs`](crates/jslite/src/runtime/mod.rs)
- [`crates/jslite/src/runtime/vm.rs`](crates/jslite/src/runtime/vm.rs)
- [`crates/jslite/src/runtime/accounting.rs`](crates/jslite/src/runtime/accounting.rs)
- [`crates/jslite/src/runtime/exceptions.rs`](crates/jslite/src/runtime/exceptions.rs)

**Impact**

The JSON helpers run large native workloads after only a single VM call step is
charged.

- `JSON.parse` calls `serde_json::from_str(...)` and then recursively converts
  the parsed tree in Rust
- `JSON.stringify` recursively walks guest values and builds native
  `String`/`Vec<String>` buffers in Rust

Neither path calls `charge_native_helper_work()` or `check_cancellation()`.

This means:

- very large JSON work completes under tiny `instructionBudget` values
- cancellation is only observed after the helper finishes
- direct `JSON.stringify(...)` returns can bypass runtime heap accounting until
  the result is stored inside an accounted guest object or cell

Inference from code: `JSON.parse` also allocates a large temporary native parse
tree outside runtime heap accounting before `value_from_json(...)` inserts the
final guest objects.

**Validation**

Local repro confirmed both budget bypasses on the current checkout:

- `JSON.parse(text).length` returned `20001` with `instructionBudget: 8`
- `JSON.stringify(values).length` returned `40001` with `instructionBudget: 8`

The independent verifier also confirmed:

- large JSON work succeeds under the same tiny budget as tiny JSON work
- cancellation is observed only after native parse completes
- direct `JSON.stringify(...)` returns bypass heap accounting until stored

**Short repro**

```js
const { Jslite } = require('./index.js');

const text = '[' + '0,'.repeat(20000) + '0]';
const parsed = await new Jslite('JSON.parse(text).length;').run({
  inputs: { text },
  limits: { instructionBudget: 8 },
});
console.log(parsed); // 20001

const values = Array.from({ length: 20000 }, () => 0);
const stringified = await new Jslite('JSON.stringify(values).length;').run({
  inputs: { values },
  limits: { instructionBudget: 8 },
});
console.log(stringified); // 40001
```

## Critical: deep structured-boundary nesting crashes instead of failing closed

**Affected files**

- [`crates/jslite/src/runtime/conversions/boundary.rs`](crates/jslite/src/runtime/conversions/boundary.rs)
- [`crates/jslite/src/runtime/mod.rs`](crates/jslite/src/runtime/mod.rs)
- [`crates/jslite/src/runtime/vm.rs`](crates/jslite/src/runtime/vm.rs)

**Impact**

The Rust structured-boundary conversions are recursively implemented with cycle
detection but no nesting-depth guard.

That leaves deep but acyclic values able to overflow the Rust stack in both
directions:

- `value_from_structured(...)` for host-to-guest inputs
- `value_to_structured_inner(...)` for guest-to-host results and capability
  arguments

Instead of returning a typed boundary rejection, the process aborts. In addon
mode this kills the embedding Node process.

**Validation**

Local repro confirmed a guest-generated nested array passed to a host
capability crashes a child process on this checkout:

- depth `1500`: completed
- depth `1600`: child exited with `SIGSEGV`

The first public-API input-side repro path also showed the boundary does not
fail closed cleanly: a very deep input value produced raw
`RangeError: Maximum call stack size exceeded` in the JavaScript wrapper rather
than a typed boundary error.

**Short repro**

```js
const { spawnSync } = require('node:child_process');

const depth = 1600;
const script = `
const { Jslite } = require('./index.js');
const runtime = new Jslite(
  'let value = 0; let i = 0; while (i < ${depth}) { value = [value]; i = i + 1; } send(value);'
);
runtime.start({ capabilities: { send(value) { return value; } } });
`;

const child = spawnSync(process.execPath, ['-e', script], {
  cwd: process.cwd(),
  encoding: 'utf8',
  timeout: 10000,
});

console.log(child.status); // null
console.log(child.signal); // SIGSEGV
```
