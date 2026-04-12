'use strict';

const { assert, isJsliteError, Jslite, Progress, runtime, test } = require('./support/helpers.js');
const { snapshotToken } = require('../../lib/policy.js');

const SNAPSHOT_KEY = Buffer.from('serialization-test-key');
const INVALID_SNAPSHOT = Buffer.from('not-a-valid-snapshot');
const INVALID_SNAPSHOT_TOKEN = snapshotToken(INVALID_SNAPSHOT, SNAPSHOT_KEY);

test('progress load surfaces snapshot failures as typed errors', () => {
  assert.throws(
    () =>
      Progress.load(
        {
          snapshot: INVALID_SNAPSHOT,
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
    isJsliteError({
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
    isJsliteError({
      kind: 'Serialization',
      guestSafe: true,
    }),
  );
});

test('dump and load preserve compiled programs', async () => {
  const copy = Jslite.load(runtime('Math.max(1, 8, 2);').dump());
  const result = await copy.run();
  assert.equal(result, 8);
});
