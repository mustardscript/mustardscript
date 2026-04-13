'use strict';

const path = require('node:path');

const {
  compareArtifacts,
  formatPercent,
  listTrackedArtifacts,
  loadArtifact,
  resolveLatestArtifacts,
} = require('../benchmarks/compare.ts');

function parseArgs(argv) {
  const options = {
    candidatePath: null,
    baselinePath: null,
    resultsDir: null,
    kind: null,
    profile: null,
    maxRegressionPct: null,
    includePrefixes: [],
    trackedBaseline: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    const next = argv[index + 1];
    if (value === '--candidate') {
      options.candidatePath = next;
      index += 1;
      continue;
    }
    if (value === '--baseline') {
      options.baselinePath = next;
      index += 1;
      continue;
    }
    if (value === '--results-dir') {
      options.resultsDir = next;
      index += 1;
      continue;
    }
    if (value === '--kind') {
      options.kind = next;
      index += 1;
      continue;
    }
    if (value === '--profile') {
      options.profile = next;
      index += 1;
      continue;
    }
    if (value === '--max-regression-pct') {
      options.maxRegressionPct = Number.parseFloat(next);
      index += 1;
      continue;
    }
    if (value === '--include-prefix') {
      options.includePrefixes.push(next);
      index += 1;
      continue;
    }
    if (value === '--tracked-baseline') {
      options.trackedBaseline = true;
      continue;
    }
    throw new Error(`Unknown benchmark compare argument: ${value}`);
  }

  if (options.kind && options.kind !== 'smoke' && options.kind !== 'workloads') {
    throw new Error(`Unsupported benchmark kind: ${options.kind}`);
  }
  if (options.profile && options.profile !== 'dev' && options.profile !== 'release') {
    throw new Error(`Unsupported benchmark profile: ${options.profile}`);
  }
  if (options.maxRegressionPct !== null && !Number.isFinite(options.maxRegressionPct)) {
    throw new Error('--max-regression-pct must be a finite number');
  }

  return options;
}

function printComparison(baselinePath, candidatePath, comparisons) {
  console.log(`Baseline: ${baselinePath}`);
  console.log(`Candidate: ${candidatePath}`);
  console.log('');
  for (const entry of comparisons) {
    console.log(
      `${entry.path}: median ${entry.baselineMedianMs.toFixed(2)}ms -> ${entry.candidateMedianMs.toFixed(2)}ms (${formatPercent(entry.medianPct)}), p95 ${entry.baselineP95Ms.toFixed(2)}ms -> ${entry.candidateP95Ms.toFixed(2)}ms (${formatPercent(entry.p95Pct)})`,
    );
  }
}

function hasRegression(comparisons, maxRegressionPct) {
  return comparisons.some(
    (entry) => entry.medianPct > maxRegressionPct || entry.p95Pct > maxRegressionPct,
  );
}

function filterComparisons(comparisons, includePrefixes) {
  if (!includePrefixes || includePrefixes.length === 0) {
    return comparisons;
  }
  return comparisons.filter((entry) =>
    includePrefixes.some((prefix) => entry.path.startsWith(prefix)),
  );
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const resultsDir = options.resultsDir ? path.resolve(options.resultsDir) : undefined;
  let baselinePath = options.baselinePath ? path.resolve(options.baselinePath) : null;
  let candidatePath = options.candidatePath ? path.resolve(options.candidatePath) : null;

  if (!baselinePath || !candidatePath) {
    const baselinePaths =
      options.trackedBaseline && !baselinePath
        ? listTrackedArtifacts({
            resultsDir,
            kind: options.kind,
            profile: options.profile,
          })
        : undefined;
    const resolved = resolveLatestArtifacts({
      resultsDir,
      candidatePath,
      kind: options.kind,
      profile: options.profile,
      baselinePaths,
    });
    candidatePath ??= resolved.candidatePath;
    baselinePath ??= resolved.baselinePath;
  }

  const comparisons = filterComparisons(compareArtifacts(
    loadArtifact(baselinePath),
    loadArtifact(candidatePath),
  ), options.includePrefixes);
  if (comparisons.length === 0) {
    throw new Error('No comparable median/p95 metrics found between the selected artifacts');
  }

  printComparison(baselinePath, candidatePath, comparisons);

  if (
    options.maxRegressionPct !== null &&
    hasRegression(comparisons, options.maxRegressionPct)
  ) {
    process.exitCode = 1;
  }
}

main();
