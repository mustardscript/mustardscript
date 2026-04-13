'use strict';

const { setTimeout: delay } = require('node:timers/promises');

const {
  assert,
  InMemoryMustardExecutorStore,
  MustardExecutor,
  test,
} = require('./support/helpers.js');

function buildExecutor(source, capabilities, options = {}) {
  const { Mustard } = require('../../index.ts');
  return new MustardExecutor({
    program: new Mustard(source, options.compileOptions),
    capabilities,
    snapshotKey: Buffer.from('executor-snapshot-key'),
    store: options.store ?? new InMemoryMustardExecutorStore(),
    limits: options.limits,
  });
}

function clonePersistedProgress(progress) {
  return {
    capability: progress.capability,
    args: structuredClone(progress.args),
    snapshot: Buffer.from(progress.snapshot),
    snapshot_id: progress.snapshot_id,
    snapshot_key_digest: progress.snapshot_key_digest,
    token: progress.token,
    suspended_manifest:
      typeof progress.suspended_manifest === 'string'
        ? progress.suspended_manifest
        : undefined,
    suspended_manifest_token:
      typeof progress.suspended_manifest_token === 'string'
        ? progress.suspended_manifest_token
        : undefined,
  };
}

test('executor runs queued jobs to completion', async () => {
  const executor = buildExecutor(
    `
      const response = fetch_data(seed);
      response + 1;
    `,
    {
      fetch_data(value) {
        return value * 2;
      },
    },
    {
      compileOptions: { inputs: ['seed'] },
    },
  );

  const jobId = await executor.enqueue({ seed: 4 }, { jobId: 'job-1' });
  await executor.runWorker({ drain: true });

  const job = await executor.get(jobId);
  assert.deepEqual(job, {
    jobId: 'job-1',
    state: 'completed',
    input: { seed: 4 },
    capability: undefined,
    args: undefined,
    result: 9,
    error: undefined,
    attempts: 1,
    createdAt: job.createdAt,
    updatedAt: job.updatedAt,
  });
  assert.ok(job.updatedAt >= job.createdAt);
});

test('executor enqueue is idempotent by job id', async () => {
  const executor = buildExecutor('seed;', {}, {
    compileOptions: { inputs: ['seed'] },
  });

  const first = await executor.enqueue({ seed: 1 }, { jobId: 'job-1' });
  const second = await executor.enqueue({ seed: 2 }, { jobId: 'job-1' });

  assert.equal(first, 'job-1');
  assert.equal(second, 'job-1');

  await executor.runWorker({ drain: true });
  const job = await executor.get('job-1');
  assert.equal(job.result, 1);
  assert.equal(job.attempts, 1);
});

test('executor respects maxConcurrentJobs across async capabilities', async () => {
  let active = 0;
  let maxActive = 0;

  const executor = buildExecutor(
    `
      const response = fetch_data(seed);
      response * 10;
    `,
    {
      async fetch_data(value) {
        active += 1;
        maxActive = Math.max(maxActive, active);
        await delay(20);
        active -= 1;
        return value;
      },
    },
    {
      compileOptions: { inputs: ['seed'] },
    },
  );

  await Promise.all([
    executor.enqueue({ seed: 1 }, { jobId: 'job-1' }),
    executor.enqueue({ seed: 2 }, { jobId: 'job-2' }),
    executor.enqueue({ seed: 3 }, { jobId: 'job-3' }),
  ]);

  await executor.runWorker({ drain: true, maxConcurrentJobs: 2 });

  assert.equal(maxActive, 2);
  assert.equal((await executor.get('job-1')).result, 10);
  assert.equal((await executor.get('job-2')).result, 20);
  assert.equal((await executor.get('job-3')).result, 30);
});

test('executor records guest-safe failures from host capability errors', async () => {
  const executor = buildExecutor(
    `
      const response = fetch_data(seed);
      response + 1;
    `,
    {
      fetch_data() {
        throw Object.assign(new Error('host exploded'), {
          code: 'E_FETCH',
          details: { retriable: false },
        });
      },
    },
    {
      compileOptions: { inputs: ['seed'] },
    },
  );

  await executor.enqueue({ seed: 4 }, { jobId: 'job-1' });
  await executor.runWorker({ drain: true });

  const job = await executor.get('job-1');
  assert.equal(job.state, 'failed');
  assert.equal(job.error.name, 'MustardRuntimeError');
  assert.match(job.error.message, /host exploded/);
});

test('executor cancel marks queued jobs as cancelled', async () => {
  const executor = buildExecutor('seed;', {}, {
    compileOptions: { inputs: ['seed'] },
  });

  await executor.enqueue({ seed: 1 }, { jobId: 'job-1' });
  await executor.cancel('job-1');
  await executor.runWorker({ drain: true });

  const job = await executor.get('job-1');
  assert.equal(job.state, 'cancelled');
  assert.equal(job.error.message, 'execution cancelled');
  assert.equal(job.attempts, 0);
});

test('executor honours cancellation requests for waiting jobs', async () => {
  let unblock;
  const gate = new Promise((resolve) => {
    unblock = resolve;
  });

  const executor = buildExecutor(
    `
      const response = fetch_data(seed);
      response + 1;
    `,
    {
      async fetch_data(value) {
        await gate;
        return value;
      },
    },
    {
      compileOptions: { inputs: ['seed'] },
    },
  );

  await executor.enqueue({ seed: 4 }, { jobId: 'job-1' });
  const worker = executor.runWorker({ maxConcurrentJobs: 1, drain: true });

  for (;;) {
    const job = await executor.get('job-1');
    if (job?.state === 'waiting') {
      break;
    }
    await delay(5);
  }

  await executor.cancel('job-1');
  unblock();
  await worker;

  const job = await executor.get('job-1');
  assert.equal(job.state, 'cancelled');
  assert.match(job.error.message, /execution cancelled/);
});

test('executor rejects waiting progress swapped across job ids', async () => {
  class SwappedProgressStore extends InMemoryMustardExecutorStore {
    constructor() {
      super();
      this._capturedAttackerProgress = null;
      this._attackerProgressReady = new Promise((resolve) => {
        this._resolveAttackerProgress = resolve;
      });
      this._victimLoadAttempted = new Promise((resolve) => {
        this._resolveVictimLoad = resolve;
      });
    }

    async saveProgress(jobId, progress) {
      await super.saveProgress(jobId, progress);
      if (jobId === 'attacker' && this._capturedAttackerProgress === null) {
        this._capturedAttackerProgress = clonePersistedProgress(progress);
        this._resolveAttackerProgress();
      }
    }

    async loadProgress(jobId) {
      if (jobId === 'victim') {
        await this._attackerProgressReady;
        this._resolveVictimLoad();
        return clonePersistedProgress(this._capturedAttackerProgress);
      }
      if (jobId === 'attacker') {
        await this._victimLoadAttempted;
      }
      return super.loadProgress(jobId);
    }
  }

  const executor = buildExecutor(
    `
      const response = fetch_data(seed);
      response;
    `,
    {
      fetch_data(value) {
        return value.label;
      },
    },
    {
      compileOptions: { inputs: ['seed'] },
      store: new SwappedProgressStore(),
    },
  );

  await Promise.all([
    executor.enqueue({ seed: { label: 'ATTACKER' } }, { jobId: 'attacker' }),
    executor.enqueue({ seed: { label: 'VICTIM' } }, { jobId: 'victim' }),
  ]);
  await executor.runWorker({ drain: true, maxConcurrentJobs: 2 });

  const attacker = await executor.get('attacker');
  const victim = await executor.get('victim');

  assert.equal(attacker.state, 'completed');
  assert.equal(attacker.result, 'ATTACKER');
  assert.equal(victim.state, 'failed');
  assert.match(victim.error.message, /snapshot key digest/);
});
