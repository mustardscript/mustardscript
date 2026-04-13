'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');
const { getLocalBuildOutputFile } = require('../../native-loader.ts');

function nativeLibraryExtension() {
  switch (process.platform) {
    case 'win32':
      return '.dll';
    case 'darwin':
      return '.dylib';
    default:
      return '.so';
  }
}

function writeExecutable(filePath, contents) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, contents);
  fs.chmodSync(filePath, 0o755);
}

function createFakeCargo(root) {
  const sentinelPath = path.join(root, 'cargo-sentinel.json');
  const outputBinaryPath = path.join(
    root,
    'target',
    'release',
    `libjslite_node${nativeLibraryExtension()}`,
  );
  const scriptPath = path.join(root, 'fake-cargo.js');
  writeExecutable(
    scriptPath,
    `#!/usr/bin/env node
'use strict';

const fs = require('node:fs');
const path = require('node:path');

const sentinelPath = ${JSON.stringify(sentinelPath)};
const artifactPath = ${JSON.stringify(outputBinaryPath)};
fs.mkdirSync(path.dirname(artifactPath), { recursive: true });
fs.writeFileSync(artifactPath, 'fake-native-bytes');
fs.writeFileSync(sentinelPath, JSON.stringify(process.argv.slice(2)));
process.stdout.write(JSON.stringify({
  reason: 'compiler-artifact',
  target: {
    name: 'jslite_node',
    crate_types: ['cdylib'],
  },
  filenames: [artifactPath],
}) + '\\n');
`,
  );
  if (process.platform === 'win32') {
    const commandPath = path.join(root, 'fake-cargo.cmd');
    fs.writeFileSync(commandPath, `@echo off\r\n"${process.execPath}" "${scriptPath}" %*\r\n`);
    return {
      commandPath,
      outputBinaryPath,
      sentinelPath,
    };
  }
  return {
    commandPath: scriptPath,
    outputBinaryPath,
    sentinelPath,
  };
}

test('dist/install.js ignores ancestor-resolved @napi-rs/cli and builds through cargo', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'jslite-install-security-'));
  try {
    const packageRoot = path.join(root, 'node_modules', '@keppoai', 'jslite');
    const fakeCliRoot = path.join(root, 'node_modules', '@napi-rs', 'cli');
    const fakeCargo = createFakeCargo(root);
    const cliSentinelPath = path.join(root, 'cli-sentinel.txt');

    fs.mkdirSync(packageRoot, { recursive: true });
    fs.mkdirSync(path.join(fakeCliRoot, 'dist'), { recursive: true });
    fs.mkdirSync(path.join(packageRoot, 'dist'), { recursive: true });
    fs.copyFileSync(path.join(repoRoot, 'dist', 'install.js'), path.join(packageRoot, 'dist', 'install.js'));
    fs.copyFileSync(
      path.join(repoRoot, 'dist', 'native-loader.js'),
      path.join(packageRoot, 'dist', 'native-loader.js'),
    );
    fs.cpSync(path.join(repoRoot, 'dist', 'lib'), path.join(packageRoot, 'dist', 'lib'), {
      recursive: true,
    });
    fs.writeFileSync(
      path.join(fakeCliRoot, 'package.json'),
      JSON.stringify({ name: '@napi-rs/cli', version: '0.0.0' }),
    );
    fs.writeFileSync(
      path.join(fakeCliRoot, 'dist', 'cli.js'),
      `require('node:fs').writeFileSync(${JSON.stringify(cliSentinelPath)}, 'ancestor-cli-ran');`,
    );

    const result = spawnSync(process.execPath, [path.join(packageRoot, 'dist', 'install.js')], {
      cwd: packageRoot,
      encoding: 'utf8',
      env: {
        ...process.env,
        CARGO: fakeCargo.commandPath,
      },
    });

    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /building from source/);
    assert.match(result.stdout, /built local addon at/);
    assert.equal(fs.existsSync(cliSentinelPath), false);
    assert.deepEqual(
      JSON.parse(fs.readFileSync(fakeCargo.sentinelPath, 'utf8')),
      [
        'build',
        '--release',
        '--manifest-path',
        'crates/jslite-node/Cargo.toml',
        '--message-format',
        'json-render-diagnostics',
      ],
    );
    const outputPath = path.join(packageRoot, getLocalBuildOutputFile());
    assert.equal(fs.readFileSync(outputPath, 'utf8'), 'fake-native-bytes');
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
