const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const { Jslite } = require('../../index.ts');
const manifest = require('./manifest.js');
const { FEATURE_CONTRACT, OUTCOME } = require('../node/conformance-contract.js');
const { normalizeValue, runJslite, runNode } = require('../node/runtime-oracle.js');

function fixtureSource(file) {
  return fs.readFileSync(path.join(__dirname, file), 'utf8');
}

test('test262 manifest keeps pass and unsupported fixture buckets explicit', () => {
  const contractById = new Map(FEATURE_CONTRACT.map((entry) => [entry.id, entry]));

  for (const entry of manifest.pass) {
    assert.match(entry.file, /^cases\/pass\//, `pass fixture ${entry.id} must live under cases/pass`);
    if (entry.contractId !== undefined) {
      const contract = contractById.get(entry.contractId);
      assert.ok(contract, `missing conformance contract entry for ${entry.contractId}`);
      assert.equal(
        contract.outcome,
        OUTCOME.NODE_PARITY,
        `pass fixture ${entry.id} must reference a Node-parity contract entry`,
      );
    }
  }

  for (const entry of manifest.unsupported) {
    assert.match(
      entry.file,
      /^cases\/unsupported\//,
      `unsupported fixture ${entry.id} must live under cases/unsupported`,
    );
  }
});

for (const entry of manifest.pass) {
  test(`test262 pass fixture ${entry.id}`, async () => {
    const source = fixtureSource(entry.file);
    const [actual, nodeValue] = await Promise.all([
      runJslite(source),
      Promise.resolve(runNode(source)),
    ]);
    assert.deepEqual(normalizeValue(actual), normalizeValue(entry.expected));
    assert.deepEqual(normalizeValue(actual), normalizeValue(nodeValue));
  });
}

for (const entry of manifest.unsupported) {
  test(`test262 unsupported fixture ${entry.id}`, () => {
    assert.match(entry.reason, /\S/);
    const source = fixtureSource(entry.file);
    assert.throws(
      () => new Jslite(source),
      (error) =>
        error &&
        error.kind === entry.errorKind &&
        error.message.includes(entry.messageIncludes),
    );
  });
}
