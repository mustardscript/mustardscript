'use strict';

const fs = require('node:fs');
const path = require('node:path');

const { RESULTS_DIR } = require('./support.ts');

function isMetricLeaf(value) {
  return (
    value !== null &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    typeof value.medianMs === 'number' &&
    typeof value.p95Ms === 'number'
  );
}

function flattenMetricTree(value, prefix = '', metrics = {}) {
  if (isMetricLeaf(value)) {
    metrics[prefix] = {
      medianMs: value.medianMs,
      p95Ms: value.p95Ms,
    };
    return metrics;
  }
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return metrics;
  }
  for (const [key, child] of Object.entries(value)) {
    if (key === 'machine' || key === 'notes' || key === 'reportPath' || key === 'ratios') {
      continue;
    }
    const childPrefix = prefix ? `${prefix}.${key}` : key;
    flattenMetricTree(child, childPrefix, metrics);
  }
  return metrics;
}

function percentChange(from, to) {
  if (from === 0) {
    return to === 0 ? 0 : Number.POSITIVE_INFINITY;
  }
  return ((to - from) / from) * 100;
}

function compareArtifacts(baselineArtifact, candidateArtifact) {
  const baselineMetrics = flattenMetricTree(baselineArtifact);
  const candidateMetrics = flattenMetricTree(candidateArtifact);
  const comparisons = [];

  for (const [pathKey, candidateMetric] of Object.entries(candidateMetrics)) {
    const baselineMetric = baselineMetrics[pathKey];
    if (!baselineMetric) {
      continue;
    }
    comparisons.push({
      path: pathKey,
      baselineMedianMs: baselineMetric.medianMs,
      candidateMedianMs: candidateMetric.medianMs,
      medianPct: percentChange(baselineMetric.medianMs, candidateMetric.medianMs),
      baselineP95Ms: baselineMetric.p95Ms,
      candidateP95Ms: candidateMetric.p95Ms,
      p95Pct: percentChange(baselineMetric.p95Ms, candidateMetric.p95Ms),
    });
  }

  return comparisons.sort((left, right) => left.path.localeCompare(right.path));
}

function inferIdentityFromFilename(filePath) {
  const name = path.basename(filePath);
  const kind =
    name.includes('-workloads') ? 'workloads' : name.includes('-smoke') ? 'smoke' : null;
  if (!kind) {
    return { kind: null, profile: null };
  }

  if (name.endsWith('-workloads.json')) {
    return { kind: 'workloads', profile: 'release' };
  }

  const profile = name.includes('-release.') ? 'release' : name.includes('-dev.') ? 'dev' : null;
  return { kind, profile };
}

function artifactIdentity(artifact, filePath) {
  const inferred = inferIdentityFromFilename(filePath);
  return {
    kind: artifact?.machine?.benchmarkKind ?? inferred.kind,
    profile: artifact?.machine?.buildProfile ?? inferred.profile,
  };
}

function loadArtifact(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function listArtifacts({ resultsDir = RESULTS_DIR, kind, profile } = {}) {
  if (!fs.existsSync(resultsDir)) {
    return [];
  }
  return fs
    .readdirSync(resultsDir)
    .filter((entry) => entry.endsWith('.json'))
    .map((entry) => path.join(resultsDir, entry))
    .filter((filePath) => {
      const identity = artifactIdentity(loadArtifact(filePath), filePath);
      if (kind && identity.kind !== kind) {
        return false;
      }
      if (profile && identity.profile !== profile) {
        return false;
      }
      return true;
    })
    .sort();
}

function resolveLatestArtifacts({ resultsDir = RESULTS_DIR, candidatePath, kind, profile } = {}) {
  const candidates = listArtifacts({ resultsDir, kind, profile });
  if (candidates.length === 0) {
    throw new Error(`No benchmark result artifacts found in ${resultsDir}`);
  }

  const resolvedCandidate = candidatePath
    ? path.resolve(candidatePath)
    : candidates[candidates.length - 1];
  const comparisonPool = candidates.filter((filePath) => path.resolve(filePath) !== resolvedCandidate);
  if (comparisonPool.length === 0) {
    throw new Error('Need at least two comparable benchmark artifacts to diff results');
  }

  return {
    candidatePath: resolvedCandidate,
    baselinePath: comparisonPool[comparisonPool.length - 1],
  };
}

function formatPercent(value) {
  if (!Number.isFinite(value)) {
    return '+inf%';
  }
  const sign = value > 0 ? '+' : '';
  return `${sign}${value.toFixed(1)}%`;
}

module.exports = {
  RESULTS_DIR,
  artifactIdentity,
  compareArtifacts,
  flattenMetricTree,
  formatPercent,
  inferIdentityFromFilename,
  listArtifacts,
  loadArtifact,
  resolveLatestArtifacts,
};
