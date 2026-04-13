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
  programIdentity,
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

function isBinaryLike(value) {
  return Buffer.isBuffer(value) || value instanceof Uint8Array;
}

function createProgressApi(native) {
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
  const snapshotHandleRegistry =
    typeof FinalizationRegistry === 'function'
      ? new FinalizationRegistry((snapshotHandle) => {
          try {
            callNative(native.releaseSnapshotHandle, snapshotHandle);
          } catch {
            // Best-effort cleanup only; process shutdown can race native teardown.
          }
        })
      : null;

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
      programHandle = null,
      program = undefined,
      programId = undefined,
      snapshotHandle = null,
      snapshotId = undefined,
    ) {
      this.#capability = capability;
      this.#args = structuredClone(args);
      this.capability = this.#capability;
      this.args = structuredClone(this.#args);
      this.#snapshot =
        snapshot === undefined || snapshot === null ? null : Buffer.from(snapshot);
      this.#snapshotIdentity =
        typeof snapshotId === 'string' && snapshotId.length > 0
          ? snapshotId
          : this.#snapshot !== null
            ? snapshotIdentity(this.#snapshot)
            : null;
      this.#snapshotKey = cloneSnapshotKey(snapshotKey);
      this.#snapshotKeyDigest = snapshotKeyDigest(this.#snapshotKey);
      this.#snapshotToken =
        typeof token === 'string' && token.length > 0
          ? token
          : this.#snapshot !== null
            ? snapshotToken(this.#snapshot, this.#snapshotKey, this.#snapshotIdentity)
            : null;
      this.#suspendedManifest =
        suspendedManifest ?? createSuspendedManifest(this.#capability, this.#args);
      this.#suspendedManifestToken =
        typeof suspendedManifestTokenValue === 'string' && suspendedManifestTokenValue.length > 0
          ? suspendedManifestTokenValue
          : this.#snapshotIdentity === null
            ? null
            : suspendedManifestToken(
                this.#snapshotIdentity,
                this.#suspendedManifest,
                this.#snapshotKey,
              );
      this.#policy = cloneSnapshotPolicy(policy);
      this.#claimState = claimState;
      this.#program = program === undefined || program === null ? null : Buffer.from(program);
      this.#programIdentity =
        typeof programId === 'string' && programId.length > 0
          ? programId
          : this.#program !== null
            ? programIdentity(this.#program)
            : null;
      this.#programHandle = null;
      this.#programHandleToken = null;
      if (typeof programHandle === 'string' && programHandle.length > 0) {
        this.#programHandle = programHandle;
        this.#programHandleToken = {};
        programHandleRegistry?.register(this, programHandle, this.#programHandleToken);
      }
      this.#snapshotHandle = null;
      this.#snapshotHandleToken = null;
      if (typeof snapshotHandle === 'string' && snapshotHandle.length > 0) {
        this.#snapshotHandle = snapshotHandle;
        this.#snapshotHandleToken = {};
        snapshotHandleRegistry?.register(this, snapshotHandle, this.#snapshotHandleToken);
      }
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
    #program;
    #programIdentity;
    #programHandle;
    #programHandleToken;
    #snapshotHandle;
    #snapshotHandleToken;

    #clearSnapshotHandle() {
      if (this.#snapshotHandleToken !== null) {
        snapshotHandleRegistry?.unregister(this.#snapshotHandleToken);
      }
      this.#snapshotHandle = null;
      this.#snapshotHandleToken = null;
    }

    #releaseSnapshotHandle() {
      if (this.#snapshotHandle === null) {
        return;
      }
      const snapshotHandle = this.#snapshotHandle;
      this.#clearSnapshotHandle();
      try {
        callNative(native.releaseSnapshotHandle, snapshotHandle);
      } catch {
        // Best-effort cleanup only.
      }
    }

    #ensureSnapshotBytes() {
      if (this.#snapshot !== null) {
        return Buffer.from(this.#snapshot);
      }
      if (this.#snapshotHandle === null) {
        throw singleUseRuntimeError();
      }
      this.#snapshot = Buffer.from(callNative(native.dumpSnapshotHandle, this.#snapshotHandle));
      this.#snapshotIdentity ??= snapshotIdentity(this.#snapshot);
      this.#snapshotToken ??= snapshotToken(
        this.#snapshot,
        this.#snapshotKey,
        this.#snapshotIdentity,
      );
      this.#suspendedManifestToken ??= suspendedManifestToken(
        this.#snapshotIdentity,
        this.#suspendedManifest,
        this.#snapshotKey,
      );
      return Buffer.from(this.#snapshot);
    }

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

    #consumeSnapshotHandle() {
      if (this.#claimState === 'consumed') {
        throw singleUseRuntimeError();
      }
      if (
        this.#claimState === 'unclaimed' &&
        this.#snapshotIdentity !== null &&
        !claimSharedProgressSnapshot(this.#snapshotIdentity)
      ) {
        throw singleUseRuntimeError();
      }
      if (this.#snapshotHandle === null) {
        return null;
      }
      const snapshotHandle = this.#snapshotHandle;
      this.#clearSnapshotHandle();
      this.#claimState = 'consumed';
      return snapshotHandle;
    }

    #ensureProgramHandle() {
      if (this.#programHandle !== null) {
        return this.#programHandle;
      }
      if (this.#program === null) {
        return null;
      }
      const programHandle = callNative(native.loadProgram, Buffer.from(this.#program));
      this.#programHandle = programHandle;
      this.#programHandleToken = {};
      programHandleRegistry?.register(this, programHandle, this.#programHandleToken);
      return programHandle;
    }

    #ensureProgramBytes() {
      if (this.#program !== null) {
        return Buffer.from(this.#program);
      }
      if (this.#programHandle === null) {
        return null;
      }
      this.#program = Buffer.from(callNative(native.dumpProgram, this.#programHandle));
      this.#programIdentity ??= programIdentity(this.#program);
      return Buffer.from(this.#program);
    }

    #resumeWithPayload(payload, signal) {
      const programHandle = this.#ensureProgramHandle();
      const snapshotHandle = this.#consumeSnapshotHandle();
      if (snapshotHandle !== null) {
        try {
          const nativeArgs = [snapshotHandle, payload];
          const step = parseStep(
            signal === undefined
              ? callNative(native.resumeSnapshotHandle, ...nativeArgs)
              : withCancellationSignal(native, native.resumeSnapshotHandle, nativeArgs, signal),
          );
          return materializeStep(step, this.#policy, this.#snapshotKey, programHandle, this.#program);
        } finally {
          try {
            callNative(native.releaseSnapshotHandle, snapshotHandle);
          } catch {
            // Best-effort cleanup only.
          }
        }
      }

      const snapshot = this.#consumeSnapshot();
      const policyJson = encodeSnapshotPolicy(this.#policy, {
        snapshotId: this.#snapshotIdentity,
        snapshotKey: this.#snapshotKey,
        snapshotToken: this.#snapshotToken,
      });
      const nativeResume =
        programHandle === null ? native.resumeProgram : native.resumeDetachedProgram;
      const nativeArgs =
        programHandle === null
          ? [snapshot, payload, policyJson]
          : [programHandle, snapshot, payload, policyJson];
      const step = parseStep(
        signal === undefined
          ? callNative(nativeResume, ...nativeArgs)
          : withCancellationSignal(native, nativeResume, nativeArgs, signal),
      );
      return materializeStep(step, this.#policy, this.#snapshotKey, programHandle, this.#program);
    }

    get snapshot() {
      return this.#ensureSnapshotBytes();
    }

    dump() {
      const snapshot = this.#ensureSnapshotBytes();
      this.#snapshotIdentity ??= snapshotIdentity(snapshot);
      this.#snapshotToken ??= snapshotToken(snapshot, this.#snapshotKey, this.#snapshotIdentity);
      this.#suspendedManifestToken ??= suspendedManifestToken(
        this.#snapshotIdentity,
        this.#suspendedManifest,
        this.#snapshotKey,
      );
      const dumped = {
        capability: this.#capability,
        args: structuredClone(this.#args),
        snapshot,
        snapshot_id: this.#snapshotIdentity,
        snapshot_key_digest: this.#snapshotKeyDigest,
        token: this.#snapshotToken,
        suspended_manifest: this.#suspendedManifest,
        suspended_manifest_token: this.#suspendedManifestToken,
      };
      const program = this.#ensureProgramBytes();
      if (program !== null) {
        dumped.program = program;
        dumped.program_id = this.#programIdentity ?? programIdentity(program);
      }
      return dumped;
    }

    resume(value, options = undefined) {
      const signal = getAbortSignal(options, 'resume options');
      if (signal?.aborted) {
        return this.cancel();
      }
      return this.#resumeWithPayload(encodeResumePayloadValue(value), signal);
    }

    resumeError(error, options = undefined) {
      const signal = getAbortSignal(options, 'resume options');
      if (signal?.aborted) {
        return this.cancel();
      }
      return this.#resumeWithPayload(encodeResumePayloadError(error), signal);
    }

    cancel() {
      return this.#resumeWithPayload(encodeResumePayloadCancel(), undefined);
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

      let dumpedProgram;
      let dumpedProgramId;
      if (state.program !== undefined) {
        if (!isBinaryLike(state.program)) {
          throw new TypeError('Progress.load() requires dumped program bytes as Buffer or Uint8Array');
        }
        if (typeof state.program_id !== 'string' || state.program_id.length === 0) {
          throw new TypeError(
            'Progress.load() requires dumped program_id metadata when program bytes are present',
          );
        }
        dumpedProgram = Buffer.from(state.program);
        dumpedProgramId = programIdentity(dumpedProgram);
        if (dumpedProgramId !== state.program_id) {
          throw new MustardError(
            'Serialization',
            'Progress.load() rejected a tampered or mismatched detached program',
          );
        }
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
      const policyJson = encodeSnapshotPolicy(context.policy, {
        snapshotId: state.snapshot_id,
        snapshotKey: context.snapshotKey,
        snapshotToken: state.token,
      });
      const releaseClaim = claimSnapshotForLoad(native, snapshotIdentityValue);
      try {
        let loadedProgramHandle = null;
        let snapshotHandle = null;
        const loadSnapshotHandle = () => {
          if (dumpedProgram === undefined) {
            return callNative(native.loadSnapshotHandle, snapshot, policyJson);
          }
          loadedProgramHandle = callNative(native.loadProgram, Buffer.from(dumpedProgram));
          return callNative(
            native.loadDetachedSnapshotHandle,
            loadedProgramHandle,
            snapshot,
            policyJson,
          );
        };
        if (suspendedManifest !== null) {
          assertAuthorizedSuspendedCapability(
            context.policy,
            suspendedManifest.capability,
          );
          try {
            snapshotHandle = loadSnapshotHandle();
          } catch (error) {
            if (loadedProgramHandle !== null) {
              try {
                callNative(native.releaseProgram, loadedProgramHandle);
              } catch {
                // Best-effort cleanup only.
              }
            }
            throw error;
          }
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
            loadedProgramHandle,
            dumpedProgram,
            dumpedProgramId,
            snapshotHandle,
            state.snapshot_id,
          );
        }

        try {
          snapshotHandle = loadSnapshotHandle();
          const inspection = parseSnapshotInspection(
            callNative(native.inspectSnapshotHandle, snapshotHandle),
          );
          return new Progress(
            snapshot,
            inspection.capability,
            inspection.args,
            context.policy,
            context.snapshotKey,
            state.token,
            'claimed',
            undefined,
            undefined,
            loadedProgramHandle,
            dumpedProgram,
            dumpedProgramId,
            snapshotHandle,
            state.snapshot_id,
          );
        } catch (error) {
          if (snapshotHandle !== null) {
            try {
              callNative(native.releaseSnapshotHandle, snapshotHandle);
            } catch {
              // Best-effort cleanup only.
            }
          }
          if (loadedProgramHandle !== null) {
            try {
              callNative(native.releaseProgram, loadedProgramHandle);
            } catch {
              // Best-effort cleanup only.
            }
          }
          throw error;
        }
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
      snapshot:
        typeof step.snapshot_base64 === 'string'
          ? Buffer.from(step.snapshot_base64, 'base64')
          : null,
      snapshotHandle:
        typeof step.snapshot_handle === 'string' && step.snapshot_handle.length > 0
          ? step.snapshot_handle
          : null,
    };
  }

  function parseSnapshotInspection(inspectionJson) {
    const inspection = JSON.parse(inspectionJson);
    return {
      capability: inspection.capability,
      args: inspection.args.map(decodeStructured),
    };
  }

  function materializeStep(step, policy, snapshotKey, programHandle = null, program = undefined) {
    if (step.type === 'completed') {
      return step.value;
    }
    let ownedProgramHandle = null;
    if (typeof programHandle === 'string' && programHandle.length > 0) {
      ownedProgramHandle = callNative(native.retainProgram, programHandle);
    }
    return new Progress(
      step.snapshot,
      step.capability,
      step.args,
      policy,
      snapshotKey,
      undefined,
      'unclaimed',
      undefined,
      undefined,
      ownedProgramHandle,
      program,
      undefined,
      step.snapshotHandle,
    );
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
