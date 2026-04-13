'use strict';

const { assert, isMustardError, runtime, test } = require('./support/helpers.js');

test('run surfaces limit failures as typed errors', async () => {
  await assert.rejects(
    runtime('while (true) {}').run({
      limits: {
        instructionBudget: 100,
      },
    }),
    isMustardError({
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
    isMustardError({
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
    isMustardError({
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
    isMustardError({
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
    isMustardError({
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
    isMustardError({
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
    isMustardError({
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
    isMustardError({
      kind: 'Limit',
      message: /instruction budget exhausted/,
    }),
  );

  await assert.rejects(
    runtime('Number.parseInt(text, 10);').run({
      inputs: {
        text: '9'.repeat(20_000),
      },
      limits: {
        instructionBudget: 8,
      },
    }),
    isMustardError({
      kind: 'Limit',
      message: /instruction budget exhausted/,
    }),
  );

  await assert.rejects(
    runtime('Number.parseFloat(text);').run({
      inputs: {
        text: `${'9'.repeat(20_000)}.5`,
      },
      limits: {
        instructionBudget: 8,
      },
    }),
    isMustardError({
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
    isMustardError({
      kind: 'Limit',
      message: /heap limit exceeded/,
    }),
  );
});

test('JSON.parse bare string results respect the heap limit before crossing the host boundary', async () => {
  await assert.rejects(
    runtime('JSON.parse(text);').run({
      inputs: {
        text: JSON.stringify('x'.repeat(10_000)),
      },
      limits: {
        heapLimitBytes: 15_000,
      },
    }),
    isMustardError({
      kind: 'Limit',
      message: /heap limit exceeded/,
    }),
  );
});

test('bare string capability results respect the heap limit before crossing the host boundary', async () => {
  await assert.rejects(
    runtime('fetch_data();').run({
      capabilities: {
        fetch_data() {
          return 'x'.repeat(5_000_000);
        },
      },
      limits: {
        heapLimitBytes: 50_000,
        allocationBudget: 1_000_000,
        instructionBudget: 10_000_000,
        callDepthLimit: 1_000,
        maxOutstandingHostCalls: 1,
      },
    }),
    isMustardError({
      kind: 'Limit',
      message: /heap limit exceeded/,
    }),
  );
});

test('bare string resume payloads respect the heap limit before crossing the host boundary', () => {
  const progress = runtime('fetch_data();').start({
    capabilities: {
      fetch_data() {},
    },
    limits: {
      heapLimitBytes: 50_000,
      allocationBudget: 1_000_000,
      instructionBudget: 10_000_000,
      callDepthLimit: 1_000,
      maxOutstandingHostCalls: 1,
    },
  });

  assert.throws(
    () => progress.resume('x'.repeat(5_000_000)),
    isMustardError({
      kind: 'Limit',
      message: /heap limit exceeded/,
    }),
  );
});

test('resume payloads charge instruction budget during structured boundary conversion', () => {
  const progress = runtime('fetch_data(); 0;').start({
    capabilities: {
      fetch_data() {},
    },
    limits: {
      instructionBudget: 5,
      heapLimitBytes: 1_000_000_000,
      allocationBudget: 2_000_000,
    },
  });
  const largeValues = Array.from({ length: 20_000 }, (_, index) => index);

  assert.throws(
    () => progress.resume(largeValues),
    isMustardError({
      kind: 'Limit',
      message: /instruction budget exhausted/,
    }),
  );
});

test('host boundary rejects oversized sparse arrays before synchronous traversal', () => {
  assert.throws(
    () =>
      runtime('value.length;').start({
        inputs: {
          value: new Array(1_000_001),
        },
      }),
    /arrays longer than 1000000 elements cannot cross the host boundary/,
  );
});

test('capability results charge instruction budget during structured boundary conversion', async () => {
  const largeValues = Array.from({ length: 20_000 }, (_, index) => index);

  await assert.rejects(
    runtime('fetch_data();').run({
      capabilities: {
        fetch_data() {
          return largeValues;
        },
      },
      limits: {
        instructionBudget: 5,
        heapLimitBytes: 1_000_000_000,
        allocationBudget: 2_000_000,
      },
    }),
    isMustardError({
      kind: 'Limit',
      message: /instruction budget exhausted/,
    }),
  );
});
