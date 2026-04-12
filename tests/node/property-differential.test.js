'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { Jslite, JsliteError } = require('../../index.js');
const {
  PROPERTY_RUNS,
  SUPPORTED_PARITY_FAMILIES,
  conformanceCaseArbitrary,
  fc,
  unsupportedValidationCaseArbitrary,
} = require('./property-generators.js');
const {
  assertDifferential,
  assertMatchesNodeOrValidation,
  assertProgressTraceDifferential,
  isValidationError,
} = require('./runtime-oracle.js');

async function assertSupportedFamilyEntry(family, entry) {
  if (family.mode === 'differential') {
    await assertDifferential(entry.source);
    return;
  }
  if (family.mode === 'progress-trace') {
    await assertProgressTraceDifferential(entry.source, entry.options);
    return;
  }
  throw new Error(`unsupported parity family mode: ${family.mode}`);
}

async function assertParityFamily(family) {
  const details = await fc.check(
    fc.asyncProperty(family.arbitrary, async (entry) => {
      await assertSupportedFamilyEntry(family, entry);
    }),
    {
      numRuns: family.numRuns,
      interruptAfterTimeLimit: 20_000,
    },
  );

  if (!details.failed) {
    return;
  }

  if (details.counterexample === null) {
    assert.fail(`property family ${family.id} failed without a minimized counterexample`);
  }

  const [entry] = details.counterexample;
  const sections = [
    `Property family \`${family.id}\` failed after ${details.numRuns} run(s).`,
    `{ seed: ${details.seed}, path: "${details.counterexamplePath}" }`,
    `Shrunk ${details.numShrinks} time(s).`,
  ];
  if (details.errorInstance instanceof Error) {
    sections.push(details.errorInstance.message);
  } else if (entry && typeof entry === 'object' && typeof entry.source === 'string') {
    sections.push(`Minimized program:\n${entry.source}`);
  }
  assert.fail(sections.join('\n\n'));
}

for (const family of SUPPORTED_PARITY_FAMILIES) {
  test(`property: supported parity family ${family.id} runs independently with canonical failure output`, async () => {
    await assertParityFamily(family);
  });
}

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
