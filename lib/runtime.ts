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
  encodeResumePayloadCancel,
  encodeResumePayloadError,
  encodeResumePayloadValue,
  encodeStartOptions,
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
      const { hostHandlers, policy } = resolveExecutionContext(options, 'run options');
      const programHandle = this._ensureProgramHandle();
      let step = parseStep(
        withCancellationSignal(
          native,
          native.startProgramWithSnapshotHandle,
          [programHandle, encodeStartOptions(options.inputs, policy)],
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
              callNative(native.resumeSnapshotHandle, snapshotHandle, encodeResumePayloadCancel()),
            );
            continue;
          }
          const payload =
            outcome.type === 'value'
              ? encodeResumePayloadValue(outcome.value)
              : encodeResumePayloadError(outcome.error);
          step = parseStep(
            withCancellationSignal(
              native,
              native.resumeSnapshotHandle,
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
      const { policy, snapshotKey } = resolveExecutionContext(options, 'start options');
      const programHandle = this._ensureProgramHandle();
      const step = parseStep(
        withCancellationSignal(
          native,
          native.startProgramWithSnapshotHandle,
          [programHandle, encodeStartOptions(options.inputs, policy)],
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
