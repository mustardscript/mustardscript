'use strict';

const { execFileSync } = require('node:child_process');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..');
const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';

function run(label, command, args) {
  process.stdout.write(`\n==> ${label}\n`);
  execFileSync(command, args, {
    cwd: repoRoot,
    stdio: 'inherit',
  });
}

const steps = [
  ['Install dependencies', npmCommand, ['install']],
  ['Run Rust tests', 'cargo', ['test', '--workspace']],
  ['Run Node, types, and package smoke tests', npmCommand, ['test']],
  ['Run lint', npmCommand, ['run', 'lint']],
  ['Inspect npm package contents', npmCommand, ['pack', '--dry-run']],
  ['Verify npm publish command shape', npmCommand, ['publish', '--dry-run', '--tag', 'next']],
  ['Verify Rust crate publish flow for jslite', 'cargo', ['publish', '--dry-run', '--allow-dirty', '-p', 'jslite']],
  ['Verify Rust package manifest for jslite-node', 'cargo', ['package', '-p', 'jslite-node', '--allow-dirty', '--list']],
  ['Verify Rust package manifest for jslite-sidecar', 'cargo', ['package', '-p', 'jslite-sidecar', '--allow-dirty', '--list']],
];

for (const [label, command, args] of steps) {
  run(label, command, args);
}
