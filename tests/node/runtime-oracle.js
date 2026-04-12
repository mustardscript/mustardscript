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
      value: Array.from({ length: value.length }, (_, index) =>
        index in value ? normalizeValue(value[index]) : { type: 'hole' }
      ),
    };
  }
  if (value && typeof value === 'object') {
    return {
      type: 'object',
      value: Object.fromEntries(
        Object.keys(value).map((key) => [key, normalizeValue(value[key])]),
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

function normalizeTraceEvent(event) {
  if ('error' in event) {
    return {
      type: event.type,
      name: event.name,
      phase: event.phase,
      error: normalizeError(event.error),
    };
  }

  if ('value' in event) {
    return {
      type: event.type,
      name: event.name,
      phase: event.phase,
      value: normalizeValue(event.value),
    };
  }

  return {
    type: event.type,
    name: event.name,
    phase: event.phase,
    args: (event.args ?? []).map(normalizeValue),
  };
}

function stripGuestSpanText(text) {
  return typeof text === 'string' ? text.replace(/\[\d+\.\.\d+\]/g, '[span]') : text;
}

function normalizeMetamorphicTraceRecord(record) {
  if (
    record.outcome.type === 'rejected' &&
    record.outcome.value.type === 'error' &&
    typeof record.outcome.value.value.message === 'string'
  ) {
    return {
      ...record,
      outcome: {
        ...record.outcome,
        value: {
          ...record.outcome.value,
          value: {
            ...record.outcome.value.value,
            message: stripGuestSpanText(record.outcome.value.value.message),
          },
        },
      },
    };
  }

  return record;
}

function wrapTraceCallable(events, type, name, impl, options = {}) {
  const { returnsUndefined = false } = options;

  return (...args) => {
    events.push({
      type,
      name,
      phase: 'call',
      args,
    });

    try {
      const result = impl(...args);
      if (result && typeof result.then === 'function') {
        return result.then(
          (value) => {
            events.push({
              type,
              name,
              phase: 'return',
              value,
            });
            return returnsUndefined ? undefined : value;
          },
          (error) => {
            events.push({
              type,
              name,
              phase: 'throw',
              error,
            });
            throw error;
          },
        );
      }

      events.push({
        type,
        name,
        phase: 'return',
        value: result,
      });
      return returnsUndefined ? undefined : result;
    } catch (error) {
      events.push({
        type,
        name,
        phase: 'throw',
        error,
      });
      throw error;
    }
  };
}

function createTraceHarness(options = {}) {
  const events = [];
  const capabilityImpls = options.capabilities ?? {
    probe(value) {
      return value;
    },
  };

  const consoleImpls = options.console ?? {};
  const console = {};
  for (const method of ['log', 'warn', 'error']) {
    console[method] = wrapTraceCallable(
      events,
      'console',
      method,
      consoleImpls[method] ?? (() => undefined),
      { returnsUndefined: true },
    );
  }

  const capabilities = Object.fromEntries(
    Object.entries(capabilityImpls).map(([name, impl]) => [
      name,
      wrapTraceCallable(events, 'capability', name, impl),
    ]),
  );

  return {
    events,
    jsliteOptions: {
      capabilities,
      console,
      inputs: options.inputs,
    },
    nodeContext: {
      console,
      ...capabilities,
      ...(options.inputs ?? {}),
    },
  };
}

async function captureTraceOutcome(run, events) {
  return {
    outcome: await captureOutcome(run),
    trace: events.map(normalizeTraceEvent),
  };
}

async function runJsliteWithTrace(source, options = {}) {
  const runtime = new Jslite(source);
  const harness = createTraceHarness(options);
  return captureTraceOutcome(() => runtime.run(harness.jsliteOptions), harness.events);
}

async function runNodeWithTrace(source, options = {}) {
  const harness = createTraceHarness(options);
  return captureTraceOutcome(
    () => Promise.resolve(vm.runInNewContext(source, harness.nodeContext)),
    harness.events,
  );
}

async function assertDifferential(source) {
  const [actual, expected] = await Promise.all([
    captureOutcome(() => runJslite(source)),
    captureOutcome(() => Promise.resolve(runNode(source))),
  ]);
  assert.deepEqual(actual, expected);
}

async function assertTraceDifferential(source, options) {
  const [actual, expected] = await Promise.all([
    runJsliteWithTrace(source, options),
    runNodeWithTrace(source, options),
  ]);
  assert.deepEqual(actual, expected);
}

function isValidationError(error, messageIncludes) {
  return (
    error instanceof JsliteError &&
    error.kind === 'Validation' &&
    (messageIncludes === undefined || error.message.includes(messageIncludes))
  );
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

async function assertMatchesNodeOrValidation(source, { messageIncludes } = {}) {
  try {
    new Jslite(source);
  } catch (error) {
    assert.ok(isValidationError(error, messageIncludes));
    return;
  }

  await assertDifferential(source);
}

async function assertMetamorphicDifferential(originalSource, rewrittenSource, options) {
  const [nodeOriginal, nodeRewritten, jsliteOriginal, jsliteRewritten] = await Promise.all([
    runNodeWithTrace(originalSource, options),
    runNodeWithTrace(rewrittenSource, options),
    runJsliteWithTrace(originalSource, options),
    runJsliteWithTrace(rewrittenSource, options),
  ]);

  assert.deepEqual(nodeOriginal, nodeRewritten);
  assert.deepEqual(
    normalizeMetamorphicTraceRecord(jsliteOriginal),
    normalizeMetamorphicTraceRecord(jsliteRewritten),
  );
  assert.deepEqual(jsliteOriginal, nodeOriginal);
  assert.deepEqual(jsliteRewritten, nodeRewritten);
}

module.exports = {
  assertDifferential,
  assertMetamorphicDifferential,
  assertMatchesNodeOrValidation,
  assertTraceDifferential,
  assertJsliteFailure,
  captureOutcome,
  captureTraceOutcome,
  isValidationError,
  normalizeValue,
  runJslite,
  runJsliteWithTrace,
  runNode,
  runNodeWithTrace,
};
