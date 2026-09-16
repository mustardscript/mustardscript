'use strict';

const { assert, runtime, test } = require('./support/helpers.js');

test('typeof unresolvable identifiers preserves TDZ, property errors, and ambient restrictions', async () => {
  assert.equal(await runtime('typeof missingName;').run(), 'undefined');
  assert.equal(await runtime('const x = 1; (() => typeof x)();').run(), 'number');
  assert.equal(await runtime('globalThis.extra = "x"; typeof extra;').run(), 'string');
  await assert.rejects(runtime('{ typeof value; let value = 1; }').run(), /before initialization/);
  await assert.rejects(runtime('typeof missingName.value;').run(), /not defined/);
  assert.throws(() => runtime('typeof process;'), /forbidden ambient global/);
});
