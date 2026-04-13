'use strict';

const crypto = require('node:crypto');
const { types } = require('node:util');
const { loadNative } = require('../native-loader.ts');

const { MustardError, callNative } = require('./errors.ts');
const {
  decodeStructured,
  defineEnumerableProperty,
  encodeStructured,
  hasOwnProperty,
  isAccessorDescriptor,
} = require('./structured.ts');

const CONSOLE_CAPABILITY_NAMES = {
  log: 'console.log',
  warn: 'console.warn',
  error: 'console.error',
};
const DEFAULT_SNAPSHOT_KEY = crypto.randomBytes(32);
const encodedSnapshotPolicyPrefixCache = new WeakMap();
let nativeSnapshotHelpers;
const executionContextHandleRegistry =
  typeof FinalizationRegistry === 'function'
    ? new FinalizationRegistry((contextHandle) => {
        try {
          callNative(snapshotNative().releaseExecutionContext, contextHandle);
        } catch {
          // Best-effort cleanup only; process shutdown can race native teardown.
        }
      })
    : null;

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

function freezePolicy(policy) {
  return Object.freeze({
    capabilities: Object.freeze(policy.capabilities.slice()),
    limits: Object.freeze({ ...policy.limits }),
  });
}

function getEncodedSnapshotPolicyPrefix(policy) {
  let cached = encodedSnapshotPolicyPrefixCache.get(policy);
  if (cached !== undefined) {
    return cached;
  }
  cached =
    `{"capabilities":${JSON.stringify(policy.capabilities)}` +
    `,"limits":${JSON.stringify(policy.limits)}`;
  encodedSnapshotPolicyPrefixCache.set(policy, cached);
  return cached;
}

function resolveSnapshotKeyEncoding(options) {
  if (
    typeof options?.snapshotKeyBase64 === 'string' &&
    options.snapshotKeyBase64.length > 0 &&
    typeof options?.snapshotKeyDigest === 'string' &&
    options.snapshotKeyDigest.length > 0
  ) {
    return {
      snapshotKeyBase64: options.snapshotKeyBase64,
      snapshotKeyDigest: options.snapshotKeyDigest,
    };
  }
  if (options?.snapshotKey === undefined) {
    return null;
  }
  const snapshotKey = cloneSnapshotKey(options.snapshotKey);
  return {
    snapshotKeyBase64: snapshotKey.toString('base64'),
    snapshotKeyDigest: snapshotKeyDigest(snapshotKey),
  };
}

function assertNoContextOverrides(options, label) {
  if (
    hasOwnProperty(options, 'capabilities') ||
    hasOwnProperty(options, 'console') ||
    hasOwnProperty(options, 'limits') ||
    hasOwnProperty(options, 'snapshotKey')
  ) {
    throw new TypeError(
      `${label}.context cannot be combined with capabilities, console, limits, or snapshotKey`,
    );
  }
}

class ExecutionContext {
  #hostHandlers;
  #policy;
  #policyJson;
  #snapshotKey;
  #snapshotKeyBase64;
  #snapshotKeyDigest;
  #nativeHandle;
  #nativeHandleToken;

  constructor(options = {}) {
    const {
      hostHandlers,
      policy,
      snapshotKey,
      snapshotKeyBase64,
      snapshotKeyDigest: snapshotKeyDigestValue,
    } = createExecutionPolicy(options);
    this.#hostHandlers = hostHandlers;
    this.#policy = freezePolicy(policy);
    this.#policyJson = null;
    this.#snapshotKey = cloneSnapshotKey(snapshotKey);
    this.#snapshotKeyBase64 = snapshotKeyBase64;
    this.#snapshotKeyDigest = snapshotKeyDigestValue;
    this.#nativeHandle = null;
    this.#nativeHandleToken = null;
  }

  hostHandlers() {
    return this.#hostHandlers;
  }

  policy() {
    return this.#policy;
  }

  snapshotKey() {
    return cloneSnapshotKey(this.#snapshotKey);
  }

  snapshotKeyMetadata() {
    return {
      snapshotKey: cloneSnapshotKey(this.#snapshotKey),
      snapshotKeyBase64: this.#snapshotKeyBase64,
      snapshotKeyDigest: this.#snapshotKeyDigest,
    };
  }

  policyJson() {
    this.#policyJson ??= JSON.stringify(this.#policy);
    return this.#policyJson;
  }

  nativeHandle() {
    if (this.#nativeHandle !== null) {
      return this.#nativeHandle;
    }
    const nativeHandle = callNative(
      snapshotNative().createExecutionContext,
      this.policyJson(),
    );
    this.#nativeHandle = nativeHandle;
    this.#nativeHandleToken = {};
    executionContextHandleRegistry?.register(this, nativeHandle, this.#nativeHandleToken);
    return nativeHandle;
  }
}

function resolveExecutionContext(options = {}, label = 'options') {
  const context = options?.context;
  if (context === undefined) {
    return createExecutionPolicy(options);
  }
  if (!(context instanceof ExecutionContext)) {
    throw new TypeError(`${label}.context must be an ExecutionContext`);
  }
  assertNoContextOverrides(options, label);
  const snapshotKeyMetadata = context.snapshotKeyMetadata();
  return {
    hostHandlers: context.hostHandlers(),
    policy: context.policy(),
    nativeContextHandle: context.nativeHandle(),
    ...snapshotKeyMetadata,
  };
}

function encodeSnapshotPolicy(policy, options = undefined) {
  const chunks = [getEncodedSnapshotPolicyPrefix(policy)];
  if (typeof options?.snapshotId === 'string' && options.snapshotId.length > 0) {
    chunks.push(',"snapshot_id":', JSON.stringify(options.snapshotId));
  }
  const snapshotKeyEncoding = resolveSnapshotKeyEncoding(options);
  if (snapshotKeyEncoding !== null) {
    chunks.push(
      ',"snapshot_key_base64":',
      JSON.stringify(snapshotKeyEncoding.snapshotKeyBase64),
      ',"snapshot_key_digest":',
      JSON.stringify(snapshotKeyEncoding.snapshotKeyDigest),
    );
  }
  if (typeof options?.snapshotToken === 'string' && options.snapshotToken.length > 0) {
    chunks.push(',"snapshot_token":', JSON.stringify(options.snapshotToken));
  }
  chunks.push('}');
  return chunks.join('');
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

function programIdentity(program) {
  return crypto.createHash('sha256').update(Buffer.from(program)).digest('hex');
}

function snapshotKeyDigest(snapshotKey) {
  return crypto.createHash('sha256').update(snapshotKey).digest('hex');
}

function suspendedManifestError() {
  return new MustardError(
    'Serialization',
    'Progress.load() rejected tampered or unauthenticated suspended metadata',
  );
}

function createSuspendedManifest(capability, args) {
  if (typeof capability !== 'string' || capability.length === 0) {
    throw new TypeError('Progress.dump() requires a suspended capability name');
  }
  if (!Array.isArray(args)) {
    throw new TypeError('Progress.dump() requires suspended args as an array');
  }
  return JSON.stringify({
    capability,
    args: args.map((value) => encodeStructured(value)),
  });
}

function parseSuspendedManifest(suspendedManifest) {
  try {
    const manifest = JSON.parse(suspendedManifest);
    if (manifest === null || typeof manifest !== 'object' || Array.isArray(manifest)) {
      throw suspendedManifestError();
    }
    if (typeof manifest.capability !== 'string' || manifest.capability.length === 0) {
      throw suspendedManifestError();
    }
    if (!Array.isArray(manifest.args)) {
      throw suspendedManifestError();
    }
    return {
      capability: manifest.capability,
      args: manifest.args.map((value) => decodeStructured(value)),
    };
  } catch (error) {
    if (error instanceof MustardError) {
      throw error;
    }
    throw suspendedManifestError();
  }
}

function suspendedManifestToken(snapshotId, suspendedManifest, snapshotKey) {
  return crypto
    .createHmac('sha256', snapshotKey)
    .update(snapshotId, 'utf8')
    .update('\0', 'utf8')
    .update(suspendedManifest, 'utf8')
    .digest('hex');
}

function assertSuspendedManifest(state, snapshotKey, expectedSnapshotId) {
  const suspendedManifest = state.suspended_manifest;
  const token = state.suspended_manifest_token;
  if (suspendedManifest === undefined && token === undefined) {
    return null;
  }
  if (
    typeof suspendedManifest !== 'string' ||
    suspendedManifest.length === 0 ||
    typeof token !== 'string' ||
    token.length === 0
  ) {
    throw suspendedManifestError();
  }
  const expected = suspendedManifestToken(
    expectedSnapshotId,
    suspendedManifest,
    snapshotKey,
  );
  if (
    token.length !== expected.length ||
    !crypto.timingSafeEqual(Buffer.from(token, 'utf8'), Buffer.from(expected, 'utf8'))
  ) {
    throw suspendedManifestError();
  }
  return parseSuspendedManifest(suspendedManifest);
}

function assertSnapshotToken(
  snapshot,
  token,
  snapshotKey,
  expectedSnapshotId = undefined,
  expectedSnapshotKeyDigest = undefined,
  actualSnapshotId = undefined,
) {
  if (typeof token !== 'string' || token.length === 0) {
    throw new TypeError('Progress.load() requires a dumped progress token');
  }
  const resolvedSnapshotId = actualSnapshotId ?? snapshotIdentity(snapshot);
  if (expectedSnapshotId !== undefined && resolvedSnapshotId !== expectedSnapshotId) {
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
  const expected = snapshotToken(snapshot, snapshotKey, resolvedSnapshotId);
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
  const normalizedSnapshotKey = normalizeSnapshotKey(snapshotKey, 'options.snapshotKey');
  return {
    hostHandlers,
    policy: {
      capabilities: Object.keys(hostHandlers),
      limits: encodeRuntimeLimits(limits),
    },
    snapshotKey: normalizedSnapshotKey,
    snapshotKeyBase64: normalizedSnapshotKey.toString('base64'),
    snapshotKeyDigest: snapshotKeyDigest(normalizedSnapshotKey),
  };
}

function resolveProgressLoadContext(state, snapshot, options, actualSnapshotId = undefined) {
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
      'Progress.load() requires an ExecutionContext or explicit capabilities, limits, and snapshotKey',
    );
  }
  if (hasOwnProperty(options, 'context')) {
    const context = options.context;
    if (!(context instanceof ExecutionContext)) {
      throw new TypeError('Progress.load() options.context must be an ExecutionContext');
    }
    assertNoContextOverrides(options, 'Progress.load() options');
    const snapshotKeyMetadata = context.snapshotKeyMetadata();
    assertSnapshotToken(
      snapshot,
      state.token,
      snapshotKeyMetadata.snapshotKey,
      expectedSnapshotId,
      expectedSnapshotKeyDigest,
      actualSnapshotId,
    );
    return {
      policy: context.policy(),
      nativeContextHandle: context.nativeHandle(),
      ...snapshotKeyMetadata,
    };
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
  const executionPolicy = createExecutionPolicy({ ...options, limits });
  assertSnapshotToken(
    snapshot,
    state.token,
    executionPolicy.snapshotKey,
    expectedSnapshotId,
    expectedSnapshotKeyDigest,
    actualSnapshotId,
  );
  return {
    policy: executionPolicy.policy,
    snapshotKey: cloneSnapshotKey(executionPolicy.snapshotKey),
    snapshotKeyBase64: executionPolicy.snapshotKeyBase64,
    snapshotKeyDigest: executionPolicy.snapshotKeyDigest,
  };
}

module.exports = {
  ExecutionContext,
  assertSuspendedManifest,
  cloneSnapshotPolicy,
  cloneSnapshotKey,
  collectHostHandlers,
  createSuspendedManifest,
  createExecutionPolicy,
  encodeRuntimeLimits,
  encodeSnapshotPolicy,
  normalizeSnapshotKey,
  parseSuspendedManifest,
  resolveExecutionContext,
  resolveProgressLoadContext,
  programIdentity,
  snapshotIdentity,
  snapshotKeyDigest,
  snapshotToken,
  suspendedManifestToken,
};
