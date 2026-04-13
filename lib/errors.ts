'use strict';

const KNOWN_ERROR_KINDS = new Set([
  'Parse',
  'Validation',
  'Runtime',
  'Limit',
  'Serialization',
]);

class MustardError extends Error {
  constructor(kind, message, cause) {
    super(message, { cause });
    this.kind = kind;
    this.name = `Mustard${kind}Error`;
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
  return new MustardError(kind, message, error);
}

function callNative(fn, ...args) {
  try {
    return fn(...args);
  } catch (error) {
    throw normalizeNativeError(error);
  }
}

module.exports = {
  MustardError,
  callNative,
  normalizeNativeError,
};
