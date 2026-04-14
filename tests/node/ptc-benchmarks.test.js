'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  PTC_WEIGHTS,
  createCapabilityTransferProbe,
  createDurablePtcScenarios,
  createPtcScenarios,
  summarizePtcWeightedScore,
} = require('../../benchmarks/ptc-fixtures.ts');
const {
  normalizeRepoRelativePath,
} = require('../../benchmarks/ptc-gallery.ts');

test('PTC scenarios cover the website demo and the three representative benchmark lanes', () => {
  const scenarios = createPtcScenarios();
  const metricNames = Object.keys(scenarios);

  assert.deepEqual(metricNames, [
    'ptc_website_demo_small',
    'ptc_incident_triage_small',
    'ptc_fraud_investigation_small',
    'ptc_vendor_review_small',
    'ptc_website_demo_medium',
    'ptc_incident_triage_medium',
    'ptc_fraud_investigation_medium',
    'ptc_vendor_review_medium',
    'ptc_website_demo_large',
    'ptc_incident_triage_large',
    'ptc_fraud_investigation_large',
    'ptc_vendor_review_large',
  ]);

  for (const scenario of Object.values(scenarios)) {
    assert.equal(typeof scenario.source, 'string');
    assert.ok(scenario.source.length > 0);
    assert.equal(typeof scenario.createCapabilities, 'function');
    assert.equal(typeof scenario.assertResult, 'function');
    assert.equal(typeof scenario.shape.toolFamilyCount, 'number');
    assert.equal(typeof scenario.shape.logicalPeakFanout, 'number');
    assert.ok(scenario.shape.sourceRef.includes('examples/programmatic-tool-calls/'));
  }
});

test('durable PTC scenarios cover the persisted vendor-review checkpoint lane across sizes', () => {
  const scenarios = createDurablePtcScenarios();

  assert.deepEqual(Object.keys(scenarios), [
    'ptc_vendor_review_durable_small',
    'ptc_vendor_review_durable_medium',
    'ptc_vendor_review_durable_large',
    'ptc_plan-database-failover_durable_medium',
    'ptc_privacy-erasure-orchestration_durable_medium',
  ]);

  for (const scenario of Object.values(scenarios)) {
    assert.equal(typeof scenario.source, 'string');
    assert.ok(scenario.source.length > 0);
    assert.equal(typeof scenario.createCapabilities, 'function');
    assert.equal(typeof scenario.assertResult, 'function');
    assert.equal(typeof scenario.checkpointCapability, 'string');
    assert.equal(scenario.shape.finalAction, true);
    assert.equal(scenario.shape.durableBoundary, true);
    assert.ok(scenario.shape.sourceRef.includes('examples/programmatic-tool-calls/'));
  }
});

test('repo-relative benchmark source refs normalize Windows separators', () => {
  assert.equal(
    normalizeRepoRelativePath(String.raw`examples\programmatic-tool-calls\workflows\vendor-compliance-renewal-durable.js`),
    'examples/programmatic-tool-calls/workflows/vendor-compliance-renewal-durable.js',
  );
});

test('PTC medium-lane weights sum to 1 and match the intended scorecard', () => {
  const sum = Object.values(PTC_WEIGHTS).reduce((total, value) => total + value, 0);
  assert.equal(sum, 1);
  assert.deepEqual(PTC_WEIGHTS, {
    ptc_incident_triage_medium: 0.4,
    ptc_fraud_investigation_medium: 0.35,
    ptc_vendor_review_medium: 0.25,
  });
});

test('summarizePtcWeightedScore computes the weighted medium-lane score', () => {
  const summary = summarizePtcWeightedScore({
    ptc_incident_triage_medium: { medianMs: 10, p95Ms: 12 },
    ptc_fraud_investigation_medium: { medianMs: 8, p95Ms: 9 },
    ptc_vendor_review_medium: { medianMs: 4, p95Ms: 5 },
  });

  assert.ok(Math.abs(summary.medianMs - 7.8) < 1e-9);
  assert.ok(Math.abs(summary.p95Ms - 9.2) < 1e-9);
});

test('capability transfer probe counts tool calls, tool bytes, and result bytes', async () => {
  const probe = createCapabilityTransferProbe({
    load_alerts() {
      return [{ severity: 'high', summary: 'timeout detected' }];
    },
    file_review() {
      return { reviewRecordId: 'review_1', state: 'filed' };
    },
  });

  const alerts = probe.capabilities.load_alerts();
  const review = probe.capabilities.file_review();
  const summary = probe.finalize({
    alertCount: alerts.length,
    reviewRecordId: review.reviewRecordId,
  });

  assert.equal(summary.toolCallCount, 2);
  assert.equal(summary.toolFamilyCount, 2);
  assert.ok(summary.toolBytesIn > 0);
  assert.ok(summary.resultBytesOut > 0);
  assert.ok(summary.reductionRatio > 1);
});
