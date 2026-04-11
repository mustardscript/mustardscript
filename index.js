'use strict';

const fs = require('node:fs');
const path = require('node:path');

function loadNative() {
  const roots = [
    __dirname,
    path.join(__dirname, 'crates', 'jslite-node'),
  ];
  const candidates = [];
  for (const root of roots) {
    if (!fs.existsSync(root)) {
      continue;
    }
    for (const entry of fs.readdirSync(root)) {
      if (entry.endsWith('.node')) {
        candidates.push(path.join(root, entry));
      }
    }
  }
  for (const candidate of candidates) {
    try {
      return require(candidate);
    } catch {
      continue;
    }
  }
  throw new Error('Unable to locate built jslite native addon');
}

const native = loadNative();

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

function encodeStartOptions({ inputs = {}, capabilities = {} } = {}) {
  const encodedInputs = {};
  for (const [key, value] of Object.entries(inputs)) {
    encodedInputs[key] = encodeStructured(value);
  }
  return JSON.stringify({
    inputs: encodedInputs,
    capabilities: Object.keys(capabilities),
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
  constructor(snapshot, capability, args) {
    this.capability = capability;
    this.args = args;
    this.#snapshot = snapshot;
  }

  #snapshot;

  get snapshot() {
    return Buffer.from(this.#snapshot);
  }

  resume(value) {
    const step = parseStep(
      native.resumeProgram(this.#snapshot, encodeResumePayloadValue(value)),
    );
    return materializeStep(step);
  }

  resumeError(error) {
    const step = parseStep(
      native.resumeProgram(this.#snapshot, encodeResumePayloadError(error)),
    );
    return materializeStep(step);
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
    this._program = native.compileProgram(code);
    this._inputNames = options.inputs ?? [];
  }

  async run(options = {}) {
    const capabilities = options.capabilities ?? {};
    let step = parseStep(
      native.startProgram(this._program, encodeStartOptions(options)),
    );
    while (step.type === 'suspended') {
      const capability = capabilities[step.capability];
      if (typeof capability !== 'function') {
        throw new Error(`Missing capability: ${step.capability}`);
      }
      try {
        const result = await capability(...step.args);
        step = parseStep(
          native.resumeProgram(step.snapshot, encodeResumePayloadValue(result)),
        );
      } catch (error) {
        step = parseStep(
          native.resumeProgram(step.snapshot, encodeResumePayloadError(error)),
        );
      }
    }
    return step.value;
  }

  start(options = {}) {
    const step = parseStep(
      native.startProgram(this._program, encodeStartOptions(options)),
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
  Jslite,
  Progress,
};
