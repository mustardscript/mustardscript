const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const { Jslite } = require('../../index.js');
const manifest = require('./manifest.js');
const { normalizeValue, runJslite, runNode } = require('../node/runtime-oracle.js');

function fixtureSource(file) {
  return fs.readFileSync(path.join(__dirname, file), 'utf8');
}

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
