'use strict';

const {
  assert,
  ExecutionContext,
  Progress,
  runtime,
  test,
} = require('./support/helpers.js');

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
