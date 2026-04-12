'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, Progress } = require('../../index.js');

function defineEnumerableDataProperty(target, key, value) {
  Object.defineProperty(target, key, {
    value,
    enumerable: true,
    writable: true,
    configurable: true,
  });
}

function assertPlainProtoDataObject(value) {
  assert.equal(Object.getPrototypeOf(value), Object.prototype);
  assert.equal(Object.hasOwn(value, '__proto__'), true);
  assert.deepEqual(value.__proto__, { admin: true });
  assert.equal(value.user, 'alice');
  assert.equal(Object.hasOwn(value, 'admin'), false);
  assert.equal(value.admin, undefined);
}

test('guest-to-host decoding keeps __proto__ as plain data on completed results', async () => {
  const result = await new Jslite(`
    ({ ['__proto__']: { admin: true }, user: 'alice' });
  `).run();

  assertPlainProtoDataObject(result);
});

test('guest-to-host decoding keeps __proto__ as plain data on suspended capability args', () => {
  const progress = new Jslite(`
    fetch_data({ ['__proto__']: { admin: true }, user: 'alice' });
  `).start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  assert.equal(progress.capability, 'fetch_data');
  assert.equal(progress.args.length, 1);
  assertPlainProtoDataObject(progress.args[0]);
});

test('host-to-guest encoding preserves __proto__ input keys as plain data', async () => {
  const value = {};
  defineEnumerableDataProperty(value, '__proto__', { admin: true });
  defineEnumerableDataProperty(value, 'user', 'alice');

  const result = await new Jslite(`
    [value['__proto__'].admin, value.admin === undefined, value.user];
  `).run({
    inputs: { value },
  });

  assert.deepEqual(result, [true, true, 'alice']);
});

test('host-to-guest encoding rejects enumerable object accessors without executing them', async () => {
  let getterRuns = 0;
  const value = {};
  Object.defineProperty(value, 'secret', {
    enumerable: true,
    get() {
      getterRuns += 1;
      return 'top-secret';
    },
  });

  await assert.rejects(
    new Jslite('value.secret;').run({
      inputs: { value },
    }),
    (error) => error instanceof TypeError && error.message.includes('accessors cannot cross'),
  );
  assert.equal(getterRuns, 0);
});

test('host-to-guest encoding rejects enumerable setter-only properties', async () => {
  let setterRuns = 0;
  const value = {};
  Object.defineProperty(value, 'secret', {
    enumerable: true,
    set(entry) {
      setterRuns += entry === undefined ? 0 : 1;
    },
  });

  await assert.rejects(
    new Jslite('value.secret;').run({
      inputs: { value },
    }),
    (error) => error instanceof TypeError && error.message.includes('accessors cannot cross'),
  );
  assert.equal(setterRuns, 0);
});

test('capability results reject enumerable object accessors without executing them', async () => {
  let getterRuns = 0;
  const value = {};
  Object.defineProperty(value, 'secret', {
    enumerable: true,
    get() {
      getterRuns += 1;
      return 'top-secret';
    },
  });

  await assert.rejects(
    new Jslite('fetch_secret();').run({
      capabilities: {
        fetch_secret() {
          return value;
        },
      },
    }),
    (error) => error instanceof TypeError && error.message.includes('accessors cannot cross'),
  );
  assert.equal(getterRuns, 0);
});

test('host arrays reject accessor-backed elements without executing them', async () => {
  let getterRuns = 0;
  const value = [1];
  Object.defineProperty(value, '0', {
    enumerable: true,
    get() {
      getterRuns += 1;
      return 1;
    },
  });

  await assert.rejects(
    new Jslite('value[0];').run({
      inputs: { value },
    }),
    (error) => error instanceof TypeError && error.message.includes('accessors cannot cross'),
  );
  assert.equal(getterRuns, 0);
});

test('host arrays fail closed on holes instead of crossing the boundary opaquely', async () => {
  const value = [];
  value.length = 1;

  await assert.rejects(
    new Jslite('value[0];').run({
      inputs: { value },
    }),
    (error) => error instanceof TypeError && error.message.includes('holes cannot cross'),
  );
});
