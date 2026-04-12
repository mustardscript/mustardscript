'use strict';

const { setTimeout: delay } = require('node:timers/promises');

const {
  assert,
  InMemoryJsliteExecutorStore,
  JsliteExecutor,
  test,
} = require('./support/helpers.js');

function buildExecutor(source, capabilities, options = {}) {
  const { Jslite } = require('../../index.js');
  return new JsliteExecutor({
    program: new Jslite(source, options.compileOptions),
    capabilities,
    snapshotKey: Buffer.from('executor-snapshot-key'),
    store: options.store ?? new InMemoryJsliteExecutorStore(),
    limits: options.limits,
  });
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
  assert.equal(job.error.name, 'JsliteRuntimeError');
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
