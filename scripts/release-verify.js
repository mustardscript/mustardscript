'use strict';

const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..');
const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const packageInfo = require(path.join(repoRoot, 'package.json'));

function tarballFilenameForPackage(name, version) {
  return `${name.replace(/^@/, '').replace(/\//g, '-')}-${version}.tgz`;
}

function run(label, command, args) {
  process.stdout.write(`\n==> ${label}\n`);
  execFileSync(command, args, {
    cwd: repoRoot,
    stdio: 'inherit',
  });
}

function capture(command, args) {
  return execFileSync(command, args, {
    cwd: repoRoot,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
}

function cleanupPackedTarball() {
  fs.rmSync(
    path.join(
      repoRoot,
      tarballFilenameForPackage(packageInfo.name, packageInfo.version),
    ),
    { force: true },
  );
}

function verifyPackedFiles() {
  process.stdout.write('\n==> Verify npm package contents\n');
  cleanupPackedTarball();
  const [packed] = JSON.parse(
    capture(npmCommand, ['pack', '--dry-run', '--json', '--silent']),
  );
  assert.equal(packed.name, packageInfo.name);
  assert.equal(
    packed.filename,
    tarballFilenameForPackage(packageInfo.name, packageInfo.version),
  );

  const packedFiles = new Set(packed.files.map((entry) => entry.path));
  const requiredFiles = [
    'Cargo.toml',
    'crates/jslite/Cargo.toml',
    'crates/jslite/src/diagnostic.rs',
    'crates/jslite/src/ir.rs',
    'crates/jslite/src/lib.rs',
    'crates/jslite/src/limits.rs',
    'crates/jslite/src/parser/mod.rs',
    'crates/jslite/src/runtime/mod.rs',
    'crates/jslite/src/span.rs',
    'crates/jslite/src/structured.rs',
    'crates/jslite-bridge/Cargo.toml',
    'crates/jslite-bridge/src/lib.rs',
    'crates/jslite-node/build.rs',
    'crates/jslite-node/Cargo.toml',
    'crates/jslite-node/src/lib.rs',
    'crates/jslite-sidecar/Cargo.toml',
    'crates/jslite-sidecar/src/lib.rs',
    'crates/jslite-sidecar/src/main.rs',
    'install.js',
    'index.d.ts',
    'index.js',
    'jslite.d.ts',
    'lib/errors.js',
    'lib/runtime.js',
    'native-loader.js',
    'package.json',
    'README.md',
  ];
  for (const file of requiredFiles) {
    assert.ok(packedFiles.has(file), `npm tarball is missing required file: ${file}`);
  }

  const disallowedPrefixes = [
    '.github/',
    'benchmarks/',
    'docs/',
    'examples/',
    'fuzz/',
    'scripts/',
    'tests/',
    'crates/jslite/tests/',
    'crates/jslite-sidecar/tests/',
  ];
  const disallowedFiles = packed.files
    .map((entry) => entry.path)
    .filter(
      (file) =>
        file.endsWith('.node') ||
        file.endsWith('.tgz') ||
        file === 'AGENTS.md' ||
        file === 'CONTRIBUTING.md' ||
        file === 'IMPLEMENT_PROMPT.md' ||
        file === 'TODOS.md' ||
        disallowedPrefixes.some((prefix) => file.startsWith(prefix)),
    );
  assert.deepEqual(
    disallowedFiles,
    [],
    `npm tarball includes unexpected files:\n${disallowedFiles.join('\n')}`,
  );
}

const steps = [
  ['Install dependencies', npmCommand, ['install']],
  ['Run Rust tests', 'cargo', ['test', '--workspace']],
  ['Run Node, types, and package smoke tests', npmCommand, ['test']],
  ['Run lint', npmCommand, ['run', 'lint']],
  ['Verify npm publish command shape', npmCommand, ['publish', '--dry-run']],
  ['Verify Rust crate publish flow for jslite', 'cargo', ['publish', '--dry-run', '--allow-dirty', '-p', 'jslite']],
  ['Verify Rust package manifest for jslite-node', 'cargo', ['package', '-p', 'jslite-node', '--allow-dirty', '--list']],
  ['Verify Rust package manifest for jslite-sidecar', 'cargo', ['package', '-p', 'jslite-sidecar', '--allow-dirty', '--list']],
];

try {
  verifyPackedFiles();

  for (const [label, command, args] of steps) {
    run(label, command, args);
  }
} finally {
  cleanupPackedTarball();
}
