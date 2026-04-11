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
const USED_PROGRESS_TOKENS = new Set();

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
    return { Array: value.map(encodeStructured) };
  }
  if (typeof value === 'object') {
    if (!isPlainStructuredObject(value)) {
      throw new TypeError(
        'Unsupported host value: only plain objects and arrays can cross the host boundary',
      );
    }
    const object = {};
    for (const [key, entry] of Object.entries(value)) {
      object[key] = encodeStructured(entry);
    }
    return { Object: object };
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
  if ('Bool' in value) {
    return value.Bool;
  }
  if ('String' in value) {
    return value.String;
  }
  if ('Number' in value) {
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
  if ('Array' in value) {
    return value.Array.map(decodeStructured);
  }
  if ('Object' in value) {
    const object = {};
    for (const [key, entry] of Object.entries(value.Object)) {
      object[key] = decodeStructured(entry);
    }
    return object;
  }
  throw new TypeError(`Unsupported structured value: ${JSON.stringify(value)}`);
}

function encodeStartOptions({ inputs = {}, limits = {}, signal, ...handlers } = {}) {
  const encodedInputs = {};
  for (const [key, value] of Object.entries(inputs)) {
    encodedInputs[key] = encodeStructured(value);
  }
  const hostHandlers = collectHostHandlers(handlers);
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
  return JSON.stringify({
    inputs: encodedInputs,
    capabilities: Object.keys(hostHandlers),
    limits: encodedLimits,
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

class Progress {
  constructor(snapshot, capability, args, token = crypto.randomUUID()) {
    this.capability = capability;
    this.args = args;
    this.#snapshot = snapshot;
    this.#token = token;
  }

  #snapshot;
  #token;

  #claimSnapshot() {
    if (USED_PROGRESS_TOKENS.has(this.#token)) {
      throw new JsliteError(
        'Runtime',
        'Progress objects are single-use; this suspended execution was already resumed',
      );
    }
    USED_PROGRESS_TOKENS.add(this.#token);
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
      token: this.#token,
    };
  }

  resume(value, options = undefined) {
    const signal = getAbortSignal(options, 'resume options');
    if (signal?.aborted) {
      return this.cancel();
    }
    const payload = encodeResumePayloadValue(value);
    const step = parseStep(
      withCancellationSignal(native.resumeProgram, [this.#claimSnapshot(), payload], signal),
    );
    return materializeStep(step);
  }

  resumeError(error, options = undefined) {
    const signal = getAbortSignal(options, 'resume options');
    if (signal?.aborted) {
      return this.cancel();
    }
    const payload = encodeResumePayloadError(error);
    const step = parseStep(
      withCancellationSignal(native.resumeProgram, [this.#claimSnapshot(), payload], signal),
    );
    return materializeStep(step);
  }

  cancel() {
    const step = parseStep(
      callNative(native.resumeProgram, this.#claimSnapshot(), encodeResumePayloadCancel()),
    );
    return materializeStep(step);
  }

  static load(state) {
    if (!state || typeof state !== 'object') {
      throw new TypeError('Progress.load() expects a dumped progress object');
    }
    if (typeof state.capability !== 'string') {
      throw new TypeError('Progress.load() requires a string capability name');
    }
    if (!Array.isArray(state.args)) {
      throw new TypeError('Progress.load() requires an args array');
    }
    if (!state.snapshot) {
      throw new TypeError('Progress.load() requires snapshot bytes');
    }
    return new Progress(
      Buffer.from(state.snapshot),
      state.capability,
      state.args.slice(),
      typeof state.token === 'string' ? state.token : crypto.randomUUID(),
    );
  }
}

function materializeStep(step) {
  if (step.type === 'completed') {
    return step.value;
  }
  return new Progress(step.snapshot, step.capability, step.args);
}

class Jslite {
  constructor(code, options = {}) {
    this._program = callNative(native.compileProgram, code);
    this._inputNames = options.inputs ?? [];
  }

  async run(options = {}) {
    const signal = getAbortSignal(options, 'run options');
    const hostHandlers = collectHostHandlers(options);
    let step = parseStep(
      withCancellationSignal(native.startProgram, [this._program, encodeStartOptions(options)], signal),
    );
    while (step.type === 'suspended') {
      const capability = hostHandlers[step.capability];
      if (typeof capability !== 'function') {
        throw new Error(`Missing capability: ${step.capability}`);
      }
      const outcome = await settleCapabilityInvocation(capability, step.args, signal);
      if (outcome.type === 'cancelled') {
        step = parseStep(
          callNative(native.resumeProgram, step.snapshot, encodeResumePayloadCancel()),
        );
        continue;
      }
      const payload =
        outcome.type === 'value'
          ? encodeResumePayloadValue(outcome.value)
          : encodeResumePayloadError(outcome.error);
      step = parseStep(
        withCancellationSignal(native.resumeProgram, [step.snapshot, payload], signal),
      );
    }
    return step.value;
  }

  start(options = {}) {
    const signal = getAbortSignal(options, 'start options');
    const step = parseStep(
      withCancellationSignal(native.startProgram, [this._program, encodeStartOptions(options)], signal),
    );
    return materializeStep(step);
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
