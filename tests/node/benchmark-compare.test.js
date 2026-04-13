'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const {
  compareArtifacts,
  flattenMetricTree,
  listArtifacts,
  resolveLatestArtifacts,
} = require('../../benchmarks/compare.ts');

test('flattenMetricTree collects nested median/p95 metrics and skips derived sections', () => {
  const metrics = flattenMetricTree({
    machine: { benchmarkKind: 'workloads', buildProfile: 'release' },
    addon: {
      latency: {
        warm_run_small: { medianMs: 10, p95Ms: 12 },
      },
      phases: {
        execution_only_small: { medianMs: 1, p95Ms: 2 },
      },
      boundary: {
        startInputs: {
          medium: { medianMs: 0.3, p95Ms: 0.4 },
        },
      },
      suspendState: {
        suspend_resume_20: {
          serializedProgramBytes: 512,
          snapshotBytes: 128,
          retainedLiveHeapBytes: 64,
        },
      },
    },
    ratios: {
      latency: {
        sidecarVsAddon: {
          warm_run_small: { medianRatio: 1.1, p95Ratio: 1.2 },
        },
      },
    },
  });

  assert.deepEqual(metrics, {
    'addon.latency.warm_run_small': { medianMs: 10, p95Ms: 12 },
    'addon.phases.execution_only_small': { medianMs: 1, p95Ms: 2 },
    'addon.boundary.startInputs.medium': { medianMs: 0.3, p95Ms: 0.4 },
  });
});

test('compareArtifacts reports median and p95 percent changes', () => {
  const comparisons = compareArtifacts(
    {
      addon: {
        latency: {
          warm_run_small: { medianMs: 10, p95Ms: 20 },
        },
      },
    },
    {
      addon: {
        latency: {
          warm_run_small: { medianMs: 12, p95Ms: 18 },
        },
      },
    },
  );

  assert.equal(comparisons.length, 1);
  assert.equal(comparisons[0].path, 'addon.latency.warm_run_small');
  assert.equal(comparisons[0].medianPct, 20);
  assert.equal(comparisons[0].p95Pct, -10);
});

test('resolveLatestArtifacts selects the previous matching artifact as the baseline', () => {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'mustard-bench-compare-'));

  try {
    const writeArtifact = (name, buildProfile) => {
      fs.writeFileSync(
        path.join(tempRoot, name),
        JSON.stringify({
          machine: {
            benchmarkKind: 'workloads',
            buildProfile,
          },
          addon: {
            latency: {
              warm_run_small: { medianMs: 1, p95Ms: 1 },
            },
          },
        }),
      );
    };

    writeArtifact('2026-04-10T00-00-00-000Z-workloads.json', 'release');
    writeArtifact('2026-04-11T00-00-00-000Z-workloads.json', 'release');
    writeArtifact('2026-04-11T00-00-00-000Z-workloads-dev.json', 'dev');

    const artifacts = listArtifacts({
      resultsDir: tempRoot,
      kind: 'workloads',
      profile: 'release',
    });
    assert.deepEqual(
      artifacts.map((entry) => path.basename(entry)),
      [
        '2026-04-10T00-00-00-000Z-workloads.json',
        '2026-04-11T00-00-00-000Z-workloads.json',
      ],
    );

    const resolved = resolveLatestArtifacts({
      resultsDir: tempRoot,
      kind: 'workloads',
      profile: 'release',
    });
    assert.equal(
      path.basename(resolved.candidatePath),
      '2026-04-11T00-00-00-000Z-workloads.json',
    );
    assert.equal(
      path.basename(resolved.baselinePath),
      '2026-04-10T00-00-00-000Z-workloads.json',
    );
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
});
