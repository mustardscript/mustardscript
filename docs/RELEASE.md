# Release Guide

This document defines the current release shape for `jslite` and the commands
maintainers should run before publishing anything.

## Current Release Shape

- The primary release artifact is the npm package `@keppoai/jslite`.
- The default npm install path still preserves source builds. If no matching
  optional prebuilt package is installed, `npm install` compiles the native
  addon locally from the bundled Rust sources.
- Optional prebuilt `.node` binaries now have a separate release workflow and
  verification path. They do not replace or weaken the source-build path.
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

### Canonical verification command

```sh
npm run verify:release
```

That command runs the same release verification flow used by the manual GitHub
Actions workflow in `.github/workflows/release-verify.yml`. It now verifies the
scoped package name, the packed file list, and the default `npm publish
--dry-run` path.

### 1. Build, test, and lint the release candidate

```sh
npm install
cargo test --workspace
npm test
npm run lint
```

This covers the current build path, the Rust workspace tests, the Node API
tests, the typecheck, and the source-package smoke test.

### 2. Verify the optional prebuilt path if you intend to publish it

```sh
npm run verify:prebuilt
```

That command runs the prebuilt smoke coverage in
`tests/package-smoke.test.js`. It verifies the configured `@napi-rs/cli`
target metadata, stages a host-matching release binary into a generated npm
package directory, installs the root tarball plus the matching optional package
with lifecycle scripts disabled, and then proves `install.js` skips the source
build when the matching prebuilt package is already present. The loader now
accepts only validated `.node` artifacts from the expected optional package
layout; JavaScript `main` fallbacks and arbitrary override module resolution are
rejected.

### 3. Verify the npm package shape

```sh
npm pack --dry-run
npm pack
```

Check the dry-run output before keeping the generated tarball:

- The tarball should include the Rust workspace files needed to build the
  addon locally: `Cargo.toml`, `crates/jslite/src/**`,
  `crates/jslite/Cargo.toml`, `crates/jslite-node/src/**`,
  `crates/jslite-node/build.rs`, `crates/jslite-node/Cargo.toml`,
  `crates/jslite-sidecar/src/**`, and
  `crates/jslite-sidecar/Cargo.toml`.
- The tarball should include the public JS and type entrypoints plus the
  install/load helpers that preserve the source-build fallback:
  `index.js`, `index.d.ts`, `jslite.d.ts`, `install.js`, and
  `native-loader.js`.
- The tarball should not include local build products, `.github/`, tests,
  planning documents, or a platform-specific `.node` binary from a maintainer
  machine.

### 4. Install smoke test from the packed tarball

```sh
repo_root="$PWD"
tmpdir="$(mktemp -d)"
tarball="$(npm pack --silent)"
mkdir "$tmpdir/consumer"
cd "$tmpdir/consumer"
npm init -y
npm install "$repo_root/$tarball"
node - <<'EOF'
const { Jslite } = require('@keppoai/jslite');

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

### 5. Upgrade or reinstall smoke test

Run the tarball install again in the same temporary consumer and rerun the same
smoke program. The repository now automates this in
`tests/package-smoke.test.js`, which installs the packed tarball, runs guest
code, reinstalls the same tarball, and reruns guest code from the consumer.

```sh
npm install "$repo_root/$tarball"
node - <<'EOF'
const { Jslite } = require('@keppoai/jslite');

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

### 6. Verify Rust package readiness if a crate release is being considered

The default release path does not publish a Rust crate. If maintainers decide
to publish one later, the current verifiable flow for the core crate is:

```sh
cargo publish --dry-run --allow-dirty -p jslite
```

For the addon and sidecar crates, maintain dry-run packaging checks:

```sh
cargo package -p jslite-node --allow-dirty --list
cargo package -p jslite-sidecar --allow-dirty --list
```

Interpretation:

- `jslite` is the only crate that currently makes sense as a future standalone
  Rust artifact.
- `jslite-node` and `jslite-sidecar` are packaging checks for completeness, not
  a recommendation to publish those crates independently.
- If maintainers later decide to publish more than the core crate, add any
  remaining metadata and remove the current path dependencies before attempting
  an actual publish.

## npm Namespace And Registry Access

As verified on April 11, 2026, `npm view jslite` resolves to an unrelated
public package at version `1.1.12`, so this repository now targets the scoped
package name `@keppoai/jslite`.

Current state:

- `npm view @keppoai/jslite` returns `404 Not Found`, which is compatible with a
  first publish for this package name.
- `npm publish --dry-run` is the command shape the automated release
  verification now checks.
- An actual public publish still requires an authenticated npm publisher with
  access to the `@keppoai` scope. That permission check cannot be proven from
  repository-local verification alone.

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

If maintainers ever move away from `@keppoai/jslite`, update the package name,
smoke tests, release verification script, and this document together.

## Optional Prebuilt Binary Flow

The optional prebuilt flow is intentionally separate from the default
source-build release path. It exists for maintainers who want faster installs
on the explicitly supported target matrix without making prebuilt availability
an assumption for every host.

Current prebuilt target matrix:

- `x86_64-unknown-linux-gnu` -> `@keppoai/jslite-linux-x64-gnu`
- `aarch64-apple-darwin` -> `@keppoai/jslite-darwin-arm64`
- `x86_64-apple-darwin` -> `@keppoai/jslite-darwin-x64`
- `x86_64-pc-windows-msvc` -> `@keppoai/jslite-win32-x64-msvc`

Current mechanics:

- `package.json` carries the target list in the `napi.targets` field so
  `@napi-rs/cli` can generate per-platform npm package directories.
- `native-loader.js` first tries only the exact expected local source-build
  artifact names (`index.<platform>.node` for configured hosts, plus
  `index.node` as the generic local-build fallback), then falls back to the
  matching optional package if one is installed.
- `install.js` preserves the source-build path by only skipping the local Cargo
  build when the matching optional package is already installed for the current
  host.
- `.github/workflows/prebuilt-binaries.yml` is the manual, explicit prebuilt
  workflow. It builds the configured targets, stages them with
  `napi create-npm-dirs` plus `napi artifacts`, runs `npm run verify:prebuilt`,
  and only publishes when `workflow_dispatch` is invoked with `publish=true`.

Local verification hook:

```sh
npm run verify:prebuilt
```

That local hook verifies the host-matching prebuilt install path. Cross-platform
artifact builds remain a GitHub Actions concern because they require the
corresponding runner environments.

External blocker for a real prebuilt publish:

- The workflow still requires a real `NPM_TOKEN` with publish rights for the
  `@keppoai` scope before `npx napi pre-publish` and the final root
  `npm publish` can succeed. Repository-local verification cannot prove those
  credentials or scope permissions.
