'use strict';

const assert = require('node:assert/strict');
const vm = require('node:vm');

const { Jslite, JsliteError } = require('../../index.js');

async function runJslite(source) {
  const runtime = new Jslite(source);
  return runtime.run();
}

function normalizeValue(value) {
  if (Array.isArray(value)) {
    return Array.from(value, normalizeValue);
  }
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).map(([key, entry]) => [key, normalizeValue(entry)]),
    );
  }
  return value;
}

function runNode(source) {
  return vm.runInNewContext(`"use strict";\n${source}`, Object.create(null));
}

async function assertDifferential(source) {
  const [actual, expected] = await Promise.all([
    runJslite(source),
    Promise.resolve(runNode(source)),
  ]);
  assert.deepEqual(normalizeValue(actual), normalizeValue(expected));
}

function assertJsliteFailure(source, { kind, messageIncludes }) {
  assert.throws(
    () => new Jslite(source),
    (error) =>
      error instanceof JsliteError &&
      error.kind === kind &&
      error.message.includes(messageIncludes),
  );
}

module.exports = {
  assertDifferential,
  assertJsliteFailure,
  normalizeValue,
  runJslite,
  runNode,
};
