'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, JsliteError, Progress } = require('../../index.js');
const {
  PROPERTY_RUNS,
  fc,
  progressActionArbitrary,
  structuredValueArbitrary,
  unsupportedHostValueCaseArbitrary,
} = require('./property-generators.js');
const { normalizeValue } = require('./runtime-oracle.js');

function performProgressAction(progress, action) {
  if (action === 'resume') {
    return progress.resume(4);
  }
  if (action === 'resumeError') {
    return progress.resumeError(new Error('boom'));
  }
  return progress.cancel();
}

function isSingleUseRuntimeError(error) {
  return (
    error instanceof JsliteError &&
    error.kind === 'Runtime' &&
    error.message.includes('single-use')
  );
}

test('property: supported structured host values round-trip across inputs, capabilities, and results', async () => {
  await fc.assert(
    fc.asyncProperty(structuredValueArbitrary, async (value) => {
      const runtime = new Jslite(`
        const echoed = echo(value);
        ({ value, echoed });
      `);

      let seenArg;
      const result = await runtime.run({
        inputs: { value },
        capabilities: {
          echo(entry) {
            seenArg = entry;
            return entry;
          },
        },
      });

      assert.deepEqual(normalizeValue(seenArg), normalizeValue(value));
      assert.deepEqual(normalizeValue(result), normalizeValue({ value, echoed: value }));
    }),
    {
      numRuns: PROPERTY_RUNS,
      interruptAfterTimeLimit: 20_000,
    },
  );
});

test('property: unsupported host values fail closed before crossing the boundary', async () => {
  await fc.assert(
    fc.asyncProperty(unsupportedHostValueCaseArbitrary, async ({ value, messageIncludes }) => {
      const runtime = new Jslite('value;');
      await assert.rejects(
        runtime.run({ inputs: { value } }),
        (error) => error instanceof TypeError && error.message.includes(messageIncludes),
      );
    }),
    {
      numRuns: PROPERTY_RUNS,
    },
  );
});

test('property: Progress wrappers remain single-use after any completion path', async () => {
  await fc.assert(
    fc.property(progressActionArbitrary, progressActionArbitrary, (firstAction, secondAction) => {
      const runtime = new Jslite('fetch_data(4);');
      const progress = runtime.start({
        capabilities: {
          fetch_data() {},
        },
      });

      assert.ok(progress instanceof Progress);

      try {
        performProgressAction(progress, firstAction);
      } catch {
        // Any first completion path should still consume the suspended snapshot.
      }

      assert.throws(
        () => performProgressAction(progress, secondAction),
        isSingleUseRuntimeError,
      );

      const dumped = progress.dump();
      assert.throws(
        () => Progress.load(dumped),
        isSingleUseRuntimeError,
      );
    }),
    {
      numRuns: PROPERTY_RUNS,
    },
  );
});
