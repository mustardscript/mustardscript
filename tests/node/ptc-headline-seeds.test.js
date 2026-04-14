'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  HEADLINE_USE_CASE_IDS,
  metricNameForUseCase,
} = require('../../benchmarks/ptc-portfolio.ts');
const {
  createHeadlineSeedScenarios,
  loadExpectedHeadlineSeedResults,
} = require('../../benchmarks/ptc-headline-seeds.ts');

test('headline skew seed scenarios cover every phase-2 headline lane with exact expected outputs', () => {
  const scenarios = createHeadlineSeedScenarios();
  const expected = loadExpectedHeadlineSeedResults();

  assert.deepEqual(
    Object.keys(scenarios).sort(),
    HEADLINE_USE_CASE_IDS.map((id) => metricNameForUseCase(id, 'medium', 'skewed')).sort(),
  );

  for (const [metricName, scenario] of Object.entries(scenarios)) {
    assert.equal(typeof scenario.source, 'string');
    assert.ok(scenario.source.length > 0);
    assert.equal(scenario.seedName, 'skewed');
    assert.equal(scenario.nominalMetricName, metricNameForUseCase(scenario.laneId));
    assert.ok(Array.isArray(scenario.skewPatterns));
    assert.ok(scenario.skewPatterns.length > 0);
    assert.equal(typeof scenario.createCapabilities, 'function');
    assert.equal(typeof scenario.assertResult, 'function');
    scenario.assertResult(expected[metricName]);
  }
});
