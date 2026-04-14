'use strict';

const assert = require('node:assert/strict');
const { performance } = require('node:perf_hooks');
const test = require('node:test');

const {
  measure,
  measureSamples,
} = require('../../benchmarks/support.ts');

test('measure batches inner runs and reports per-run sample durations', async () => {
  const originalNow = performance.now;
  let tick = 0;
  let calls = 0;
  Object.defineProperty(performance, 'now', {
    configurable: true,
    value: () => tick,
  });

  try {
    const [, summary] = await measure('batched', async () => {
      calls += 1;
      tick += 4;
    }, { warmup: 1, iterations: 2, batch: 3 });

    assert.equal(calls, 9);
    assert.equal(summary.iterations, 2);
    assert.equal(summary.medianMs, 4);
    assert.equal(summary.p95Ms, 4);
  } finally {
    Object.defineProperty(performance, 'now', {
      configurable: true,
      value: originalNow,
    });
  }
});

test('measureSamples averages batched sample durations', async () => {
  let calls = 0;
  const [, summary] = await measureSamples('batched_samples', async () => {
    calls += 1;
    return 7;
  }, { warmup: 1, iterations: 3, batch: 4 });

  assert.equal(calls, 16);
  assert.equal(summary.iterations, 3);
  assert.equal(summary.medianMs, 7);
  assert.equal(summary.p95Ms, 7);
});
