'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, JsliteError, Progress } = require('../../index.js');
const { assertDifferential } = require('./runtime-oracle.js');

test('run supports guest-internal BigInt arithmetic and keyed-collection semantics', async () => {
  const runtime = new Jslite(`
    const reserve = 9007199254740993n;
    const record = {};
    record[10n] = 'ok';
    const set = new Set([1n, 1n, 2n]);
    const map = new Map([[1n, 'one'], [2n, 'two']]);
    ({
      sum: String(reserve + 25n),
      diff: String(reserve - 5n),
      product: String(21n * 2n),
      quotient: String(25n / 3n),
      remainder: String(25n % 3n),
      type: typeof reserve,
      truthy: !!1n,
      falsy: !!0n,
      compare: [2n < 10n, 10n >= 10n, 10n === 10n, 10n === 11n],
      key: record['10'],
      setSize: set.size,
      mapValue: map.get(2n),
    });
  `);

  const result = await runtime.run();
  assert.deepEqual(result, {
    sum: '9007199254741018',
    diff: '9007199254740988',
    product: '42',
    quotient: '8',
    remainder: '1',
    type: 'bigint',
    truthy: true,
    falsy: false,
    compare: [true, true, true, false],
    key: 'ok',
    setSize: 2,
    mapValue: 'two',
  });
});

test('progress dump/load preserve guest BigInt state across suspension', () => {
  const runtime = new Jslite(`
    async function main() {
      const reserve = 9007199254740993n;
      const status = await fetch_step('A-9');
      return { status, total: String(reserve + 7n) };
    }
    main();
  `);

  const progress = runtime.start({
    capabilities: {
      fetch_step() {},
    },
  });

  const restored = Progress.load(progress.dump());
  assert.deepEqual(restored.resume('approved'), {
    status: 'approved',
    total: '9007199254741000',
  });
});

test('BigInt mixed edges and JSON.stringify fail closed with explicit errors', async () => {
  const runtime = new Jslite(`
    [
      (() => {
        try {
          return 1n + 1;
        } catch (error) {
          return error.message;
        }
      })(),
      (() => {
        try {
          return 1n < 2;
        } catch (error) {
          return error.message;
        }
      })(),
      (() => {
        try {
          return Number(1n);
        } catch (error) {
          return error.message;
        }
      })(),
      (() => {
        try {
          return +1n;
        } catch (error) {
          return error.message;
        }
      })(),
      (() => {
        try {
          return JSON.stringify({ amount: 1n });
        } catch (error) {
          return error.message;
        }
      })(),
    ];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [
    'cannot mix BigInt and Number values in arithmetic',
    'cannot compare BigInt and Number values',
    'cannot coerce BigInt values to numbers',
    'unary plus is not supported for BigInt values',
    'JSON.stringify does not support BigInt values',
  ]);
});

test('BigInt values still fail closed at the structured host boundary', async () => {
  await assert.rejects(
    () => new Jslite('1n;').run(),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('BigInt values cannot cross the structured host boundary'),
  );

  await assert.rejects(
    () =>
      new Jslite('send_amount(1n);').run({
        capabilities: {
          send_amount() {},
        },
      }),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Runtime' &&
      error.message.includes('BigInt values cannot cross the structured host boundary'),
  );
});

test('matches Node for the supported guest-internal BigInt surface', async () => {
  await assertDifferential(`
    const reserve = 9007199254740993n;
    const set = new Set([1n, 1n, 2n]);
    const map = new Map([[1n, 'one'], [2n, 'two']]);
    ({
      sum: String(reserve + 25n),
      diff: String(reserve - 5n),
      product: String(21n * 2n),
      quotient: String(25n / 3n),
      remainder: String(25n % 3n),
      type: typeof reserve,
      truthy: !!1n,
      falsy: !!0n,
      compare: [2n < 10n, 10n >= 10n, 10n === 10n, 10n === 11n],
      setSize: set.size,
      mapValue: map.get(2n),
    });
  `);
});
