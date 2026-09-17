'use strict';

const { assert, Mustard, Progress, runtime, test } = require('./support/helpers.js');
const { assertDifferential } = require('./runtime-oracle.js');

test('loose equality rejects at validation instead of silently using strict equality', async () => {
  for (const source of ['undefined == null;', '"1" == 1;', '1 != "1";', 'if (false) { 0 == 0; }']) {
    for (const lenientMode of [false, true]) {
      const check = error => error.kind === 'Validation' && /loose equality/.test(error.message);
      assert.throws(() => runtime(source, { lenientMode }), check);
      assert.throws(() => Mustard.validateProgram(source, { lenientMode }), check);
    }
  }
  await assertDifferential('[undefined === null, "1" === 1, Number("1") === 1, undefined === null || undefined === undefined];');
});
