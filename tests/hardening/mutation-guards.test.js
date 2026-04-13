'use strict';

const { snapshotToken } = require('../../lib/policy.ts');
const { Mustard, Progress, assert, isMustardError, test } = require('../node/support/helpers.js');

const SNAPSHOT_KEY = Buffer.from('mutation-guards-snapshot-key');

function isBoundaryTypeError(messageIncludes) {
  return (error) => error instanceof TypeError && error.message.includes(messageIncludes);
}

function mutateHexDigit(value) {
  const replacement = value[0] === 'a' ? 'b' : 'a';
  return `${replacement}${value.slice(1)}`;
}

test('mutation guards keep validator rejection conditions fail-closed', async () => {
  const baseline = new Mustard('const value = 1; value + 1;');
  assert.equal(await baseline.run(), 2);

  const cases = [
    {
      label: 'var binding',
      source: 'var value = 1; value;',
      message: 'only let and const are supported',
    },
    {
      label: 'default parameter',
      source: 'function wrap(value = 1) { return value; }',
      message: 'default parameters are not supported in v1',
    },
    {
      label: 'class declaration',
      source: 'class Box {}',
      message: 'classes are not supported in v1',
    },
    {
      label: 'delete operator',
      source: 'const value = { prop: 1 }; delete value.prop;',
      message: 'delete is not supported in v1',
    },
  ];

  for (const entry of cases) {
    assert.throws(
      () => new Mustard(entry.source),
      isMustardError({
        kind: 'Validation',
        message: entry.message,
      }),
      entry.label,
    );
  }
});

test('mutation guards keep snapshot authorization and replay protections fail-closed', () => {
  const progress = new Mustard('const value = fetch_data(4); value + 1;').start({
    capabilities: {
      fetch_data() {},
    },
    snapshotKey: SNAPSHOT_KEY,
  });

  assert.ok(progress instanceof Progress);
  const dumped = progress.dump();

  assert.throws(
    () =>
      Progress.load(
        {
          ...dumped,
          token: mutateHexDigit(dumped.token),
        },
        {
          capabilities: {
            fetch_data() {},
          },
          limits: {},
          snapshotKey: SNAPSHOT_KEY,
        },
      ),
    isMustardError({
      kind: 'Serialization',
      message: 'tampered or unauthenticated snapshot',
    }),
  );

  const mutatedSnapshot = Buffer.from(dumped.snapshot);
  mutatedSnapshot[mutatedSnapshot.length - 1] ^= 0x01;
  assert.throws(
    () =>
      Progress.load(
        {
          ...dumped,
          snapshot: mutatedSnapshot,
          token: snapshotToken(mutatedSnapshot, SNAPSHOT_KEY),
        },
        {
          capabilities: {
            fetch_data() {},
          },
          limits: {},
          snapshotKey: SNAPSHOT_KEY,
        },
      ),
    isMustardError({
      kind: 'Serialization',
    }),
  );

  const first = Progress.load(dumped);
  const second = Progress.load(dumped);
  assert.equal(first.resume(4), 5);
  assert.throws(
    () => second.resume(4),
    isMustardError({
      kind: 'Runtime',
      message: 'single-use',
    }),
  );
});

test('mutation guards keep limit comparisons explicit at tight versus relaxed thresholds', async () => {
  const cases = [
    {
      label: 'instruction budget',
      source: `
        let total = 0;
        for (let index = 0; index < 80; index += 1) {
          total += index;
        }
        total;
      `,
      relaxed: { instructionBudget: 10_000 },
      tight: { instructionBudget: 8 },
      expected: 3160,
      error: /instruction budget exhausted/,
    },
    {
      label: 'call depth',
      source: `
        function recurse(value) {
          if (value === 0) {
            return 0;
          }
          return recurse(value - 1) + 1;
        }
        recurse(4);
      `,
      relaxed: { callDepthLimit: 16 },
      tight: { callDepthLimit: 3 },
      expected: 4,
      error: /call depth limit exceeded/,
    },
    {
      label: 'heap bytes',
      source: '1;',
      relaxed: { heapLimitBytes: 32 * 1_024 },
      tight: { heapLimitBytes: 1 },
      expected: 1,
      error: /heap limit exceeded/,
    },
  ];

  for (const entry of cases) {
    const runtime = new Mustard(entry.source);
    assert.equal(await runtime.run({ limits: entry.relaxed }), entry.expected, entry.label);
    await assert.rejects(
      () => runtime.run({ limits: entry.tight }),
      isMustardError({
        kind: 'Limit',
        message: entry.error,
      }),
      entry.label,
    );
  }
});

test('mutation guards keep structured boundary rejections fail-closed across inputs and resumes', async () => {
  const cases = [
    {
      label: 'function input',
      makeValue() {
        return () => 1;
      },
      message: 'Unsupported host value',
    },
    {
      label: 'symbol input',
      makeValue() {
        return Symbol('edge');
      },
      message: 'Unsupported host value',
    },
    {
      label: 'bigint input',
      makeValue() {
        return 1n;
      },
      message: 'Unsupported host value',
    },
    {
      label: 'proxy input',
      makeValue() {
        return new Proxy({ ok: true }, {});
      },
      message: 'Proxy values cannot cross',
    },
    {
      label: 'custom prototype input',
      makeValue() {
        return Object.create({ inherited: true });
      },
      message: 'only plain objects and arrays can cross the host boundary',
    },
    {
      label: 'accessor input',
      makeValue() {
        const value = {};
        Object.defineProperty(value, 'secret', {
          enumerable: true,
          get() {
            return 1;
          },
        });
        return value;
      },
      message: 'accessors cannot cross',
    },
    {
      label: 'cyclic input',
      makeValue() {
        const value = {};
        value.self = value;
        return value;
      },
      message: 'cyclic values cannot cross the host boundary',
    },
  ];

  for (const entry of cases) {
    const value = entry.makeValue();
    await assert.rejects(
      () => new Mustard('value;').run({ inputs: { value } }),
      isBoundaryTypeError(entry.message),
      entry.label,
    );

    const progress = new Mustard('fetch_data(1);').start({
      capabilities: {
        fetch_data() {},
      },
    });
    assert.ok(progress instanceof Progress, entry.label);
    assert.throws(
      () => progress.resume(entry.makeValue()),
      isBoundaryTypeError(entry.message),
      entry.label,
    );
  }
});
