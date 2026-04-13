'use strict';

const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const {
  resolvePrebuiltPackage,
  getCurrentPrebuiltTarget,
  getLocalBuildOutputFile,
} = require('./native-loader.ts');

const packageRoot = path.basename(__dirname) === 'dist' ? path.dirname(__dirname) : __dirname;

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

function nativeLibraryExtension(platform) {
  switch (platform) {
    case 'win32':
      return '.dll';
    case 'darwin':
      return '.dylib';
    default:
      return '.so';
  }
}

function parseCargoArtifactPath(output) {
  const expectedExtension = nativeLibraryExtension(process.platform);
  let artifactPath = null;
  for (const line of output.split(/\r?\n/u)) {
    if (line.trim() === '') {
      continue;
    }
    let event;
    try {
      event = JSON.parse(line);
    } catch {
      continue;
    }
    if (event.reason !== 'compiler-artifact') {
      continue;
    }
    if (event.target?.name !== 'jslite_node') {
      continue;
    }
    if (
      !Array.isArray(event.target?.crate_types) ||
      !event.target.crate_types.includes('cdylib')
    ) {
      continue;
    }
    if (!Array.isArray(event.filenames)) {
      continue;
    }
    const filename = event.filenames.find((entry) => entry.endsWith(expectedExtension));
    if (filename) {
      artifactPath = filename;
    }
  }
  if (!artifactPath) {
    throw new Error('jslite: cargo did not report a native cdylib artifact');
  }
  return artifactPath;
}

const cargo = process.env.CARGO || 'cargo';
const cargoOutput = execFileSync(
  cargo,
  [
    'build',
    '--release',
    '--manifest-path',
    'crates/jslite-node/Cargo.toml',
    '--message-format',
    'json-render-diagnostics',
  ],
  {
    cwd: packageRoot,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'inherit'],
    env: process.env,
  },
);

const artifactPath = parseCargoArtifactPath(cargoOutput);
const outputPath = path.join(packageRoot, getLocalBuildOutputFile());
fs.copyFileSync(artifactPath, outputPath);
process.stdout.write(`jslite: built local addon at ${path.basename(outputPath)}\n`);
