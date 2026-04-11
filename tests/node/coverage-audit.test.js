const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite } = require('../../index.js');

test('JSON.stringify uses the documented sorted-key object order', async () => {
  const runtime = new Jslite(`
    JSON.stringify({
      zebra: 1,
      alpha: 2,
      middle: 3,
    });
  `);

  const result = await runtime.run();
  assert.equal(result, '{"alpha":2.0,"middle":3.0,"zebra":1.0}');
});

test('built-in error constructors round-trip visible fields', async () => {
  const runtime = new Jslite(`
    const range = new RangeError('too far');
    const type = new TypeError('wrong kind');
    [
      range.name,
      range.message,
      type.name,
      type.message,
    ];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [
    'RangeError',
    'too far',
    'TypeError',
    'wrong kind',
  ]);
});

test('globalThis remains a stable guest-visible object', async () => {
  const runtime = new Jslite(`
    globalThis.answer = 3;
    [
      typeof globalThis,
      globalThis.answer,
      globalThis === globalThis,
    ];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, ['object', 3, true]);
});

test('deferring await does not inject a guest-visible cancellation signal', async () => {
  const runtime = new Jslite(`
    const value = fetch_data(2);
    value + 1;
  `);

  let calls = 0;
  let completed = 0;
  const pending = runtime.run({
    capabilities: {
      async fetch_data(value) {
        calls += 1;
        await new Promise((resolve) => setTimeout(resolve, 50));
        completed += 1;
        return value;
      },
    },
  });

  await new Promise((resolve) => setTimeout(resolve, 100));
  assert.equal(calls, 1);
  assert.equal(completed, 1);
  assert.equal(await pending, 3);
});
