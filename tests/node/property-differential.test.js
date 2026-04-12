'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, JsliteError } = require('../../index.js');
const {
  PROPERTY_RUNS,
  conformanceCaseArbitrary,
  fc,
  supportedProgramArbitrary,
  unsupportedValidationCaseArbitrary,
} = require('./property-generators.js');
const {
  assertDifferential,
  assertMatchesNodeOrValidation,
  isValidationError,
} = require('./runtime-oracle.js');

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
        (error) => isValidationError(error, messageIncludes),
      );
    }),
    {
      numRuns: PROPERTY_RUNS,
    },
  );
});

test('property: generated conformance cases either match Node or fail in validation', async () => {
  await fc.assert(
    fc.asyncProperty(conformanceCaseArbitrary, async ({ source, messageIncludes }) => {
      await assertMatchesNodeOrValidation(source, { messageIncludes });
    }),
    {
      numRuns: Math.max(25, Math.floor(PROPERTY_RUNS / 2)),
      interruptAfterTimeLimit: 20_000,
    },
  );
});
