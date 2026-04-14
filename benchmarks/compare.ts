'use strict';

const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const { RESULTS_DIR } = require('./support.ts');
const REPO_ROOT = path.join(__dirname, '..');

function metricLeafKind(value) {
  if (
    value !== null &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    typeof value.medianMs === 'number' &&
    typeof value.p95Ms === 'number'
  ) {
    return 'ms';
  }
  if (
    value !== null &&
    typeof value === 'object' &&
    !Array.isArray(value) &&
    typeof value.medianRatio === 'number' &&
    typeof value.p95Ratio === 'number'
  ) {
    return 'ratio';
  }
  return null;
}

function isMetricLeaf(value) {
  return metricLeafKind(value) !== null;
}

function flattenMetricTree(value, prefix = '', metrics = {}) {
  const kind = metricLeafKind(value);
  if (kind === 'ms') {
    metrics[prefix] = {
      kind,
      medianMs: value.medianMs,
      p95Ms: value.p95Ms,
    };
    return metrics;
  }
  if (kind === 'ratio') {
    metrics[prefix] = {
      kind,
      medianRatio: value.medianRatio,
      p95Ratio: value.p95Ratio,
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

function metricValues(metric) {
  if (metric.kind === 'ratio') {
    return {
      median: metric.medianRatio,
      p95: metric.p95Ratio,
    };
  }
  return {
    median: metric.medianMs,
    p95: metric.p95Ms,
  };
}

function compareArtifacts(baselineArtifact, candidateArtifact) {
  const baselineMetrics = flattenMetricTree(baselineArtifact);
  const candidateMetrics = flattenMetricTree(candidateArtifact);
  const comparisons = [];

  for (const [pathKey, candidateMetric] of Object.entries(candidateMetrics)) {
    const baselineMetric = baselineMetrics[pathKey];
    if (!baselineMetric || baselineMetric.kind !== candidateMetric.kind) {
      continue;
    }
    const baselineValues = metricValues(baselineMetric);
    const candidateValues = metricValues(candidateMetric);
    comparisons.push({
      path: pathKey,
      kind: candidateMetric.kind,
      baselineMedian: baselineValues.median,
      candidateMedian: candidateValues.median,
      medianPct: percentChange(baselineValues.median, candidateValues.median),
      baselineP95: baselineValues.p95,
      candidateP95: candidateValues.p95,
      p95Pct: percentChange(baselineValues.p95, candidateValues.p95),
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

function listTrackedArtifacts({ resultsDir = RESULTS_DIR, kind, profile } = {}) {
  let trackedEntries;
  try {
    trackedEntries = execFileSync('git', ['ls-files', 'benchmarks/results/*.json'], {
      cwd: REPO_ROOT,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    })
      .split('\n')
      .map((entry) => entry.trim())
      .filter(Boolean);
  } catch {
    return [];
  }

  const trackedPaths = trackedEntries
    .map((entry) => path.resolve(REPO_ROOT, entry))
    .filter((filePath) => {
      if (!filePath.startsWith(path.resolve(resultsDir))) {
        return false;
      }
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

  return trackedPaths;
}

function resolveLatestArtifacts({
  resultsDir = RESULTS_DIR,
  candidatePath,
  kind,
  profile,
  baselinePaths = undefined,
} = {}) {
  const candidates = listArtifacts({ resultsDir, kind, profile });
  if (candidates.length === 0) {
    throw new Error(`No benchmark result artifacts found in ${resultsDir}`);
  }

  const resolvedCandidate = candidatePath
    ? path.resolve(candidatePath)
    : candidates[candidates.length - 1];
  const comparisonSource = baselinePaths
    ? baselinePaths.map((filePath) => path.resolve(filePath))
    : candidates;
  const comparisonPool = comparisonSource.filter(
    (filePath) => path.resolve(filePath) !== resolvedCandidate,
  );
  if (comparisonPool.length === 0) {
    throw new Error('Need at least two comparable benchmark artifacts to diff results');
  }

  return {
    candidatePath: resolvedCandidate,
    baselinePath: comparisonPool[comparisonPool.length - 1],
  };
}

function formatMetricValue(kind, value) {
  if (!Number.isFinite(value)) {
    return kind === 'ratio' ? 'infx' : 'infms';
  }
  return kind === 'ratio' ? `${value.toFixed(2)}x` : `${value.toFixed(2)}ms`;
}

function formatPercent(value) {
  if (!Number.isFinite(value)) {
    return '+inf%';
  }
  const sign = value > 0 ? '+' : '';
  return `${sign}${value.toFixed(1)}%`;
}

function printComparisonLine(entry) {
  return `${entry.path}: median ${formatMetricValue(entry.kind, entry.baselineMedian)} -> ${formatMetricValue(entry.kind, entry.candidateMedian)} (${formatPercent(entry.medianPct)}), p95 ${formatMetricValue(entry.kind, entry.baselineP95)} -> ${formatMetricValue(entry.kind, entry.candidateP95)} (${formatPercent(entry.p95Pct)})`;
}

module.exports = {
  RESULTS_DIR,
  artifactIdentity,
  compareArtifacts,
  flattenMetricTree,
  formatMetricValue,
  formatPercent,
  inferIdentityFromFilename,
  isMetricLeaf,
  listArtifacts,
  loadArtifact,
  printComparisonLine,
  resolveLatestArtifacts,
  listTrackedArtifacts,
};
