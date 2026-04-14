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
const generatePrebuiltPackagesScriptPath = path.join(
  repoRoot,
  'scripts',
  'generate-prebuilt-packages.ts',
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
  // On Windows, Node 20+ refuses to spawn .cmd/.bat files directly (EINVAL
  // since CVE-2024-27980). Route them through the shell and quote args that
  // contain whitespace or shell metacharacters.
  const useShell = process.platform === 'win32' && /\.(cmd|bat)$/i.test(command);
  const finalArgs = useShell
    ? args.map((arg) =>
        /[\s"&|<>^()]/.test(arg) ? `"${String(arg).replace(/"/g, '\\"')}"` : arg,
      )
    : args;
  return execFileSync(command, finalArgs, {
    cwd,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    env: options.env ?? process.env,
    shell: useShell,
  });
}

function runGuestProgram(consumerRoot, source) {
  return run(
    process.execPath,
    [
      '-e',
      `
        const { Mustard } = require(${JSON.stringify(packageInfo.name)});
        (async () => {
          const runtime = new Mustard(${JSON.stringify(source)});
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
  return path.join(consumerRoot, 'node_modules', ...packageInfo.name.split('/'));
}

function readInstalledPackageManifest(consumerRoot) {
  return JSON.parse(
    fs.readFileSync(path.join(installedPackageRoot(consumerRoot), 'package.json'), 'utf8'),
  );
}

function assertInstalledReleaseFiles(consumerRoot) {
  const packageRoot = installedPackageRoot(consumerRoot);
  for (const file of ['LICENSE', 'README.md', 'SECURITY.md']) {
    assert.ok(fs.existsSync(path.join(packageRoot, file)), `${file} should be shipped`);
  }
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
      generatePrebuiltPackagesScriptPath,
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
      'crates/mustard-node/Cargo.toml',
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
  'published root package fails closed without a matching prebuilt package',
  { concurrency: false },
  async () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'mustard-package-smoke-'));
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
    run(npmCommand, ['install', '--omit=optional', tarballPath], consumerRoot);
    assertInstalledReleaseFiles(consumerRoot);
    assert.equal(readInstalledPackageManifest(consumerRoot).license, packageInfo.license);
    assert.equal(readInstalledPackageManifest(consumerRoot).scripts?.install, undefined);

    assert.throws(
      () => runGuestProgram(consumerRoot, 'const answer = 2; answer + 3;'),
      /Unable to locate a MustardScript native addon/,
    );
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
    fs.rmSync(tarballPath, { force: true });
  }
  },
);

const hostPrebuiltTarget = getCurrentPrebuiltTarget();

test(
  'published root package loads when the matching optional binding package is installed',
  {
    concurrency: false,
    skip: !hostPrebuiltTarget,
  },
  async () => {
    const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'mustard-prebuilt-smoke-'));
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
        ['install'],
        consumerRoot,
        {
          env: {
            ...process.env,
            CARGO: path.join(tempRoot, 'definitely-missing-cargo'),
          },
        },
      );
      assert.doesNotMatch(installOutput, /building from source|built local addon|using optional prebuilt addon/);
      assertInstalledReleaseFiles(consumerRoot);
      assert.equal(readInstalledPackageManifest(consumerRoot).scripts?.install, undefined);
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
