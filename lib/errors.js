'use strict';

const KNOWN_ERROR_KINDS = new Set([
  'Parse',
  'Validation',
  'Runtime',
  'Limit',
  'Serialization',
]);

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

module.exports = {
  JsliteError,
  callNative,
  normalizeNativeError,
};
