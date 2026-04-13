'use strict';

const { assert, isMustardError, Progress, runtime, test } = require('./support/helpers.js');

test('run exposes structured inputs with preserved numeric edge cases', async () => {
  const result = await runtime(`
    ({ value, inf, negZero, nan });
  `).run({
    inputs: {
      value: 7,
      inf: Infinity,
      negZero: -0,
      nan: NaN,
    },
  });

  assert.equal(result.value, 7);
  assert.equal(result.inf, Infinity);
  assert.ok(Object.is(result.negZero, -0));
  assert.ok(Number.isNaN(result.nan));
});

test('run drives host capabilities', async () => {
  const result = await runtime(`
    const response = fetch_data(9);
    response + 1;
  `).run({
    capabilities: {
      fetch_data(value) {
        return value;
      },
    },
  });

  assert.equal(result, 10);
});

test('run resolves inputs and capabilities through the real global object', async () => {
  const result = await runtime(`
    value += 2;
    ({
      inputLookup: value,
      inputOnGlobal: globalThis.value,
      capabilityIdentity: globalThis.fetch_data === fetch_data,
      capabilityResult: globalThis.fetch_data(5),
    });
  `).run({
    inputs: {
      value: 5,
    },
    capabilities: {
      fetch_data(value) {
        return value + 1;
      },
    },
  });

  assert.deepEqual(result, {
    inputLookup: 7,
    inputOnGlobal: 7,
    capabilityIdentity: true,
    capabilityResult: 6,
  });
});

test('run awaits async host capabilities', async () => {
  const result = await runtime(`
    const response = fetch_data(5);
    response * 3;
  `).run({
    capabilities: {
      async fetch_data(value) {
        return Promise.resolve(value);
      },
    },
  });

  assert.equal(result, 15);
});

test('run routes deterministic console callbacks and ignores host return values', async () => {
  const events = [];
  const result = await runtime(`
    const first = console.log('alpha', 1);
    const second = console.warn({ ok: true });
    const third = console.error('omega');
    [first, second, third];
  `).run({
    console: {
      log(...args) {
        events.push(['log', args]);
        return 'ignored';
      },
      warn(...args) {
        events.push(['warn', args]);
        return 42;
      },
      error(...args) {
        events.push(['error', args]);
        return { ignored: true };
      },
    },
  });

  assert.deepEqual(events, [
    ['log', ['alpha', 1]],
    ['warn', [{ ok: true }]],
    ['error', ['omega']],
  ]);
  assert.deepEqual(result, [undefined, undefined, undefined]);
});

test('start exposes console callbacks as suspension points with undefined guest results', () => {
  const progress = runtime(`
    const logged = console.log('alpha');
    logged === undefined ? 2 : 0;
  `).start({
    console: {
      log() {},
    },
  });

  assert.ok(progress instanceof Progress);
  assert.equal(progress.capability, 'console.log');
  assert.deepEqual(progress.args, ['alpha']);
  assert.equal(progress.resume('ignored by guest semantics'), 2);
});

test('console methods fail guest-safely when callbacks are not registered', async () => {
  await assert.rejects(
    runtime(`
      console.log('alpha');
    `).run(),
    isMustardError({
      kind: 'Runtime',
      message: /value is not callable/,
      guestSafe: true,
    }),
  );
});

test('run surfaces sanitized host capability errors', async () => {
  await assert.rejects(
    runtime(`
      fetch_data(1);
    `).run({
      capabilities: {
        fetch_data() {
          const error = new Error('upstream failed');
          error.name = 'CapabilityError';
          error.code = 'E_UPSTREAM';
          error.details = { retriable: false };
          throw error;
        },
      },
    }),
    isMustardError({
      kind: 'Runtime',
      message: /CapabilityError: upstream failed \[code=E_UPSTREAM\]/,
    }),
  );
});

test('capability calls reject guest functions across the host boundary', async () => {
  await assert.rejects(
    runtime(`
      fetch_data(() => 1);
    `).run({
      capabilities: {
        fetch_data() {
          return 1;
        },
      },
    }),
    /functions cannot cross the structured host boundary/,
  );
});
