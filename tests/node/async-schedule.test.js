'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');

const { Jslite, JsliteError, Progress } = require('../../index.js');
const { normalizeValue } = require('./runtime-oracle.js');

const SNAPSHOT_KEY = Buffer.from('async-schedule-test-key');
const EXPLICIT_LOAD_OPTIONS = Object.freeze({
  snapshotKey: SNAPSHOT_KEY,
  capabilities: {
    fetch_data() {},
  },
  limits: {},
});

function createDeferred() {
  let resolve = () => {};
  let reject = () => {};
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

async function flushMicrotasks(turns = 1) {
  for (let index = 0; index < turns; index += 1) {
    await Promise.resolve();
  }
}

function normalizeError(error) {
  if (error instanceof Error) {
    const normalized = {
      type: 'error',
      value: {
        name: error.name,
        message:
          typeof error.message === 'string'
            ? error.message.replace(/\[\d+\.\.\d+\]/g, '[span]')
            : error.message,
      },
    };
    if ('kind' in error && error.kind !== undefined) {
      normalized.value.kind = normalizeValue(error.kind);
    }
    if ('code' in error && error.code !== undefined) {
      normalized.value.code = normalizeValue(error.code);
    }
    if ('details' in error && error.details !== undefined) {
      normalized.value.details = normalizeValue(error.details);
    }
    return normalized;
  }
  return {
    type: 'thrown',
    value: normalizeValue(error),
  };
}

function normalizeHostTraceEvent(event) {
  if (event.phase === 'call') {
    return {
      type: 'capability',
      phase: 'call',
      name: event.name,
      callId: event.callId,
      args: event.args.map(normalizeValue),
    };
  }
  if (event.phase === 'resolve') {
    return {
      type: 'capability',
      phase: 'resolve',
      name: event.name,
      callId: event.callId,
      value: normalizeValue(event.value),
    };
  }
  if (event.phase === 'reject') {
    return {
      type: 'capability',
      phase: 'reject',
      name: event.name,
      callId: event.callId,
      error: normalizeError(event.error),
    };
  }
  if (event.phase === 'microtask-checkpoint') {
    return {
      type: 'runtime',
      phase: 'microtask-checkpoint',
      turn: event.turn,
    };
  }
  if (event.phase === 'abort') {
    return {
      type: 'runtime',
      phase: 'abort',
      turn: event.turn,
    };
  }
  if (event.phase === 'completed') {
    return {
      type: 'runtime',
      phase: 'completed',
    };
  }
  if (event.phase === 'failed') {
    return {
      type: 'runtime',
      phase: 'failed',
      error: normalizeError(event.error),
    };
  }
  throw new Error(`unsupported host trace phase: ${event.phase}`);
}

function captureScheduleRecordFromOutcome(outcome, hostTrace) {
  if (outcome.type === 'fulfilled') {
    const payload = outcome.value;
    const guestTrace = Array.isArray(payload?.guestTrace)
      ? Array.from(payload.guestTrace, (entry) => String(entry))
      : [];
    const result =
      payload &&
      typeof payload === 'object' &&
      !Array.isArray(payload) &&
      Object.prototype.hasOwnProperty.call(payload, 'result')
        ? payload.result
        : payload;
    return {
      outcome: {
        type: 'fulfilled',
        value: normalizeValue(result),
      },
      guestTrace,
      hostTrace: [...hostTrace.map(normalizeHostTraceEvent), { type: 'runtime', phase: 'completed' }],
    };
  }

  return {
    outcome: {
      type: 'rejected',
      value: normalizeError(outcome.error),
    },
    guestTrace: [],
    hostTrace: [
      ...hostTrace.map(normalizeHostTraceEvent),
      { type: 'runtime', phase: 'failed', error: normalizeError(outcome.error) },
    ],
  };
}

async function captureRawOutcome(run) {
  try {
    return {
      type: 'fulfilled',
      value: await run(),
    };
  } catch (error) {
    return {
      type: 'rejected',
      error,
    };
  }
}

function renderCanonical(value) {
  return JSON.stringify(value, null, 2);
}

function assertScheduleRecordEqual(label, actual, expected) {
  try {
    assert.deepEqual(actual, expected);
  } catch {
    throw new assert.AssertionError({
      message: [
        `Async schedule mismatch for ${label}`,
        'expected:',
        renderCanonical(expected),
        'actual:',
        renderCanonical(actual),
      ].join('\n'),
      actual,
      expected,
      operator: 'asyncScheduleEqual',
    });
  }
}

function renderPromiseInput(input) {
  if (input.outcome === 'resolve') {
    return `Promise.resolve(${JSON.stringify(`${input.label}:value`)})`;
  }
  return `Promise.reject(${JSON.stringify(`${input.label}:error`)})`;
}

function renderPromiseChainSource({ baseOutcome, finallyRejects }) {
  return `
    async function main() {
      const guestTrace = [];
      const base = ${
        baseOutcome === 'resolve'
          ? 'Promise.resolve("base:value")'
          : 'Promise.reject("base:error")'
      };
      const outcome = await base
        .finally(() => {
          guestTrace[guestTrace.length] = 'finally';
          return ${
            finallyRejects
              ? 'Promise.reject("finally:error")'
              : 'Promise.resolve("finally:pass")'
          };
        })
        .then((value) => {
          guestTrace[guestTrace.length] = 'then:' + value;
          return ['value', value];
        })
        .catch((reason) => {
          guestTrace[guestTrace.length] = 'catch:' + reason;
          return ['error', reason];
        });
      const nested = await Promise.resolve(await Promise.resolve('nested:${baseOutcome}:${finallyRejects ? 'reject' : 'pass'}'));
      guestTrace[guestTrace.length] = nested;
      return { result: { outcome, nested }, guestTrace };
    }
    main();
  `;
}

function renderPromiseCombinatorSource({ combinator, orderedInputs }) {
  return `
    async function main() {
      const guestTrace = [];
      function observe(label, promise) {
        return promise
          .then(
            (value) => {
              guestTrace[guestTrace.length] = label + ':value:' + value;
              return value;
            },
            (reason) => {
              guestTrace[guestTrace.length] = label + ':error:' + reason;
              throw reason;
            },
          )
          .finally(() => {
            guestTrace[guestTrace.length] = label + ':finally';
          });
      }
      const first = observe(${JSON.stringify(orderedInputs[0].label)}, ${renderPromiseInput(orderedInputs[0])});
      const second = observe(${JSON.stringify(orderedInputs[1].label)}, ${renderPromiseInput(orderedInputs[1])});
      let combined;
      try {
        combined = await Promise.${combinator}([first, second]);
        guestTrace[guestTrace.length] = 'combined:value';
      } catch (error) {
        if (error && typeof error === 'object' && Array.isArray(error.errors)) {
          combined = ['AggregateError', error.message, error.errors];
        } else {
          combined = ['error', String(error)];
        }
        guestTrace[guestTrace.length] = 'combined:error';
      }
      const nested = await Promise.resolve(await Promise.resolve(${JSON.stringify(
        `nested:${orderedInputs.map((input) => input.label).join('-')}`,
      )}));
      guestTrace[guestTrace.length] = nested;
      return { result: { combined, nested }, guestTrace };
    }
    main();
  `;
}

async function captureGuestScheduleRecord(source, runner) {
  const outcome = await captureRawOutcome(runner);
  return captureScheduleRecordFromOutcome(outcome, []);
}

async function runJsliteSchedule(source) {
  const runtime = new Jslite(source);
  return runtime.run();
}

async function runNodeSchedule(source) {
  return Promise.resolve(vm.runInNewContext(source, Object.create(null)));
}

function capabilityValue(label, value) {
  return `${label}:${value * 10}`;
}

function capabilityError(label, value) {
  const error = new Error(`${label}:${value}:boom`);
  error.name = 'CapabilityError';
  return error;
}

const HOST_SCHEDULE_SOURCE = `
  async function settle(label, value, guestTrace) {
    try {
      const resolved = await fetch_data(label, value);
      guestTrace[guestTrace.length] = label + ':value:' + resolved;
      return 'value:' + resolved;
    } catch (error) {
      guestTrace[guestTrace.length] = label + ':error:' + error.name + ':' + error.message;
      return 'error:' + error.name + ':' + error.message;
    } finally {
      guestTrace[guestTrace.length] = label + ':finally';
    }
  }
  async function main() {
    const guestTrace = [];
    const first = settle('first', 1, guestTrace);
    guestTrace[guestTrace.length] = 'queued:first';
    const second = settle('second', 2, guestTrace);
    guestTrace[guestTrace.length] = 'queued:second';
    const result = await Promise.all([first, second]);
    guestTrace[guestTrace.length] = 'combined';
    return { result, guestTrace };
  }
  main();
`;

function loadProgress(progress, transport) {
  if (transport === 'direct') {
    return progress;
  }
  if (transport === 'explicit-load') {
    return Progress.load(progress.dump(), EXPLICIT_LOAD_OPTIONS);
  }
  throw new Error(`unsupported transport: ${transport}`);
}

async function captureRunHostScheduleRecord(schedule) {
  const runtime = new Jslite(HOST_SCHEDULE_SOURCE);
  const hostTrace = [];
  const pendingCalls = [];
  const pending = runtime.run({
    capabilities: {
      fetch_data(label, value) {
        const callId = pendingCalls.length;
        const deferred = createDeferred();
        pendingCalls.push({ label, value, deferred });
        hostTrace.push({
          type: 'capability',
          phase: 'call',
          name: 'fetch_data',
          callId,
          args: [label, value],
        });
        return deferred.promise;
      },
    },
  });

  await flushMicrotasks();
  assert.equal(pendingCalls.length, 1, 'run() should surface the first queued host request');

  for (let index = 0; index < schedule.length; index += 1) {
    const { outcome } = schedule[index];
    const pendingCall = pendingCalls[index];
    assert.ok(pendingCall, `missing pending host call ${index + 1}`);
    if (outcome === 'value') {
      const value = capabilityValue(pendingCall.label, pendingCall.value);
      hostTrace.push({
        type: 'capability',
        phase: 'resolve',
        name: 'fetch_data',
        callId: index,
        value,
      });
      pendingCall.deferred.resolve(value);
    } else {
      const error = capabilityError(pendingCall.label, pendingCall.value);
      hostTrace.push({
        type: 'capability',
        phase: 'reject',
        name: 'fetch_data',
        callId: index,
        error,
      });
      pendingCall.deferred.reject(error);
    }

    hostTrace.push({
      type: 'runtime',
      phase: 'microtask-checkpoint',
      turn: index + 1,
    });
    if (index < schedule.length - 1) {
      await flushMicrotasks();
      assert.equal(
        pendingCalls.length,
        index + 2,
        'run() should expose the next queued host request after a checkpoint',
      );
    }
  }

  return captureScheduleRecordFromOutcome(await captureRawOutcome(() => pending), hostTrace);
}

async function captureProgressHostScheduleRecord(schedule) {
  const runtime = new Jslite(HOST_SCHEDULE_SOURCE);
  const hostTrace = [];
  let current = runtime.start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
    limits: {},
  });

  for (let index = 0; index < schedule.length; index += 1) {
    const { outcome, transport } = schedule[index];
    current = loadProgress(current, transport);
    assert.ok(current instanceof Progress, 'progress flow should remain suspended until the final step');
    hostTrace.push({
      type: 'capability',
      phase: 'call',
      name: current.capability,
      callId: index,
      args: current.args,
    });

    if (outcome === 'value') {
      const value = capabilityValue(current.args[0], current.args[1]);
      hostTrace.push({
        type: 'capability',
        phase: 'resolve',
        name: current.capability,
        callId: index,
        value,
      });
      current = current.resume(value);
    } else {
      const error = capabilityError(current.args[0], current.args[1]);
      hostTrace.push({
        type: 'capability',
        phase: 'reject',
        name: current.capability,
        callId: index,
        error,
      });
      current = current.resumeError(error);
    }

    if (index < schedule.length - 1) {
      assert.ok(current instanceof Progress, 'resume should expose the next queued host request');
    }
    hostTrace.push({
      type: 'runtime',
      phase: 'microtask-checkpoint',
      turn: index + 1,
    });
  }

  return captureScheduleRecordFromOutcome(
    {
      type: 'fulfilled',
      value: current,
    },
    hostTrace,
  );
}

function isCancelledLimit(error) {
  return (
    error instanceof JsliteError &&
    error.kind === 'Limit' &&
    error.message.includes('execution cancelled')
  );
}

const CANCELLATION_SOURCE = `
  async function main() {
    const guestTrace = [];
    try {
      const resolved = await fetch_data('task');
      guestTrace[guestTrace.length] = 'value:' + resolved;
      return { result: ['value', resolved], guestTrace };
    } catch (error) {
      guestTrace[guestTrace.length] = 'caught:' + error.name + ':' + error.message;
      return { result: ['caught', error.name, error.message], guestTrace };
    } finally {
      guestTrace[guestTrace.length] = 'finally';
    }
  }
  main();
`;

async function runCancellationSchedule(caseEntry) {
  const controller = new AbortController();
  const hostTrace = [];
  const deferred = createDeferred();
  const runtime = new Jslite(CANCELLATION_SOURCE);
  const pending = runtime.run({
    signal: controller.signal,
    capabilities: {
      fetch_data(label) {
        hostTrace.push({
          type: 'capability',
          phase: 'call',
          name: 'fetch_data',
          callId: 0,
          args: [label],
        });
        return deferred.promise;
      },
    },
  });

  await flushMicrotasks();

  if (caseEntry.order === 'abort-before-settle') {
    controller.abort();
    hostTrace.push({ type: 'runtime', phase: 'abort', turn: 0 });
    if (caseEntry.settlement === 'value') {
      const value = 'task:done';
      hostTrace.push({
        type: 'capability',
        phase: 'resolve',
        name: 'fetch_data',
        callId: 0,
        value,
      });
      deferred.resolve(value);
    } else {
      const error = capabilityError('task', 0);
      hostTrace.push({
        type: 'capability',
        phase: 'reject',
        name: 'fetch_data',
        callId: 0,
        error,
      });
      deferred.reject(error);
    }
  } else if (caseEntry.order === 'settle-then-abort') {
    if (caseEntry.settlement === 'value') {
      const value = 'task:done';
      hostTrace.push({
        type: 'capability',
        phase: 'resolve',
        name: 'fetch_data',
        callId: 0,
        value,
      });
      deferred.resolve(value);
    } else {
      const error = capabilityError('task', 0);
      hostTrace.push({
        type: 'capability',
        phase: 'reject',
        name: 'fetch_data',
        callId: 0,
        error,
      });
      deferred.reject(error);
    }
    controller.abort();
    hostTrace.push({ type: 'runtime', phase: 'abort', turn: 0 });
  } else {
    if (caseEntry.settlement === 'value') {
      deferred.resolve('task:done');
    } else {
      deferred.reject(capabilityError('task', 0));
    }
    await pending.catch(() => {});
    controller.abort();
    hostTrace.push({ type: 'runtime', phase: 'abort', turn: 1 });
  }

  return {
    record: captureScheduleRecordFromOutcome(await captureRawOutcome(() => pending), hostTrace),
    hostTrace,
  };
}

test('async schedule matrix: promise chains match Node across exhaustive finally schedules', async (t) => {
  for (const baseOutcome of ['resolve', 'reject']) {
    for (const finallyRejects of [false, true]) {
      const label = `chain:${baseOutcome}:finally-${finallyRejects ? 'reject' : 'pass'}`;
      const source = renderPromiseChainSource({ baseOutcome, finallyRejects });
      await t.test(label, async () => {
        const [actual, expected] = await Promise.all([
          captureGuestScheduleRecord(source, () => runJsliteSchedule(source)),
          captureGuestScheduleRecord(source, () => runNodeSchedule(source)),
        ]);
        assertScheduleRecordEqual(label, actual, expected);
      });
    }
  }
});

test('async schedule matrix: promise combinators match Node across exhaustive two-input schedules', async (t) => {
  const inputs = [
    { label: 'alpha', outcome: 'resolve' },
    { label: 'beta', outcome: 'resolve' },
  ];
  const inputOrders = [
    inputs,
    [inputs[1], inputs[0]],
  ];
  const combinators = ['all', 'allSettled', 'race', 'any'];

  for (const combinator of combinators) {
    for (const firstOutcome of ['resolve', 'reject']) {
      for (const secondOutcome of ['resolve', 'reject']) {
        for (const order of inputOrders) {
          const orderedInputs = [
            { ...order[0], outcome: order[0].label === 'alpha' ? firstOutcome : secondOutcome },
            { ...order[1], outcome: order[1].label === 'alpha' ? firstOutcome : secondOutcome },
          ];
          const label = `${combinator}:${orderedInputs
            .map((entry) => `${entry.label}-${entry.outcome}`)
            .join(':')}`;
          const source = renderPromiseCombinatorSource({ combinator, orderedInputs });
          await t.test(label, async () => {
            const [actual, expected] = await Promise.all([
              captureGuestScheduleRecord(source, () => runJsliteSchedule(source)),
              captureGuestScheduleRecord(source, () => runNodeSchedule(source)),
            ]);
            assertScheduleRecordEqual(label, actual, expected);
          });
        }
      }
    }
  }
});

test('async schedule matrix: run() and start()/load()/resume() agree while pending promise work remains queued', async (t) => {
  const transports = ['direct', 'explicit-load'];
  for (const firstOutcome of ['value', 'error']) {
    for (const secondOutcome of ['value', 'error']) {
      for (const firstTransport of transports) {
        for (const secondTransport of transports) {
          const schedule = [
            { outcome: firstOutcome, transport: firstTransport },
            { outcome: secondOutcome, transport: secondTransport },
          ];
          const label = `pending-work:${firstOutcome}:${secondOutcome}:${firstTransport}:${secondTransport}`;
          await t.test(label, async () => {
            const [actual, expected] = await Promise.all([
              captureRunHostScheduleRecord(schedule),
              captureProgressHostScheduleRecord(schedule),
            ]);
            assertScheduleRecordEqual(label, actual, expected);
          });
        }
      }
    }
  }
});

test('async schedule matrix: cancellation races are deterministic around host settlement order', async (t) => {
  const cases = [
    { id: 'abort-before-settle:value', order: 'abort-before-settle', settlement: 'value' },
    { id: 'abort-before-settle:error', order: 'abort-before-settle', settlement: 'error' },
    { id: 'settle-then-abort:value', order: 'settle-then-abort', settlement: 'value' },
    { id: 'settle-then-abort:error', order: 'settle-then-abort', settlement: 'error' },
    { id: 'complete-before-abort:value', order: 'complete-before-abort', settlement: 'value' },
    { id: 'complete-before-abort:error', order: 'complete-before-abort', settlement: 'error' },
  ];

  for (const caseEntry of cases) {
    await t.test(caseEntry.id, async () => {
      const { record } = await runCancellationSchedule(caseEntry);
      if (caseEntry.order === 'complete-before-abort') {
        assert.equal(record.outcome.type, 'fulfilled');
        if (caseEntry.settlement === 'value') {
          assert.deepEqual(record.outcome.value, normalizeValue(['value', 'task:done']));
          assert.deepEqual(record.guestTrace, ['value:task:done', 'finally']);
        } else {
          assert.deepEqual(
            record.outcome.value,
            normalizeValue(['caught', 'CapabilityError', 'task:0:boom']),
          );
          assert.deepEqual(record.guestTrace, ['caught:CapabilityError:task:0:boom', 'finally']);
        }
        return;
      }

      assert.equal(record.outcome.type, 'rejected');
      const { value } = record.outcome;
      assert.ok(isCancelledLimit(Object.assign(new JsliteError('Limit', value.value.message), { kind: 'Limit' })) || value.value.kind?.value === 'Limit');
      assert.match(value.value.message, /execution cancelled/);
      assert.deepEqual(record.guestTrace, []);
    });
  }
});
