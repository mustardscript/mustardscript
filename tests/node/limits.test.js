'use strict';

const { assert, isJsliteError, runtime, test } = require('./support/helpers.js');

test('run surfaces limit failures as typed errors', async () => {
  await assert.rejects(
    runtime('while (true) {}').run({
      limits: {
        instructionBudget: 100,
      },
    }),
    isJsliteError({
      kind: 'Limit',
      message: /instruction budget exhausted/,
    }),
  );
});

test('run surfaces heap and allocation limit failures as typed errors', async () => {
  const program = runtime('1;');

  await assert.rejects(
    program.run({
      limits: {
        heapLimitBytes: 1,
      },
    }),
    isJsliteError({
      kind: 'Limit',
      message: /heap limit exceeded/,
    }),
  );

  await assert.rejects(
    program.run({
      limits: {
        allocationBudget: 1,
      },
    }),
    isJsliteError({
      kind: 'Limit',
      message: /allocation budget exhausted/,
    }),
  );
});

test('run surfaces call-depth limit failures as typed errors', async () => {
  await assert.rejects(
    runtime(`
      function recurse(value) {
        if (value === 0) {
          return 0;
        }
        return recurse(value - 1);
      }
      recurse(3);
    `).run({
      limits: {
        callDepthLimit: 3,
      },
    }),
    isJsliteError({
      kind: 'Limit',
      message: /call depth limit exceeded/,
    }),
  );
});

test('limit errors do not leak host internals', async () => {
  await assert.rejects(
    runtime('while (true) {}').run({
      limits: {
        instructionBudget: 100,
      },
    }),
    isJsliteError({
      kind: 'Limit',
      guestSafe: true,
    }),
  );

  await assert.rejects(
    runtime('1;').run({
      limits: {
        heapLimitBytes: 1,
      },
    }),
    isJsliteError({
      kind: 'Limit',
      guestSafe: true,
    }),
  );
});
