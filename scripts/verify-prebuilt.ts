'use strict';

const { execFileSync } = require('node:child_process');
const path = require('node:path');

const repoRoot = path.resolve(__dirname, '..');

execFileSync(
  process.execPath,
  [
    '--test',
    '--test-name-pattern',
    'published prebuilt package',
    'tests/package-smoke.test.js',
  ],
  {
    cwd: repoRoot,
    stdio: 'inherit',
  },
);
