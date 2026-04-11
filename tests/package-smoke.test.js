'use strict';

const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const repoRoot = path.resolve(__dirname, '..');
const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';

function run(command, args, cwd) {
  return execFileSync(command, args, {
    cwd,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
}

test('published source package installs and runs from a fresh consumer project', async () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'jslite-package-smoke-'));
  const consumerRoot = path.join(tempRoot, 'consumer');
  const tarballName = `jslite-${require(path.join(repoRoot, 'package.json')).version}.tgz`;
  const tarballPath = path.join(repoRoot, tarballName);

  fs.mkdirSync(consumerRoot);
  fs.rmSync(tarballPath, { force: true });

  try {
    const packOutput = run(npmCommand, ['pack', '--json'], repoRoot);
    const [packed] = JSON.parse(packOutput);
    assert.equal(packed.filename, tarballName);

    run(npmCommand, ['init', '-y'], consumerRoot);
    run(npmCommand, ['install', tarballPath], consumerRoot);

    const result = run(
      process.execPath,
      [
        '-e',
        `
          const { Jslite } = require('jslite');
          (async () => {
            const runtime = new Jslite('const answer = 2; answer + 3;');
            const value = await runtime.run();
            process.stdout.write(String(value));
          })().catch((error) => {
            console.error(error);
            process.exit(1);
          });
        `,
      ],
      consumerRoot,
    );

    assert.equal(result, '5');
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
    fs.rmSync(tarballPath, { force: true });
  }
});
