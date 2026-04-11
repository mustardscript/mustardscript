'use strict';

const path = require('node:path');
const test = require('node:test');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');

function readAuditSummary() {
  const script = path.join(__dirname, '../../scripts/audit-use-cases.js');
  const output = execFileSync(process.execPath, [script, '--json'], {
    encoding: 'utf8',
  });
  return JSON.parse(output);
}

test('programmatic tool-call gallery audits the realistic use-case catalog', async () => {
  const summary = readAuditSummary();

  assert.ok(summary.total >= 24);
  assert.equal(summary.passed, summary.total);
  assert.equal(summary.failed, 0);

  const categories = new Set(summary.results.map((result) => result.category));
  assert.deepEqual([...categories].sort(), ['analytics', 'operations', 'workflows']);
});

test('builtin-focused gallery cases now pass end to end', async () => {
  const summary = readAuditSummary();
  const results = new Map(summary.results.map((result) => [result.id, result]));

  for (const id of [
    'analytics_fraud_ring',
    'analytics_supplier_disruption',
    'analytics_market_event_brief',
    'analyze-queue-backlog-regression',
    'guard-payments-rollout',
    'assess-global-deployment-freeze',
    'approval-exception-routing',
    'privacy-erasure-orchestration',
    'vip-support-escalation',
    'vendor-compliance-renewal',
  ]) {
    assert.equal(results.get(id)?.ok, true, `${id} should pass after builtin/runtime support`);
  }
});
