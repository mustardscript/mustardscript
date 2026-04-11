'use strict';

const assert = require('node:assert/strict');
const vm = require('node:vm');

const { Jslite, JsliteError } = require('../../index.js');

async function runJslite(source) {
  const runtime = new Jslite(source);
  return runtime.run();
}

function normalizeNumber(value) {
  if (Number.isNaN(value)) {
    return { type: 'number', value: 'NaN' };
  }
  if (Object.is(value, -0)) {
    return { type: 'number', value: '-0' };
  }
  if (value === Infinity) {
    return { type: 'number', value: 'Infinity' };
  }
  if (value === -Infinity) {
    return { type: 'number', value: '-Infinity' };
  }
  return { type: 'number', value };
}

function normalizeValue(value) {
  if (value === undefined) {
    return { type: 'undefined' };
  }
  if (value === null) {
    return { type: 'null' };
  }
  if (typeof value === 'boolean') {
    return { type: 'boolean', value };
  }
  if (typeof value === 'number') {
    return normalizeNumber(value);
  }
  if (typeof value === 'string') {
    return { type: 'string', value };
  }
  if (Array.isArray(value)) {
    return {
      type: 'array',
      value: Array.from(value, normalizeValue),
    };
  }
  if (value && typeof value === 'object') {
    return {
      type: 'object',
      value: Object.fromEntries(
        Object.keys(value)
          .sort()
          .map((key) => [key, normalizeValue(value[key])]),
      ),
    };
  }
  return { type: typeof value, value: String(value) };
}

function normalizeError(error) {
  if (error instanceof Error) {
    const normalized = {
      type: 'error',
      value: {
        name: error.name,
        message: error.message,
      },
    };
    if ('kind' in error && error.kind !== undefined) {
      normalized.value.kind = normalizeValue(error.kind);
    }
    if ('code' in error && error.code !== undefined) {
      normalized.value.code = normalizeValue(error.code);
    }
    if ('details' in error && error.details !== undefined) {
      normalized.value.details = normalizeValue(error.details);
    }
    return normalized;
  }
  return {
    type: 'thrown',
    value: normalizeValue(error),
  };
}

async function captureOutcome(run) {
  try {
    return {
      type: 'fulfilled',
      value: normalizeValue(await run()),
    };
  } catch (error) {
    return {
      type: 'rejected',
      value: normalizeError(error),
    };
  }
}

function runNode(source) {
  return vm.runInNewContext(`"use strict";\n${source}`, Object.create(null));
}

async function assertDifferential(source) {
  const [actual, expected] = await Promise.all([
    captureOutcome(() => runJslite(source)),
    captureOutcome(() => Promise.resolve(runNode(source))),
  ]);
  assert.deepEqual(actual, expected);
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
  captureOutcome,
  normalizeValue,
  runJslite,
  runNode,
};
