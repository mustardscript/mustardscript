'use strict';

const { types } = require('node:util');

const { JsliteError, callNative } = require('./errors.ts');

function throwIfAborted(signal) {
  if (signal?.aborted) {
    throw new JsliteError('Limit', 'execution cancelled');
  }
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

function withCancellationSignal(native, fn, args, signal) {
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
    pending = capability(...args);
  } catch (error) {
    return { type: 'error', error };
  }

  if (!types.isPromise(pending)) {
    return { type: 'value', value: pending };
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

module.exports = {
  getAbortSignal,
  settleCapabilityInvocation,
  throwIfAborted,
  withCancellationSignal,
};
