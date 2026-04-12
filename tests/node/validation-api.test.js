'use strict';

const { assert, isJsliteError, Jslite, test } = require('./support/helpers.js');

test('Jslite.validateProgram accepts supported programs and rejects invalid ones with typed errors', () => {
  assert.doesNotThrow(() => {
    Jslite.validateProgram('const value = 1; value + 1;');
  });

  assert.throws(
    () => {
      Jslite.validateProgram("eval('1 + 1');");
    },
    isJsliteError({
      kind: 'Validation',
      message: 'eval',
      guestSafe: true,
    }),
  );
});
