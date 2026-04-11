'use strict';

const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const repoRoot = path.resolve(__dirname, '..');
const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const packageInfo = require(path.join(repoRoot, 'package.json'));

function tarballFilenameForPackage(name, version) {
  return `${name.replace(/^@/, '').replace(/\//g, '-')}-${version}.tgz`;
}

function run(command, args, cwd) {
  return execFileSync(command, args, {
    cwd,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
}

function runGuestProgram(consumerRoot, source) {
  return run(
    process.execPath,
    [
      '-e',
      `
        const { Jslite } = require(${JSON.stringify(packageInfo.name)});
        (async () => {
          const runtime = new Jslite(${JSON.stringify(source)});
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
}

test('published source package installs, reinstalls, and runs from a fresh consumer project', async () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'jslite-package-smoke-'));
  const consumerRoot = path.join(tempRoot, 'consumer');
  const tarballName = tarballFilenameForPackage(packageInfo.name, packageInfo.version);
  const tarballPath = path.join(repoRoot, tarballName);

  fs.mkdirSync(consumerRoot);
  fs.rmSync(tarballPath, { force: true });

  try {
    const packOutput = run(npmCommand, ['pack', '--json'], repoRoot);
    const [packed] = JSON.parse(packOutput);
    assert.equal(packed.name, packageInfo.name);
    assert.equal(packed.filename, tarballName);

    run(npmCommand, ['init', '-y'], consumerRoot);
    run(npmCommand, ['install', tarballPath], consumerRoot);
    assert.equal(
      runGuestProgram(consumerRoot, 'const answer = 2; answer + 3;'),
      '5',
    );

    run(npmCommand, ['install', tarballPath], consumerRoot);
    assert.equal(
      runGuestProgram(consumerRoot, 'let total = 40; total = total + 2; total;'),
      '42',
    );
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
    fs.rmSync(tarballPath, { force: true });
  }
});
