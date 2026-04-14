'use strict';

const { types } = require('node:util');

const HOST_BOUNDARY_MAX_DEPTH = 128;
const HOST_BOUNDARY_MAX_ARRAY_LENGTH = 1_000_000;
const encodedStartOptionsSuffixCache = new WeakMap();
const encodedStartOptionsBinarySuffixCache = new WeakMap();
const BOUNDARY_BINARY_MAGIC = Buffer.from([0x4d, 0x53, 0x42, 0x01]);
const BOUNDARY_BINARY_KIND = Object.freeze({
  START_OPTIONS: 1,
  STRUCTURED_INPUTS: 2,
  RESUME_PAYLOAD: 3,
});
const STRUCTURED_BINARY_TAG = Object.freeze({
  UNDEFINED: 0,
  NULL: 1,
  HOLE: 2,
  BOOL_FALSE: 3,
  BOOL_TRUE: 4,
  STRING: 5,
  NUMBER_FINITE: 6,
  NUMBER_NAN: 7,
  NUMBER_INFINITY: 8,
  NUMBER_NEG_INFINITY: 9,
  NUMBER_NEG_ZERO: 10,
  ARRAY: 11,
  OBJECT: 12,
});
const RESUME_BINARY_TAG = Object.freeze({
  VALUE: 0,
  ERROR: 1,
  CANCELLED: 2,
});
const LIMIT_FIELD_LAYOUT = Object.freeze([
  ['instruction_budget', 1 << 0],
  ['heap_limit_bytes', 1 << 1],
  ['allocation_budget', 1 << 2],
  ['call_depth_limit', 1 << 3],
  ['max_outstanding_host_calls', 1 << 4],
]);

class BinaryWriter {
  constructor(initialSize = 256) {
    this._buffer = Buffer.allocUnsafe(initialSize);
    this._offset = 0;
  }

  _ensureCapacity(additionalBytes) {
    const required = this._offset + additionalBytes;
    if (required <= this._buffer.length) {
      return;
    }
    let nextLength = this._buffer.length;
    while (nextLength < required) {
      nextLength *= 2;
    }
    const nextBuffer = Buffer.allocUnsafe(nextLength);
    this._buffer.copy(nextBuffer, 0, 0, this._offset);
    this._buffer = nextBuffer;
  }

  writeHeader(kind) {
    this.writeBuffer(BOUNDARY_BINARY_MAGIC);
    this.writeU8(kind);
  }

  writeBuffer(value) {
    const buffer = Buffer.from(value);
    this._ensureCapacity(buffer.length);
    buffer.copy(this._buffer, this._offset);
    this._offset += buffer.length;
  }

  writeU8(value) {
    this._ensureCapacity(1);
    this._buffer.writeUInt8(value, this._offset);
    this._offset += 1;
  }

  writeU32(value) {
    this._ensureCapacity(4);
    this._buffer.writeUInt32LE(value >>> 0, this._offset);
    this._offset += 4;
  }

  writeF64(value) {
    this._ensureCapacity(8);
    this._buffer.writeDoubleLE(value, this._offset);
    this._offset += 8;
  }

  writeString(value) {
    const byteLength = Buffer.byteLength(value, 'utf8');
    this.writeU32(byteLength);
    this._ensureCapacity(byteLength);
    this._buffer.write(value, this._offset, byteLength, 'utf8');
    this._offset += byteLength;
  }

  toBuffer() {
    return Buffer.from(this._buffer.subarray(0, this._offset));
  }
}

function assertBoundaryDepth(depth, label) {
  if (depth > HOST_BOUNDARY_MAX_DEPTH) {
    throw new TypeError(`${label} nesting limit exceeded`);
  }
}

function assertBoundaryArrayLength(length, label) {
  if (length > HOST_BOUNDARY_MAX_ARRAY_LENGTH) {
    throw new TypeError(
      `${label} arrays longer than ${HOST_BOUNDARY_MAX_ARRAY_LENGTH} elements cannot cross the host boundary`,
    );
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
  const keys = Object.keys(value);
  const entries = new Array(keys.length);
  let entryCount = 0;
  for (const key of keys) {
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    if (descriptor === undefined) {
      continue;
    }
    if (isAccessorDescriptor(descriptor)) {
      throw new TypeError('host objects with accessors cannot cross the host boundary');
    }
    entries[entryCount] = [key, descriptor];
    entryCount += 1;
  }
  entries.length = entryCount;
  return entries;
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
  assertBoundaryArrayLength(value.length, 'host boundary');
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
    assertBoundaryArrayLength(value.Array.length, 'structured host boundary');
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

function getEncodedStartOptionsSuffix(policy) {
  let cached = encodedStartOptionsSuffixCache.get(policy);
  if (cached !== undefined) {
    return cached;
  }
  cached =
    `,"capabilities":${JSON.stringify(policy.capabilities)}` +
    `,"limits":${JSON.stringify(policy.limits)}}`;
  encodedStartOptionsSuffixCache.set(policy, cached);
  return cached;
}

function writeLimitValue(writer, value) {
  writer.writeF64(value);
}

function writeEncodedLimits(writer, limits = {}) {
  let mask = 0;
  for (const [field, bit] of LIMIT_FIELD_LAYOUT) {
    if (limits[field] !== undefined) {
      mask |= bit;
    }
  }
  writer.writeU8(mask);
  for (const [field, bit] of LIMIT_FIELD_LAYOUT) {
    if ((mask & bit) !== 0) {
      writeLimitValue(writer, limits[field]);
    }
  }
}

function writeEncodedCapabilities(writer, capabilities = []) {
  writer.writeU32(capabilities.length);
  for (const capability of capabilities) {
    writer.writeString(capability);
  }
}

function getEncodedStartOptionsBinarySuffix(policy) {
  let cached = encodedStartOptionsBinarySuffixCache.get(policy);
  if (cached !== undefined) {
    return cached;
  }
  const writer = new BinaryWriter();
  writeEncodedCapabilities(writer, policy.capabilities);
  writeEncodedLimits(writer, policy.limits);
  cached = writer.toBuffer();
  encodedStartOptionsBinarySuffixCache.set(policy, cached);
  return cached;
}

function writeStructured(value, writer, traversal = { active: new WeakSet() }, depth = 1) {
  assertBoundaryDepth(depth, 'host boundary');
  if (value === undefined) {
    writer.writeU8(STRUCTURED_BINARY_TAG.UNDEFINED);
    return;
  }
  if (value === null) {
    writer.writeU8(STRUCTURED_BINARY_TAG.NULL);
    return;
  }
  if (typeof value === 'boolean') {
    writer.writeU8(value ? STRUCTURED_BINARY_TAG.BOOL_TRUE : STRUCTURED_BINARY_TAG.BOOL_FALSE);
    return;
  }
  if (typeof value === 'number') {
    if (Number.isNaN(value)) {
      writer.writeU8(STRUCTURED_BINARY_TAG.NUMBER_NAN);
      return;
    }
    if (Object.is(value, -0)) {
      writer.writeU8(STRUCTURED_BINARY_TAG.NUMBER_NEG_ZERO);
      return;
    }
    if (value === Infinity) {
      writer.writeU8(STRUCTURED_BINARY_TAG.NUMBER_INFINITY);
      return;
    }
    if (value === -Infinity) {
      writer.writeU8(STRUCTURED_BINARY_TAG.NUMBER_NEG_INFINITY);
      return;
    }
    writer.writeU8(STRUCTURED_BINARY_TAG.NUMBER_FINITE);
    writer.writeF64(value);
    return;
  }
  if (typeof value === 'string') {
    writer.writeU8(STRUCTURED_BINARY_TAG.STRING);
    writer.writeString(value);
    return;
  }
  if (Array.isArray(value)) {
    assertNotProxy(value);
    assertBoundaryArrayLength(value.length, 'host boundary');
    writer.writeU8(STRUCTURED_BINARY_TAG.ARRAY);
    writer.writeU32(value.length);
    const leave = enterStructuredTraversal(value, traversal);
    try {
      for (let index = 0; index < value.length; index += 1) {
        const descriptor = Object.getOwnPropertyDescriptor(value, String(index));
        if (descriptor === undefined) {
          writer.writeU8(STRUCTURED_BINARY_TAG.HOLE);
          continue;
        }
        if (isAccessorDescriptor(descriptor)) {
          throw new TypeError('host objects with accessors cannot cross the host boundary');
        }
        writeStructured(descriptor.value, writer, traversal, depth + 1);
      }
    } finally {
      leave();
    }
    return;
  }
  if (typeof value === 'object') {
    if (!isPlainStructuredObject(value)) {
      throw new TypeError(
        'Unsupported host value: only plain objects and arrays can cross the host boundary',
      );
    }
    writer.writeU8(STRUCTURED_BINARY_TAG.OBJECT);
    const entries = enumerateDataProperties(value);
    writer.writeU32(entries.length);
    const leave = enterStructuredTraversal(value, traversal);
    try {
      for (const [key, descriptor] of entries) {
        writer.writeString(key);
        writeStructured(descriptor.value, writer, traversal, depth + 1);
      }
    } finally {
      leave();
    }
    return;
  }
  throw new TypeError('Unsupported host value');
}

function writeStructuredInputEntries(writer, inputs = {}) {
  const entries = enumerateDataProperties(inputs);
  writer.writeU32(entries.length);
  for (const [key, descriptor] of entries) {
    writer.writeString(key);
    writeStructured(descriptor.value, writer);
  }
}

function encodeStructuredInputs(inputs = {}) {
  const encodedInputs = {};
  for (const [key, descriptor] of enumerateDataProperties(inputs)) {
    defineEnumerableProperty(encodedInputs, key, encodeStructured(descriptor.value));
  }
  return JSON.stringify(encodedInputs);
}

function encodeStartOptions(inputs = {}, policy) {
  const encodedInputs = encodeStructuredInputs(inputs);
  return `{"inputs":${encodedInputs}${getEncodedStartOptionsSuffix(policy)}`;
}

function encodeStructuredInputsBuffer(inputs = {}) {
  const writer = new BinaryWriter();
  writer.writeHeader(BOUNDARY_BINARY_KIND.STRUCTURED_INPUTS);
  writeStructuredInputEntries(writer, inputs);
  return writer.toBuffer();
}

function encodeStartOptionsBuffer(inputs = {}, policy) {
  const writer = new BinaryWriter();
  writer.writeHeader(BOUNDARY_BINARY_KIND.START_OPTIONS);
  writeStructuredInputEntries(writer, inputs);
  writer.writeBuffer(getEncodedStartOptionsBinarySuffix(policy));
  return writer.toBuffer();
}

function encodeResumePayloadValue(value) {
  return JSON.stringify({
    type: 'value',
    value: encodeStructured(value),
  });
}

function encodeResumePayloadValueBuffer(value) {
  const writer = new BinaryWriter();
  writer.writeHeader(BOUNDARY_BINARY_KIND.RESUME_PAYLOAD);
  writer.writeU8(RESUME_BINARY_TAG.VALUE);
  writeStructured(value, writer);
  return writer.toBuffer();
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

function encodeResumePayloadErrorBuffer(error) {
  const source = error instanceof Error ? error : Object(error);
  assertNotProxy(source);
  const name = readOwnDataProperty(source, 'name', 'host errors');
  const message = readOwnDataProperty(source, 'message', 'host errors');
  const code = readOwnDataProperty(source, 'code', 'host errors');
  const details = readOwnDataProperty(source, 'details', 'host errors');
  const writer = new BinaryWriter();
  writer.writeHeader(BOUNDARY_BINARY_KIND.RESUME_PAYLOAD);
  writer.writeU8(RESUME_BINARY_TAG.ERROR);
  writer.writeString(typeof name === 'string' ? name : 'Error');
  writer.writeString(typeof message === 'string' ? message : '');
  if (typeof code === 'string') {
    writer.writeU8(1);
    writer.writeString(code);
  } else {
    writer.writeU8(0);
  }
  if (details === undefined) {
    writer.writeU8(0);
  } else {
    writer.writeU8(1);
    writeStructured(details, writer);
  }
  return writer.toBuffer();
}

function encodeResumePayloadCancel() {
  return JSON.stringify({
    type: 'cancelled',
  });
}

function encodeResumePayloadCancelBuffer() {
  const writer = new BinaryWriter();
  writer.writeHeader(BOUNDARY_BINARY_KIND.RESUME_PAYLOAD);
  writer.writeU8(RESUME_BINARY_TAG.CANCELLED);
  return writer.toBuffer();
}

module.exports = {
  BOUNDARY_BINARY_KIND,
  decodeStructured,
  defineEnumerableProperty,
  encodeResumePayloadCancel,
  encodeResumePayloadCancelBuffer,
  encodeResumePayloadError,
  encodeResumePayloadErrorBuffer,
  encodeResumePayloadValue,
  encodeResumePayloadValueBuffer,
  encodeStartOptions,
  encodeStartOptionsBuffer,
  encodeStructuredInputs,
  encodeStructuredInputsBuffer,
  encodeStructured,
  enumerateDataProperties,
  hasOwnProperty,
  isAccessorDescriptor,
};
