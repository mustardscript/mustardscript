'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..', '..');
const packageInfo = require(path.join(repoRoot, 'package.json'));

test('root package does not define a source-build install script', () => {
  assert.equal(packageInfo.scripts.install, undefined);
});

test('dist build does not emit an install helper', () => {
  assert.equal(fs.existsSync(path.join(repoRoot, 'dist', 'install.js')), false);
});
