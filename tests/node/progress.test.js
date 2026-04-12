'use strict';

const { assert, isJsliteError, Progress, runtime, test } = require('./support/helpers.js');

const SNAPSHOT_KEY = Buffer.from('progress-test-snapshot-key');
const PROGRESS_LOAD_OPTIONS = Object.freeze({
  snapshotKey: SNAPSHOT_KEY,
  capabilities: {
    fetch_data() {},
  },
  limits: {},
});

test('start returns resumable progress objects', () => {
  const progress = runtime(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  assert.equal(progress.capability, 'fetch_data');
  assert.deepEqual(progress.args, [4]);

  const finalValue = progress.resume(4);
  assert.equal(finalValue, 8);
});

test('dump only exists on suspended progress objects', () => {
  const completed = runtime('4 + 4;').start();
  assert.equal(completed, 8);
  assert.ok(!(completed instanceof Progress));
  assert.equal(typeof completed.dump, 'undefined');

  const progress = runtime('fetch_data(4);').start({
    capabilities: {
      fetch_data() {},
    },
  });
  assert.ok(progress instanceof Progress);
  assert.equal(typeof progress.dump, 'function');

  const finished = progress.resume(4);
  assert.equal(finished, 4);
  assert.equal(typeof finished.dump, 'undefined');
});

test('progress objects are single-use', () => {
  const progress = runtime(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.equal(progress.resume(4), 8);
  assert.throws(
    () => progress.resume(4),
    isJsliteError({
      kind: 'Runtime',
      message: /single-use/,
    }),
  );
});

test('progress dump and load preserve suspended execution state', () => {
  const progress = runtime(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  const restored = Progress.load(progress.dump(), PROGRESS_LOAD_OPTIONS);
  assert.ok(restored instanceof Progress);
  assert.equal(restored.capability, 'fetch_data');
  assert.deepEqual(restored.args, [4]);
  assert.equal(restored.resume(4), 8);
});

test('start snapshots guest state before async host futures exist', () => {
  let calls = 0;
  const progress = runtime(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      async fetch_data() {
        calls += 1;
        return 4;
      },
    },
  });

  assert.equal(calls, 0);
  const dumped = progress.dump();
  const restored = Progress.load(dumped, PROGRESS_LOAD_OPTIONS);
  assert.equal(restored.resume(4), 8);
});

test('progress.load rejects reused snapshots in the same process', () => {
  const progress = runtime(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    capabilities: {
      fetch_data() {},
    },
  });
  const dumped = progress.dump();
  assert.equal(progress.resume(4), 8);

  assert.throws(
    () => Progress.load(dumped),
    isJsliteError({
      kind: 'Runtime',
      message: /single-use/,
    }),
  );
});
