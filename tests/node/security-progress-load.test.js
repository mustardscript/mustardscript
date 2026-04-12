'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');

const { Jslite, JsliteError, Progress } = require('../../index.js');

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

test('progress load derives capability and args from the snapshot instead of caller metadata', () => {
  const progress = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  const restored = Progress.load({
    ...progress.dump(),
    capability: 'drop_table',
    args: ['users'],
    token: 'forged-token',
  });

  assert.equal(restored.capability, 'fetch_data');
  assert.deepEqual(restored.args, [4]);
  assert.equal(restored.resume(4), 8);
});

test('progress load ignores caller tokens and preserves single-use by snapshot identity', () => {
  const progress = new Jslite(`
    const response = fetch_data(4);
    response * 2;
  `).start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  const dumped = progress.dump();
  const first = Progress.load({ ...dumped, token: 'token-a' });
  const second = Progress.load({ ...dumped, token: 'token-b' });

  assert.equal(first.resume(4), 8);
  assert.throws(() => second.resume(4), isSingleUseRuntimeError);
});

test('progress load requires explicit policy outside the current process', () => {
  const progress = new Jslite('fetch_data(1);').start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  const child = spawnSync(
    process.execPath,
    [
      '-e',
      `
        const { Progress } = require('./index.js');
        try {
          Progress.load({ snapshot: Buffer.from(process.env.SNAPSHOT_BASE64, 'base64') });
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
        SNAPSHOT_BASE64: progress.dump().snapshot.toString('base64'),
      },
      encoding: 'utf8',
    },
  );

  assert.equal(child.status, 0);
  assert.match(
    child.stdout,
    /requires explicit capabilities and limits when restoring progress outside the current process/,
  );
});

test('progress load rejects forged snapshots that switch to unauthorized capabilities', () => {
  const progress = new Jslite(`
    const first = fetch_data(1);
    const second = fetch_data(2);
    [first, second];
  `).start({
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  const forgedSnapshot = Buffer.from(progress.dump().snapshot);
  replaceAllAscii(forgedSnapshot, 'fetch_data', 'drop_table');

  assert.throws(
    () =>
      Progress.load(
        {
          snapshot: forgedSnapshot,
        },
        {
          capabilities: {
            fetch_data() {},
          },
          limits: {},
        },
      ),
    (error) =>
      error instanceof JsliteError &&
      error.kind === 'Serialization' &&
      error.message.includes('unauthorized capability `drop_table`'),
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
    limits: {
      instructionBudget: 5_000_000,
    },
    capabilities: {
      fetch_data() {},
    },
  });

  assert.ok(progress instanceof Progress);
  const restored = Progress.load(progress.dump(), {
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
