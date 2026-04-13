'use strict';

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { performance } = require('node:perf_hooks');

const { MustardError, callNative } = require('./errors.ts');
const { getAbortSignal, withCancellationSignal } = require('./cancellation.ts');
const {
  assertSuspendedManifest,
  cloneSnapshotPolicy,
  cloneSnapshotKey,
  createSuspendedManifest,
  encodeSnapshotPolicy,
  resolveProgressLoadContext,
  snapshotIdentity,
  snapshotKeyDigest,
  snapshotToken,
  suspendedManifestToken,
} = require('./policy.ts');
const {
  decodeStructured,
  encodeResumePayloadCancel,
  encodeResumePayloadError,
  encodeResumePayloadValue,
} = require('./structured.ts');

const SHARED_PROGRESS_REGISTRY_ROOT = path.join(
  os.tmpdir(),
  'mustard-progress-registry',
  `${process.pid}-${Math.round(performance.timeOrigin)}`,
);

function sharedProgressSnapshotPath(snapshotIdentityValue) {
  return path.join(SHARED_PROGRESS_REGISTRY_ROOT, snapshotIdentityValue);
}

function ensureSharedProgressRegistryRoot() {
  fs.mkdirSync(SHARED_PROGRESS_REGISTRY_ROOT, { recursive: true });
}

function isSharedProgressSnapshotUsed(snapshotIdentityValue) {
  ensureSharedProgressRegistryRoot();
  return fs.existsSync(sharedProgressSnapshotPath(snapshotIdentityValue));
}

function releaseSharedProgressSnapshot(snapshotIdentityValue) {
  try {
    fs.rmSync(sharedProgressSnapshotPath(snapshotIdentityValue));
  } catch (error) {
    if (error && typeof error === 'object' && error.code !== 'ENOENT') {
      throw error;
    }
  }
}

function claimSharedProgressSnapshot(snapshotIdentityValue) {
  ensureSharedProgressRegistryRoot();
  try {
    const fd = fs.openSync(sharedProgressSnapshotPath(snapshotIdentityValue), 'wx', 0o600);
    fs.closeSync(fd);
    return true;
  } catch (error) {
    if (error && typeof error === 'object' && error.code === 'EEXIST') {
      return false;
    }
    throw error;
  }
}

function singleUseRuntimeError() {
  return new MustardError(
    'Runtime',
    'Progress objects are single-use; this suspended execution was already resumed',
  );
}

function releaseClaimedSnapshot(native, snapshotIdentityValue) {
  void native;
  releaseSharedProgressSnapshot(snapshotIdentityValue);
}

function claimSnapshotForLoad(native, snapshotIdentityValue) {
  void native;
  if (!claimSharedProgressSnapshot(snapshotIdentityValue)) {
    throw singleUseRuntimeError();
  }
  return () => {
    releaseClaimedSnapshot(native, snapshotIdentityValue);
  };
}

function assertSnapshotNotUsed(native, snapshotIdentityValue) {
  void native;
  if (isSharedProgressSnapshotUsed(snapshotIdentityValue)) {
    throw singleUseRuntimeError();
  }
}

function createProgressApi(native) {
  function assertAuthorizedSuspendedCapability(policy, capability) {
    if (policy.capabilities.includes(capability)) {
      return;
    }
    throw new MustardError(
      'Serialization',
      `snapshot policy rejected unauthorized capability \`${capability}\``,
    );
  }

  class Progress {
    constructor(
      snapshot,
      capability,
      args,
      policy,
      snapshotKey,
      token = undefined,
      claimState = 'unclaimed',
      suspendedManifest = undefined,
      suspendedManifestTokenValue = undefined,
    ) {
      this.#capability = capability;
      this.#args = structuredClone(args);
      this.capability = this.#capability;
      this.args = structuredClone(this.#args);
      this.#snapshot = Buffer.from(snapshot);
      this.#snapshotIdentity = snapshotIdentity(this.#snapshot);
      this.#snapshotKey = cloneSnapshotKey(snapshotKey);
      this.#snapshotKeyDigest = snapshotKeyDigest(this.#snapshotKey);
      this.#snapshotToken = token ?? snapshotToken(this.#snapshot, this.#snapshotKey);
      this.#suspendedManifest =
        suspendedManifest ?? createSuspendedManifest(this.#capability, this.#args);
      this.#suspendedManifestToken =
        suspendedManifestTokenValue ??
        suspendedManifestToken(
          this.#snapshotIdentity,
          this.#suspendedManifest,
          this.#snapshotKey,
        );
      this.#policy = cloneSnapshotPolicy(policy);
      this.#claimState = claimState;
    }

    #capability;
    #args;
    #snapshot;
    #snapshotIdentity;
    #snapshotKey;
    #snapshotKeyDigest;
    #snapshotToken;
    #suspendedManifest;
    #suspendedManifestToken;
    #policy;
    #claimState;

    #consumeSnapshot() {
      if (this.#claimState === 'consumed') {
        throw singleUseRuntimeError();
      }
      if (this.#claimState === 'claimed') {
        this.#claimState = 'consumed';
        return Buffer.from(this.#snapshot);
      }
      if (!claimSharedProgressSnapshot(this.#snapshotIdentity)) {
        throw singleUseRuntimeError();
      }
      this.#claimState = 'consumed';
      return Buffer.from(this.#snapshot);
    }

    get snapshot() {
      return Buffer.from(this.#snapshot);
    }

    dump() {
      return {
        capability: this.#capability,
        args: structuredClone(this.#args),
        snapshot: this.snapshot,
        snapshot_id: this.#snapshotIdentity,
        snapshot_key_digest: this.#snapshotKeyDigest,
        token: this.#snapshotToken,
        suspended_manifest: this.#suspendedManifest,
        suspended_manifest_token: this.#suspendedManifestToken,
      };
    }

    resume(value, options = undefined) {
      const signal = getAbortSignal(options, 'resume options');
      if (signal?.aborted) {
        return this.cancel();
      }
      const payload = encodeResumePayloadValue(value);
      const policyJson = encodeSnapshotPolicy(this.#policy, {
        snapshotId: this.#snapshotIdentity,
        snapshotKey: this.#snapshotKey,
        snapshotToken: this.#snapshotToken,
      });
      const step = parseStep(
        withCancellationSignal(
          native,
          native.resumeProgram,
          [this.#consumeSnapshot(), payload, policyJson],
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
      const policyJson = encodeSnapshotPolicy(this.#policy, {
        snapshotId: this.#snapshotIdentity,
        snapshotKey: this.#snapshotKey,
        snapshotToken: this.#snapshotToken,
      });
      const step = parseStep(
        withCancellationSignal(
          native,
          native.resumeProgram,
          [this.#consumeSnapshot(), payload, policyJson],
          signal,
        ),
      );
      return materializeStep(step, this.#policy, this.#snapshotKey);
    }

    cancel() {
      const policyJson = encodeSnapshotPolicy(this.#policy, {
        snapshotId: this.#snapshotIdentity,
        snapshotKey: this.#snapshotKey,
        snapshotToken: this.#snapshotToken,
      });
      const step = parseStep(
        callNative(
          native.resumeProgram,
          this.#consumeSnapshot(),
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
      if (typeof state.snapshot_id !== 'string' || state.snapshot_id.length === 0) {
        throw new TypeError('Progress.load() requires dumped snapshot_id metadata');
      }
      if (
        typeof state.snapshot_key_digest !== 'string' ||
        state.snapshot_key_digest.length === 0
      ) {
        throw new TypeError('Progress.load() requires dumped snapshot_key_digest metadata');
      }
      if (typeof state.token !== 'string' || state.token.length === 0) {
        throw new TypeError('Progress.load() requires a dumped progress token');
      }
      const snapshot = Buffer.from(state.snapshot);
      let snapshotIdentityValue;
      try {
        snapshotIdentityValue = snapshotIdentity(snapshot);
      } catch (error) {
        if (error instanceof MustardError && error.kind === 'Serialization') {
          throw new MustardError(
            'Serialization',
            'Progress.load() rejected a tampered or unauthenticated snapshot',
            error,
          );
        }
        throw error;
      }
      if (state.snapshot_id !== snapshotIdentityValue) {
        throw new MustardError(
          'Serialization',
          'Progress.load() rejected a tampered or unauthenticated snapshot',
        );
      }
      assertSnapshotNotUsed(native, snapshotIdentityValue);
      const context = resolveProgressLoadContext(
        state,
        snapshot,
        options,
        snapshotIdentityValue,
      );
      const suspendedManifest = assertSuspendedManifest(
        state,
        context.snapshotKey,
        snapshotIdentityValue,
      );
      const releaseClaim = claimSnapshotForLoad(native, snapshotIdentityValue);
      try {
        if (suspendedManifest !== null) {
          assertAuthorizedSuspendedCapability(
            context.policy,
            suspendedManifest.capability,
          );
          return new Progress(
            snapshot,
            suspendedManifest.capability,
            suspendedManifest.args,
            context.policy,
            context.snapshotKey,
            state.token,
            'claimed',
            state.suspended_manifest,
            state.suspended_manifest_token,
          );
        }
        const inspection = parseSnapshotInspection(
          callNative(
            native.inspectSnapshot,
            snapshot,
            encodeSnapshotPolicy(context.policy, {
              snapshotId: state.snapshot_id,
              snapshotKey: context.snapshotKey,
              snapshotToken: state.token,
            }),
          ),
        );
        return new Progress(
          snapshot,
          inspection.capability,
          inspection.args,
          context.policy,
          context.snapshotKey,
          state.token,
          'claimed',
        );
      } catch (error) {
        releaseClaim();
        throw error;
      }
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
};
