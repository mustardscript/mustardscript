const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, JsliteError, Progress } = require('../../index.js');

function assertGuestSafeMessage(message) {
  assert.ok(!message.includes(process.cwd()));
  assert.ok(!message.includes('crates/jslite'));
  assert.ok(!message.includes('.rs'));
}

test('run executes sync programs', async () => {
  const j = new Jslite(`
    const values = [1, 2, 3];
    values[0] + values[2];
  `);

  const result = await j.run();
  assert.equal(result, 4);
});

test('run exposes structured inputs with preserved numeric edge cases', async () => {
  const j = new Jslite(`
    ({ value, inf, negZero, nan });
  `);

  const result = await j.run({
    inputs: {
      value: 7,
      inf: Infinity,
      negZero: -0,
      nan: NaN,
    },
  });

  assert.equal(result.value, 7);
  assert.equal(result.inf, Infinity);
  assert.ok(Object.is(result.negZero, -0));
  assert.ok(Number.isNaN(result.nan));
});

test('run drives host capabilities', async () => {
  const j = new Jslite(`
    const response = fetch_data(9);
    response + 1;
  `);

  const result = await j.run({
    capabilities: {
      fetch_data(value) {
        return value;
      },
    },
  });

  assert.equal(result, 10);
});

test('run awaits async host capabilities', async () => {
  const j = new Jslite(`
    const response = fetch_data(5);
    response * 3;
  `);

  const result = await j.run({
    capabilities: {
      async fetch_data(value) {
        return Promise.resolve(value);
      },
    },
  });

  assert.equal(result, 15);
});

test('run routes deterministic console callbacks and ignores host return values', async () => {
  const events = [];
  const j = new Jslite(`
    const first = console.log('alpha', 1);
    const second = console.warn({ ok: true });
    const third = console.error('omega');
    [first, second, third];
  `);

  const result = await j.run({
    console: {
      log(...args) {
        events.push(['log', args]);
        return 'ignored';
      },
      warn(...args) {
        events.push(['warn', args]);
        return 42;
      },
      error(...args) {
        events.push(['error', args]);
        return { ignored: true };
      },
    },
  });

  assert.deepEqual(events, [
    ['log', ['alpha', 1]],
    ['warn', [{ ok: true }]],
    ['error', ['omega']],
  ]);
  assert.deepEqual(result, [undefined, undefined, undefined]);
});

test('start exposes console callbacks as suspension points with undefined guest results', () => {
  const j = new Jslite(`
    const logged = console.log('alpha');
    logged === undefined ? 2 : 0;
  `);

  const progress = j.start({
    console: {
      log() {},
    },
  });

  assert.ok(progress instanceof Progress);
  assert.equal(progress.capability, 'console.log');
  assert.deepEqual(progress.args, ['alpha']);
  assert.equal(progress.resume('ignored by guest semantics'), 2);
});

test('console methods fail guest-safely when callbacks are not registered', async () => {
  const j = new Jslite(`
    console.log('alpha');
  `);

  await assert.rejects(
    j.run(),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteRuntimeError' &&
      error.kind === 'Runtime' &&
      /value is not callable/.test(error.message),
  );
});

test('run surfaces sanitized host capability errors', async () => {
  const j = new Jslite(`
    fetch_data(1);
  `);

  await assert.rejects(
    j.run({
      capabilities: {
        fetch_data() {
          const error = new Error('upstream failed');
          error.name = 'CapabilityError';
          error.code = 'E_UPSTREAM';
          error.details = { retriable: false };
          throw error;
        },
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteRuntimeError' &&
      error.kind === 'Runtime' &&
      /CapabilityError: upstream failed \[code=E_UPSTREAM\]/.test(error.message),
  );
});

test('run executes throw, try/catch, finally, and Error constructors', async () => {
  const j = new Jslite(`
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
  `);

  const result = await j.run();
  assert.deepEqual(result, ['body', 'TypeError', 'boom', 'finally']);
});

test('run catches runtime failures as guest-visible errors', async () => {
  const j = new Jslite(`
    let captured;
    try {
      const value = null;
      value.answer;
    } catch (error) {
      captured = [error.name, error.message];
    }
    captured;
  `);

  const result = await j.run();
  assert.deepEqual(result, [
    'TypeError',
    'cannot read properties of nullish value',
  ]);
});

test('run catches resumed host capability errors inside guest try/catch', async () => {
  const j = new Jslite(`
    let captured;
    try {
      fetch_data(1);
    } catch (error) {
      captured = [error.name, error.message, error.code, error.details.status];
    }
    captured;
  `);

  const result = await j.run({
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

test('finally runs for return, break, and continue completions', async () => {
  const j = new Jslite(`
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
  `);

  const result = await j.run();
  assert.deepEqual(result, ['body', [1, 2, 'return']]);
});

test('nested exception unwinds preserve catch and finally ordering', async () => {
  const j = new Jslite(`
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
  `);

  const result = await j.run();
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
    () => new Jslite('const value = ;'),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteParseError' &&
      error.kind === 'Parse' &&
      error.message.length > 0 &&
      (assertGuestSafeMessage(error.message), true),
  );
});

test('constructor converts native validation failures into typed errors', () => {
  assert.throws(
    () => new Jslite('export const value = 1;'),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteValidationError' &&
      error.kind === 'Validation' &&
      /module syntax is not supported/.test(error.message),
  );
});

test('capability calls reject guest functions across the host boundary', async () => {
  const j = new Jslite(`
    fetch_data(() => 1);
  `);

  await assert.rejects(
    j.run({
      capabilities: {
        fetch_data() {
          return 1;
        },
      },
    }),
    /functions cannot cross the structured host boundary/,
  );
});

test('run surfaces limit failures as typed errors', async () => {
  const j = new Jslite('while (true) {}');
  await assert.rejects(
    j.run({
      limits: {
        instructionBudget: 100,
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteLimitError' &&
      error.kind === 'Limit' &&
      /instruction budget exhausted/.test(error.message),
  );
});

test('run surfaces heap and allocation limit failures as typed errors', async () => {
  const j = new Jslite('1;');

  await assert.rejects(
    j.run({
      limits: {
        heapLimitBytes: 1,
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteLimitError' &&
      error.kind === 'Limit' &&
      /heap limit exceeded/.test(error.message),
  );

  await assert.rejects(
    j.run({
      limits: {
        allocationBudget: 1,
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteLimitError' &&
      error.kind === 'Limit' &&
      /allocation budget exhausted/.test(error.message),
  );
});

test('start returns resumable progress objects', () => {
  const j = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `);

  const progress = j.start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  assert.equal(progress.capability, 'fetch_data');
  assert.deepEqual(progress.args, [4]);

  const finalValue = progress.resume(4);
  assert.equal(finalValue, 8);
});

test('progress objects are single-use', () => {
  const j = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `);

  const progress = j.start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.equal(progress.resume(4), 8);
  assert.throws(
    () => progress.resume(4),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteRuntimeError' &&
      error.kind === 'Runtime' &&
      /single-use/.test(error.message),
  );
});

test('progress dump and load preserve suspended execution state', () => {
  const j = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `);

  const progress = j.start({
    capabilities: {
      fetch_data() {},
    },
  });

  const restored = Progress.load(progress.dump());
  assert.ok(restored instanceof Progress);
  assert.equal(restored.capability, 'fetch_data');
  assert.deepEqual(restored.args, [4]);
  assert.equal(restored.resume(4), 8);
});

test('start snapshots guest state before async host futures exist', () => {
  let calls = 0;
  const j = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `);

  const progress = j.start({
    capabilities: {
      async fetch_data() {
        calls += 1;
        return 4;
      },
    },
  });

  assert.equal(calls, 0);
  const dumped = progress.dump();
  const restored = Progress.load(dumped);
  assert.equal(restored.resume(4), 8);
});

test('progress.load rejects reused snapshots in the same process', () => {
  const j = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `);

  const progress = j.start({
    capabilities: {
      fetch_data() {},
    },
  });
  const dumped = progress.dump();
  assert.equal(progress.resume(4), 8);

  const restored = Progress.load(dumped);
  assert.throws(
    () => restored.resume(4),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteRuntimeError' &&
      error.kind === 'Runtime' &&
      /single-use/.test(error.message),
  );
});

test('progress resume surfaces snapshot failures as typed errors', () => {
  const restored = Progress.load({
    capability: 'fetch_data',
    args: [],
    snapshot: Buffer.from('not-a-valid-snapshot'),
  });
  assert.throws(
    () => restored.resume(1),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteSerializationError' &&
      error.kind === 'Serialization',
  );
});

test('runtime errors do not leak host internals in guest tracebacks', async () => {
  const j = new Jslite(`
    function outer() {
      const value = null;
      return value.answer;
    }
    outer();
  `);

  await assert.rejects(
    j.run(),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteRuntimeError' &&
      error.kind === 'Runtime' &&
      (assertGuestSafeMessage(error.message), true),
  );
});

test('limit errors do not leak host internals', async () => {
  const runaway = new Jslite('while (true) {}');
  await assert.rejects(
    runaway.run({
      limits: {
        instructionBudget: 100,
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteLimitError' &&
      error.kind === 'Limit' &&
      (assertGuestSafeMessage(error.message), true),
  );

  const tinyHeap = new Jslite('1;');
  await assert.rejects(
    tinyHeap.run({
      limits: {
        heapLimitBytes: 1,
      },
    }),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteLimitError' &&
      error.kind === 'Limit' &&
      (assertGuestSafeMessage(error.message), true),
  );
});

test('serialization errors do not leak host internals', () => {
  const restored = Progress.load({
    capability: 'fetch_data',
    args: [],
    snapshot: Buffer.from('not-a-valid-snapshot'),
  });

  assert.throws(
    () => restored.resume(1),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteSerializationError' &&
      error.kind === 'Serialization' &&
      (assertGuestSafeMessage(error.message), true),
  );
});

test('dump and load preserve compiled programs', async () => {
  const j = new Jslite('Math.max(1, 8, 2);');
  const copy = Jslite.load(j.dump());
  const result = await copy.run();
  assert.equal(result, 8);
});
