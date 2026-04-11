'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, JsliteError, Progress } = require('../../index.js');
const { assertDifferential } = require('./runtime-oracle.js');

test('run executes guest async functions, await, and Promise microtasks in order', async () => {
  const runtime = new Jslite(`
    let events = [];
    async function tick(label, value) {
      events[events.length] = label + ':start';
      const resolved = await Promise.resolve(value);
      events[events.length] = label + ':end:' + resolved;
      return resolved;
    }
    async function main() {
      const first = tick('a', 1);
      const second = tick('b', 2);
      events[events.length] = 'sync';
      return [await first, await second, events];
    }
    main();
  `);

  const result = await runtime.run();
  assert.deepEqual(result, [
    1,
    2,
    ['a:start', 'b:start', 'sync', 'a:end:1', 'b:end:2'],
  ]);
});

test('start and resume drive async host capability suspension inside guest async functions', () => {
  const runtime = new Jslite(`
    async function load(value) {
      const resolved = await fetch_data(value);
      return resolved * 3;
    }
    load(7);
  `);

  const progress = runtime.start({
    capabilities: {
      fetch_data() {
        throw new Error('start should suspend before invoking JS handlers');
      },
    },
  });

  assert.ok(progress instanceof Progress);
  assert.equal(progress.capability, 'fetch_data');
  assert.deepEqual(progress.args, [7]);
  assert.equal(progress.resume(7), 21);
});

test('run composes async host rejections with guest try/catch', async () => {
  const runtime = new Jslite(`
    async function load() {
      try {
        await fetch_data(1);
      } catch (error) {
        return [error.name, error.message, error.code, error.details.reason];
      }
    }
    load();
  `);

  const result = await runtime.run({
    capabilities: {
      async fetch_data() {
        const error = new Error('upstream failed');
        error.name = 'CapabilityError';
        error.code = 'E_UPSTREAM';
        error.details = { reason: 'timeout' };
        throw error;
      },
    },
  });

  assert.deepEqual(result, [
    'CapabilityError',
    'upstream failed',
    'E_UPSTREAM',
    'timeout',
  ]);
});

test('run enforces maxOutstandingHostCalls for guest async fan-out', async () => {
  const runtime = new Jslite(`
    async function fanOut() {
      const first = fetch_data(1);
      const second = fetch_data(2);
      return (await first) + (await second);
    }
    fanOut();
  `);

  await assert.rejects(
    runtime.run({
      limits: {
        maxOutstandingHostCalls: 1,
      },
      capabilities: {
        async fetch_data(value) {
          return value;
        },
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Limit' &&
      /outstanding host-call limit exhausted/.test(error.message),
  );
});

test('matches Node for supported async microtask ordering', async () => {
  await assertDifferential(`
    let events = [];
    async function tick(label, value) {
      events[events.length] = label + ':start';
      const resolved = await Promise.resolve(value);
      events[events.length] = label + ':end:' + resolved;
      return resolved;
    }
    async function main() {
      const first = tick('a', 1);
      const second = tick('b', 2);
      events[events.length] = 'sync';
      return [await first, await second, events];
    }
    main();
  `);
});
