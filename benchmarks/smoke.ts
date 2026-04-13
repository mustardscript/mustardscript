'use strict';

const assert = require('node:assert/strict');

const budgets = require('./budgets.json');
const { Mustard, Progress } = require('../index.ts');
const {
  machineMetadata,
  measure,
  writeBenchmarkArtifact,
} = require('./support.ts');

const SNAPSHOT_KEY = Buffer.from('benchmark-snapshot-key');
const FIXTURE_VERSION = 2;

function parseArgs(argv) {
  let profile = 'dev';
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === '--profile') {
      profile = argv[index + 1];
      index += 1;
      continue;
    }
    throw new Error(`Unknown benchmark argument: ${value}`);
  }
  if (profile !== 'dev' && profile !== 'release') {
    throw new Error(`Unsupported smoke profile: ${profile}`);
  }
  return { profile };
}

async function benchmarkStartup(profileBudgets) {
  const [, summary] = await measure('startup', async () => {
    const runtime = new Mustard(`
      const values = [1, 2, 3, 4];
      let total = 0;
      for (let i = 0; i < values.length; i += 1) {
        total += values[i];
      }
      total;
    `);
    const result = await runtime.run();
    assert.equal(result, 10);
  }, profileBudgets.startup);
  return summary;
}

async function benchmarkCompute(profileBudgets) {
  const runtime = new Mustard(`
    function double(value) {
      return value * 2;
    }
    let total = 0;
    for (let i = 0; i < 200; i += 1) {
      total += double(i) + 1;
    }
    total;
  `);
  const [, summary] = await measure('compute', async () => {
    const result = await runtime.run();
    assert.equal(result, 40000);
  }, profileBudgets.compute);
  return summary;
}

async function benchmarkHostCallOverhead(profileBudgets) {
  const guestRuntime = new Mustard(`
    function echo(value) {
      return value;
    }
    let total = 0;
    for (let i = 0; i < 24; i += 1) {
      total += echo(i);
    }
    total;
  `);
  const hostRuntime = new Mustard(`
    let total = 0;
    for (let i = 0; i < 24; i += 1) {
      total += fetch_value(i);
    }
    total;
  `);

  const [, guestBaseline] = await measure('host_baseline_guest', async () => {
    const result = await guestRuntime.run();
    assert.equal(result, 276);
  }, profileBudgets.hostCall);
  const [, hostCalls] = await measure('host_calls', async () => {
    const result = await hostRuntime.run({
      capabilities: {
        fetch_value(value) {
          return value;
        },
      },
    });
    assert.equal(result, 276);
  }, profileBudgets.hostCall);

  return {
    guestBaseline,
    hostCalls,
    medianRatio: hostCalls.medianMs / guestBaseline.medianMs,
    p95Ratio: hostCalls.p95Ms / guestBaseline.p95Ms,
  };
}

function driveSuspension({ reloadSnapshots }) {
  const runtime = new Mustard(`
    const first = fetch_value(4);
    const second = fetch_value(first + 1);
    second * 2;
  `);
  const capabilities = {
    fetch_value() {},
  };
  let step = runtime.start({
    capabilities,
    snapshotKey: SNAPSHOT_KEY,
  });
  while (step instanceof Progress) {
    if (reloadSnapshots) {
      step = Progress.load(step.dump(), {
        capabilities,
        limits: {},
        snapshotKey: SNAPSHOT_KEY,
      });
    }
    step = step.resume(step.args[0]);
  }
  assert.equal(step, 10);
}

async function benchmarkSnapshotRoundTrip(profileBudgets) {
  const [, direct] = await measure('snapshot_direct', async () => {
    driveSuspension({ reloadSnapshots: false });
  }, profileBudgets.snapshot);
  const [, snapshotRoundTrip] = await measure('snapshot_round_trip', async () => {
    driveSuspension({ reloadSnapshots: true });
  }, profileBudgets.snapshot);

  return {
    direct,
    snapshotRoundTrip,
    medianRatio: snapshotRoundTrip.medianMs / direct.medianMs,
    p95Ratio: snapshotRoundTrip.p95Ms / direct.p95Ms,
  };
}

async function benchmarkMemory(profileBudgets) {
  if (typeof global.gc !== 'function') {
    throw new Error('benchmarks/smoke.ts requires node --expose-gc');
  }
  global.gc();
  const before = process.memoryUsage().heapUsed;
  for (let i = 0; i < profileBudgets.memory.runs; i += 1) {
    driveSuspension({ reloadSnapshots: true });
  }
  global.gc();
  const after = process.memoryUsage().heapUsed;
  return {
    heapDeltaBytes: after - before,
  };
}

async function main() {
  const { profile } = parseArgs(process.argv.slice(2));
  const profileBudgets = budgets[profile];
  if (!profileBudgets) {
    throw new Error(`Missing smoke budgets for profile ${profile}`);
  }

  const startup = await benchmarkStartup(profileBudgets);
  const compute = await benchmarkCompute(profileBudgets);
  const hostCall = await benchmarkHostCallOverhead(profileBudgets);
  const snapshot = await benchmarkSnapshotRoundTrip(profileBudgets);
  const memory = await benchmarkMemory(profileBudgets);

  assert.ok(startup.medianMs <= profileBudgets.startup.medianMsMax, `startup median ${startup.medianMs.toFixed(2)}ms exceeded ${profileBudgets.startup.medianMsMax}ms`);
  assert.ok(startup.p95Ms <= profileBudgets.startup.p95MsMax, `startup p95 ${startup.p95Ms.toFixed(2)}ms exceeded ${profileBudgets.startup.p95MsMax}ms`);
  assert.ok(compute.medianMs <= profileBudgets.compute.medianMsMax, `compute median ${compute.medianMs.toFixed(2)}ms exceeded ${profileBudgets.compute.medianMsMax}ms`);
  assert.ok(compute.p95Ms <= profileBudgets.compute.p95MsMax, `compute p95 ${compute.p95Ms.toFixed(2)}ms exceeded ${profileBudgets.compute.p95MsMax}ms`);
  assert.ok(hostCall.medianRatio <= profileBudgets.hostCall.medianRatioMax, `host-call median ratio ${hostCall.medianRatio.toFixed(2)} exceeded ${profileBudgets.hostCall.medianRatioMax}`);
  assert.ok(hostCall.p95Ratio <= profileBudgets.hostCall.p95RatioMax, `host-call p95 ratio ${hostCall.p95Ratio.toFixed(2)} exceeded ${profileBudgets.hostCall.p95RatioMax}`);
  assert.ok(snapshot.medianRatio <= profileBudgets.snapshot.medianRatioMax, `snapshot median ratio ${snapshot.medianRatio.toFixed(2)} exceeded ${profileBudgets.snapshot.medianRatioMax}`);
  assert.ok(snapshot.p95Ratio <= profileBudgets.snapshot.p95RatioMax, `snapshot p95 ratio ${snapshot.p95Ratio.toFixed(2)} exceeded ${profileBudgets.snapshot.p95RatioMax}`);
  assert.ok(memory.heapDeltaBytes <= profileBudgets.memory.heapDeltaBytesMax, `heap delta ${memory.heapDeltaBytes} exceeded ${profileBudgets.memory.heapDeltaBytesMax}`);

  const result = {
    machine: machineMetadata({
      fixtureVersion: FIXTURE_VERSION,
      benchmarkKind: 'smoke',
      buildProfile: profile,
    }),
    metrics: {
      startup,
      compute,
      hostCall,
      snapshot,
      memory,
    },
    budgets: profileBudgets,
  };
  const reportPath = writeBenchmarkArtifact(result);

  console.log(JSON.stringify({
    ...result,
    reportPath,
  }, null, 2));
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
