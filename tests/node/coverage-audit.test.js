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

test('in operator follows the conservative supported property surface and rejects primitives', async () => {
  const runtime = new Jslite(`
    const object = { alpha: undefined };
    const array = [4];
    array.extra = 5;
    const map = new Map();
    const set = new Set();
    const promise = Promise.resolve(1);
    const regex = /a/g;
    const date = new Date(5);
    [
      "alpha" in object,
      "missing" in object,
      0 in array,
      1 in array,
      "length" in array,
      "push" in array,
      "extra" in array,
      "log" in Math,
      "parse" in JSON,
      "then" in promise,
      "exec" in regex,
      "getTime" in date,
      "size" in map,
      "add" in set,
      "from" in Array,
      "assign" in Object,
      "now" in Date,
      "resolve" in Promise,
    ];
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [
    true,
    false,
    true,
    false,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
    true,
  ]);

  await assert.rejects(
    () => new Jslite(`"length" in "abc";`).run(),
    (error) =>
      error &&
      error.kind === 'Runtime' &&
      error.message.includes("right-hand side of 'in' must be an object in the supported surface"),
  );
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
