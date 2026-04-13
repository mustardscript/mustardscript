'use strict';

const { assert, isJsliteError, Jslite, Progress, runtime, test } = require('./support/helpers.js');
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

test('Jslite.load surfaces invalid compiled-program blobs as typed errors', async () => {
  const copy = Jslite.load(Buffer.from('not-a-valid-program'));
  await assert.rejects(
    () => copy.run(),
    isJsliteError({
      kind: 'Serialization',
      guestSafe: true,
    }),
  );
});
