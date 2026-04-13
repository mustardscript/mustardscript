'use strict';

const { assert, isMustardError, Mustard, test } = require('./support/helpers.js');

test('Mustard.validateProgram accepts supported programs and rejects invalid ones with typed errors', () => {
  assert.doesNotThrow(() => {
    Mustard.validateProgram('const value = 1; value + 1;');
  });

  assert.throws(
    () => {
      Mustard.validateProgram("eval('1 + 1');");
    },
    isMustardError({
      kind: 'Validation',
      message: 'eval',
      guestSafe: true,
    }),
  );
});
