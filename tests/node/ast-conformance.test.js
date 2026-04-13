'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fc = require('fast-check');

const {
  AST_PROPERTY_RUNS,
  astProgramArbitrary,
  astTraceProgramArbitrary,
  contractCoverageExpectations,
  coveredFeatureIds,
  enumerateExhaustivePrograms,
  metamorphicVariants,
  renderProgram,
} = require('./ast-conformance.js');
const { FEATURE_CONTRACT, OUTCOME } = require('./conformance-contract.js');
const {
  assertMetamorphicDifferential,
  assertTraceDifferential,
  runMustardWithLoadedProgramTrace,
  runMustardWithProgressTrace,
  runMustardWithTrace,
  runMustardWithSerializedProgressTrace,
  runNodeWithTrace,
} = require('./runtime-oracle.js');

test('machine-readable conformance contract stays internally consistent', () => {
  const ids = FEATURE_CONTRACT.map((entry) => entry.id);
  assert.equal(new Set(ids).size, ids.length);

  const validationEntries = FEATURE_CONTRACT.filter((entry) => entry.outcome === OUTCOME.VALIDATION_REJECT);
  assert.ok(validationEntries.length > 0);
  for (const entry of validationEntries) {
    assert.match(entry.source, /\S/);
    assert.match(entry.messageIncludes, /\S/);
    assert.match(entry.phase, /\S/);
    assert.match(entry.category, /\S/);
  }

  const runtimeRejectEntries = FEATURE_CONTRACT.filter((entry) => entry.outcome === OUTCOME.RUNTIME_REJECT);
  assert.ok(runtimeRejectEntries.length > 0);
  for (const entry of runtimeRejectEntries) {
    assert.match(entry.source, /\S/);
    assert.match(entry.messageIncludes, /\S/);
    assert.match(entry.phase, /\S/);
    assert.match(entry.category, /\S/);
  }

  const divergenceEntries = FEATURE_CONTRACT.filter((entry) => entry.outcome === OUTCOME.KNOWN_DIVERGENCE);
  for (const entry of divergenceEntries) {
    assert.match(entry.note, /\S/);
  }

  const covered = coveredFeatureIds();
  const expected = contractCoverageExpectations();
  const missing = [...expected].filter((featureId) => !covered.has(featureId)).sort();
  assert.deepEqual(missing, []);
});

for (const entry of enumerateExhaustivePrograms()) {
  test(`ast exhaustive differential ${entry.id}`, async () => {
    await assertTraceDifferential(renderProgram(entry.program));
  });
}

test('property: AST-generated programs match Node on both outcomes and traces', async () => {
  await fc.assert(
    fc.asyncProperty(astProgramArbitrary(), async (program) => {
      await assertTraceDifferential(renderProgram(program));
    }),
    {
      numRuns: AST_PROPERTY_RUNS,
      interruptAfterTimeLimit: 20_000,
    },
  );
});

test('property: AST metamorphic rewrites preserve semantics and traces', async () => {
  await fc.assert(
    fc.asyncProperty(astProgramArbitrary(), async (program) => {
      const original = renderProgram(program);
      for (const variant of metamorphicVariants(program)) {
        await assertMetamorphicDifferential(original, variant.source);
      }
    }),
    {
      numRuns: Math.max(20, Math.floor(AST_PROPERTY_RUNS / 2)),
      interruptAfterTimeLimit: 20_000,
    },
  );
});

test('property: AST-generated programs preserve traces through compiled-program round-trips', async () => {
  await fc.assert(
    fc.asyncProperty(astProgramArbitrary(), async (program) => {
      const source = renderProgram(program);
      const [direct, loaded, expected] = await Promise.all([
        runMustardWithTrace(source),
        runMustardWithLoadedProgramTrace(source),
        runNodeWithTrace(source),
      ]);
      assert.deepEqual(loaded, direct);
      assert.deepEqual(loaded, expected);
    }),
    {
      numRuns: Math.max(20, Math.floor(AST_PROPERTY_RUNS / 2)),
      interruptAfterTimeLimit: 20_000,
    },
  );
});

test('property: AST trace programs preserve semantics through explicit snapshot round-trips', async () => {
  await fc.assert(
    fc.asyncProperty(astTraceProgramArbitrary(), async (program) => {
      const source = renderProgram(program);
      const [direct, serialized, expected] = await Promise.all([
        runMustardWithProgressTrace(source),
        runMustardWithSerializedProgressTrace(source),
        runNodeWithTrace(source),
      ]);
      assert.deepEqual(serialized, direct);
      assert.deepEqual(serialized, expected);
    }),
    {
      numRuns: Math.max(20, Math.floor(AST_PROPERTY_RUNS / 2)),
      interruptAfterTimeLimit: 20_000,
    },
  );
});
