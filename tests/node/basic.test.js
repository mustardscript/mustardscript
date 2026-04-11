const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, JsliteError, Progress } = require('../../index.js');

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

test('constructor converts native parse failures into typed errors', () => {
  assert.throws(
    () => new Jslite('const value = ;'),
    (error) =>
      error instanceof JsliteError &&
      error.name === 'JsliteParseError' &&
      error.kind === 'Parse' &&
      error.message.length > 0,
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

test('dump and load preserve compiled programs', async () => {
  const j = new Jslite('Math.max(1, 8, 2);');
  const copy = Jslite.load(j.dump());
  const result = await copy.run();
  assert.equal(result, 8);
});
