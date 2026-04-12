'use strict';

const { types } = require('node:util');

const HOST_BOUNDARY_MAX_DEPTH = 128;

function assertBoundaryDepth(depth, label) {
  if (depth > HOST_BOUNDARY_MAX_DEPTH) {
    throw new TypeError(`${label} nesting limit exceeded`);
  }
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

function isObjectLike(value) {
  return value !== null && (typeof value === 'object' || typeof value === 'function');
}

function assertNotProxy(value) {
  if (isObjectLike(value) && types.isProxy(value)) {
    throw new TypeError('Proxy values cannot cross the host boundary');
  }
}

function isPlainStructuredObject(value) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return false;
  }
  assertNotProxy(value);
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
  assertNotProxy(value);
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

function enterStructuredTraversal(value, traversal) {
  if (!isObjectLike(value)) {
    return () => {};
  }
  if (traversal.active.has(value)) {
    throw new TypeError('cyclic values cannot cross the host boundary');
  }
  traversal.active.add(value);
  return () => {
    traversal.active.delete(value);
  };
}

function encodeStructuredArray(value, traversal, depth) {
  assertNotProxy(value);
  const leave = enterStructuredTraversal(value, traversal);
  try {
    const entries = new Array(value.length);
    for (let index = 0; index < value.length; index += 1) {
      const descriptor = Object.getOwnPropertyDescriptor(value, String(index));
      if (descriptor === undefined) {
        entries[index] = 'Hole';
        continue;
      }
      if (isAccessorDescriptor(descriptor)) {
        throw new TypeError('host objects with accessors cannot cross the host boundary');
      }
      entries[index] = encodeStructured(descriptor.value, traversal, depth + 1);
    }
    return { Array: entries };
  } finally {
    leave();
  }
}

function encodeStructuredObject(value, traversal, depth) {
  const leave = enterStructuredTraversal(value, traversal);
  try {
    const object = {};
    for (const [key, descriptor] of enumerateDataProperties(value)) {
      defineEnumerableProperty(
        object,
        key,
        encodeStructured(descriptor.value, traversal, depth + 1),
      );
    }
    return { Object: object };
  } finally {
    leave();
  }
}

function encodeStructured(value, traversal = { active: new WeakSet() }, depth = 1) {
  assertBoundaryDepth(depth, 'host boundary');
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
    return encodeStructuredArray(value, traversal, depth);
  }
  if (typeof value === 'object') {
    if (!isPlainStructuredObject(value)) {
      throw new TypeError(
        'Unsupported host value: only plain objects and arrays can cross the host boundary',
      );
    }
    return encodeStructuredObject(value, traversal, depth);
  }
  throw new TypeError('Unsupported host value');
}

function decodeStructured(value, depth = 1) {
  assertBoundaryDepth(depth, 'structured host boundary');
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
    const array = new Array(value.Array.length);
    value.Array.forEach((entry, index) => {
      if (entry !== 'Hole') {
        array[index] = decodeStructured(entry, depth + 1);
      }
    });
    return array;
  }
  if (value !== null && typeof value === 'object' && hasOwnProperty(value, 'Object')) {
    const object = {};
    for (const [key, entry] of Object.entries(value.Object)) {
      defineEnumerableProperty(object, key, decodeStructured(entry, depth + 1));
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

function readOwnDataProperty(value, key, label) {
  const descriptor = Object.getOwnPropertyDescriptor(value, key);
  if (descriptor === undefined) {
    return undefined;
  }
  if (isAccessorDescriptor(descriptor)) {
    throw new TypeError(`${label} cannot use accessor-backed ${key} properties`);
  }
  return descriptor.value;
}

function encodeResumePayloadError(error) {
  const source = error instanceof Error ? error : Object(error);
  assertNotProxy(source);
  const name = readOwnDataProperty(source, 'name', 'host errors');
  const message = readOwnDataProperty(source, 'message', 'host errors');
  const code = readOwnDataProperty(source, 'code', 'host errors');
  const details = readOwnDataProperty(source, 'details', 'host errors');
  return JSON.stringify({
    type: 'error',
    error: {
      name: typeof name === 'string' ? name : 'Error',
      message: typeof message === 'string' ? message : '',
      code: typeof code === 'string' ? code : null,
      details: details === undefined ? null : encodeStructured(details),
    },
  });
}

function encodeResumePayloadCancel() {
  return JSON.stringify({
    type: 'cancelled',
  });
}

module.exports = {
  decodeStructured,
  defineEnumerableProperty,
  encodeResumePayloadCancel,
  encodeResumePayloadError,
  encodeResumePayloadValue,
  encodeStartOptions,
  encodeStructured,
  enumerateDataProperties,
  hasOwnProperty,
  isAccessorDescriptor,
};
