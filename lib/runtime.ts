'use strict';

const { callNative } = require('./errors.ts');
const {
  getAbortSignal,
  settleCapabilityInvocation,
  throwIfAborted,
  withCancellationSignal,
} = require('./cancellation.ts');
const {
  createExecutionPolicy,
  encodeSnapshotPolicy,
  snapshotIdentity,
  snapshotToken,
} = require('./policy.ts');
const {
  encodeResumePayloadCancel,
  encodeResumePayloadError,
  encodeResumePayloadValue,
  encodeStartOptions,
} = require('./structured.ts');

function createJsliteClass({ native, materializeStep, parseStep }) {
  function compileProgram(code) {
    return callNative(native.compileProgram, code);
  }

  return class Jslite {
    constructor(code, options = {}) {
      this._program = compileProgram(code);
      this._inputNames = options.inputs ?? [];
    }

    static validateProgram(code) {
      compileProgram(code);
    }

    async run(options = {}) {
      const signal = getAbortSignal(options, 'run options');
      throwIfAborted(signal);
      const { hostHandlers, policy, snapshotKey } = createExecutionPolicy(options);
      let step = parseStep(
        withCancellationSignal(
          native,
          native.startProgram,
          [this._program, encodeStartOptions(options.inputs, policy)],
          signal,
        ),
      );
      while (step.type === 'suspended') {
        const capability = hostHandlers[step.capability];
        if (typeof capability !== 'function') {
          throw new Error(`Missing capability: ${step.capability}`);
        }
        const outcome = await settleCapabilityInvocation(capability, step.args, signal);
        const snapshotId = snapshotIdentity(step.snapshot);
        const policyJson = encodeSnapshotPolicy(policy, {
          snapshotId,
          snapshotKey,
          snapshotToken: snapshotToken(step.snapshot, snapshotKey, snapshotId),
        });
        if (outcome.type === 'cancelled') {
          step = parseStep(
            callNative(native.resumeProgram, step.snapshot, encodeResumePayloadCancel(), policyJson),
          );
          continue;
        }
        const payload =
          outcome.type === 'value'
            ? encodeResumePayloadValue(outcome.value)
            : encodeResumePayloadError(outcome.error);
        step = parseStep(
          withCancellationSignal(native, native.resumeProgram, [step.snapshot, payload, policyJson], signal),
        );
      }
      return step.value;
    }

    start(options = {}) {
      const signal = getAbortSignal(options, 'start options');
      throwIfAborted(signal);
      const { policy, snapshotKey } = createExecutionPolicy(options);
      const step = parseStep(
        withCancellationSignal(
          native,
          native.startProgram,
          [this._program, encodeStartOptions(options.inputs, policy)],
          signal,
        ),
      );
      return materializeStep(step, policy, snapshotKey);
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
  };
}

module.exports = {
  createJsliteClass,
};
