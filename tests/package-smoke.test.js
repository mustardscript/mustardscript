'use strict';

const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const repoRoot = path.resolve(__dirname, '..');
const napiCliPath = path.join(
  path.dirname(require.resolve('@napi-rs/cli/package.json', { paths: [repoRoot] })),
  'dist',
  'cli.js',
);
const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const packageInfo = require(path.join(repoRoot, 'package.json'));
const {
  PREBUILT_TARGETS,
  getCurrentPrebuiltTarget,
} = require(path.join(repoRoot, 'native-loader.ts'));
const SIDECAR_PROTOCOL_VERSION = 1;

function tarballFilenameForPackage(name, version) {
  return `${name.replace(/^@/, '').replace(/\//g, '-')}-${version}.tgz`;
}

function run(command, args, cwd, options = {}) {
  return execFileSync(command, args, {
    cwd,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    env: options.env ?? process.env,
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

function installedPackageRoot(consumerRoot) {
  return path.join(consumerRoot, 'node_modules', '@keppoai', 'jslite');
}

function readInstalledPackageManifest(consumerRoot) {
  return JSON.parse(
    fs.readFileSync(path.join(installedPackageRoot(consumerRoot), 'package.json'), 'utf8'),
  );
}

function assertInstalledReleaseFiles(consumerRoot) {
  const packageRoot = installedPackageRoot(consumerRoot);
  for (const file of ['Cargo.lock', 'LICENSE', 'SECURITY.md']) {
    assert.ok(fs.existsSync(path.join(packageRoot, file)), `${file} should be shipped`);
  }
}

function runInstalledSidecarSmoke(consumerRoot, source) {
  const packageRoot = installedPackageRoot(consumerRoot);
  run('cargo', ['build', '-q', '-p', 'jslite-sidecar'], packageRoot);

  const response = JSON.parse(
    run(
      process.execPath,
      [
        '-e',
        `
          const path = require('node:path');
          const readline = require('node:readline');
          const { spawn } = require('node:child_process');

          const packageRoot = ${JSON.stringify(packageRoot)};
          const executable = path.join(
            packageRoot,
            'target',
            'debug',
            process.platform === 'win32' ? 'jslite-sidecar.exe' : 'jslite-sidecar'
          );
          const child = spawn(executable, [], {
            cwd: packageRoot,
            stdio: ['pipe', 'pipe', 'pipe'],
          });
          const reader = readline.createInterface({ input: child.stdout });
          let stderr = '';
          child.stderr.on('data', (chunk) => {
            stderr += chunk.toString('utf8');
          });

          function readResponse() {
            return new Promise((resolve, reject) => {
              reader.once('line', resolve);
              child.once('error', reject);
            });
          }

          (async () => {
            child.stdin.write(JSON.stringify({
              protocol_version: ${SIDECAR_PROTOCOL_VERSION},
              method: 'compile',
              id: 1,
              source: ${JSON.stringify(source)},
            }) + '\\n');
            const compile = JSON.parse(await readResponse());
            if (!compile.ok) {
              throw new Error('compile failed: ' + JSON.stringify(compile));
            }

            child.stdin.write(JSON.stringify({
              protocol_version: ${SIDECAR_PROTOCOL_VERSION},
              method: 'start',
              id: 2,
              program_base64: compile.result.program_base64,
              options: { inputs: {}, capabilities: [], limits: {} },
            }) + '\\n');
            const start = JSON.parse(await readResponse());
            process.stdout.write(JSON.stringify(start));
            reader.close();
            child.stdin.end();
            await new Promise((resolve) => child.once('close', resolve));
          })().catch((error) => {
            console.error(error);
            console.error(stderr);
            process.exit(1);
          });
        `,
      ],
      packageRoot,
    ),
  );

  assert.equal(response.protocol_version, SIDECAR_PROTOCOL_VERSION);
  assert.equal(response.ok, true);
  assert.equal(response.result.step.type, 'completed');
}

function packTarball(cwd) {
  const [packed] = JSON.parse(run(npmCommand, ['pack', '--json'], cwd));
  return {
    ...packed,
    tarballPath: path.join(cwd, packed.filename),
  };
}

function createPrebuiltStagingRoot(tempRoot) {
  const stagingRoot = path.join(tempRoot, 'prebuilt-staging');
  fs.mkdirSync(stagingRoot);
  fs.writeFileSync(
    path.join(stagingRoot, 'package.json'),
    `${JSON.stringify(packageInfo, null, 2)}\n`,
  );
  run(
    process.execPath,
    [
      napiCliPath,
      'create-npm-dirs',
      '--cwd',
      stagingRoot,
      '--package-json-path',
      'package.json',
      '--npm-dir',
      'npm',
    ],
    repoRoot,
  );
  return stagingRoot;
}

function verifyPrebuiltPackageMetadata(stagingRoot) {
  assert.deepEqual(
    packageInfo.napi.targets,
    PREBUILT_TARGETS.map((target) => target.triple),
  );
  assert.deepEqual(
    packageInfo.optionalDependencies,
    Object.fromEntries(
      PREBUILT_TARGETS.map((target) => [target.packageName, packageInfo.version]),
    ),
  );

  for (const target of PREBUILT_TARGETS) {
    const packageRoot = path.join(stagingRoot, 'npm', target.platformArchABI);
    const manifest = JSON.parse(
      fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8'),
    );
    assert.equal(manifest.name, target.packageName);
    assert.equal(manifest.version, packageInfo.version);
    assert.equal(manifest.main, target.localFile);
    assert.deepEqual(manifest.files, [target.localFile]);
    assert.deepEqual(manifest.os, target.os);
    assert.deepEqual(manifest.cpu, target.cpu);
    if (target.libc) {
      assert.deepEqual(manifest.libc, target.libc);
    } else {
      assert.equal(manifest.libc, undefined);
    }
    assert.equal(manifest.publishConfig.access, packageInfo.publishConfig.access);

    const readme = fs.readFileSync(path.join(packageRoot, 'README.md'), 'utf8');
    assert.match(readme, new RegExp(target.packageName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
    assert.match(readme, new RegExp(target.triple.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  }
}

function stageHostPrebuiltBinary(tempRoot, stagingRoot, hostTarget) {
  const artifactsRoot = path.join(tempRoot, 'prebuilt-artifacts');
  const artifactsDirFromStaging = path.relative(stagingRoot, artifactsRoot);
  fs.mkdirSync(artifactsRoot);
  run(
    process.execPath,
    [
      napiCliPath,
      'build',
      '--release',
      '--platform',
      '--manifest-path',
      'crates/jslite-node/Cargo.toml',
      '--js-package-name',
      packageInfo.name,
      '--output-dir',
      artifactsRoot,
      '--no-js',
    ],
    repoRoot,
  );
  run(
    process.execPath,
    [
      napiCliPath,
      'artifacts',
      '--cwd',
      stagingRoot,
      '--package-json-path',
      'package.json',
      '--output-dir',
      artifactsDirFromStaging,
      '--npm-dir',
      'npm',
    ],
    repoRoot,
  );

  const packageRoot = path.join(stagingRoot, 'npm', hostTarget.platformArchABI);
  assert.ok(fs.existsSync(path.join(packageRoot, hostTarget.localFile)));
  return packageRoot;
}

test(
  'published source package installs, reinstalls, and runs from a fresh consumer project',
  { concurrency: false },
  async () => {
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
    assertInstalledReleaseFiles(consumerRoot);
    assert.equal(readInstalledPackageManifest(consumerRoot).license, packageInfo.license);
    assert.equal(
      runGuestProgram(consumerRoot, 'const answer = 2; answer + 3;'),
      '5',
    );
    runInstalledSidecarSmoke(consumerRoot, 'const answer = 40; answer + 2;');

    run(npmCommand, ['install', tarballPath], consumerRoot);
    assert.equal(
      runGuestProgram(consumerRoot, 'let total = 40; total = total + 2; total;'),
      '42',
    );
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
    fs.rmSync(tarballPath, { force: true });
  }
  },
);

const hostPrebuiltTarget = getCurrentPrebuiltTarget();

test(
  'published prebuilt package loads without a source build when the matching optional package is installed',
  {
    concurrency: false,
    skip: !hostPrebuiltTarget,
  },
  async () => {
    const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'jslite-prebuilt-smoke-'));
    const consumerRoot = path.join(tempRoot, 'consumer');
    const rootTarballName = tarballFilenameForPackage(packageInfo.name, packageInfo.version);
    const rootTarballPath = path.join(repoRoot, rootTarballName);

    fs.mkdirSync(consumerRoot);
    fs.rmSync(rootTarballPath, { force: true });

    try {
      const stagingRoot = createPrebuiltStagingRoot(tempRoot);
      verifyPrebuiltPackageMetadata(stagingRoot);

      const hostPackageRoot = stageHostPrebuiltBinary(
        tempRoot,
        stagingRoot,
        hostPrebuiltTarget,
      );
      const rootTarball = packTarball(repoRoot);
      const hostTarball = packTarball(hostPackageRoot);

      assert.equal(rootTarball.filename, rootTarballName);
      assert.equal(hostTarball.name, hostPrebuiltTarget.packageName);

      fs.writeFileSync(
        path.join(consumerRoot, 'package.json'),
        `${JSON.stringify(
          {
            name: 'consumer',
            private: true,
            version: '1.0.0',
            dependencies: {
              [packageInfo.name]: rootTarball.tarballPath,
            },
            overrides: {
              [hostPrebuiltTarget.packageName]: hostTarball.tarballPath,
            },
          },
          null,
          2,
        )}\n`,
      );

      const installOutput = run(
        npmCommand,
        ['install', '--foreground-scripts'],
        consumerRoot,
        {
          env: {
            ...process.env,
            CARGO: path.join(tempRoot, 'definitely-missing-cargo'),
          },
        },
      );
      assert.match(installOutput, /using optional prebuilt addon/);
      assertInstalledReleaseFiles(consumerRoot);
      assert.deepEqual(
        readInstalledPackageManifest(consumerRoot).optionalDependencies,
        packageInfo.optionalDependencies,
      );
      assert.equal(
        runGuestProgram(consumerRoot, 'let total = 40; total = total + 2; total;'),
        '42',
      );
    } finally {
      fs.rmSync(tempRoot, { recursive: true, force: true });
      fs.rmSync(rootTarballPath, { force: true });
    }
  },
);
