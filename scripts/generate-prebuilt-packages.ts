'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { PREBUILT_TARGETS } = require('../native-loader.ts');

const repoRoot = path.resolve(__dirname, '..');

function parseArgs(argv) {
  const options = {
    cwd: repoRoot,
    packageJsonPath: 'package.json',
    npmDir: 'npm',
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    switch (arg) {
      case '--cwd':
        options.cwd = path.resolve(argv[++index]);
        break;
      case '--package-json-path':
        options.packageJsonPath = argv[++index];
        break;
      case '--npm-dir':
        options.npmDir = argv[++index];
        break;
      default:
        throw new Error(`Unknown argument: ${arg}`);
    }
  }

  return options;
}

function readPackageInfo(options) {
  const packageJsonPath = path.resolve(options.cwd, options.packageJsonPath);
  return JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
}

function bindingReadme(rootPackageName, target) {
  return `# \`${target.packageName}\`

This is the **${target.triple}** native binding package for \`${rootPackageName}\`.
`;
}

function bindingManifest(rootPackage, target) {
  const manifest = {
    name: target.packageName,
    version: rootPackage.version,
    cpu: target.cpu,
    main: target.localFile,
    files: [target.localFile],
    description: rootPackage.description,
    keywords: rootPackage.keywords,
    author: rootPackage.author,
    homepage: rootPackage.homepage,
    license: rootPackage.license,
    repository: rootPackage.repository,
    bugs: rootPackage.bugs,
    publishConfig: rootPackage.publishConfig,
    os: target.os,
  };

  if (target.libc) {
    manifest.libc = target.libc;
  }

  return manifest;
}

function writeBindingPackage(options, rootPackage, target) {
  const packageDir = path.join(options.cwd, options.npmDir, target.platformArchABI);
  fs.mkdirSync(packageDir, { recursive: true });
  fs.writeFileSync(
    path.join(packageDir, 'package.json'),
    `${JSON.stringify(bindingManifest(rootPackage, target), null, 2)}\n`,
  );
  fs.writeFileSync(
    path.join(packageDir, 'README.md'),
    bindingReadme(rootPackage.name, target),
  );
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const rootPackage = readPackageInfo(options);
  const npmRoot = path.join(options.cwd, options.npmDir);
  fs.rmSync(npmRoot, { recursive: true, force: true });

  for (const target of PREBUILT_TARGETS) {
    writeBindingPackage(options, rootPackage, target);
  }
}

main();
