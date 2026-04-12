'use strict';

const { assert, isJsliteError, Jslite, Progress, runtime, test } = require('./support/helpers.js');

test('progress load surfaces snapshot failures as typed errors', () => {
  assert.throws(
    () =>
      Progress.load(
        {
          snapshot: Buffer.from('not-a-valid-snapshot'),
        },
        {
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
          snapshot: Buffer.from('not-a-valid-snapshot'),
        },
        {
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
