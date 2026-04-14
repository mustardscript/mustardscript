'use strict';

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { performance } = require('node:perf_hooks');

const REPO_ROOT = path.join(__dirname, '..');
const RESULTS_DIR = path.join(REPO_ROOT, 'benchmarks', 'results');
const DEFAULT_MEASURE_OPTIONS = Object.freeze({ warmup: 1, iterations: 3, batch: 1 });

function percentile(sortedValues, ratio) {
  if (sortedValues.length === 0) {
    return 0;
  }
  const index = Math.min(
    sortedValues.length - 1,
    Math.max(0, Math.ceil(sortedValues.length * ratio) - 1),
  );
  return sortedValues[index];
}

function summarize(samples) {
  if (!Array.isArray(samples) || samples.length === 0) {
    throw new TypeError('summarize() requires at least one sample');
  }
  const sorted = [...samples].sort((left, right) => left - right);
  const total = sorted.reduce((sum, value) => sum + value, 0);
  return {
    iterations: sorted.length,
    minMs: sorted[0],
    medianMs: percentile(sorted, 0.5),
    p95Ms: percentile(sorted, 0.95),
    maxMs: sorted[sorted.length - 1],
    meanMs: total / sorted.length,
  };
}

async function measure(name, fn, options = DEFAULT_MEASURE_OPTIONS) {
  const batch = Math.max(1, Math.trunc(options.batch ?? 1));
  for (let i = 0; i < options.warmup; i += 1) {
    for (let repeat = 0; repeat < batch; repeat += 1) {
      await fn();
    }
  }
  const samples = [];
  for (let i = 0; i < options.iterations; i += 1) {
    const start = performance.now();
    for (let repeat = 0; repeat < batch; repeat += 1) {
      await fn();
    }
    samples.push((performance.now() - start) / batch);
  }
  return [name, summarize(samples)];
}

async function measureSamples(name, sampleFn, options = DEFAULT_MEASURE_OPTIONS) {
  const batch = Math.max(1, Math.trunc(options.batch ?? 1));
  for (let i = 0; i < options.warmup; i += 1) {
    for (let repeat = 0; repeat < batch; repeat += 1) {
      await sampleFn();
    }
  }
  const samples = [];
  for (let i = 0; i < options.iterations; i += 1) {
    let totalDurationMs = 0;
    for (let repeat = 0; repeat < batch; repeat += 1) {
      const durationMs = await sampleFn();
      if (!Number.isFinite(durationMs) || durationMs < 0) {
        throw new TypeError(`${name} sampleFn must return a finite non-negative duration`);
      }
      totalDurationMs += durationMs;
    }
    samples.push(totalDurationMs / batch);
  }
  return [name, summarize(samples)];
}

function gitSha() {
  try {
    return execFileSync('git', ['rev-parse', '--short', 'HEAD'], {
      cwd: REPO_ROOT,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
  } catch {
    return null;
  }
}

function machineMetadata({ fixtureVersion, benchmarkKind, buildProfile }) {
  return {
    timestamp: new Date().toISOString(),
    benchmarkKind,
    buildProfile,
    gitSha: gitSha(),
    fixtureVersion,
    hostname: os.hostname(),
    platform: `${process.platform} ${os.release()}`,
    arch: process.arch,
    nodeVersion: process.version,
    cpuModel: os.cpus()[0]?.model ?? 'unknown',
    cpuCount: os.cpus().length,
    totalMemoryBytes: os.totalmem(),
  };
}

function artifactFilename({ timestamp, benchmarkKind, buildProfile }) {
  const normalizedTimestamp = timestamp.replace(/[:.]/g, '-');
  if (benchmarkKind === 'workloads' && buildProfile === 'release') {
    return `${normalizedTimestamp}-workloads.json`;
  }
  return `${normalizedTimestamp}-${benchmarkKind}-${buildProfile}.json`;
}

function writeBenchmarkArtifact(result) {
  fs.mkdirSync(RESULTS_DIR, { recursive: true });
  const reportPath = path.join(RESULTS_DIR, artifactFilename(result.machine));
  fs.writeFileSync(reportPath, `${JSON.stringify(result, null, 2)}\n`);
  return reportPath;
}

module.exports = {
  DEFAULT_MEASURE_OPTIONS,
  RESULTS_DIR,
  machineMetadata,
  measure,
  measureSamples,
  summarize,
  writeBenchmarkArtifact,
};
