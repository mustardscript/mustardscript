'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, JsliteError, Progress } = require('../../index.js');

function isCancelledLimit(error) {
  return (
    error instanceof JsliteError &&
    error.name === 'JsliteLimitError' &&
    error.kind === 'Limit' &&
    /execution cancelled/.test(error.message)
  );
}

test('run rejects immediately when the abort signal is already cancelled', async () => {
  const controller = new AbortController();
  controller.abort();

  const runtime = new Jslite('while (true) {}');
  await assert.rejects(runtime.run({ signal: controller.signal }), isCancelledLimit);
});

test('progress.cancel aborts suspended execution without guest catch interception', () => {
  const runtime = new Jslite(`
    try {
      const value = fetch_data(1);
      value + 1;
    } catch (error) {
      'guest-caught';
    }
  `);

  const progress = runtime.start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  assert.throws(() => progress.cancel(), isCancelledLimit);
});

test('run cancels while guest async code is awaiting a host promise', async () => {
  let resolveHost = () => {};
  const hostPromise = new Promise((resolve) => {
    resolveHost = resolve;
  });
  const controller = new AbortController();

  const runtime = new Jslite(`
    async function main() {
      try {
        await fetch_data(1);
        return 'done';
      } catch (error) {
        return 'guest-caught';
      }
    }
    main();
  `);

  const pending = runtime.run({
    signal: controller.signal,
    capabilities: {
      fetch_data() {
        return hostPromise;
      },
    },
  });

  await new Promise((resolve) => setImmediate(resolve));
  controller.abort();
  resolveHost(1);

  await assert.rejects(pending, isCancelledLimit);
});

test('progress.resume respects already-aborted signals', () => {
  const controller = new AbortController();
  controller.abort();

  const runtime = new Jslite(`
    const value = fetch_data(1);
    value + 1;
  `);

  const progress = runtime.start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  assert.throws(
    () => progress.resume(1, { signal: controller.signal }),
    isCancelledLimit,
  );
});
