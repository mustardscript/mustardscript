'use strict';

const { callNative } = require('./errors');
const { getAbortSignal, settleCapabilityInvocation, withCancellationSignal } = require('./cancellation');
const { createExecutionPolicy, encodeSnapshotPolicy, snapshotToken } = require('./policy');
const {
  encodeResumePayloadCancel,
  encodeResumePayloadError,
  encodeResumePayloadValue,
  encodeStartOptions,
} = require('./structured');

function createJsliteClass({ native, materializeStep, parseStep }) {
  return class Jslite {
    constructor(code, options = {}) {
      this._program = callNative(native.compileProgram, code);
      this._inputNames = options.inputs ?? [];
    }

    async run(options = {}) {
      const signal = getAbortSignal(options, 'run options');
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
        const policyJson = encodeSnapshotPolicy(policy, {
          snapshotKey,
          snapshotToken: snapshotToken(step.snapshot, snapshotKey),
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
