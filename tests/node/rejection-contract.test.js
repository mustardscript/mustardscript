'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  CURATED_REJECTION_REGRESSION_CASES,
  DIAGNOSTIC_CATEGORY,
  REJECT_PHASE,
} = require('./conformance-contract.js');
const { assertContractReject } = require('./runtime-oracle.js');

test('curated rejection regressions span both phases and every fail-closed category in use', () => {
  const phases = new Set(CURATED_REJECTION_REGRESSION_CASES.map((entry) => entry.phase));
  assert.deepEqual([...phases].sort(), [...Object.values(REJECT_PHASE)].sort());

  const categories = new Set(CURATED_REJECTION_REGRESSION_CASES.map((entry) => entry.category));
  assert.deepEqual(
    [...categories].sort(),
    [
      DIAGNOSTIC_CATEGORY.AMBIENT_GLOBAL,
      DIAGNOSTIC_CATEGORY.UNSUPPORTED_BINDING,
      DIAGNOSTIC_CATEGORY.UNSUPPORTED_GLOBAL_BUILTIN,
      DIAGNOSTIC_CATEGORY.UNSUPPORTED_OPERATOR,
      DIAGNOSTIC_CATEGORY.UNSUPPORTED_RUNTIME_SURFACE,
      DIAGNOSTIC_CATEGORY.UNSUPPORTED_SYNTAX,
    ].sort(),
  );
});

for (const entry of CURATED_REJECTION_REGRESSION_CASES) {
  test(`curated rejection regression ${entry.id} stays fail-closed at ${entry.phase}`, async () => {
    await assertContractReject(entry.source, entry);
  });
}
