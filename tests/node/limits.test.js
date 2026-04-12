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

test('JSON.parse and JSON.stringify charge instruction budget inside native helper work', async () => {
  const largeText = `[${'0,'.repeat(20_000)}0]`;
  const largeValues = Array.from({ length: 20_001 }, () => 0);

  await assert.rejects(
    runtime('JSON.parse(text).length;').run({
      inputs: {
        text: largeText,
      },
      limits: {
        instructionBudget: 8,
      },
    }),
    isJsliteError({
      kind: 'Limit',
      message: /instruction budget exhausted/,
    }),
  );

  await assert.rejects(
    runtime('JSON.stringify(values).length;').run({
      inputs: {
        values: largeValues,
      },
      limits: {
        instructionBudget: 8,
      },
    }),
    isJsliteError({
      kind: 'Limit',
      message: /instruction budget exhausted/,
    }),
  );
});

test('direct JSON.stringify returns respect the heap limit before crossing the host boundary', async () => {
  const text = 'x'.repeat(10_000);

  await assert.rejects(
    runtime('JSON.stringify(text);').run({
      inputs: {
        text,
      },
      limits: {
        heapLimitBytes: 15_000,
      },
    }),
    isJsliteError({
      kind: 'Limit',
      message: /heap limit exceeded/,
    }),
  );
});
