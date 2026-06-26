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

test('lenientMode accepts only final top-level return snippets', async () => {
  assert.doesNotThrow(() => {
    Mustard.validateProgram('const value = 41; return value + 1;', { lenientMode: true });
  });

  assert.equal(
    await new Mustard('const value = 41; return value + 1;', { lenientMode: true }).run(),
    42,
  );

  assert.equal(await new Mustard('return;', { lenientMode: true }).run(), undefined);

  assert.throws(
    () => {
      new Mustard('return 1;');
    },
    isMustardError({
      kind: 'Parse',
      message: "A 'return' statement can only be used within a function body.",
      guestSafe: true,
    }),
  );

  assert.throws(
    () => {
      new Mustard('return 1; 2;', { lenientMode: true });
    },
    isMustardError({
      kind: 'Validation',
      message: 'final top-level statement when lenientMode is enabled',
      guestSafe: true,
    }),
  );

  assert.throws(
    () => {
      new Mustard('if (true) { return 1; } return 2;', { lenientMode: true });
    },
    isMustardError({
      kind: 'Validation',
      message: 'final top-level statement when lenientMode is enabled',
      guestSafe: true,
    }),
  );
});

test('lenientMode compile option must be boolean when provided', () => {
  assert.throws(
    () => {
      new Mustard('1;', { lenientMode: 'yes' });
    },
    {
      name: 'TypeError',
      message: 'options.lenientMode must be a boolean',
    },
  );
});
