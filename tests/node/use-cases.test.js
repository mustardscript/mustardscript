'use strict';

const path = require('node:path');
const test = require('node:test');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');

test('programmatic tool-call gallery audits the realistic use-case catalog', async () => {
  const script = path.join(__dirname, '../../scripts/audit-use-cases.js');
  const output = execFileSync(process.execPath, [script, '--json'], {
    encoding: 'utf8',
  });
  const summary = JSON.parse(output);

  assert.ok(summary.total >= 24);
  assert.ok(summary.passed > 0);
  assert.ok(summary.failed > 0);

  const categories = new Set(summary.results.map((result) => result.category));
  assert.deepEqual([...categories].sort(), ['analytics', 'operations', 'workflows']);
});
