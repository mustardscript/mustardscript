'use strict';

const { assert, isMustardError, runtime, test } = require('./support/helpers.js');

test('run executes throw, try/catch, finally, and Error constructors', async () => {
  const result = await runtime(`
    let log = [];
    try {
      log[log.length] = 'body';
      throw new TypeError('boom');
    } catch (error) {
      log[log.length] = error.name;
      log[log.length] = error.message;
    } finally {
      log[log.length] = 'finally';
    }
    log;
  `).run();

  assert.deepEqual(result, ['body', 'TypeError', 'boom', 'finally']);
});

test('run catches runtime failures as guest-visible errors', async () => {
  const result = await runtime(`
    let captured;
    try {
      const value = null;
      value.answer;
    } catch (error) {
      captured = [error.name, error.message];
    }
    captured;
  `).run();

  assert.deepEqual(result, [
    'TypeError',
    'cannot read properties of nullish value',
  ]);
});

test('run catches resumed host capability errors inside guest try/catch', async () => {
  const result = await runtime(`
    let captured;
    try {
      fetch_data(1);
    } catch (error) {
      captured = [error.name, error.message, error.code, error.details.status];
    }
    captured;
  `).run({
    capabilities: {
      async fetch_data() {
        const error = new Error('upstream failed');
        error.name = 'CapabilityError';
        error.code = 'E_UPSTREAM';
        error.details = { status: 503 };
        throw error;
      },
    },
  });

  assert.deepEqual(result, [
    'CapabilityError',
    'upstream failed',
    'E_UPSTREAM',
    503,
  ]);
});

test('run matches supported Error constructor message, cause, and fail-closed option semantics', async () => {
  const result = await runtime(`
    const empty = new Error(undefined);
    const caused = new Error('boom', { cause: 1 });
    let unsupported;
    try {
      new Error('boom', 1);
    } catch (error) {
      unsupported = [error.name, error.message];
    }
    ({
      emptyMessage: empty.message,
      causedMessage: caused.message,
      causedValue: caused.cause,
      causedCtor: caused.constructor === Error,
      unsupported,
    });
  `).run();

  assert.deepEqual(result, {
    emptyMessage: '',
    causedMessage: 'boom',
    causedValue: 1,
    causedCtor: true,
    unsupported: [
      'TypeError',
      'Error options must be an object in the supported surface',
    ],
  });
});

test('finally runs for return, break, and continue completions', async () => {
  const result = await runtime(`
    let events = [];
    function earlyReturn() {
      try {
        return 'body';
      } finally {
        events[events.length] = 'return';
      }
    }
    let index = 0;
    while (index < 2) {
      index += 1;
      try {
        if (index === 1) {
          continue;
        }
        break;
      } finally {
        events[events.length] = index;
      }
    }
    [earlyReturn(), events];
  `).run();

  assert.deepEqual(result, ['body', [1, 2, 'return']]);
});

test('nested exception unwinds preserve catch and finally ordering', async () => {
  const result = await runtime(`
    let events = [];
    function nested() {
      try {
        try {
          events[events.length] = 'inner-body';
          throw new Error('boom');
        } catch (error) {
          events[events.length] = error.message;
          throw new TypeError('wrapped');
        } finally {
          events[events.length] = 'inner-finally';
        }
      } catch (error) {
        events[events.length] = error.name;
      } finally {
        events[events.length] = 'outer-finally';
      }
      return events;
    }
    nested();
  `).run();

  assert.deepEqual(result, [
    'inner-body',
    'boom',
    'inner-finally',
    'TypeError',
    'outer-finally',
  ]);
});

test('constructor converts native parse failures into typed errors', () => {
  assert.throws(
    () => runtime('const value = ;'),
    isMustardError({
      kind: 'Parse',
      check(error) {
        assert.ok(error.message.length > 0);
      },
      guestSafe: true,
    }),
  );
});

test('constructor converts native validation failures into typed errors', () => {
  assert.throws(
    () => runtime('export const value = 1;'),
    isMustardError({
      kind: 'Validation',
      message: /module syntax is not supported/,
    }),
  );
});

test('constructor rejects free arguments while allowing supported parameter defaults', () => {
  assert.throws(
    () => runtime('function wrap() { return arguments[0]; }'),
    isMustardError({
      kind: 'Validation',
      message: /forbidden ambient global `arguments`/,
    }),
  );
});

test('logical assignment executes with short-circuit semantics', async () => {
  const result = await runtime(`
    let orValue = 0;
    let andValue = 1;
    let keptOr = 3;
    let keptAnd = 0;
    [orValue ||= 2, andValue &&= 4, keptOr ||= 9, keptAnd &&= 8, orValue, andValue, keptOr, keptAnd];
  `).run();

  assert.deepEqual(result, [2, 4, 3, 0, 2, 4, 3, 0]);
});

test('runtime errors do not leak host internals in guest tracebacks', async () => {
  await assert.rejects(
    runtime(`
      function outer() {
        const value = null;
        return value.answer;
      }
      outer();
    `).run(),
    isMustardError({
      kind: 'Runtime',
      guestSafe: true,
    }),
  );
});
