'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { Mustard, MustardError, Progress } = require('../../index.ts');
const { loadNative } = require('../../native-loader.ts');

function isCancelledLimit(error) {
  return (
    error instanceof MustardError &&
    error.name === 'MustardLimitError' &&
    error.kind === 'Limit' &&
    /execution cancelled/.test(error.message)
  );
}

test('run rejects immediately when the abort signal is already cancelled', async () => {
  const controller = new AbortController();
  controller.abort();

  const runtime = new Mustard('while (true) {}');
  await assert.rejects(runtime.run({ signal: controller.signal }), isCancelledLimit);
});

test('start short-circuits already-aborted signals before host boundary traversal', () => {
  const controller = new AbortController();
  controller.abort();

  assert.throws(
    () =>
      new Mustard('value;').start({
        inputs: {
          value: new Array(1_000_001),
        },
        signal: controller.signal,
      }),
    isCancelledLimit,
  );
});

test('progress.cancel aborts suspended execution without guest catch interception', () => {
  const runtime = new Mustard(`
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
  let hostStarted = () => {};
  const hostPromise = new Promise((resolve) => {
    resolveHost = resolve;
  });
  const started = new Promise((resolve) => {
    hostStarted = resolve;
  });
  const controller = new AbortController();

  const runtime = new Mustard(`
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
          hostStarted();
          return hostPromise;
        },
      },
  });

  await started;
  controller.abort();
  resolveHost(1);

  await assert.rejects(pending, isCancelledLimit);
});

test('progress.resume respects already-aborted signals', () => {
  const controller = new AbortController();
  controller.abort();

  const runtime = new Mustard(`
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

test('native cancellation token ids are unguessable process-local handles', () => {
  const native = loadNative();
  const tokenIds = [native.createCancellationToken(), native.createCancellationToken()];

  try {
    for (const tokenId of tokenIds) {
      assert.match(tokenId, /^cancel-[0-9a-f]{32}$/);
    }
    assert.notEqual(tokenIds[0], tokenIds[1]);
    assert.throws(
      () => native.cancelCancellationToken('cancel-1'),
      /unknown cancellation token/,
    );
  } finally {
    for (const tokenId of tokenIds) {
      native.releaseCancellationToken(tokenId);
    }
  }
});
