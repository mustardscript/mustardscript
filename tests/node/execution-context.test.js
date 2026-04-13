'use strict';

const {
  assert,
  ExecutionContext,
  Progress,
  runtime,
  test,
} = require('./support/helpers.js');
const { loadNative } = require('../../native-loader.ts');

const SNAPSHOT_KEY = Buffer.from('execution-context-test-snapshot-key');

function createContext() {
  return new ExecutionContext({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data(value) {
        return value + 1;
      },
    },
    limits: {},
  });
}

test('execution contexts drive run start and Progress.load with stable policy state', async () => {
  const context = createContext();
  const program = runtime(`
    const first = fetch_data(seed);
    const second = fetch_data(first);
    second;
  `);

  assert.equal(await program.run({ context, inputs: { seed: 1 } }), 3);

  const firstStep = program.start({ context, inputs: { seed: 1 } });
  assert.ok(firstStep instanceof Progress);
  assert.equal(firstStep.capability, 'fetch_data');
  assert.deepEqual(firstStep.args, [1]);

  const restored = Progress.load(firstStep.dump(), { context });
  assert.ok(restored instanceof Progress);
  assert.equal(restored.capability, 'fetch_data');
  assert.deepEqual(restored.args, [1]);

  const secondStep = restored.resume(2);
  assert.ok(secondStep instanceof Progress);
  assert.equal(secondStep.capability, 'fetch_data');
  assert.deepEqual(secondStep.args, [2]);
  assert.equal(secondStep.resume(3), 3);
});

test('run and start reject mixing an execution context with raw policy fields', async () => {
  const context = createContext();

  await assert.rejects(
    runtime('fetch_data(1);').run({
      context,
      capabilities: {
        fetch_data(value) {
          return value;
        },
      },
    }),
    /run options\.context cannot be combined with capabilities, console, limits, or snapshotKey/,
  );

  assert.throws(
    () =>
      runtime('fetch_data(1);').start({
        context,
        snapshotKey: SNAPSHOT_KEY,
      }),
    /start options\.context cannot be combined with capabilities, console, limits, or snapshotKey/,
  );
});

test('Progress.load rejects mixing an execution context with raw restore policy fields', () => {
  const context = createContext();
  const progress = runtime('fetch_data(1);').start({ context });

  assert.ok(progress instanceof Progress);
  assert.throws(
    () =>
      Progress.load(progress.dump(), {
        context,
        limits: {},
      }),
    /Progress\.load\(\) options\.context cannot be combined with capabilities, console, limits, or snapshotKey/,
  );
});

test('execution contexts validate handler containers before reuse', () => {
  const capabilities = {};
  Object.defineProperty(capabilities, 'fetch_data', {
    enumerable: true,
    get() {
      return () => 1;
    },
  });

  assert.throws(
    () =>
      new ExecutionContext({
        capabilities,
      }),
    /options\.capabilities cannot define accessor properties/,
  );
});

test('execution contexts do not reuse stale encoded inputs across runs and starts', async () => {
  const context = createContext();
  const program = runtime('fetch_data(seed);');

  assert.equal(await program.run({ context, inputs: { seed: 1 } }), 2);
  assert.equal(await program.run({ context, inputs: { seed: 7 } }), 8);

  const firstStep = program.start({ context, inputs: { seed: 2 } });
  assert.ok(firstStep instanceof Progress);
  assert.deepEqual(firstStep.args, [2]);
  assert.equal(firstStep.resume(22), 22);

  const secondStep = program.start({ context, inputs: { seed: 9 } });
  assert.ok(secondStep instanceof Progress);
  assert.deepEqual(secondStep.args, [9]);
  assert.equal(secondStep.resume(29), 29);
});

test('execution contexts do not reuse stale snapshot auth across Progress.load calls', () => {
  const context = createContext();
  const program = runtime(`
    const first = fetch_data(seed);
    const second = fetch_data(first);
    second;
  `);

  const firstDump = program.start({ context, inputs: { seed: 1 } }).dump();
  const secondDump = program.start({ context, inputs: { seed: 10 } }).dump();

  const firstRestored = Progress.load(firstDump, { context });
  assert.ok(firstRestored instanceof Progress);
  assert.deepEqual(firstRestored.args, [1]);
  const firstNext = firstRestored.resume(2);
  assert.ok(firstNext instanceof Progress);
  assert.deepEqual(firstNext.args, [2]);
  assert.equal(firstNext.resume(3), 3);

  const secondRestored = Progress.load(secondDump, { context });
  assert.ok(secondRestored instanceof Progress);
  assert.deepEqual(secondRestored.args, [10]);
  const secondNext = secondRestored.resume(11);
  assert.ok(secondNext instanceof Progress);
  assert.deepEqual(secondNext.args, [11]);
  assert.equal(secondNext.resume(12), 12);
});

test('execution contexts reuse one native handle across repeated start and load calls', async () => {
  const native = loadNative();
  const originalCreateExecutionContext = native.createExecutionContext;
  const originalStartProgramWithExecutionContextHandle =
    native.startProgramWithExecutionContextHandle;
  const originalLoadSnapshotHandleWithExecutionContext =
    native.loadSnapshotHandleWithExecutionContext;
  const originalLoadDetachedSnapshotHandleWithExecutionContext =
    native.loadDetachedSnapshotHandleWithExecutionContext;
  const counts = {
    createExecutionContext: 0,
    startProgramWithExecutionContextHandle: 0,
    loadSnapshotHandleWithExecutionContext: 0,
    loadDetachedSnapshotHandleWithExecutionContext: 0,
  };
  native.createExecutionContext = (...args) => {
    counts.createExecutionContext += 1;
    return originalCreateExecutionContext(...args);
  };
  native.startProgramWithExecutionContextHandle = (...args) => {
    counts.startProgramWithExecutionContextHandle += 1;
    return originalStartProgramWithExecutionContextHandle(...args);
  };
  native.loadSnapshotHandleWithExecutionContext = (...args) => {
    counts.loadSnapshotHandleWithExecutionContext += 1;
    return originalLoadSnapshotHandleWithExecutionContext(...args);
  };
  native.loadDetachedSnapshotHandleWithExecutionContext = (...args) => {
    counts.loadDetachedSnapshotHandleWithExecutionContext += 1;
    return originalLoadDetachedSnapshotHandleWithExecutionContext(...args);
  };

  try {
    const context = createContext();
    const program = runtime(`
      const first = fetch_data(seed);
      const second = fetch_data(first);
      second;
    `);

    assert.equal(await program.run({ context, inputs: { seed: 1 } }), 3);
    assert.equal(await program.run({ context, inputs: { seed: 5 } }), 7);

    const dumped = program.start({ context, inputs: { seed: 9 } }).dump();
    const restored = Progress.load(dumped, { context });
    assert.ok(restored instanceof Progress);
    assert.deepEqual(restored.args, [9]);
    assert.equal(restored.resume(10).resume(11), 11);

    assert.equal(counts.createExecutionContext, 1);
    assert.equal(counts.startProgramWithExecutionContextHandle, 3);
    assert.equal(counts.loadSnapshotHandleWithExecutionContext, 0);
    assert.equal(counts.loadDetachedSnapshotHandleWithExecutionContext, 1);
  } finally {
    native.createExecutionContext = originalCreateExecutionContext;
    native.startProgramWithExecutionContextHandle =
      originalStartProgramWithExecutionContextHandle;
    native.loadSnapshotHandleWithExecutionContext =
      originalLoadSnapshotHandleWithExecutionContext;
    native.loadDetachedSnapshotHandleWithExecutionContext =
      originalLoadDetachedSnapshotHandleWithExecutionContext;
  }
});
