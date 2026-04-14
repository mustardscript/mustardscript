'use strict';

const CATEGORY_ORDER = Object.freeze(['analytics', 'operations', 'workflows']);

const HEADLINE_USE_CASE_IDS = Object.freeze([
  'analytics_fraud_ring',
  'analytics_revenue_quality',
  'triage-multi-region-auth-outage',
  'analyze-queue-backlog-regression',
  'vendor-compliance-renewal',
  'privacy-erasure-orchestration',
]);

const BROAD_USE_CASE_IDS = Object.freeze([
  'analytics_revenue_quality',
  'analytics_fraud_ring',
  'analytics_supplier_disruption',
  'analytics_model_regression',
  'triage-multi-region-auth-outage',
  'reconcile-marketplace-payouts',
  'analyze-queue-backlog-regression',
  'plan-database-failover',
  'security-access-recertification',
  'vendor-compliance-renewal',
  'privacy-erasure-orchestration',
  'chargeback-evidence-assembly',
]);

const HOLDOUT_USE_CASE_IDS = Object.freeze([
  'analytics_market_event_brief',
  'analytics_enterprise_renewal',
  'analytics_market_abuse_review',
  'analytics_capital_allocation',
  'guard-payments-rollout',
  'stabilize-oncall-handoff',
  'coordinate-warehouse-exception',
  'assess-global-deployment-freeze',
  'approval-exception-routing',
  'vip-support-escalation',
  'payout-batch-release-review',
  'enterprise-renewal-save-plan',
]);

const SENTINEL_FAMILY_METADATA = Object.freeze({
  code_mode_search: {
    description:
      'Large preloaded typed-API search surfaces, preload footprint, and result-size sensitivity.',
    delegatedShapes: [
      'preloaded typed-API catalog scans',
      'first-search latency after large catalog install',
      'warm repeated-search latency',
      'result-size sensitivity for compact versus structured answers',
    ],
  },
  result_materialization: {
    description:
      'Boundary output-materialization cost when guest work stays mostly fixed but the returned structure expands.',
    delegatedShapes: [
      'boundary encode cost dominated by result bytes',
      'structured versus expanded answer reflection',
      'result-shape sensitivity with comparable guest execution',
    ],
  },
  low_compaction_fanout: {
    description:
      'Realistic fanout workloads where large intermediate data stays inside the runtime but final answers compact less aggressively.',
    delegatedShapes: [
      'moderate-compaction host fanout',
      'tool-bytes-in versus result-bytes-out curves below the current gallery sweet spot',
      'memory retained during broader local reductions',
    ],
  },
});

const USE_CASE_METADATA = Object.freeze({
  analytics_revenue_quality: {
    category: 'analytics',
    logicalPeakFanout: 9,
    compactionExpectation: 'lower_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: true,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: false,
      localRankingOrSort: true,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: false,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  analytics_fraud_ring: {
    category: 'analytics',
    logicalPeakFanout: 4,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: true,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: true,
      localRankingOrSort: false,
      stringNormalizationOrClassification: true,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  analytics_supplier_disruption: {
    category: 'analytics',
    logicalPeakFanout: 4,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: true,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: true,
      localRankingOrSort: true,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: false,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  analytics_market_event_brief: {
    category: 'analytics',
    logicalPeakFanout: 5,
    compactionExpectation: 'lower_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: false,
      localRankingOrSort: false,
      stringNormalizationOrClassification: true,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  analytics_model_regression: {
    category: 'analytics',
    logicalPeakFanout: 7,
    compactionExpectation: 'lower_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: true,
      mapHeavyJoinOrCounter: false,
      setHeavyDedupe: false,
      localRankingOrSort: false,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  analytics_enterprise_renewal: {
    category: 'analytics',
    logicalPeakFanout: 1,
    compactionExpectation: 'lower_compaction',
    shapes: {
      firstStageAsyncFanout: false,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: false,
      setHeavyDedupe: false,
      localRankingOrSort: false,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: false,
      finalActionWriteback: false,
      durableBoundary: true,
    },
  },
  analytics_market_abuse_review: {
    category: 'analytics',
    logicalPeakFanout: 1,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: false,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: false,
      setHeavyDedupe: false,
      localRankingOrSort: false,
      stringNormalizationOrClassification: true,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: false,
      durableBoundary: true,
    },
  },
  analytics_capital_allocation: {
    category: 'analytics',
    logicalPeakFanout: 4,
    compactionExpectation: 'lower_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: false,
      localRankingOrSort: true,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: false,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  'triage-multi-region-auth-outage': {
    category: 'operations',
    logicalPeakFanout: 10,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: true,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: true,
      localRankingOrSort: false,
      stringNormalizationOrClassification: true,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  'guard-payments-rollout': {
    category: 'operations',
    logicalPeakFanout: 7,
    compactionExpectation: 'lower_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: true,
      mapHeavyJoinOrCounter: false,
      setHeavyDedupe: false,
      localRankingOrSort: true,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: false,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  'reconcile-marketplace-payouts': {
    category: 'operations',
    logicalPeakFanout: 7,
    compactionExpectation: 'lower_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: true,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: false,
      localRankingOrSort: false,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: false,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  'stabilize-oncall-handoff': {
    category: 'operations',
    logicalPeakFanout: 4,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: false,
      localRankingOrSort: false,
      stringNormalizationOrClassification: true,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  'analyze-queue-backlog-regression': {
    category: 'operations',
    logicalPeakFanout: 12,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: true,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: false,
      localRankingOrSort: true,
      stringNormalizationOrClassification: true,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  'plan-database-failover': {
    category: 'operations',
    logicalPeakFanout: 1,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: false,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: false,
      setHeavyDedupe: false,
      localRankingOrSort: false,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: true,
      durableBoundary: true,
    },
  },
  'coordinate-warehouse-exception': {
    category: 'operations',
    logicalPeakFanout: 5,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: false,
      localRankingOrSort: false,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  'assess-global-deployment-freeze': {
    category: 'operations',
    logicalPeakFanout: 8,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: true,
      mapHeavyJoinOrCounter: false,
      setHeavyDedupe: false,
      localRankingOrSort: true,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: false,
      durableBoundary: false,
    },
  },
  'approval-exception-routing': {
    category: 'workflows',
    logicalPeakFanout: 1,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: false,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: false,
      setHeavyDedupe: true,
      localRankingOrSort: false,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: false,
      finalActionWriteback: true,
      durableBoundary: true,
    },
  },
  'security-access-recertification': {
    category: 'workflows',
    logicalPeakFanout: 5,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: true,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: true,
      localRankingOrSort: false,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: false,
      finalActionWriteback: true,
      durableBoundary: false,
    },
  },
  'vip-support-escalation': {
    category: 'workflows',
    logicalPeakFanout: 5,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: false,
      setHeavyDedupe: false,
      localRankingOrSort: true,
      stringNormalizationOrClassification: true,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: true,
      durableBoundary: false,
    },
  },
  'payout-batch-release-review': {
    category: 'workflows',
    logicalPeakFanout: 4,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: true,
      localRankingOrSort: false,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: false,
      finalActionWriteback: true,
      durableBoundary: false,
    },
  },
  'enterprise-renewal-save-plan': {
    category: 'workflows',
    logicalPeakFanout: 4,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: true,
      setHeavyDedupe: false,
      localRankingOrSort: false,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: false,
      finalActionWriteback: true,
      durableBoundary: false,
    },
  },
  'vendor-compliance-renewal': {
    category: 'workflows',
    logicalPeakFanout: 4,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: false,
      setHeavyDedupe: false,
      localRankingOrSort: false,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: false,
      finalActionWriteback: true,
      durableBoundary: false,
    },
  },
  'privacy-erasure-orchestration': {
    category: 'workflows',
    logicalPeakFanout: 1,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: false,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: false,
      setHeavyDedupe: false,
      localRankingOrSort: false,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: true,
      durableBoundary: true,
    },
  },
  'chargeback-evidence-assembly': {
    category: 'workflows',
    logicalPeakFanout: 4,
    compactionExpectation: 'moderate_compaction',
    shapes: {
      firstStageAsyncFanout: true,
      derivedIdSecondStageFanout: false,
      mapHeavyJoinOrCounter: false,
      setHeavyDedupe: false,
      localRankingOrSort: false,
      stringNormalizationOrClassification: false,
      repeatedStaticPropertyReads: true,
      chronologyReasoning: true,
      finalActionWriteback: true,
      durableBoundary: false,
    },
  },
});

function metricNameForUseCase(id, sizeName = 'medium', seedName = 'nominal') {
  return seedName === 'nominal'
    ? `ptc_${id}_${sizeName}`
    : `ptc_${id}_${sizeName}_${seedName}`;
}

function metricNamesForUseCases(ids, sizeName = 'medium', seedName = 'nominal') {
  return ids.map((id) => metricNameForUseCase(id, sizeName, seedName));
}

function averageMetric(latencyByName, metricNames) {
  if (!Array.isArray(metricNames) || metricNames.length === 0) {
    return null;
  }

  let medianMs = 0;
  let p95Ms = 0;
  let matched = 0;

  for (const metricName of metricNames) {
    const metric = latencyByName[metricName];
    if (!metric) {
      continue;
    }
    medianMs += metric.medianMs;
    p95Ms += metric.p95Ms;
    matched += 1;
  }

  if (matched === 0) {
    return null;
  }

  return {
    medianMs: medianMs / matched,
    p95Ms: p95Ms / matched,
  };
}

function percentile(sortedValues, ratio) {
  if (sortedValues.length === 0) {
    return 1;
  }
  const index = Math.min(
    sortedValues.length - 1,
    Math.max(0, Math.ceil(sortedValues.length * ratio) - 1),
  );
  return sortedValues[index];
}

function ratioLeaf(runtimeLatencyByName, isolateLatencyByName, metricNames, pick) {
  const values = [];
  for (const metricName of metricNames) {
    const runtimeMetric = runtimeLatencyByName[metricName];
    const isolateMetric = isolateLatencyByName[metricName];
    if (!runtimeMetric || !isolateMetric) {
      continue;
    }
    const ratio = pick(runtimeMetric, isolateMetric);
    if (Number.isFinite(ratio)) {
      values.push(ratio);
    }
  }
  if (values.length === 0) {
    return null;
  }
  values.sort((left, right) => left - right);
  return {
    medianRatio: percentile(values, 0.9),
    p95Ratio: values[values.length - 1],
  };
}

function categoryMetricNames(useCaseIds) {
  return Object.fromEntries(
    CATEGORY_ORDER.map((category) => [
      category,
      metricNamesForUseCases(
        useCaseIds.filter((id) => USE_CASE_METADATA[id].category === category),
      ),
    ]),
  );
}

const CATEGORY_METRIC_NAMES = Object.freeze(categoryMetricNames(BROAD_USE_CASE_IDS));

function buildPhase2Scorecards(runtimeLatencyByName, isolateLatencyByName, sentinelFamilyScores) {
  const headlineMetricNames = metricNamesForUseCases(HEADLINE_USE_CASE_IDS);
  const broadMetricNames = metricNamesForUseCases(BROAD_USE_CASE_IDS);
  const holdoutMetricNames = metricNamesForUseCases(HOLDOUT_USE_CASE_IDS);

  return {
    headlineScore: {
      medium: averageMetric(runtimeLatencyByName, headlineMetricNames),
    },
    broadScore: {
      medium: averageMetric(runtimeLatencyByName, broadMetricNames),
    },
    holdoutScore: {
      medium: averageMetric(runtimeLatencyByName, holdoutMetricNames),
    },
    categoryScore: Object.fromEntries(
      CATEGORY_ORDER.map((category) => [
        category,
        {
          medium: averageMetric(runtimeLatencyByName, CATEGORY_METRIC_NAMES[category]),
        },
      ]),
    ),
    p90LaneRatio: {
      medium: ratioLeaf(runtimeLatencyByName, isolateLatencyByName, broadMetricNames, (runtime, isolate) =>
        runtime.medianMs / isolate.medianMs),
    },
    worstLaneRatio: {
      medium: ratioLeaf(runtimeLatencyByName, isolateLatencyByName, broadMetricNames, (runtime, isolate) =>
        runtime.p95Ms / isolate.p95Ms),
    },
    sentinelFamily: sentinelFamilyScores,
  };
}

function verifyPortfolioIntegrity() {
  const allIds = [...BROAD_USE_CASE_IDS, ...HOLDOUT_USE_CASE_IDS];
  const uniqueIds = new Set(allIds);
  if (uniqueIds.size !== allIds.length) {
    throw new Error('Phase-2 PTC panels contain duplicate use-case ids');
  }
  for (const useCaseId of uniqueIds) {
    if (!USE_CASE_METADATA[useCaseId]) {
      throw new Error(`Missing PTC portfolio metadata for ${useCaseId}`);
    }
  }
  for (const useCaseId of HEADLINE_USE_CASE_IDS) {
    if (!BROAD_USE_CASE_IDS.includes(useCaseId)) {
      throw new Error(`Headline panel id ${useCaseId} must remain a subset of the broad panel`);
    }
  }
}

verifyPortfolioIntegrity();

module.exports = {
  BROAD_USE_CASE_IDS,
  CATEGORY_ORDER,
  CATEGORY_METRIC_NAMES,
  HEADLINE_USE_CASE_IDS,
  HOLDOUT_USE_CASE_IDS,
  SENTINEL_FAMILY_METADATA,
  USE_CASE_METADATA,
  averageMetric,
  buildPhase2Scorecards,
  metricNameForUseCase,
  metricNamesForUseCases,
};
