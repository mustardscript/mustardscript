'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { Worker } = require('node:worker_threads');

const { Jslite, JsliteError, Progress } = require('../../index.js');
const { loadNative } = require('../../native-loader.js');
const { KNOWN_PROGRESS_POLICY_CACHE_LIMIT, snapshotToken } = require('../../lib/policy.js');

const SNAPSHOT_KEY = Buffer.from('progress-security-test-key');

function replaceAllAscii(buffer, from, to) {
  const source = Buffer.from(from, 'utf8');
  const target = Buffer.from(to, 'utf8');
  assert.equal(source.length, target.length, 'replacement must preserve byte length');

  for (let index = 0; index <= buffer.length - source.length; index += 1) {
    if (buffer.subarray(index, index + source.length).equals(source)) {
      target.copy(buffer, index);
      index += source.length - 1;
    }
  }
}

function isSingleUseRuntimeError(error) {
  return (
    error instanceof JsliteError &&
    error.kind === 'Runtime' &&
    error.message.includes('single-use')
  );
}

function isTamperedSnapshotError(error) {
  return (
    error instanceof JsliteError &&
    error.kind === 'Serialization' &&
    error.message.includes('tampered or unauthenticated snapshot')
  );
}

function runWorker(script, workerData) {
  return new Promise((resolve, reject) => {
    const worker = new Worker(script, { eval: true, workerData });
    worker.once('message', resolve);
    worker.once('error', reject);
    worker.once('exit', (code) => {
      if (code !== 0) {
        reject(new Error(`worker exited with code ${code}`));
      }
    });
  });
}

test('progress load derives capability and args from the snapshot instead of caller metadata', () => {
  const progress = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  const restored = Progress.load({
    ...progress.dump(),
    capability: 'drop_table',
    args: ['users'],
  });

  assert.equal(restored.capability, 'fetch_data');
  assert.deepEqual(restored.args, [4]);
  assert.equal(restored.resume(4), 8);
});

test('progress load preserves single-use by authenticated snapshot token', () => {
  const progress = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  const dumped = progress.dump();
  const first = Progress.load(dumped);
  const second = Progress.load(dumped);

  assert.equal(first.resume(4), 8);
  assert.throws(() => second.resume(4), isSingleUseRuntimeError);
});

test('progress load rejects already-consumed snapshots before exposing capability metadata', () => {
  const progress = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  const dumped = progress.dump();
  assert.equal(progress.resume(4), 8);
  assert.throws(() => Progress.load(dumped), isSingleUseRuntimeError);
});

test('progress load rejects replay attempts that re-key an already-consumed dump', () => {
  const originalKey = Buffer.from('progress-security-key-a');
  const replayKey = Buffer.from('progress-security-key-b');
  const progress = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    snapshotKey: originalKey,
    capabilities: {
      fetch_data() {},
    },
  });

  const dumped = progress.dump();
  assert.equal(progress.resume(4), 8);

  assert.throws(
    () =>
      Progress.load(
        {
          ...dumped,
          token: snapshotToken(dumped.snapshot, replayKey, dumped.snapshot_id),
        },
        {
          snapshotKey: replayKey,
          capabilities: {
            fetch_data(value) {
              return value;
            },
          },
          limits: {},
        },
      ),
    isSingleUseRuntimeError,
  );
});

test('progress snapshots remain single-use after unrelated same-process churn', () => {
  const runtime = new Jslite(`
    const response = fetch_data(seed);
    response;
  `);
  const original = runtime.start({
    inputs: {
      seed: 1,
    },
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(original instanceof Progress);
  const dumped = original.dump();
  assert.equal(original.resume(11), 11);

  for (let index = 0; index < 6_000; index += 1) {
    const progress = runtime.start({
      inputs: {
        seed: index + 2,
      },
      snapshotKey: SNAPSHOT_KEY,
      capabilities: {
        fetch_data() {},
      },
    });
    assert.ok(progress instanceof Progress);
    assert.equal(progress.resume(index), index);
  }

  assert.throws(
    () =>
      Progress.load(dumped, {
        snapshotKey: SNAPSHOT_KEY,
        capabilities: {
          fetch_data() {},
        },
        limits: {},
      }),
    isSingleUseRuntimeError,
  );
});

test('progress load rejects already-consumed snapshots across same-process worker threads', async () => {
  const progress = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  const dumped = progress.dump();
  assert.equal(progress.resume(4), 8);

  const message = await runWorker(
    `
      const { parentPort, workerData } = require('node:worker_threads');
      const { Progress } = require(workerData.indexPath);

      const snapshotKey = Buffer.from(workerData.snapshotKeyBase64, 'base64');
      let withoutOptions;
      try {
        Progress.load({
          snapshot: Buffer.from(workerData.snapshotBase64, 'base64'),
          snapshot_id: workerData.snapshotId,
          snapshot_key_digest: workerData.snapshotKeyDigest,
          token: workerData.token,
        });
      } catch (error) {
        withoutOptions = String(error && error.message);
      }

      let withOptions;
      try {
        const restored = Progress.load({
          snapshot: Buffer.from(workerData.snapshotBase64, 'base64'),
          snapshot_id: workerData.snapshotId,
          snapshot_key_digest: workerData.snapshotKeyDigest,
          token: workerData.token,
        }, {
          snapshotKey,
          capabilities: {
            fetch_data() {},
          },
          limits: {},
        });
        withOptions = {
          capability: restored.capability,
          args: restored.args,
          result: restored.resume(4),
        };
      } catch (error) {
        withOptions = {
          name: error && error.name,
          message: String(error && error.message),
        };
      }

      parentPort.postMessage({
        pid: process.pid,
        withoutOptions,
        withOptions,
      });
    `,
    {
      indexPath: require.resolve('../../index.js'),
      snapshotBase64: dumped.snapshot.toString('base64'),
      snapshotId: dumped.snapshot_id,
      snapshotKeyDigest: dumped.snapshot_key_digest,
      token: dumped.token,
      snapshotKeyBase64: SNAPSHOT_KEY.toString('base64'),
    },
  );

  assert.equal(message.pid, process.pid);
  assert.match(message.withoutOptions, /single-use/);
  assert.equal(message.withOptions.name, 'JsliteRuntimeError');
  assert.match(message.withOptions.message, /single-use/);
});

test('progress load rejects forged progress tokens', () => {
  const progress = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  assert.throws(
    () =>
      Progress.load(
        {
          ...progress.dump(),
          token: 'forged-token',
        },
        {
          snapshotKey: SNAPSHOT_KEY,
          capabilities: {
            fetch_data() {},
          },
          limits: {},
        },
      ),
    isTamperedSnapshotError,
  );
});

test('raw native snapshot inspect and resume require snapshot authentication', () => {
  const native = loadNative();
  const progress = new Jslite('const value = fetch_data(7); value * 2;').start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  const dumped = progress.dump();
  const payload = JSON.stringify({ type: 'value', value: { Number: { Finite: 7 } } });
  const authenticatedPolicy = JSON.stringify({
    capabilities: ['fetch_data'],
    limits: {},
    snapshot_id: dumped.snapshot_id,
    snapshot_key_base64: SNAPSHOT_KEY.toString('base64'),
    snapshot_key_digest: dumped.snapshot_key_digest,
    snapshot_token: snapshotToken(dumped.snapshot, SNAPSHOT_KEY, dumped.snapshot_id),
  });

  assert.throws(
    () => native.inspectSnapshot(dumped.snapshot, JSON.stringify({ capabilities: ['fetch_data'], limits: {} })),
    /raw snapshot restore requires snapshot_id/,
  );
  assert.throws(
    () => native.resumeProgram(dumped.snapshot, payload, JSON.stringify({ capabilities: ['fetch_data'], limits: {} })),
    /raw snapshot restore requires snapshot_id/,
  );

  const inspection = JSON.parse(native.inspectSnapshot(dumped.snapshot, authenticatedPolicy));
  assert.equal(inspection.capability, 'fetch_data');
  assert.deepEqual(inspection.args, [{ Number: { Finite: 7 } }]);
});

test('progress load requires explicit policy and snapshotKey outside the current process', () => {
  const progress = new Jslite('fetch_data(1);').start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  const dumped = progress.dump();
  assert.ok(progress instanceof Progress);
  const child = spawnSync(
    process.execPath,
    [
      '-e',
      `
        const { Progress } = require('./index.js');
        try {
          Progress.load({
            snapshot: Buffer.from(process.env.SNAPSHOT_BASE64, 'base64'),
            snapshot_id: process.env.SNAPSHOT_ID,
            snapshot_key_digest: process.env.SNAPSHOT_KEY_DIGEST,
            token: process.env.SNAPSHOT_TOKEN,
          });
          process.stdout.write('loaded');
        } catch (error) {
          process.stdout.write(String(error && error.message));
        }
      `,
    ],
    {
      cwd: process.cwd(),
      env: {
        ...process.env,
        SNAPSHOT_BASE64: dumped.snapshot.toString('base64'),
        SNAPSHOT_ID: dumped.snapshot_id,
        SNAPSHOT_KEY_DIGEST: dumped.snapshot_key_digest,
        SNAPSHOT_TOKEN: dumped.token,
      },
      encoding: 'utf8',
    },
  );

  assert.equal(child.status, 0);
  assert.match(
    child.stdout,
    /requires explicit capabilities, limits, and snapshotKey when restoring progress outside the current process/,
  );
});

test('progress load works across processes when explicit policy and snapshotKey are provided', () => {
  const progress = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  const dumped = progress.dump();
  assert.ok(progress instanceof Progress);
  const child = spawnSync(
    process.execPath,
    [
      '-e',
      `
        const { Progress } = require('./index.js');
        const restored = Progress.load(
          {
            snapshot: Buffer.from(process.env.SNAPSHOT_BASE64, 'base64'),
            snapshot_id: process.env.SNAPSHOT_ID,
            snapshot_key_digest: process.env.SNAPSHOT_KEY_DIGEST,
            token: process.env.SNAPSHOT_TOKEN,
          },
          {
            snapshotKey: Buffer.from(process.env.SNAPSHOT_KEY_BASE64, 'base64'),
            capabilities: {
              fetch_data(value) {
                return value;
              },
            },
            limits: {},
          },
        );
        process.stdout.write(String(restored.resume(4)));
      `,
    ],
    {
      cwd: process.cwd(),
      env: {
        ...process.env,
        SNAPSHOT_BASE64: dumped.snapshot.toString('base64'),
        SNAPSHOT_ID: dumped.snapshot_id,
        SNAPSHOT_KEY_DIGEST: dumped.snapshot_key_digest,
        SNAPSHOT_TOKEN: dumped.token,
        SNAPSHOT_KEY_BASE64: SNAPSHOT_KEY.toString('base64'),
      },
      encoding: 'utf8',
    },
  );

  assert.equal(child.status, 0);
  assert.equal(child.stdout, '8');
});

test('progress load rejects tampered snapshots that switch capability bytes', () => {
  const progress = new Jslite(`
    const first = fetch_data(1);
    const second = fetch_data(2);
    [first, second];
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  const dumped = progress.dump();
  const forgedSnapshot = Buffer.from(dumped.snapshot);
  replaceAllAscii(forgedSnapshot, 'fetch_data', 'drop_table');

  assert.throws(
    () =>
      Progress.load(
        {
          ...dumped,
          snapshot: forgedSnapshot,
        },
        {
          snapshotKey: SNAPSHOT_KEY,
          capabilities: {
            fetch_data() {},
          },
          limits: {},
        },
      ),
    isTamperedSnapshotError,
  );
});

test('progress load rejects tampered snapshots that lower serialized instruction counters', () => {
  const progress = new Jslite(`
    const ready = fetch_data(1);
    let total = 0;
    for (let i = 0; i < 10000; i = i + 1) {
      total = total + 1;
    }
    total;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    limits: {
      instructionBudget: 5_000_000,
    },
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  const dumped = progress.dump();
  const forgedSnapshot = Buffer.from(dumped.snapshot);
  forgedSnapshot[1554] ^= 1;

  assert.throws(
    () =>
      Progress.load(
        {
          ...dumped,
          snapshot: forgedSnapshot,
        },
        {
          snapshotKey: SNAPSHOT_KEY,
          capabilities: {
            fetch_data() {},
          },
          limits: {
            instructionBudget: 3,
          },
        },
      ),
    isTamperedSnapshotError,
  );
});

test('progress load reapplies explicit host limits before resume', () => {
  const progress = new Jslite(`
    const ready = fetch_data(1);
    let total = 0;
    for (let i = 0; i < 10000; i = i + 1) {
      total = total + 1;
    }
    total;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    limits: {
      instructionBudget: 5_000_000,
    },
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  const restored = Progress.load(progress.dump(), {
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
    limits: {
      instructionBudget: 50,
    },
  });

  assert.throws(
    () => restored.resume(1),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Limit' &&
      error.message.includes('instruction budget exhausted'),
  );
});

test('same-process progress policy cache is bounded and falls back to explicit restore policy', () => {
  const snapshotKey = Buffer.from('bounded-progress-policy-key');
  const progresses = [];
  for (let index = 0; index <= KNOWN_PROGRESS_POLICY_CACHE_LIMIT; index += 1) {
    progresses.push(
      new Jslite(`fetch_data(${index});`).start({
        snapshotKey,
        capabilities: {
          fetch_data() {},
        },
      }),
    );
  }

  const oldest = progresses[0].dump();
  assert.throws(
    () => Progress.load(oldest),
    /requires explicit capabilities, limits, and snapshotKey when restoring progress outside the current process/,
  );

  const restored = Progress.load(oldest, {
    snapshotKey,
    capabilities: {
      fetch_data() {},
    },
    limits: {},
  });
  assert.equal(restored.capability, 'fetch_data');
  assert.equal(restored.resume(0), 0);
});
