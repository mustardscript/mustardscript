'use strict';

const { JsliteError, callNative } = require('./errors');
const { getAbortSignal, withCancellationSignal } = require('./cancellation');
const {
  cloneSnapshotPolicy,
  encodeSnapshotPolicy,
  rememberProgressPolicy,
  resolveProgressLoadPolicy,
  snapshotIdentity,
} = require('./policy');
const {
  decodeStructured,
  encodeResumePayloadCancel,
  encodeResumePayloadError,
  encodeResumePayloadValue,
} = require('./structured');

const USED_PROGRESS_SNAPSHOTS = new Set();

function createProgressApi(native) {
  class Progress {
    constructor(snapshot, capability, args, policy) {
      this.capability = capability;
      this.args = args;
      this.#snapshot = Buffer.from(snapshot);
      this.#snapshotId = snapshotIdentity(this.#snapshot);
      this.#policy = cloneSnapshotPolicy(policy);
      rememberProgressPolicy(this.#snapshotId, this.#policy);
    }

    #snapshot;
    #snapshotId;
    #policy;

    #claimSnapshot() {
      if (USED_PROGRESS_SNAPSHOTS.has(this.#snapshotId)) {
        throw new JsliteError(
          'Runtime',
          'Progress objects are single-use; this suspended execution was already resumed',
        );
      }
      USED_PROGRESS_SNAPSHOTS.add(this.#snapshotId);
      return Buffer.from(this.#snapshot);
    }

    get snapshot() {
      return Buffer.from(this.#snapshot);
    }

    dump() {
      return {
        capability: this.capability,
        args: this.args.slice(),
        snapshot: this.snapshot,
        token: this.#snapshotId,
      };
    }

    resume(value, options = undefined) {
      const signal = getAbortSignal(options, 'resume options');
      if (signal?.aborted) {
        return this.cancel();
      }
      const payload = encodeResumePayloadValue(value);
      const policyJson = encodeSnapshotPolicy(this.#policy);
      const step = parseStep(
        withCancellationSignal(
          native,
          native.resumeProgram,
          [this.#claimSnapshot(), payload, policyJson],
          signal,
        ),
      );
      return materializeStep(step, this.#policy);
    }

    resumeError(error, options = undefined) {
      const signal = getAbortSignal(options, 'resume options');
      if (signal?.aborted) {
        return this.cancel();
      }
      const payload = encodeResumePayloadError(error);
      const policyJson = encodeSnapshotPolicy(this.#policy);
      const step = parseStep(
        withCancellationSignal(
          native,
          native.resumeProgram,
          [this.#claimSnapshot(), payload, policyJson],
          signal,
        ),
      );
      return materializeStep(step, this.#policy);
    }

    cancel() {
      const policyJson = encodeSnapshotPolicy(this.#policy);
      const step = parseStep(
        callNative(
          native.resumeProgram,
          this.#claimSnapshot(),
          encodeResumePayloadCancel(),
          policyJson,
        ),
      );
      return materializeStep(step, this.#policy);
    }

    static load(state, options = undefined) {
      if (!state || typeof state !== 'object') {
        throw new TypeError('Progress.load() expects a dumped progress object');
      }
      if (!state.snapshot) {
        throw new TypeError('Progress.load() requires snapshot bytes');
      }
      const snapshot = Buffer.from(state.snapshot);
      const snapshotId = snapshotIdentity(snapshot);
      const policy = resolveProgressLoadPolicy(snapshotId, options);
      const inspection = parseSnapshotInspection(
        callNative(native.inspectSnapshot, snapshot, encodeSnapshotPolicy(policy)),
      );
      return new Progress(snapshot, inspection.capability, inspection.args, policy);
    }
  }

  function parseStep(stepJson) {
    const step = JSON.parse(stepJson);
    if (step.type === 'completed') {
      return {
        type: 'completed',
        value: decodeStructured(step.value),
      };
    }
    return {
      type: 'suspended',
      capability: step.capability,
      args: step.args.map(decodeStructured),
      snapshot: Buffer.from(step.snapshot_base64, 'base64'),
    };
  }

  function parseSnapshotInspection(inspectionJson) {
    const inspection = JSON.parse(inspectionJson);
    return {
      capability: inspection.capability,
      args: inspection.args.map(decodeStructured),
    };
  }

  function materializeStep(step, policy) {
    if (step.type === 'completed') {
      return step.value;
    }
    return new Progress(step.snapshot, step.capability, step.args, policy);
  }

  return {
    Progress,
    materializeStep,
    parseStep,
  };
}

module.exports = {
  createProgressApi,
};
