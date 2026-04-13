#!/usr/bin/env node
'use strict';

const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..');
const distRoot = path.join(repoRoot, 'dist');
const runtimeSources = [
  'index.ts',
  'install.ts',
  'native-loader.ts',
  'lib/cancellation.ts',
  'lib/errors.ts',
  'lib/executor.ts',
  'lib/policy.ts',
  'lib/progress.ts',
  'lib/runtime.ts',
  'lib/structured.ts',
];

function toDistRelativePath(sourceRelativePath) {
  return sourceRelativePath.replace(/\.ts$/u, '.js');
}

function rewriteRuntimeImports(source) {
  return source.replace(
    /require\((['"])(\.{1,2}\/[^'"]+?)\.ts\1\)/gu,
    'require($1$2.js$1)',
  );
}

function buildRuntimeFile(sourceRelativePath) {
  const sourcePath = path.join(repoRoot, sourceRelativePath);
  const outputPath = path.join(distRoot, toDistRelativePath(sourceRelativePath));
  const contents = fs.readFileSync(sourcePath, 'utf8');
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, rewriteRuntimeImports(contents));
}

function main() {
  fs.rmSync(distRoot, { recursive: true, force: true });
  for (const sourceRelativePath of runtimeSources) {
    buildRuntimeFile(sourceRelativePath);
  }
}

main();
