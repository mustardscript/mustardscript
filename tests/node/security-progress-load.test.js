'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { Worker } = require('node:worker_threads');

const { Jslite, JsliteError, Progress } = require('../../index.js');
const {
  loadNative,
  localBinaryCandidates,
  resolvePrebuiltPackage,
} = require('../../native-loader.js');
const { snapshotToken } = require('../../lib/policy.js');

const SNAPSHOT_KEY = Buffer.from('progress-security-test-key');

function createLoadOptions(
  snapshotKey = SNAPSHOT_KEY,
  capabilities = {
    fetch_data() {},
  },
  limits = {},
) {
  return {
    snapshotKey,
    capabilities,
    limits,
  };
}

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
    error &&
    error.name === 'JsliteRuntimeError' &&
    error.kind === 'Runtime' &&
    typeof error.message === 'string' &&
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

function resolveCurrentNativeBinaryPath() {
  const localCandidates = localBinaryCandidates(path.join(__dirname, '..', '..'));
  if (localCandidates.length > 0) {
    return localCandidates[0];
  }
  const prebuilt = resolvePrebuiltPackage(path.join(__dirname, '..', '..'));
  if (prebuilt) {
    return prebuilt.binaryPath;
  }
  throw new Error('unable to resolve the current jslite native addon path');
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
  }, createLoadOptions());

  assert.equal(restored.capability, 'fetch_data');
  assert.deepEqual(restored.args, [4]);
  assert.equal(restored.resume(4), 8);
});

test('progress load burns same-process snapshots before a second load can expose metadata', () => {
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
  const first = Progress.load(dumped, createLoadOptions());

  assert.equal(first.capability, 'fetch_data');
  assert.deepEqual(first.args, [4]);
  assert.equal(first.resume(4), 8);
  assert.throws(() => Progress.load(dumped, createLoadOptions()), isSingleUseRuntimeError);
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

test('progress load requires explicit restore policy even in the same process', () => {
  const progress = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  assert.throws(
    () => Progress.load(progress.dump()),
    /requires explicit capabilities, limits, and snapshotKey/,
  );
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
        createLoadOptions(replayKey, {
          fetch_data(value) {
            return value;
          },
        }),
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
        ...createLoadOptions(),
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

test('progress load rejects already-consumed snapshots across duplicate package copies', () => {
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

  const packageRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'jslite-dup-copy-'));
  try {
    fs.copyFileSync(
      path.join(__dirname, '..', '..', 'index.js'),
      path.join(packageRoot, 'index.js'),
    );
    fs.copyFileSync(
      path.join(__dirname, '..', '..', 'native-loader.js'),
      path.join(packageRoot, 'native-loader.js'),
    );
    fs.cpSync(path.join(__dirname, '..', '..', 'lib'), path.join(packageRoot, 'lib'), {
      recursive: true,
    });

    const nativeBinaryPath = resolveCurrentNativeBinaryPath();
    fs.copyFileSync(
      nativeBinaryPath,
      path.join(packageRoot, path.basename(nativeBinaryPath)),
    );

    const duplicateCopy = require(path.join(packageRoot, 'index.js'));
    assert.throws(
      () =>
        duplicateCopy.Progress.load(dumped, {
          snapshotKey: SNAPSHOT_KEY,
          capabilities: {
            fetch_data() {},
          },
          limits: {},
        }),
      isSingleUseRuntimeError,
    );
  } finally {
    fs.rmSync(packageRoot, { recursive: true, force: true });
  }
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

test('progress load releases a claimed snapshot when restore inspection fails', () => {
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
  assert.throws(
    () =>
      Progress.load(dumped, {
        snapshotKey: SNAPSHOT_KEY,
        capabilities: {},
        limits: {},
      }),
    /unauthorized capability/,
  );

  const restored = Progress.load(dumped, createLoadOptions());
  assert.equal(restored.capability, 'fetch_data');
  assert.deepEqual(restored.args, [4]);
  assert.equal(restored.resume(4), 8);
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

test('progress load requires explicit policy, limits, and snapshotKey', () => {
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
    /requires explicit capabilities, limits, and snapshotKey/,
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

test('progress load rejects explicit undefined limits during restore', () => {
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

  assert.throws(
    () =>
      Progress.load(progress.dump(), {
        snapshotKey: SNAPSHOT_KEY,
        capabilities: {
          fetch_data() {},
        },
        limits: undefined,
      }),
    /Progress\.load\(\) options\.limits must be a plain object/,
  );
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

test('raw native snapshot inspect and resume require explicit limits in restore policy', () => {
  const native = loadNative();
  const progress = new Jslite('const value = fetch_data(7); value * 2;').start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
  });

  const dumped = progress.dump();
  const payload = JSON.stringify({ type: 'value', value: { Number: { Finite: 7 } } });
  const missingLimitsPolicy = JSON.stringify({
    capabilities: ['fetch_data'],
    snapshot_id: dumped.snapshot_id,
    snapshot_key_base64: SNAPSHOT_KEY.toString('base64'),
    snapshot_key_digest: dumped.snapshot_key_digest,
    snapshot_token: snapshotToken(dumped.snapshot, SNAPSHOT_KEY, dumped.snapshot_id),
  });

  assert.throws(
    () => native.inspectSnapshot(dumped.snapshot, missingLimitsPolicy),
    /raw snapshot restore requires explicit limits/,
  );
  assert.throws(
    () => native.resumeProgram(dumped.snapshot, payload, missingLimitsPolicy),
    /raw snapshot restore requires explicit limits/,
  );
});

test('progress load rejects same-process restore-policy rebinding without an explicit snapshotKey', () => {
  const progress = new Jslite(`
    const secret = fetch_data(7);
    const next = write_audit(secret);
    next;
  `).start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
      write_audit() {},
    },
    limits: {
      instructionBudget: 5_000_000,
    },
  });

  const dumped = progress.dump();
  assert.throws(
    () =>
      Progress.load(dumped, {
        capabilities: {
          fetch_data() {},
          write_audit() {},
        },
        limits: {
          instructionBudget: 5_000_000,
        },
      }),
    /requires explicit snapshotKey/,
  );
});
