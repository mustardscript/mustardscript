'use strict';

const crypto = require('node:crypto');

const { hasOwnProperty } = require('./structured');

const CONSOLE_CAPABILITY_NAMES = {
  log: 'console.log',
  warn: 'console.warn',
  error: 'console.error',
};
const KNOWN_PROGRESS_POLICIES = new Map();

function collectHostHandlers({ capabilities = {}, console = {} } = {}) {
  const handlers = { ...capabilities };
  for (const [method, capabilityName] of Object.entries(CONSOLE_CAPABILITY_NAMES)) {
    const handler = console[method];
    if (handler === undefined) {
      continue;
    }
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

function encodeSnapshotPolicy(policy) {
  return JSON.stringify(cloneSnapshotPolicy(policy));
}

function snapshotIdentity(snapshot) {
  return crypto.createHash('sha256').update(snapshot).digest('hex');
}

function rememberProgressPolicy(snapshotId, policy) {
  KNOWN_PROGRESS_POLICIES.set(snapshotId, cloneSnapshotPolicy(policy));
}

function createExecutionPolicy({ limits = {}, ...handlers } = {}) {
  const hostHandlers = collectHostHandlers(handlers);
  return {
    hostHandlers,
    policy: {
      capabilities: Object.keys(hostHandlers),
      limits: encodeRuntimeLimits(limits),
    },
  };
}

function resolveProgressLoadPolicy(snapshotId, options) {
  if (options === undefined) {
    const cached = KNOWN_PROGRESS_POLICIES.get(snapshotId);
    if (cached === undefined) {
      throw new TypeError(
        'Progress.load() requires explicit capabilities and limits when restoring progress outside the current process',
      );
    }
    return cloneSnapshotPolicy(cached);
  }
  if (options === null || typeof options !== 'object') {
    throw new TypeError('Progress.load() options must be an object');
  }
  if (!hasOwnProperty(options, 'limits')) {
    throw new TypeError(
      'Progress.load() requires explicit limits when restoring progress outside the current process',
    );
  }
  return createExecutionPolicy(options).policy;
}

module.exports = {
  cloneSnapshotPolicy,
  collectHostHandlers,
  createExecutionPolicy,
  encodeRuntimeLimits,
  encodeSnapshotPolicy,
  rememberProgressPolicy,
  resolveProgressLoadPolicy,
  snapshotIdentity,
};
