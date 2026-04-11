'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, JsliteError } = require('../../index.js');
const {
  PROPERTY_RUNS,
  fc,
  supportedProgramArbitrary,
  unsupportedValidationCaseArbitrary,
} = require('./property-generators.js');
const { assertDifferential } = require('./runtime-oracle.js');

test('property: bounded supported generated programs match Node canonically', async () => {
  await fc.assert(
    fc.asyncProperty(supportedProgramArbitrary, async (source) => {
      await assertDifferential(source);
    }),
    {
      numRuns: PROPERTY_RUNS,
      interruptAfterTimeLimit: 20_000,
    },
  );
});

test('property: documented unsupported generated forms fail with constructor-time validation', async () => {
  await fc.assert(
    fc.property(unsupportedValidationCaseArbitrary, ({ source, messageIncludes }) => {
      assert.throws(
        () => new Jslite(source),
        (error) =>
          error instanceof JsliteError &&
          error.kind === 'Validation' &&
          error.message.toLowerCase().includes(messageIncludes.toLowerCase()),
      );
    }),
    {
      numRuns: PROPERTY_RUNS,
    },
  );
});
