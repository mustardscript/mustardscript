'use strict';

const { JsliteError, callNative } = require('./errors');
const { getAbortSignal, withCancellationSignal } = require('./cancellation');
const {
  cloneSnapshotPolicy,
  cloneSnapshotKey,
  encodeSnapshotPolicy,
  rememberProgressPolicy,
  resolveProgressLoadContext,
  snapshotToken,
} = require('./policy');
const {
  decodeStructured,
  encodeResumePayloadCancel,
  encodeResumePayloadError,
  encodeResumePayloadValue,
} = require('./structured');

const USED_PROGRESS_SNAPSHOTS = new Set();
const USED_PROGRESS_SNAPSHOT_CACHE_LIMIT = 4096;

function rememberUsedSnapshot(snapshotTokenValue) {
  if (USED_PROGRESS_SNAPSHOTS.has(snapshotTokenValue)) {
    USED_PROGRESS_SNAPSHOTS.delete(snapshotTokenValue);
  }
  USED_PROGRESS_SNAPSHOTS.add(snapshotTokenValue);
  while (USED_PROGRESS_SNAPSHOTS.size > USED_PROGRESS_SNAPSHOT_CACHE_LIMIT) {
    const oldest = USED_PROGRESS_SNAPSHOTS.values().next().value;
    USED_PROGRESS_SNAPSHOTS.delete(oldest);
  }
}

function createProgressApi(native) {
  class Progress {
    constructor(snapshot, capability, args, policy, snapshotKey, token = undefined) {
      this.capability = capability;
      this.args = args;
      this.#snapshot = Buffer.from(snapshot);
      this.#snapshotKey = cloneSnapshotKey(snapshotKey);
      this.#snapshotToken = token ?? snapshotToken(this.#snapshot, this.#snapshotKey);
      this.#policy = cloneSnapshotPolicy(policy);
      rememberProgressPolicy(this.#snapshotToken, this.#policy, this.#snapshotKey);
    }

    #snapshot;
    #snapshotKey;
    #snapshotToken;
    #policy;

    #claimSnapshot() {
      if (USED_PROGRESS_SNAPSHOTS.has(this.#snapshotToken)) {
        throw new JsliteError(
          'Runtime',
          'Progress objects are single-use; this suspended execution was already resumed',
        );
      }
      rememberUsedSnapshot(this.#snapshotToken);
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
        token: this.#snapshotToken,
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
      return materializeStep(step, this.#policy, this.#snapshotKey);
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
      return materializeStep(step, this.#policy, this.#snapshotKey);
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
      return materializeStep(step, this.#policy, this.#snapshotKey);
    }

    static load(state, options = undefined) {
      if (!state || typeof state !== 'object') {
        throw new TypeError('Progress.load() expects a dumped progress object');
      }
      if (!state.snapshot) {
        throw new TypeError('Progress.load() requires snapshot bytes');
      }
      if (typeof state.token !== 'string' || state.token.length === 0) {
        throw new TypeError('Progress.load() requires a dumped progress token');
      }
      const snapshot = Buffer.from(state.snapshot);
      const context = resolveProgressLoadContext(state, snapshot, options);
      const inspection = parseSnapshotInspection(
        callNative(native.inspectSnapshot, snapshot, encodeSnapshotPolicy(context.policy)),
      );
      return new Progress(
        snapshot,
        inspection.capability,
        inspection.args,
        context.policy,
        context.snapshotKey,
        state.token,
      );
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

  function materializeStep(step, policy, snapshotKey) {
    if (step.type === 'completed') {
      return step.value;
    }
    return new Progress(step.snapshot, step.capability, step.args, policy, snapshotKey);
  }

  return {
    Progress,
    materializeStep,
    parseStep,
  };
}

module.exports = {
  createProgressApi,
  USED_PROGRESS_SNAPSHOT_CACHE_LIMIT,
};
