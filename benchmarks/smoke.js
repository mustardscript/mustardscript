'use strict';

const assert = require('node:assert/strict');
const { performance } = require('node:perf_hooks');

const budgets = require('./budgets.json');
const { Jslite } = require('../index.js');

function average(values) {
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

async function measure({ iterations, warmup }, fn) {
  for (let i = 0; i < warmup; i += 1) {
    await fn();
  }
  const samples = [];
  for (let i = 0; i < iterations; i += 1) {
    const start = performance.now();
    await fn();
    samples.push(performance.now() - start);
  }
  return {
    averageMs: average(samples),
    maxMs: Math.max(...samples),
  };
}

async function benchmarkStartup() {
  return measure(budgets.startup, async () => {
    const runtime = new Jslite(`
      const values = [1, 2, 3, 4];
      let total = 0;
      for (let i = 0; i < values.length; i += 1) {
        total += values[i];
      }
      total;
    `);
    const result = await runtime.run();
    assert.equal(result, 10);
  });
}

async function benchmarkCompute() {
  const runtime = new Jslite(`
    function double(value) {
      return value * 2;
    }
    let total = 0;
    for (let i = 0; i < 200; i += 1) {
      total += double(i) + 1;
    }
    total;
  `);
  return measure(budgets.compute, async () => {
    const result = await runtime.run();
    assert.equal(result, 40000);
  });
}

async function benchmarkSuspension() {
  return measure(budgets.suspension, async () => {
    const runtime = new Jslite(`
      const first = fetch_value(4);
      const second = fetch_value(first + 1);
      second * 2;
    `);
    let step = runtime.start({
      capabilities: {
        fetch_value(value) {
          return value;
        },
      },
    });
    while (step && typeof step.resume === 'function') {
      step = step.resume(step.args[0]);
    }
    assert.equal(step, 10);
  });
}

async function benchmarkMemory() {
  if (typeof global.gc !== 'function') {
    throw new Error('benchmarks/smoke.js requires node --expose-gc');
  }
  global.gc();
  const before = process.memoryUsage().heapUsed;
  for (let i = 0; i < budgets.memory.runs; i += 1) {
    const runtime = new Jslite(`
      const values = [1, 2, 3];
      values[0] + values[2];
    `);
    const result = await runtime.run();
    assert.equal(result, 4);
  }
  global.gc();
  const after = process.memoryUsage().heapUsed;
  return {
    heapDeltaBytes: after - before,
  };
}

async function main() {
  const startup = await benchmarkStartup();
  const compute = await benchmarkCompute();
  const suspension = await benchmarkSuspension();
  const memory = await benchmarkMemory();

  assert.ok(startup.averageMs <= budgets.startup.averageMsMax, `startup average ${startup.averageMs.toFixed(2)}ms exceeded ${budgets.startup.averageMsMax}ms`);
  assert.ok(compute.averageMs <= budgets.compute.averageMsMax, `compute average ${compute.averageMs.toFixed(2)}ms exceeded ${budgets.compute.averageMsMax}ms`);
  assert.ok(suspension.averageMs <= budgets.suspension.averageMsMax, `suspension average ${suspension.averageMs.toFixed(2)}ms exceeded ${budgets.suspension.averageMsMax}ms`);
  assert.ok(memory.heapDeltaBytes <= budgets.memory.heapDeltaBytesMax, `heap delta ${memory.heapDeltaBytes} exceeded ${budgets.memory.heapDeltaBytesMax}`);

  console.log(JSON.stringify({ startup, compute, suspension, memory }, null, 2));
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
