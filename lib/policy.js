'use strict';

const crypto = require('node:crypto');
const { types } = require('node:util');

const { JsliteError } = require('./errors');
const { defineEnumerableProperty, hasOwnProperty, isAccessorDescriptor } = require('./structured');

const CONSOLE_CAPABILITY_NAMES = {
  log: 'console.log',
  warn: 'console.warn',
  error: 'console.error',
};
const KNOWN_PROGRESS_POLICIES = new Map();
const KNOWN_PROGRESS_POLICY_CACHE_LIMIT = 1024;
const DEFAULT_SNAPSHOT_KEY = crypto.randomBytes(32);

function rememberBoundedMapEntry(map, key, value, limit) {
  if (map.has(key)) {
    map.delete(key);
  }
  map.set(key, value);
  while (map.size > limit) {
    const oldest = map.keys().next().value;
    map.delete(oldest);
  }
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

function cloneSnapshotPolicy(policy) {
  return {
    capabilities: policy.capabilities.slice(),
    limits: { ...policy.limits },
  };
}

function cloneSnapshotKey(snapshotKey) {
  return Buffer.from(snapshotKey);
}

function encodeSnapshotPolicy(policy) {
  return JSON.stringify(cloneSnapshotPolicy(policy));
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

function snapshotToken(snapshot, snapshotKey) {
  return crypto.createHmac('sha256', snapshotKey).update(snapshot).digest('hex');
}

function assertSnapshotToken(snapshot, token, snapshotKey) {
  if (typeof token !== 'string' || token.length === 0) {
    throw new TypeError('Progress.load() requires a dumped progress token');
  }
  const expected = snapshotToken(snapshot, snapshotKey);
  if (
    token.length !== expected.length ||
    !crypto.timingSafeEqual(Buffer.from(token, 'utf8'), Buffer.from(expected, 'utf8'))
  ) {
    throw new JsliteError(
      'Serialization',
      'Progress.load() rejected a tampered or unauthenticated snapshot',
    );
  }
}

function rememberProgressPolicy(snapshotTokenValue, policy, snapshotKey) {
  rememberBoundedMapEntry(
    KNOWN_PROGRESS_POLICIES,
    snapshotTokenValue,
    {
      policy: cloneSnapshotPolicy(policy),
      snapshotKey: cloneSnapshotKey(snapshotKey),
    },
    KNOWN_PROGRESS_POLICY_CACHE_LIMIT,
  );
}

function forgetProgressPolicy(snapshotTokenValue) {
  KNOWN_PROGRESS_POLICIES.delete(snapshotTokenValue);
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
  const token = state.token;
  const cached = typeof token === 'string' ? KNOWN_PROGRESS_POLICIES.get(token) : undefined;

  if (options === undefined) {
    if (cached === undefined) {
      throw new TypeError(
        'Progress.load() requires explicit capabilities, limits, and snapshotKey when restoring progress outside the current process',
      );
    }
    assertSnapshotToken(snapshot, token, cached.snapshotKey);
    return {
      policy: cloneSnapshotPolicy(cached.policy),
      snapshotKey: cloneSnapshotKey(cached.snapshotKey),
    };
  }
  if (options === null || typeof options !== 'object') {
    throw new TypeError('Progress.load() options must be an object');
  }
  if (!hasOwnProperty(options, 'limits')) {
    throw new TypeError(
      'Progress.load() requires explicit limits when restoring progress outside the current process',
    );
  }
  const snapshotKey =
    options.snapshotKey !== undefined
      ? normalizeSnapshotKey(options.snapshotKey, 'Progress.load() options.snapshotKey')
      : cached?.snapshotKey;
  if (snapshotKey === undefined) {
    throw new TypeError(
      'Progress.load() requires explicit snapshotKey when restoring progress outside the current process',
    );
  }
  assertSnapshotToken(snapshot, token, snapshotKey);
  return {
    policy: createExecutionPolicy(options).policy,
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
  forgetProgressPolicy,
  KNOWN_PROGRESS_POLICY_CACHE_LIMIT,
  normalizeSnapshotKey,
  rememberProgressPolicy,
  resolveProgressLoadContext,
  snapshotToken,
};
