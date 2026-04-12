'use strict';

const { execFileSync } = require('node:child_process');
const path = require('node:path');
const { resolvePrebuiltPackage, getCurrentPrebuiltTarget } = require('./native-loader');

let prebuilt = null;
let prebuiltError = null;
try {
  prebuilt = resolvePrebuiltPackage();
} catch (error) {
  prebuiltError = error;
}

if (prebuilt) {
  process.stdout.write(
    `jslite: using optional prebuilt addon from ${prebuilt.packageName}\n`,
  );
  process.exit(0);
}

const target = getCurrentPrebuiltTarget();
if (prebuiltError) {
  process.stdout.write(
    `jslite: ignoring invalid optional prebuilt for ${target?.platformArchABI ?? `${process.platform}-${process.arch}`}: ${prebuiltError.message}\n`,
  );
}
if (target) {
  process.stdout.write(
    `jslite: no installed prebuilt for ${target.platformArchABI}; building from source\n`,
  );
} else {
  process.stdout.write(
    `jslite: no configured prebuilt for ${process.platform}-${process.arch}; building from source\n`,
  );
}

execFileSync(
  process.execPath,
  [
    path.join(
      path.dirname(require.resolve('@napi-rs/cli/package.json')),
      'dist',
      'cli.js',
    ),
    'build',
    '--platform',
    '--manifest-path',
    'crates/jslite-node/Cargo.toml',
    '--js-package-name',
    '@keppoai/jslite',
    '--output-dir',
    '.',
    '--no-js',
  ],
  {
    cwd: __dirname,
    stdio: 'inherit',
    env: process.env,
  },
);
