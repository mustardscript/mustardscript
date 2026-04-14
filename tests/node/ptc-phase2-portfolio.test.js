'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  BROAD_USE_CASE_IDS,
  CATEGORY_ORDER,
  HEADLINE_USE_CASE_IDS,
  HOLDOUT_USE_CASE_IDS,
  USE_CASE_METADATA,
  buildPhase2Scorecards,
  metricNameForUseCase,
} = require('../../benchmarks/ptc-portfolio.ts');
const {
  createGalleryScenarios,
  loadExpectedGalleryResults,
} = require('../../benchmarks/ptc-gallery.ts');
const {
  createSentinelScenarios,
} = require('../../benchmarks/ptc-sentinels.ts');

test('phase-2 portfolio metadata covers all 24 audited gallery lanes with balanced panels', () => {
  const allIds = [...BROAD_USE_CASE_IDS, ...HOLDOUT_USE_CASE_IDS];
  assert.equal(new Set(allIds).size, 24);
  assert.deepEqual(
    CATEGORY_ORDER.map((category) => BROAD_USE_CASE_IDS.filter((id) => USE_CASE_METADATA[id].category === category).length),
    [4, 4, 4],
  );
  assert.deepEqual(
    CATEGORY_ORDER.map((category) => HOLDOUT_USE_CASE_IDS.filter((id) => USE_CASE_METADATA[id].category === category).length),
    [4, 4, 4],
  );
  assert.deepEqual(
    CATEGORY_ORDER.map((category) => HEADLINE_USE_CASE_IDS.filter((id) => USE_CASE_METADATA[id].category === category).length),
    [2, 2, 2],
  );

  for (const useCaseId of allIds) {
    const metadata = USE_CASE_METADATA[useCaseId];
    assert.ok(metadata, `${useCaseId} should have benchmark metadata`);
    assert.equal(typeof metadata.logicalPeakFanout, 'number');
    assert.ok(metadata.compactionExpectation.length > 0);
    assert.equal(typeof metadata.shapes.firstStageAsyncFanout, 'boolean');
    assert.equal(typeof metadata.shapes.repeatedStaticPropertyReads, 'boolean');
  }
});

test('gallery scenarios map the audited catalog into exact-check benchmark lanes', () => {
  const scenarios = createGalleryScenarios();
  const expected = loadExpectedGalleryResults();

  assert.equal(Object.keys(scenarios).length, 24);
  for (const [metricName, scenario] of Object.entries(scenarios)) {
    assert.equal(metricName, metricNameForUseCase(scenario.laneId));
    assert.equal(typeof scenario.source, 'string');
    assert.ok(scenario.source.includes('Inputs:'));
    assert.equal(typeof scenario.createCapabilities, 'function');
    assert.equal(typeof scenario.assertResult, 'function');
    scenario.assertResult(expected[scenario.laneId]);
  }
});

test('sentinel families expose the required initial variants', () => {
  const sentinels = createSentinelScenarios();

  assert.deepEqual(Object.keys(sentinels), [
    'code_mode_search',
    'result_materialization',
    'low_compaction_fanout',
  ]);
  assert.deepEqual(Object.keys(sentinels.code_mode_search), [
    'medium_compact',
    'large_compact',
    'large_structured',
  ]);
  assert.deepEqual(Object.keys(sentinels.result_materialization), [
    'medium_summary',
    'medium_structured',
    'medium_expanded',
  ]);
  assert.deepEqual(Object.keys(sentinels.low_compaction_fanout), [
    'medium_high_compaction',
    'medium_moderate_compaction',
    'medium_low_compaction',
  ]);
});

test('phase-2 scorecards average panel metrics and preserve ratio leaves', () => {
  const latency = Object.fromEntries(
    [...BROAD_USE_CASE_IDS, ...HOLDOUT_USE_CASE_IDS].map((id, index) => [
      metricNameForUseCase(id),
      { medianMs: index + 1, p95Ms: index + 2 },
    ]),
  );
  const isolateLatency = Object.fromEntries(
    [...BROAD_USE_CASE_IDS, ...HOLDOUT_USE_CASE_IDS].map((id, index) => [
      metricNameForUseCase(id),
      { medianMs: (index + 1) / 2, p95Ms: index + 1 },
    ]),
  );
  const scorecards = buildPhase2Scorecards(latency, isolateLatency, {
    code_mode_search: { medianMs: 3, p95Ms: 4 },
  });

  assert.ok(scorecards.headlineScore.medium.medianMs > 0);
  assert.ok(scorecards.broadScore.medium.medianMs > 0);
  assert.ok(scorecards.holdoutScore.medium.medianMs > 0);
  assert.ok(scorecards.categoryScore.analytics.medium.medianMs > 0);
  assert.ok(scorecards.p90LaneRatio.medium.medianRatio > 1);
  assert.ok(scorecards.worstLaneRatio.medium.p95Ratio > 1);
  assert.equal(scorecards.sentinelFamily.code_mode_search.medianMs, 3);
});
