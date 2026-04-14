'use strict';

const { callNative } = require('./errors.ts');
const {
  getAbortSignal,
  settleCapabilityInvocation,
  throwIfAborted,
  withCancellationSignal,
} = require('./cancellation.ts');
const { resolveExecutionContext } = require('./policy.ts');
const {
  encodeResumePayloadCancelBuffer,
  encodeResumePayloadErrorBuffer,
  encodeResumePayloadValueBuffer,
  encodeStartOptionsBuffer,
  encodeStructuredInputsBuffer,
} = require('./structured.ts');

function createMustardClass({ native, materializeStep, parseStep }) {
  const programHandleRegistry =
    typeof FinalizationRegistry === 'function'
      ? new FinalizationRegistry((programHandle) => {
          try {
            callNative(native.releaseProgram, programHandle);
          } catch {
            // Best-effort cleanup only; process shutdown can race native teardown.
          }
        })
      : null;

  function compileProgram(code) {
    return callNative(native.compileProgram, code);
  }

  function releaseProgram(programHandle) {
    callNative(native.releaseProgram, programHandle);
  }

  return class Mustard {
    constructor(code, options = {}) {
      this._programHandle = compileProgram(code);
      this._program = null;
      this._inputNames = options.inputs ?? [];
      this._programHandleToken = {};
      programHandleRegistry?.register(this, this._programHandle, this._programHandleToken);
    }

    static validateProgram(code) {
      const programHandle = compileProgram(code);
      releaseProgram(programHandle);
    }

    _ensureProgramHandle() {
      if (this._programHandle !== null) {
        return this._programHandle;
      }
      const programHandle = callNative(native.loadProgram, this._program);
      this._programHandle = programHandle;
      this._programHandleToken = {};
      programHandleRegistry?.register(this, programHandle, this._programHandleToken);
      return programHandle;
    }

    async run(options = {}) {
      const signal = getAbortSignal(options, 'run options');
      throwIfAborted(signal);
      const { hostHandlers, policy, nativeContextHandle } = resolveExecutionContext(
        options,
        'run options',
      );
      const programHandle = this._ensureProgramHandle();
      const startProgram =
        typeof nativeContextHandle === 'string' && nativeContextHandle.length > 0
          ? native.startProgramWithExecutionContextHandleBuffer
          : native.startProgramWithSnapshotHandleBuffer;
      const startArgs =
        typeof nativeContextHandle === 'string' && nativeContextHandle.length > 0
          ? [programHandle, nativeContextHandle, encodeStructuredInputsBuffer(options.inputs)]
          : [programHandle, encodeStartOptionsBuffer(options.inputs, policy)];
      let step = parseStep(
        withCancellationSignal(
          native,
          startProgram,
          startArgs,
          signal,
        ),
      );
      while (step.type === 'suspended') {
        const snapshotHandle = step.snapshotHandle;
        try {
          const capability = hostHandlers[step.capability];
          if (typeof capability !== 'function') {
            throw new Error(`Missing capability: ${step.capability}`);
          }
          const outcome = await settleCapabilityInvocation(capability, step.args, signal);
          if (outcome.type === 'cancelled') {
            step = parseStep(
              callNative(
                native.resumeSnapshotHandleBuffer,
                snapshotHandle,
                encodeResumePayloadCancelBuffer(),
              ),
            );
            continue;
          }
          const payload =
            outcome.type === 'value'
              ? encodeResumePayloadValueBuffer(outcome.value)
              : encodeResumePayloadErrorBuffer(outcome.error);
          step = parseStep(
            withCancellationSignal(
              native,
              native.resumeSnapshotHandleBuffer,
              [snapshotHandle, payload],
              signal,
            ),
          );
        } finally {
          if (typeof snapshotHandle === 'string' && snapshotHandle.length > 0) {
            try {
              callNative(native.releaseSnapshotHandle, snapshotHandle);
            } catch {
              // Best-effort cleanup only.
            }
          }
        }
      }
      return step.value;
    }

    start(options = {}) {
      const signal = getAbortSignal(options, 'start options');
      throwIfAborted(signal);
      const { policy, snapshotKey, nativeContextHandle } = resolveExecutionContext(
        options,
        'start options',
      );
      const programHandle = this._ensureProgramHandle();
      const startProgram =
        typeof nativeContextHandle === 'string' && nativeContextHandle.length > 0
          ? native.startProgramWithExecutionContextHandleBuffer
          : native.startProgramWithSnapshotHandleBuffer;
      const startArgs =
        typeof nativeContextHandle === 'string' && nativeContextHandle.length > 0
          ? [programHandle, nativeContextHandle, encodeStructuredInputsBuffer(options.inputs)]
          : [programHandle, encodeStartOptionsBuffer(options.inputs, policy)];
      const step = parseStep(
        withCancellationSignal(
          native,
          startProgram,
          startArgs,
          signal,
        ),
      );
      return materializeStep(step, policy, snapshotKey, programHandle);
    }

    dump() {
      if (this._program === null) {
        this._program = Buffer.from(callNative(native.dumpProgram, this._ensureProgramHandle()));
      }
      return Buffer.from(this._program);
    }

    static load(buffer) {
      const instance = Object.create(Mustard.prototype);
      instance._program = Buffer.from(buffer);
      instance._programHandle = null;
      instance._inputNames = [];
      instance._programHandleToken = {};
      return instance;
    }
  };
}

module.exports = {
  createMustardClass,
};
