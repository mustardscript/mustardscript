'use strict';

const crypto = require('node:crypto');
const { types } = require('node:util');
const { loadNative } = require('../native-loader.ts');

const { MustardError, callNative } = require('./errors.ts');
const { defineEnumerableProperty, hasOwnProperty, isAccessorDescriptor } = require('./structured.ts');

const CONSOLE_CAPABILITY_NAMES = {
  log: 'console.log',
  warn: 'console.warn',
  error: 'console.error',
};
const DEFAULT_SNAPSHOT_KEY = crypto.randomBytes(32);
let nativeSnapshotHelpers;

function snapshotNative() {
  nativeSnapshotHelpers ??= loadNative();
  return nativeSnapshotHelpers;
}

function validatePlainHandlerContainer(value, label) {
  if (value === undefined) {
    return null;
  }
  if (value === null || typeof value !== 'object' || Array.isArray(value) || types.isProxy(value)) {
    throw new TypeError(`${label} must be a plain object`);
  }
  const prototype = Object.getPrototypeOf(value);
  if (prototype !== Object.prototype && prototype !== null) {
    throw new TypeError(`${label} must be a plain object`);
  }
  return value;
}

function enumerateHandlerProperties(value, label) {
  const container = validatePlainHandlerContainer(value, label);
  if (container === null) {
    return [];
  }
  return Object.entries(Object.getOwnPropertyDescriptors(container)).filter(([, descriptor]) => {
    if (!descriptor.enumerable) {
      return false;
    }
    if (isAccessorDescriptor(descriptor)) {
      throw new TypeError(`${label} cannot define accessor properties`);
    }
    return true;
  });
}

function collectHostHandlers({ capabilities = {}, console = {} } = {}) {
  const handlers = {};
  for (const [name, descriptor] of enumerateHandlerProperties(
    capabilities,
    'options.capabilities',
  )) {
    defineEnumerableProperty(handlers, name, descriptor.value);
  }

  const consoleDescriptors = new Map(
    enumerateHandlerProperties(console, 'options.console'),
  );
  for (const [method, capabilityName] of Object.entries(CONSOLE_CAPABILITY_NAMES)) {
    const descriptor = consoleDescriptors.get(method);
    if (!descriptor) {
      continue;
    }
    const handler = descriptor.value;
    if (typeof handler !== 'function') {
      throw new TypeError(`console.${method} must be a function`);
    }
    if (handlers[capabilityName] !== undefined) {
      throw new TypeError(
        `Duplicate handler for ${capabilityName}; use either options.console or options.capabilities`,
      );
    }
    handlers[capabilityName] = handler;
  }
  return handlers;
}

function encodeRuntimeLimits(limits = {}) {
  const encodedLimits = {};
  if (limits.instructionBudget !== undefined) {
    encodedLimits.instruction_budget = limits.instructionBudget;
  }
  if (limits.heapLimitBytes !== undefined) {
    encodedLimits.heap_limit_bytes = limits.heapLimitBytes;
  }
  if (limits.allocationBudget !== undefined) {
    encodedLimits.allocation_budget = limits.allocationBudget;
  }
  if (limits.callDepthLimit !== undefined) {
    encodedLimits.call_depth_limit = limits.callDepthLimit;
  }
  if (limits.maxOutstandingHostCalls !== undefined) {
    encodedLimits.max_outstanding_host_calls = limits.maxOutstandingHostCalls;
  }
  return encodedLimits;
}

function validateRuntimeLimitsObject(limits, label) {
  if (limits === undefined || limits === null || typeof limits !== 'object') {
    throw new TypeError(`${label} must be a plain object`);
  }
  if (Array.isArray(limits) || types.isProxy(limits)) {
    throw new TypeError(`${label} must be a plain object`);
  }
  const prototype = Object.getPrototypeOf(limits);
  if (prototype !== Object.prototype && prototype !== null) {
    throw new TypeError(`${label} must be a plain object`);
  }
  return limits;
}

function cloneSnapshotPolicy(policy) {
  return {
    capabilities: policy.capabilities.slice(),
    limits: { ...policy.limits },
  };
}

function cloneSnapshotKey(snapshotKey) {
  return Buffer.from(snapshotKey);
}

function encodeSnapshotPolicy(policy, options = undefined) {
  const encoded = cloneSnapshotPolicy(policy);
  if (typeof options?.snapshotId === 'string' && options.snapshotId.length > 0) {
    encoded.snapshot_id = options.snapshotId;
  }
  if (options?.snapshotKey !== undefined) {
    const snapshotKey = cloneSnapshotKey(options.snapshotKey);
    encoded.snapshot_key_base64 = snapshotKey.toString('base64');
    encoded.snapshot_key_digest = snapshotKeyDigest(snapshotKey);
  }
  if (typeof options?.snapshotToken === 'string' && options.snapshotToken.length > 0) {
    encoded.snapshot_token = options.snapshotToken;
  }
  return JSON.stringify(encoded);
}

function normalizeSnapshotKey(snapshotKey, label) {
  if (snapshotKey === undefined) {
    return cloneSnapshotKey(DEFAULT_SNAPSHOT_KEY);
  }
  if (typeof snapshotKey === 'string') {
    return Buffer.from(snapshotKey, 'utf8');
  }
  if (Buffer.isBuffer(snapshotKey) || snapshotKey instanceof Uint8Array) {
    return Buffer.from(snapshotKey);
  }
  throw new TypeError(`${label} must be a string, Buffer, or Uint8Array`);
}

function snapshotToken(snapshot, snapshotKey, snapshotId = undefined) {
  const identity = snapshotId ?? snapshotIdentity(snapshot);
  return crypto.createHmac('sha256', snapshotKey).update(identity, 'utf8').digest('hex');
}

function snapshotIdentity(snapshot) {
  return callNative(snapshotNative().snapshotIdentity, Buffer.from(snapshot));
}

function snapshotKeyDigest(snapshotKey) {
  return crypto.createHash('sha256').update(snapshotKey).digest('hex');
}

function assertSnapshotToken(
  snapshot,
  token,
  snapshotKey,
  expectedSnapshotId = undefined,
  expectedSnapshotKeyDigest = undefined,
) {
  if (typeof token !== 'string' || token.length === 0) {
    throw new TypeError('Progress.load() requires a dumped progress token');
  }
  const actualSnapshotId = snapshotIdentity(snapshot);
  if (expectedSnapshotId !== undefined && actualSnapshotId !== expectedSnapshotId) {
    throw new MustardError(
      'Serialization',
      'Progress.load() rejected a tampered or unauthenticated snapshot',
    );
  }
  if (
    expectedSnapshotKeyDigest !== undefined &&
    snapshotKeyDigest(snapshotKey) !== expectedSnapshotKeyDigest
  ) {
    throw new MustardError(
      'Serialization',
      'Progress.load() rejected a mismatched snapshot key digest',
    );
  }
  const expected = snapshotToken(snapshot, snapshotKey, actualSnapshotId);
  if (
    token.length !== expected.length ||
    !crypto.timingSafeEqual(Buffer.from(token, 'utf8'), Buffer.from(expected, 'utf8'))
  ) {
    throw new MustardError(
      'Serialization',
      'Progress.load() rejected a tampered or unauthenticated snapshot',
    );
  }
}

function createExecutionPolicy({ limits = {}, snapshotKey, ...handlers } = {}) {
  const hostHandlers = collectHostHandlers(handlers);
  return {
    hostHandlers,
    policy: {
      capabilities: Object.keys(hostHandlers),
      limits: encodeRuntimeLimits(limits),
    },
    snapshotKey: normalizeSnapshotKey(snapshotKey, 'options.snapshotKey'),
  };
}

function resolveProgressLoadContext(state, snapshot, options) {
  const expectedSnapshotId =
    typeof state.snapshot_id === 'string' && state.snapshot_id.length > 0
      ? state.snapshot_id
      : undefined;
  const expectedSnapshotKeyDigest =
    typeof state.snapshot_key_digest === 'string' && state.snapshot_key_digest.length > 0
      ? state.snapshot_key_digest
      : undefined;
  if (expectedSnapshotId === undefined) {
    throw new TypeError('Progress.load() requires dumped snapshot_id metadata');
  }
  if (expectedSnapshotKeyDigest === undefined) {
    throw new TypeError('Progress.load() requires dumped snapshot_key_digest metadata');
  }
  if (options === undefined || options === null || typeof options !== 'object') {
    throw new TypeError(
      'Progress.load() requires explicit capabilities, limits, and snapshotKey',
    );
  }
  if (
    !hasOwnProperty(options, 'capabilities') &&
    !hasOwnProperty(options, 'console')
  ) {
    throw new TypeError(
      'Progress.load() requires explicit capabilities when restoring progress',
    );
  }
  if (!hasOwnProperty(options, 'limits')) {
    throw new TypeError(
      'Progress.load() requires explicit limits when restoring progress',
    );
  }
  const limits = validateRuntimeLimitsObject(
    options.limits,
    'Progress.load() options.limits',
  );
  if (options.snapshotKey === undefined) {
    throw new TypeError(
      'Progress.load() requires explicit snapshotKey when restoring progress',
    );
  }
  const snapshotKey = normalizeSnapshotKey(
    options.snapshotKey,
    'Progress.load() options.snapshotKey',
  );
  assertSnapshotToken(
    snapshot,
    state.token,
    snapshotKey,
    expectedSnapshotId,
    expectedSnapshotKeyDigest,
  );
  return {
    policy: createExecutionPolicy({ ...options, limits }).policy,
    snapshotKey: cloneSnapshotKey(snapshotKey),
  };
}

module.exports = {
  cloneSnapshotPolicy,
  cloneSnapshotKey,
  collectHostHandlers,
  createExecutionPolicy,
  encodeRuntimeLimits,
  encodeSnapshotPolicy,
  normalizeSnapshotKey,
  resolveProgressLoadContext,
  snapshotIdentity,
  snapshotKeyDigest,
  snapshotToken,
};
