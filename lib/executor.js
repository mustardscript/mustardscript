'use strict';

const { randomUUID } = require('node:crypto');
const { setTimeout: delay } = require('node:timers/promises');
const { types } = require('node:util');

const { JsliteError, normalizeNativeError } = require('./errors');

const TERMINAL_STATES = new Set(['completed', 'failed', 'cancelled']);
const RUNNABLE_STATES = new Set(['queued', 'waiting']);
const DEFAULT_MAX_CONCURRENT_JOBS = 1;
const DEFAULT_POLL_INTERVAL_MS = 25;

function assertPlainObject(value, label) {
  if (value === null || typeof value !== 'object' || Array.isArray(value) || types.isProxy(value)) {
    throw new TypeError(`${label} must be a plain object`);
  }
  const prototype = Object.getPrototypeOf(value);
  if (prototype !== Object.prototype && prototype !== null) {
    throw new TypeError(`${label} must be a plain object`);
  }
}

function cloneJobRecord(record) {
  return structuredClone(record);
}

function clonePersistedProgress(progress) {
  return {
    capability: progress.capability,
    args: structuredClone(progress.args),
    snapshot: Buffer.from(progress.snapshot),
    token: progress.token,
  };
}

function sanitizeFailure(error) {
  const normalized = normalizeNativeError(error);
  if (normalized instanceof JsliteError) {
    return {
      name: normalized.name,
      message: normalized.message,
    };
  }
  if (normalized && typeof normalized === 'object') {
    const name =
      typeof normalized.name === 'string' && normalized.name.length > 0
        ? normalized.name
        : 'Error';
    const message =
      typeof normalized.message === 'string' && normalized.message.length > 0
        ? normalized.message
        : String(normalized);
    const sanitized = { name, message };
    if (typeof normalized.code === 'string') {
      sanitized.code = normalized.code;
    }
    if (normalized.details !== undefined) {
      sanitized.details = normalized.details;
    }
    return sanitized;
  }
  return {
    name: 'Error',
    message: String(normalized),
  };
}

function isCancellationFailure(error) {
  const normalized = normalizeNativeError(error);
  return (
    normalized instanceof JsliteError &&
    normalized.kind === 'Limit' &&
    normalized.message.includes('execution cancelled')
  );
}

function validateTransition(currentState, nextState) {
  if (currentState === nextState) {
    return;
  }
  const allowed = new Set([
    'queued:running',
    'queued:cancelled',
    'running:waiting',
    'running:completed',
    'running:failed',
    'running:cancelled',
    'waiting:running',
    'waiting:failed',
    'waiting:cancelled',
  ]);
  if (!allowed.has(`${currentState}:${nextState}`)) {
    throw new Error(`invalid job transition from ${currentState} to ${nextState}`);
  }
}

async function waitForSignalOrDelay(signal, ms) {
  if (signal?.aborted) {
    return;
  }
  try {
    await delay(ms, undefined, { signal });
  } catch (error) {
    if (error?.name !== 'AbortError') {
      throw error;
    }
  }
}

class InMemoryJsliteExecutorStore {
  constructor() {
    this._jobs = new Map();
    this._progress = new Map();
    this._claims = new Map();
    this._cancellationRequests = new Set();
  }

  _jobs;
  _progress;
  _claims;
  _cancellationRequests;

  async enqueue(record) {
    if (this._jobs.has(record.jobId)) {
      return {
        jobId: record.jobId,
        inserted: false,
      };
    }
    this._jobs.set(record.jobId, cloneJobRecord(record));
    return {
      jobId: record.jobId,
      inserted: true,
    };
  }

  async get(jobId) {
    const record = this._jobs.get(jobId);
    return record ? cloneJobRecord(record) : null;
  }

  async claimRunnable(limit, workerId) {
    const claimed = [];
    for (const [jobId, record] of this._jobs.entries()) {
      if (claimed.length >= limit) {
        break;
      }
      if (!RUNNABLE_STATES.has(record.state)) {
        continue;
      }
      if (this._claims.has(jobId)) {
        continue;
      }
      this._claims.set(jobId, workerId);
      claimed.push(jobId);
    }
    return claimed;
  }

  async releaseClaim(jobId, workerId) {
    const owner = this._claims.get(jobId);
    if (owner === workerId) {
      this._claims.delete(jobId);
    }
  }

  async update(jobId, patch) {
    const current = this._jobs.get(jobId);
    if (!current) {
      throw new Error(`unknown job id ${jobId}`);
    }
    const nextState = patch.state ?? current.state;
    validateTransition(current.state, nextState);
    const next = {
      ...current,
      ...cloneJobRecord({ ...current, ...patch }),
      state: nextState,
      updatedAt: Date.now(),
    };
    this._jobs.set(jobId, next);
  }

  async saveProgress(jobId, progress) {
    const record = this._jobs.get(jobId);
    if (!record) {
      throw new Error(`unknown job id ${jobId}`);
    }
    if (TERMINAL_STATES.has(record.state)) {
      throw new Error(`cannot save progress for terminal job ${jobId}`);
    }
    this._progress.set(jobId, clonePersistedProgress(progress));
  }

  async loadProgress(jobId) {
    const progress = this._progress.get(jobId);
    return progress ? clonePersistedProgress(progress) : null;
  }

  async deleteProgress(jobId) {
    this._progress.delete(jobId);
  }

  async requestCancel(jobId) {
    const record = this._jobs.get(jobId);
    if (!record) {
      return 'ignored';
    }
    if (TERMINAL_STATES.has(record.state)) {
      return 'ignored';
    }
    if (record.state === 'queued') {
      this._jobs.set(jobId, {
        ...record,
        state: 'cancelled',
        capability: undefined,
        args: undefined,
        result: undefined,
        error: {
          name: 'JsliteLimitError',
          message: 'execution cancelled',
        },
        updatedAt: Date.now(),
      });
      this._progress.delete(jobId);
      return 'cancelled';
    }
    this._cancellationRequests.add(jobId);
    return 'requested';
  }

  async consumeCancel(jobId) {
    if (!this._cancellationRequests.has(jobId)) {
      return false;
    }
    this._cancellationRequests.delete(jobId);
    return true;
  }
}

function createExecutorApi({ Jslite, Progress }) {
  class JsliteExecutor {
    constructor(options) {
      if (options === null || typeof options !== 'object') {
        throw new TypeError('JsliteExecutor options must be an object');
      }
      const {
        program,
        capabilities,
        snapshotKey,
        store,
        limits = {},
      } = options;
      if (!(program instanceof Jslite)) {
        throw new TypeError('JsliteExecutor options.program must be a Jslite instance');
      }
      assertPlainObject(capabilities, 'JsliteExecutor options.capabilities');
      if (store === undefined || store === null || typeof store !== 'object') {
        throw new TypeError('JsliteExecutor options.store must be a JsliteExecutorStore');
      }
      for (const method of [
        'enqueue',
        'get',
        'claimRunnable',
        'releaseClaim',
        'update',
        'saveProgress',
        'loadProgress',
        'deleteProgress',
        'requestCancel',
        'consumeCancel',
      ]) {
        if (typeof store[method] !== 'function') {
          throw new TypeError(`JsliteExecutor options.store is missing ${method}()`);
        }
      }
      this._program = program;
      this._capabilities = capabilities;
      this._snapshotKey = snapshotKey;
      this._store = store;
      this._limits = { ...limits };
    }

    _program;
    _capabilities;
    _snapshotKey;
    _store;
    _limits;

    async enqueue(input, options = {}) {
      assertPlainObject(input, 'JsliteExecutor.enqueue() input');
      if (options === null || typeof options !== 'object') {
        throw new TypeError('JsliteExecutor.enqueue() options must be an object');
      }
      const now = Date.now();
      const jobId = options.jobId ?? randomUUID();
      const record = {
        jobId,
        state: 'queued',
        input: structuredClone(input),
        attempts: 0,
        createdAt: now,
        updatedAt: now,
      };
      const result = await this._store.enqueue(record);
      return result.jobId;
    }

    async get(jobId) {
      return this._store.get(jobId);
    }

    async cancel(jobId) {
      await this._store.requestCancel(jobId);
    }

    async runWorker(options = {}) {
      if (options === null || typeof options !== 'object') {
        throw new TypeError('JsliteExecutor.runWorker() options must be an object');
      }
      const workerId = randomUUID();
      const maxConcurrentJobs = options.maxConcurrentJobs ?? DEFAULT_MAX_CONCURRENT_JOBS;
      if (!Number.isInteger(maxConcurrentJobs) || maxConcurrentJobs <= 0) {
        throw new TypeError('JsliteExecutor.runWorker() maxConcurrentJobs must be a positive integer');
      }
      const signal = options.signal;
      const drain = options.drain === true;
      const inFlight = new Set();

      while (!signal?.aborted) {
        const available = maxConcurrentJobs - inFlight.size;
        let claimed = [];
        if (available > 0) {
          claimed = await this._store.claimRunnable(available, workerId, Date.now());
          for (const jobId of claimed) {
            const task = this._processClaimedJob(jobId, workerId).finally(() => {
              inFlight.delete(task);
            });
            inFlight.add(task);
          }
        }

        if (drain && inFlight.size === 0 && claimed.length === 0) {
          return;
        }

        if (inFlight.size === 0) {
          await waitForSignalOrDelay(signal, DEFAULT_POLL_INTERVAL_MS);
          continue;
        }

        if (claimed.length === 0 || inFlight.size >= maxConcurrentJobs) {
          await Promise.race(inFlight);
        }
      }

      await Promise.allSettled(inFlight);
    }

    async _processClaimedJob(jobId, workerId) {
      try {
        const record = await this._store.get(jobId);
        if (record === null || TERMINAL_STATES.has(record.state)) {
          return;
        }

        if (record.state === 'queued') {
          await this._store.update(jobId, {
            state: 'running',
            attempts: record.attempts + 1,
            capability: undefined,
            args: undefined,
            result: undefined,
            error: undefined,
          });
          let step;
          try {
            step = this._program.start({
              inputs: record.input,
              capabilities: this._capabilities,
              limits: this._limits,
              snapshotKey: this._snapshotKey,
            });
          } catch (error) {
            await this._failJob(jobId, error);
            return;
          }
          await this._driveExecution(jobId, step);
          return;
        }

        if (record.state === 'waiting') {
          await this._resumeWaitingJob(jobId, record);
        }
      } finally {
        await this._store.releaseClaim(jobId, workerId);
      }
    }

    async _resumeWaitingJob(jobId, record) {
      const dumped = await this._store.loadProgress(jobId);
      if (dumped === null) {
        await this._failJob(jobId, new JsliteError('Serialization', `missing stored progress for job ${jobId}`));
        return;
      }

      let progress;
      try {
        progress = Progress.load(dumped, {
          capabilities: this._capabilities,
          limits: this._limits,
          snapshotKey: this._snapshotKey,
        });
      } catch (error) {
        await this._failJob(jobId, error);
        return;
      }

      if (await this._store.consumeCancel(jobId)) {
        await this._store.update(jobId, {
          state: 'running',
          capability: undefined,
          args: undefined,
        });
        try {
          const step = progress.cancel();
          await this._driveExecution(jobId, step);
        } catch (error) {
          if (isCancellationFailure(error)) {
            await this._cancelJob(jobId, error);
          } else {
            await this._failJob(jobId, error);
          }
        }
        return;
      }

      const handler = this._capabilities[progress.capability];
      if (typeof handler !== 'function') {
        await this._failJob(
          jobId,
          new JsliteError('Runtime', `Missing capability: ${progress.capability}`),
        );
        return;
      }

      let outcome;
      try {
        outcome = {
          type: 'value',
          value: await handler(...progress.args),
        };
      } catch (error) {
        outcome = {
          type: 'error',
          error,
        };
      }

      await this._store.update(jobId, {
        state: 'running',
        capability: undefined,
        args: undefined,
      });

      try {
        const shouldCancel = await this._store.consumeCancel(jobId);
        const step = shouldCancel
          ? progress.cancel()
          : outcome.type === 'value'
            ? progress.resume(outcome.value)
            : progress.resumeError(outcome.error);
        await this._driveExecution(jobId, step);
      } catch (error) {
        if (isCancellationFailure(error)) {
          await this._cancelJob(jobId, error);
        } else {
          await this._failJob(jobId, error);
        }
      }
    }

    async _driveExecution(jobId, step) {
      let current = step;
      while (current instanceof Progress) {
        await this._store.saveProgress(jobId, current.dump());
        await this._store.update(jobId, {
          state: 'waiting',
          capability: current.capability,
          args: structuredClone(current.args),
        });
        await this._resumeWaitingJob(jobId, await this._store.get(jobId));
        return;
      }

      await this._store.update(jobId, {
        state: 'completed',
        capability: undefined,
        args: undefined,
        result: current,
        error: undefined,
      });
      await this._store.deleteProgress(jobId);
    }

    async _failJob(jobId, error) {
      await this._store.update(jobId, {
        state: 'failed',
        capability: undefined,
        args: undefined,
        result: undefined,
        error: sanitizeFailure(error),
      });
      await this._store.deleteProgress(jobId);
    }

    async _cancelJob(jobId, error) {
      await this._store.update(jobId, {
        state: 'cancelled',
        capability: undefined,
        args: undefined,
        result: undefined,
        error: sanitizeFailure(error),
      });
      await this._store.deleteProgress(jobId);
    }
  }

  return {
    InMemoryJsliteExecutorStore,
    JsliteExecutor,
  };
}

module.exports = {
  createExecutorApi,
  InMemoryJsliteExecutorStore,
};
