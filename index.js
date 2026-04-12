'use strict';

const crypto = require('node:crypto');
const { loadNative } = require('./native-loader');

const native = loadNative();
const KNOWN_ERROR_KINDS = new Set([
  'Parse',
  'Validation',
  'Runtime',
  'Limit',
  'Serialization',
]);
const CONSOLE_CAPABILITY_NAMES = {
  log: 'console.log',
  warn: 'console.warn',
  error: 'console.error',
};
const USED_PROGRESS_SNAPSHOTS = new Set();
const KNOWN_PROGRESS_POLICIES = new Map();

class JsliteError extends Error {
  constructor(kind, message, cause) {
    super(message, { cause });
    this.kind = kind;
    this.name = `Jslite${kind}Error`;
  }
}

function normalizeNativeError(error) {
  if (!(error instanceof Error)) {
    return error;
  }
  const match = /^([A-Za-z]+):\s([\s\S]+)$/.exec(error.message);
  if (!match) {
    return error;
  }
  const [, kind, message] = match;
  if (!KNOWN_ERROR_KINDS.has(kind)) {
    return error;
  }
  return new JsliteError(kind, message, error);
}

function callNative(fn, ...args) {
  try {
    return fn(...args);
  } catch (error) {
    throw normalizeNativeError(error);
  }
}

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

function encodeNumber(value) {
  if (Number.isNaN(value)) {
    return { Number: 'NaN' };
  }
  if (Object.is(value, -0)) {
    return { Number: 'NegZero' };
  }
  if (value === Infinity) {
    return { Number: 'Infinity' };
  }
  if (value === -Infinity) {
    return { Number: 'NegInfinity' };
  }
  return { Number: { Finite: value } };
}

function isPlainStructuredObject(value) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return false;
  }
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function hasOwnProperty(value, key) {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function defineEnumerableProperty(target, key, value) {
  Object.defineProperty(target, key, {
    value,
    enumerable: true,
    writable: true,
    configurable: true,
  });
}

function isAccessorDescriptor(descriptor) {
  return hasOwnProperty(descriptor, 'get') || hasOwnProperty(descriptor, 'set');
}

function enumerateDataProperties(value) {
  return Object.entries(Object.getOwnPropertyDescriptors(value)).filter(([, descriptor]) => {
    if (!descriptor.enumerable) {
      return false;
    }
    if (isAccessorDescriptor(descriptor)) {
      throw new TypeError('host objects with accessors cannot cross the host boundary');
    }
    return true;
  });
}

function encodeStructuredArray(value) {
  const entries = new Array(value.length);
  for (let index = 0; index < value.length; index += 1) {
    const descriptor = Object.getOwnPropertyDescriptor(value, String(index));
    if (descriptor === undefined) {
      throw new TypeError('host arrays with holes cannot cross the host boundary');
    }
    if (isAccessorDescriptor(descriptor)) {
      throw new TypeError('host objects with accessors cannot cross the host boundary');
    }
    entries[index] = encodeStructured(descriptor.value);
  }
  return { Array: entries };
}

function encodeStructuredObject(value) {
  const object = {};
  for (const [key, descriptor] of enumerateDataProperties(value)) {
    defineEnumerableProperty(object, key, encodeStructured(descriptor.value));
  }
  return { Object: object };
}

function encodeStructured(value) {
  if (value === undefined) {
    return 'Undefined';
  }
  if (value === null) {
    return 'Null';
  }
  if (typeof value === 'boolean') {
    return { Bool: value };
  }
  if (typeof value === 'number') {
    return encodeNumber(value);
  }
  if (typeof value === 'string') {
    return { String: value };
  }
  if (Array.isArray(value)) {
    return encodeStructuredArray(value);
  }
  if (typeof value === 'object') {
    if (!isPlainStructuredObject(value)) {
      throw new TypeError(
        'Unsupported host value: only plain objects and arrays can cross the host boundary',
      );
    }
    return encodeStructuredObject(value);
  }
  throw new TypeError('Unsupported host value');
}

function decodeStructured(value) {
  if (value === 'Undefined') {
    return undefined;
  }
  if (value === 'Null') {
    return null;
  }
  if (value !== null && typeof value === 'object' && hasOwnProperty(value, 'Bool')) {
    return value.Bool;
  }
  if (value !== null && typeof value === 'object' && hasOwnProperty(value, 'String')) {
    return value.String;
  }
  if (value !== null && typeof value === 'object' && hasOwnProperty(value, 'Number')) {
    const encoded = value.Number;
    if (encoded === 'NaN') {
      return NaN;
    }
    if (encoded === 'Infinity') {
      return Infinity;
    }
    if (encoded === 'NegInfinity') {
      return -Infinity;
    }
    if (encoded === 'NegZero') {
      return -0;
    }
    return encoded.Finite;
  }
  if (value !== null && typeof value === 'object' && hasOwnProperty(value, 'Array')) {
    return value.Array.map(decodeStructured);
  }
  if (value !== null && typeof value === 'object' && hasOwnProperty(value, 'Object')) {
    const object = {};
    for (const [key, entry] of Object.entries(value.Object)) {
      defineEnumerableProperty(object, key, decodeStructured(entry));
    }
    return object;
  }
  throw new TypeError(`Unsupported structured value: ${JSON.stringify(value)}`);
}

function encodeStartOptions(inputs = {}, policy) {
  const encodedInputs = {};
  for (const [key, descriptor] of enumerateDataProperties(inputs)) {
    defineEnumerableProperty(encodedInputs, key, encodeStructured(descriptor.value));
  }
  return JSON.stringify({
    inputs: encodedInputs,
    capabilities: policy.capabilities,
    limits: policy.limits,
  });
}

function encodeResumePayloadValue(value) {
  return JSON.stringify({
    type: 'value',
    value: encodeStructured(value),
  });
}

function encodeResumePayloadError(error) {
  const source = error instanceof Error ? error : Object(error);
  return JSON.stringify({
    type: 'error',
    error: {
      name: source.name || 'Error',
      message: source.message || String(error),
      code: source.code ?? null,
      details: source.details === undefined ? null : encodeStructured(source.details),
    },
  });
}

function encodeResumePayloadCancel() {
  return JSON.stringify({
    type: 'cancelled',
  });
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

function getAbortSignal(options, label) {
  if (options === undefined) {
    return undefined;
  }
  if (options === null || typeof options !== 'object') {
    throw new TypeError(`${label} must be an object`);
  }
  const { signal } = options;
  if (signal === undefined) {
    return undefined;
  }
  if (
    typeof signal !== 'object' ||
    signal === null ||
    typeof signal.aborted !== 'boolean' ||
    typeof signal.addEventListener !== 'function' ||
    typeof signal.removeEventListener !== 'function'
  ) {
    throw new TypeError(`${label}.signal must be an AbortSignal`);
  }
  return signal;
}

function withCancellationSignal(fn, args, signal) {
  if (signal === undefined) {
    return callNative(fn, ...args);
  }
  const tokenId = callNative(native.createCancellationToken);
  const cancel = () => {
    try {
      callNative(native.cancelCancellationToken, tokenId);
    } catch {
      // Ignore late cancellation after cleanup wins the race.
    }
  };

  if (signal.aborted) {
    cancel();
  } else {
    signal.addEventListener('abort', cancel, { once: true });
  }

  try {
    return callNative(fn, ...args, tokenId);
  } finally {
    if (!signal.aborted) {
      signal.removeEventListener('abort', cancel);
    }
    callNative(native.releaseCancellationToken, tokenId);
  }
}

async function settleCapabilityInvocation(capability, args, signal) {
  if (signal?.aborted) {
    return { type: 'cancelled' };
  }

  let pending;
  try {
    pending = Promise.resolve(capability(...args));
  } catch (error) {
    return { type: 'error', error };
  }

  if (signal === undefined) {
    try {
      return {
        type: 'value',
        value: await pending,
      };
    } catch (error) {
      return { type: 'error', error };
    }
  }

  if (signal.aborted) {
    pending.catch(() => {});
    return { type: 'cancelled' };
  }

  const ABORTED = Symbol('aborted');
  let onAbort = null;
  const raced = await Promise.race([
    pending.then(
      (value) => ({ type: 'value', value }),
      (error) => ({ type: 'error', error }),
    ),
    new Promise((resolve) => {
      onAbort = () => resolve(ABORTED);
      signal.addEventListener('abort', onAbort, { once: true });
    }),
  ]);
  signal.removeEventListener('abort', onAbort);

  if (raced === ABORTED) {
    pending.catch(() => {});
    return { type: 'cancelled' };
  }

  return raced;
}

function parseStep(stepJson) {
  const step = JSON.parse(stepJson);
  if (step.type === 'completed') {
    return {
      type: 'completed',
      value: decodeStructured(step.value),
    };
  }
  return {
    type: 'suspended',
    capability: step.capability,
    args: step.args.map(decodeStructured),
    snapshot: Buffer.from(step.snapshot_base64, 'base64'),
  };
}

function parseSnapshotInspection(inspectionJson) {
  const inspection = JSON.parse(inspectionJson);
  return {
    capability: inspection.capability,
    args: inspection.args.map(decodeStructured),
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

class Progress {
  constructor(snapshot, capability, args, policy) {
    this.capability = capability;
    this.args = args;
    this.#snapshot = Buffer.from(snapshot);
    this.#snapshotId = snapshotIdentity(this.#snapshot);
    this.#policy = cloneSnapshotPolicy(policy);
    rememberProgressPolicy(this.#snapshotId, this.#policy);
  }

  #snapshot;
  #snapshotId;
  #policy;

  #claimSnapshot() {
    if (USED_PROGRESS_SNAPSHOTS.has(this.#snapshotId)) {
      throw new JsliteError(
        'Runtime',
        'Progress objects are single-use; this suspended execution was already resumed',
      );
    }
    USED_PROGRESS_SNAPSHOTS.add(this.#snapshotId);
    return Buffer.from(this.#snapshot);
  }

  get snapshot() {
    return Buffer.from(this.#snapshot);
  }

  dump() {
    return {
      capability: this.capability,
      args: this.args.slice(),
      snapshot: this.snapshot,
      token: this.#snapshotId,
    };
  }

  resume(value, options = undefined) {
    const signal = getAbortSignal(options, 'resume options');
    if (signal?.aborted) {
      return this.cancel();
    }
    const payload = encodeResumePayloadValue(value);
    const policyJson = encodeSnapshotPolicy(this.#policy);
    const step = parseStep(
      withCancellationSignal(native.resumeProgram, [this.#claimSnapshot(), payload, policyJson], signal),
    );
    return materializeStep(step, this.#policy);
  }

  resumeError(error, options = undefined) {
    const signal = getAbortSignal(options, 'resume options');
    if (signal?.aborted) {
      return this.cancel();
    }
    const payload = encodeResumePayloadError(error);
    const policyJson = encodeSnapshotPolicy(this.#policy);
    const step = parseStep(
      withCancellationSignal(native.resumeProgram, [this.#claimSnapshot(), payload, policyJson], signal),
    );
    return materializeStep(step, this.#policy);
  }

  cancel() {
    const policyJson = encodeSnapshotPolicy(this.#policy);
    const step = parseStep(
      callNative(native.resumeProgram, this.#claimSnapshot(), encodeResumePayloadCancel(), policyJson),
    );
    return materializeStep(step, this.#policy);
  }

  static load(state, options = undefined) {
    if (!state || typeof state !== 'object') {
      throw new TypeError('Progress.load() expects a dumped progress object');
    }
    if (!state.snapshot) {
      throw new TypeError('Progress.load() requires snapshot bytes');
    }
    const snapshot = Buffer.from(state.snapshot);
    const snapshotId = snapshotIdentity(snapshot);
    const policy = resolveProgressLoadPolicy(snapshotId, options);
    const inspection = parseSnapshotInspection(
      callNative(native.inspectSnapshot, snapshot, encodeSnapshotPolicy(policy)),
    );
    return new Progress(snapshot, inspection.capability, inspection.args, policy);
  }
}

function materializeStep(step, policy) {
  if (step.type === 'completed') {
    return step.value;
  }
  return new Progress(step.snapshot, step.capability, step.args, policy);
}

class Jslite {
  constructor(code, options = {}) {
    this._program = callNative(native.compileProgram, code);
    this._inputNames = options.inputs ?? [];
  }

  async run(options = {}) {
    const signal = getAbortSignal(options, 'run options');
    const { hostHandlers, policy } = createExecutionPolicy(options);
    const policyJson = encodeSnapshotPolicy(policy);
    let step = parseStep(
      withCancellationSignal(
        native.startProgram,
        [this._program, encodeStartOptions(options.inputs, policy)],
        signal,
      ),
    );
    while (step.type === 'suspended') {
      const capability = hostHandlers[step.capability];
      if (typeof capability !== 'function') {
        throw new Error(`Missing capability: ${step.capability}`);
      }
      const outcome = await settleCapabilityInvocation(capability, step.args, signal);
      if (outcome.type === 'cancelled') {
        step = parseStep(
          callNative(native.resumeProgram, step.snapshot, encodeResumePayloadCancel(), policyJson),
        );
        continue;
      }
      const payload =
        outcome.type === 'value'
          ? encodeResumePayloadValue(outcome.value)
          : encodeResumePayloadError(outcome.error);
      step = parseStep(
        withCancellationSignal(native.resumeProgram, [step.snapshot, payload, policyJson], signal),
      );
    }
    return step.value;
  }

  start(options = {}) {
    const signal = getAbortSignal(options, 'start options');
    const { policy } = createExecutionPolicy(options);
    const step = parseStep(
      withCancellationSignal(
        native.startProgram,
        [this._program, encodeStartOptions(options.inputs, policy)],
        signal,
      ),
    );
    return materializeStep(step, policy);
  }

  dump() {
    return Buffer.from(this._program);
  }

  static load(buffer) {
    const instance = Object.create(Jslite.prototype);
    instance._program = Buffer.from(buffer);
    instance._inputNames = [];
    return instance;
  }
}

module.exports = {
  JsliteError,
  Jslite,
  Progress,
};
