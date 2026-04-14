'use strict';

const { assert, ExecutionContext, test } = require('./support/helpers.js');
const { loadNative } = require('../../native-loader.ts');
const {
  decodeStructured,
  encodeResumePayloadValueBuffer,
  encodeStartOptionsBuffer,
  encodeStructuredInputsBuffer,
} = require('../../lib/structured.ts');

const SNAPSHOT_KEY = Buffer.from('addon-boundary-profile-test-key');

function parseProfiledStep(profiledJson) {
  const profiled = JSON.parse(profiledJson);
  const step = profiled.step;
  const parsedStep =
    step.type === 'completed'
      ? {
          type: 'completed',
          value: decodeStructured(step.value),
        }
      : {
          type: 'suspended',
          capability: step.capability,
          args: step.args.map(decodeStructured),
          snapshotHandle: step.snapshot_handle,
        };
  return {
    step: parsedStep,
    profile: profiled.profile,
  };
}

function assertProfileShape(profile) {
  for (const field of ['parse_ns', 'execute_ns', 'encode_ns']) {
    assert.equal(typeof profile[field], 'number');
    assert.ok(Number.isFinite(profile[field]));
    assert.ok(profile[field] >= 0);
  }
}

test('profiled execution-context start and resume preserve hot-path behavior', () => {
  const native = loadNative();
  const context = new ExecutionContext({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      checkpoint(value) {
        return value + 1;
      },
    },
    limits: {},
  });
  const programHandle = native.compileProgram(`
    const first = checkpoint(seed);
    const second = checkpoint(first);
    second;
  `);
  const contextHandle = context.nativeHandle();
  let snapshotHandle = null;

  try {
    const started = parseProfiledStep(
      native.profileStartProgramWithExecutionContextHandleBuffer(
        programHandle,
        contextHandle,
        encodeStructuredInputsBuffer({ seed: 1 }),
      ),
    );
    assertProfileShape(started.profile);
    assert.equal(started.step.type, 'suspended');
    assert.equal(started.step.capability, 'checkpoint');
    assert.deepEqual(started.step.args, [1]);
    snapshotHandle = started.step.snapshotHandle;
    assert.equal(typeof snapshotHandle, 'string');

    const resumed = parseProfiledStep(
      native.profileResumeSnapshotHandleBuffer(snapshotHandle, encodeResumePayloadValueBuffer(2)),
    );
    snapshotHandle = null;
    assertProfileShape(resumed.profile);
    assert.equal(resumed.step.type, 'suspended');
    assert.equal(resumed.step.capability, 'checkpoint');
    assert.deepEqual(resumed.step.args, [2]);
    snapshotHandle = resumed.step.snapshotHandle;

    const completed = parseProfiledStep(
      native.profileResumeSnapshotHandleBuffer(snapshotHandle, encodeResumePayloadValueBuffer(3)),
    );
    snapshotHandle = null;
    assertProfileShape(completed.profile);
    assert.equal(completed.step.type, 'completed');
    assert.equal(completed.step.value, 3);
  } finally {
    if (snapshotHandle) {
      native.releaseSnapshotHandle(snapshotHandle);
    }
    native.releaseExecutionContext(contextHandle);
    native.releaseProgram(programHandle);
  }
});

test('profiled snapshot-handle start accepts the binary start-options hot path', () => {
  const native = loadNative();
  const context = new ExecutionContext({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      checkpoint() {},
    },
    limits: {},
  });
  const programHandle = native.compileProgram('checkpoint(seed);');
  let snapshotHandle = null;

  try {
    const started = parseProfiledStep(
      native.profileStartProgramWithSnapshotHandleBuffer(
        programHandle,
        encodeStartOptionsBuffer({ seed: 7 }, context.policy()),
      ),
    );
    assertProfileShape(started.profile);
    assert.equal(started.step.type, 'suspended');
    assert.equal(started.step.capability, 'checkpoint');
    assert.deepEqual(started.step.args, [7]);
    snapshotHandle = started.step.snapshotHandle;
    assert.equal(typeof snapshotHandle, 'string');
  } finally {
    if (snapshotHandle) {
      native.releaseSnapshotHandle(snapshotHandle);
    }
    native.releaseProgram(programHandle);
  }
});
