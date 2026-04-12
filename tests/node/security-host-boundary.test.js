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

function createTrapProxy(events, values) {
  return new Proxy(
    {},
    {
      getPrototypeOf() {
        events.push('getPrototypeOf');
        return Object.prototype;
      },
      ownKeys() {
        events.push('ownKeys');
        return Object.keys(values);
      },
      getOwnPropertyDescriptor(_target, key) {
        events.push(`getOwnPropertyDescriptor:${String(key)}`);
        if (!Object.hasOwn(values, key)) {
          return undefined;
        }
        return {
          enumerable: true,
          configurable: true,
          writable: true,
          value: values[key],
        };
      },
      get(_target, key) {
        events.push(`get:${String(key)}`);
        return values[key];
      },
    },
  );
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

test('host-to-guest encoding rejects proxy-backed inputs without executing traps', async () => {
  const events = [];
  const value = createTrapProxy(events, { answer: 42 });

  await assert.rejects(
    new Jslite('value.answer;').run({
      inputs: { value },
    }),
    (error) => error instanceof TypeError && error.message.includes('Proxy values cannot cross'),
  );
  assert.deepEqual(events, []);
});

test('capability registration rejects proxy-backed handler containers without executing traps', async () => {
  const events = [];
  const capabilities = createTrapProxy(events, {
    fetch_data() {
      return 1;
    },
  });

  await assert.rejects(
    new Jslite('fetch_data();').run({
      capabilities,
    }),
    (error) =>
      error instanceof TypeError && error.message.includes('options.capabilities must be a plain object'),
  );
  assert.deepEqual(events, []);
});

test('capability results reject proxy-backed values without executing traps', async () => {
  const events = [];
  const value = createTrapProxy(events, { answer: 42 });

  await assert.rejects(
    new Jslite('fetch_data().answer;').run({
      capabilities: {
        fetch_data() {
          return value;
        },
      },
    }),
    (error) => error instanceof TypeError && error.message.includes('Proxy values cannot cross'),
  );
  assert.deepEqual(events, []);
});

test('resumeError rejects proxy-backed details without executing traps', () => {
  const progress = new Jslite('fetch_data(1);').start({
    capabilities: {
      fetch_data() {},
    },
  });
  const events = [];
  const details = createTrapProxy(events, { answer: 42 });
  const error = new Error('boom');
  error.details = details;

  assert.throws(
    () => progress.resumeError(error),
    (entry) => entry instanceof TypeError && entry.message.includes('Proxy values cannot cross'),
  );
  assert.deepEqual(events, []);
});

test('host-to-guest encoding rejects cyclic inputs with a typed boundary error', async () => {
  const value = {};
  value.self = value;

  await assert.rejects(
    new Jslite('value;').run({
      inputs: { value },
    }),
    (error) =>
      error instanceof TypeError &&
      error.message.includes('cyclic values cannot cross the host boundary'),
  );
});

test('capability results reject cyclic values with a typed boundary error', async () => {
  await assert.rejects(
    new Jslite('fetch_data();').run({
      capabilities: {
        fetch_data() {
          const value = {};
          value.self = value;
          return value;
        },
      },
    }),
    (error) =>
      error instanceof TypeError &&
      error.message.includes('cyclic values cannot cross the host boundary'),
  );
});

test('resumeError rejects cyclic details with a typed boundary error', () => {
  const progress = new Jslite('fetch_data(1);').start({
    capabilities: {
      fetch_data() {},
    },
  });
  const details = {};
  details.self = details;
  const error = new Error('boom');
  error.details = details;

  assert.throws(
    () => progress.resumeError(error),
    (entry) =>
      entry instanceof TypeError &&
      entry.message.includes('cyclic values cannot cross the host boundary'),
  );
});
