# Release Guide

This document defines the current release shape for `jslite` and the commands
maintainers should run before publishing anything.

## Current Release Shape

- The primary release artifact is the npm package `jslite`.
- The npm package is source-build-only today. `npm install` compiles the native
  addon locally from the bundled Rust sources.
- Prebuilt `.node` binaries are intentionally deferred until the package shape
  and supported target matrix are stable.
- The Rust crates are implementation crates first. A separate `cargo publish`
  flow is optional and is not required for the npm release path.

## Release Prerequisites

- Start from a clean checkout or an isolated release worktree.
- Confirm the version in `package.json` and the Cargo workspace metadata is the
  release candidate you intend to publish.
- Wait for the normal CI matrix to pass on Linux, macOS, and Windows before
  starting the release checklist.

## Release Verification Checklist

Run these commands from the repository root.

### 1. Build, test, and lint the release candidate

```sh
npm install
cargo test --workspace
npm test
npm run lint
```

This covers the current build path, the Rust workspace tests, the Node API
tests, the typecheck, and the source-package smoke test.

### 2. Verify the npm package shape

```sh
npm pack --dry-run
npm pack
```

Check the dry-run output before keeping the generated tarball:

- The tarball should include the Rust workspace files needed to build the
  addon locally: `Cargo.toml`, `Cargo.lock`, `crates/jslite/**`,
  `crates/jslite-node/**`, and the currently referenced workspace member
  `crates/jslite-sidecar/**`.
- The tarball should include the public JS and type entrypoints:
  `index.js`, `index.d.ts`, and `jslite.d.ts`.
- The tarball should not include local build products, `.github/`, tests,
  planning documents, or a platform-specific `.node` binary from a maintainer
  machine.

### 3. Install smoke test from the packed tarball

```sh
repo_root="$PWD"
tmpdir="$(mktemp -d)"
tarball="$(npm pack --silent)"
mkdir "$tmpdir/consumer"
cd "$tmpdir/consumer"
npm init -y
npm install "$repo_root/$tarball"
node - <<'EOF'
const { Jslite } = require('jslite');

async function main() {
  const runtime = new Jslite('let total = 1; total = total + 41; total;');
  const value = await runtime.run();
  if (value !== 42) {
    throw new Error(`expected 42, got ${value}`);
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
EOF
```

This validates the exact release tarball, not just the checkout.

### 4. Upgrade or reinstall smoke test

Run the tarball install again in the same temporary consumer and rerun the same
smoke program:

```sh
npm install "$repo_root/$tarball"
node - <<'EOF'
const { Jslite } = require('jslite');

async function main() {
  const runtime = new Jslite('let total = 40; total = total + 2; total;');
  const value = await runtime.run();
  if (value !== 42) {
    throw new Error(`expected 42, got ${value}`);
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
EOF
```

There is not yet a previously published package to upgrade from, so the current
pre-release proxy is reinstalling the candidate tarball over an existing
consumer install. Once real published versions exist, replace this with an
install-from-previous-version then upgrade-to-candidate flow.

### 5. Verify Rust package readiness if a crate release is being considered

The default release path does not publish a Rust crate. If maintainers decide
to publish one later, start with dry-run packaging:

```sh
cargo package -p jslite --allow-dirty --list
cargo package -p jslite-node --allow-dirty --list
cargo package -p jslite-sidecar --allow-dirty --list
```

Interpretation:

- `jslite` is the only crate that currently makes sense as a future standalone
  Rust artifact.
- `jslite-node` and `jslite-sidecar` are packaging checks for completeness, not
  a recommendation to publish those crates independently.
- If a future `cargo publish --dry-run` is desired, add any remaining metadata
  such as a dedicated crate README before attempting an actual publish.

## Publishing The npm Package

Once the checklist passes:

```sh
npm publish
```

Recommended follow-up:

- tag the release commit in git
- attach release notes that summarize the supported subset and the source-build
  installation requirement
- link to `README.md`, `docs/LANGUAGE.md`, `docs/HOST_API.md`, and
  `docs/SECURITY_MODEL.md`

The repository also includes a manual GitHub Actions workflow,
`.github/workflows/release-verify.yml`, that runs the same verification flow on
an Ubuntu release candidate checkout.

## Deferred Prebuilt Binary Flow

Prebuilt binaries remain deliberately out of scope for the current release
process. Do not attach or publish per-platform `.node` artifacts yet.

When the package shape and support policy are stable, add a separate prebuilt
workflow with all of the following before enabling it:

- an explicit supported target matrix
- a reproducible binary naming and lookup strategy in `index.js`
- checksum and provenance guidance
- CI coverage for prebuilt download and fallback-to-source-build paths
- documentation for when hosts should prefer source builds instead
