'use strict';

const { performance } = require('node:perf_hooks');

function encodeFrame(payload, blob = undefined) {
  const header = Buffer.from(JSON.stringify(payload), 'utf8');
  const body = blob === undefined || blob === null ? Buffer.alloc(0) : Buffer.from(blob);
  const frame = Buffer.allocUnsafe(8 + header.length + body.length);
  frame.writeUInt32LE(header.length, 0);
  frame.writeUInt32LE(body.length, 4);
  header.copy(frame, 8);
  body.copy(frame, 8 + header.length);
  return frame;
}

function createFrameReader(stream) {
  let buffered = Buffer.alloc(0);
  let ended = false;
  let endError = null;
  const waiters = [];

  function tryResolve() {
    while (waiters.length > 0) {
      if (buffered.length >= 8) {
        const headerLength = buffered.readUInt32LE(0);
        const blobLength = buffered.readUInt32LE(4);
        const frameLength = 8 + headerLength + blobLength;
        if (buffered.length < frameLength) {
          break;
        }
        const header = buffered.subarray(8, 8 + headerLength);
        const blob = buffered.subarray(8 + headerLength, frameLength);
        buffered = buffered.subarray(frameLength);
        const waiter = waiters.shift();
        const decodeStarted = performance.now();
        waiter.resolve({
          payload: JSON.parse(header.toString('utf8')),
          blob: Buffer.from(blob),
          responseDecodeMs: performance.now() - decodeStarted,
        });
        continue;
      }
      if (ended) {
        const waiter = waiters.shift();
        waiter.reject(endError ?? new Error('sidecar closed before a full frame was read'));
      }
      break;
    }
  }

  stream.on('data', (chunk) => {
    buffered = buffered.length === 0 ? Buffer.from(chunk) : Buffer.concat([buffered, chunk]);
    tryResolve();
  });
  stream.on('end', () => {
    ended = true;
    tryResolve();
  });
  stream.on('close', () => {
    ended = true;
    tryResolve();
  });
  stream.on('error', (error) => {
    ended = true;
    endError = error;
    tryResolve();
  });

  return function readFrame() {
    return new Promise((resolve, reject) => {
      waiters.push({ resolve, reject });
      tryResolve();
    });
  };
}

function createBinarySidecarClient(child) {
  const readFrame = createFrameReader(child.stdout);
  return {
    async request(payload, blob = undefined) {
      const frame = encodeFrame(payload, blob);
      const roundTripStarted = performance.now();
      child.stdin.write(frame);
      const response = await readFrame();
      response.roundTripMs = performance.now() - roundTripStarted;
      return response;
    },
  };
}

module.exports = {
  createBinarySidecarClient,
  createFrameReader,
  encodeFrame,
};
