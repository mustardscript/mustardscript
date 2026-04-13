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
    'crates/mustard/Cargo.toml',
    'crates/mustard/src/diagnostic.rs',
    'crates/mustard/src/ir.rs',
    'crates/mustard/src/lib.rs',
    'crates/mustard/src/limits.rs',
    'crates/mustard/src/parser/mod.rs',
    'crates/mustard/src/runtime/mod.rs',
    'crates/mustard/src/span.rs',
    'crates/mustard/src/structured.rs',
    'crates/mustard-bridge/Cargo.toml',
    'crates/mustard-bridge/src/lib.rs',
    'crates/mustard-node/build.rs',
    'crates/mustard-node/Cargo.toml',
    'crates/mustard-node/src/lib.rs',
    'crates/mustard-sidecar/Cargo.toml',
    'crates/mustard-sidecar/src/lib.rs',
    'crates/mustard-sidecar/src/main.rs',
    'dist/install.js',
    'index.d.ts',
    'dist/index.js',
    'mustard.d.ts',
    'dist/lib/errors.js',
    'dist/lib/runtime.js',
    'dist/native-loader.js',
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
    'crates/mustard/tests/',
    'crates/mustard-sidecar/tests/',
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
  ['Verify Rust crate publish flow for mustard', 'cargo', ['publish', '--dry-run', '--allow-dirty', '-p', 'mustard']],
  ['Verify Rust package manifest for mustard-node', 'cargo', ['package', '-p', 'mustard-node', '--allow-dirty', '--list']],
  ['Verify Rust package manifest for mustard-sidecar', 'cargo', ['package', '-p', 'mustard-sidecar', '--allow-dirty', '--list']],
];

try {
  verifyPackedFiles();

  for (const [label, command, args] of steps) {
    run(label, command, args);
  }
} finally {
  cleanupPackedTarball();
}
