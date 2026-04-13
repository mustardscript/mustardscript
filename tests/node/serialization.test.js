'use strict';

const { assert, isMustardError, Mustard, Progress, runtime, test } = require('./support/helpers.js');
const { snapshotKeyDigest } = require('../../lib/policy.ts');

const SNAPSHOT_KEY = Buffer.from('serialization-test-key');
const INVALID_SNAPSHOT = Buffer.from('not-a-valid-snapshot');
const INVALID_SNAPSHOT_TOKEN = 'invalid-snapshot-token';
const INVALID_SNAPSHOT_ID = 'invalid-snapshot-id';
const INVALID_SNAPSHOT_KEY_DIGEST = snapshotKeyDigest(SNAPSHOT_KEY);

test('progress load surfaces snapshot failures as typed errors', () => {
  assert.throws(
    () =>
      Progress.load(
        {
          snapshot: INVALID_SNAPSHOT,
          snapshot_id: INVALID_SNAPSHOT_ID,
          snapshot_key_digest: INVALID_SNAPSHOT_KEY_DIGEST,
          token: INVALID_SNAPSHOT_TOKEN,
        },
        {
          snapshotKey: SNAPSHOT_KEY,
          capabilities: {
            fetch_data() {},
          },
          limits: {},
        },
      ),
    isMustardError({
      kind: 'Serialization',
    }),
  );
});

test('serialization errors do not leak host internals', () => {
  assert.throws(
    () =>
      Progress.load(
        {
          snapshot: INVALID_SNAPSHOT,
          snapshot_id: INVALID_SNAPSHOT_ID,
          snapshot_key_digest: INVALID_SNAPSHOT_KEY_DIGEST,
          token: INVALID_SNAPSHOT_TOKEN,
        },
        {
          snapshotKey: SNAPSHOT_KEY,
          capabilities: {
            fetch_data() {},
          },
          limits: {},
        },
      ),
    isMustardError({
      kind: 'Serialization',
      guestSafe: true,
    }),
  );
});

test('progress load rejects mismatched detached program bytes', () => {
  const progress = runtime(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });
  const dumped = progress.dump();
  const forgedProgram = runtime('1 + 1;').dump();

  assert.throws(
    () =>
      Progress.load(
        {
          ...dumped,
          program: forgedProgram,
          program_id: dumped.program_id,
        },
        {
          snapshotKey: SNAPSHOT_KEY,
          capabilities: {
            fetch_data() {},
          },
          limits: {},
        },
      ),
    isMustardError({
      kind: 'Serialization',
      message: /mismatched detached program/,
    }),
  );
});

test('dump and load preserve compiled programs', async () => {
  const copy = Mustard.load(runtime('Math.max(1, 8, 2);').dump());
  const result = await copy.run();
  assert.equal(result, 8);
});

test('Mustard.load surfaces invalid compiled-program blobs as typed errors', async () => {
  const copy = Mustard.load(Buffer.from('not-a-valid-program'));
  await assert.rejects(
    () => copy.run(),
    isMustardError({
      kind: 'Serialization',
      guestSafe: true,
    }),
  );
});
