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

      run(npmCommand, ['init', '-y'], consumerRoot);
      run(
        npmCommand,
        [
          'install',
          '--ignore-scripts',
          rootTarball.tarballPath,
          hostTarball.tarballPath,
        ],
        consumerRoot,
      );

      const installOutput = run(
        process.execPath,
        [
          path.join(
            consumerRoot,
            'node_modules',
            '@keppoai',
            'jslite',
            'dist/install.js',
          ),
        ],
        consumerRoot,
        {
          env: {
            ...process.env,
            CARGO: path.join(tempRoot, 'definitely-missing-cargo'),
          },
        },
      );
      assert.match(installOutput, /using optional prebuilt addon/);
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
