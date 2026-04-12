'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const { once } = require('node:events');
const { spawn, spawnSync } = require('node:child_process');

const { Jslite, Progress } = require('../../index.js');
const {
  decodeStructured,
  encodeResumePayloadError,
  encodeResumePayloadValue,
} = require('../../lib/structured.js');
const { snapshotIdentity, snapshotKeyDigest, snapshotToken } = require('../../lib/policy.js');
const { normalizeValue } = require('./runtime-oracle.js');

const REPO_ROOT = path.join(__dirname, '../..');
const CORPUS = JSON.parse(
  fs.readFileSync(path.join(REPO_ROOT, 'tests/shared/equivalence-corpus.json'), 'utf8'),
);
const EXPLICIT_SNAPSHOT_KEY = 'equivalence-corpus-snapshot-key';
const EXPLICIT_SNAPSHOT_KEY_BASE64 = Buffer.from(EXPLICIT_SNAPSHOT_KEY, 'utf8').toString('base64');
const EXPLICIT_LOAD_OPTIONS = Object.freeze({
  capabilities: {},
  limits: {},
  snapshotKey: EXPLICIT_SNAPSHOT_KEY,
});
const ALLOWLISTED_MODE_DIFFERENCES = Object.freeze(new Map());

let sidecarBuildChecked = false;

function makeHostError(step) {
  const error = new Error(step.message);
  error.name = step.name;
  if (step.code !== undefined) {
    error.code = step.code;
  }
  if (step.details !== undefined) {
    error.details = step.details;
  }
  return error;
}

function createCapabilitySequence(entry) {
  let index = 0;
  const capabilities = Object.fromEntries(
    entry.capabilities.map((name) => [
      name,
      () => {
        const step = entry.steps[index];
        assert.ok(step, `case \`${entry.id}\` exhausted host outcomes before completion`);
        index += 1;
        if (step.type === 'error') {
          throw makeHostError(step);
        }
        return step.value;
      },
    ]),
  );

  return {
    capabilities,
    assertConsumed() {
      assert.equal(
        index,
        entry.steps.length,
        `case \`${entry.id}\` consumed ${index} host outcomes but corpus defines ${entry.steps.length}`,
      );
    },
  };
}

function baseExecutionOptions(capabilities) {
  return {
    capabilities,
    limits: {},
    snapshotKey: EXPLICIT_SNAPSHOT_KEY,
  };
}

async function runAddon(entry) {
  const sequence = createCapabilitySequence(entry);
  const runtime = new Jslite(entry.source);
  const value = await runtime.run(baseExecutionOptions(sequence.capabilities));
  sequence.assertConsumed();
  return normalizeValue(value);
}

function applyProgressStep(progress, step) {
  return step.type === 'error' ? progress.resumeError(makeHostError(step)) : progress.resume(step.value);
}

function assertProgressCapability(entry, progress) {
  assert.ok(progress instanceof Progress, `case \`${entry.id}\` should still be suspended`);
  assert.ok(
    entry.capabilities.includes(progress.capability),
    `case \`${entry.id}\` suspended on unexpected capability \`${progress.capability}\``,
  );
}

function driveAddonProgress(entry, { reloadSnapshots }) {
  const capabilities = Object.fromEntries(entry.capabilities.map((name) => [name, () => undefined]));
  let current = new Jslite(entry.source).start(baseExecutionOptions(capabilities));
  let index = 0;

  while (current instanceof Progress) {
    assertProgressCapability(entry, current);
    const corpusStep = entry.steps[index];
    assert.ok(corpusStep, `case \`${entry.id}\` suspended more often than the corpus defines`);
    if (reloadSnapshots) {
      current = Progress.load(current.dump(), {
        ...EXPLICIT_LOAD_OPTIONS,
        capabilities,
      });
    }
    current = applyProgressStep(current, corpusStep);
    index += 1;
  }

  assert.equal(
    index,
    entry.steps.length,
    `case \`${entry.id}\` completed after ${index} host resumes but corpus defines ${entry.steps.length}`,
  );
  return normalizeValue(current);
}

function ensureSidecarBuilt() {
  if (sidecarBuildChecked) {
    return;
  }
  sidecarBuildChecked = true;
  const result = spawnSync('cargo', ['build', '-q', '-p', 'jslite-sidecar'], {
    cwd: REPO_ROOT,
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    throw new Error(
      `failed to build jslite-sidecar for sidecar equivalence tests\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
    );
  }
}

function sidecarExecutablePath() {
  return path.join(
    REPO_ROOT,
    'target',
    'debug',
    process.platform === 'win32' ? 'jslite-sidecar.exe' : 'jslite-sidecar',
  );
}

function decodeSidecarStep(step) {
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
    snapshotBase64: step.snapshot_base64,
  };
}

async function withSidecar(run) {
  ensureSidecarBuilt();
  const child = spawn(sidecarExecutablePath(), [], {
    cwd: REPO_ROOT,
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  const stderr = [];
  child.stderr.on('data', (chunk) => {
    stderr.push(chunk.toString('utf8'));
  });
  const reader = readline.createInterface({ input: child.stdout });
  const lines = reader[Symbol.asyncIterator]();

  async function request(payload) {
    child.stdin.write(`${JSON.stringify(payload)}\n`);
    const next = await lines.next();
    if (next.done) {
      throw new Error(`sidecar closed early\nstderr:\n${stderr.join('')}`);
    }
    return JSON.parse(next.value);
  }

  try {
    return await run({ request });
  } finally {
    child.stdin.end();
    reader.close();
    const [code] = await once(child, 'close');
    assert.equal(code, 0, `sidecar exited unsuccessfully\nstderr:\n${stderr.join('')}`);
  }
}

async function runSidecar(entry) {
  return withSidecar(async ({ request }) => {
    const compile = await request({
      method: 'compile',
      id: 1,
      source: entry.source,
    });
    assert.equal(compile.ok, true, `case \`${entry.id}\` failed to compile via sidecar`);

    const start = await request({
      method: 'start',
      id: 2,
      program_base64: compile.result.program_base64,
      options: {
        inputs: {},
        capabilities: entry.capabilities,
        limits: {},
      },
    });
    assert.equal(start.ok, true, `case \`${entry.id}\` failed to start via sidecar`);

    let step = decodeSidecarStep(start.result.step);
    let index = 0;
    while (step.type === 'suspended') {
      assert.ok(
        entry.capabilities.includes(step.capability),
        `case \`${entry.id}\` suspended on unexpected capability \`${step.capability}\``,
      );
      const corpusStep = entry.steps[index];
      assert.ok(corpusStep, `case \`${entry.id}\` suspended more often than the corpus defines`);
      const payload =
        corpusStep.type === 'error'
          ? JSON.parse(encodeResumePayloadError(makeHostError(corpusStep)))
          : JSON.parse(encodeResumePayloadValue(corpusStep.value));
      const resume = await request({
        method: 'resume',
        id: 3 + index,
        snapshot_base64: step.snapshotBase64,
        policy: {
          capabilities: entry.capabilities,
          limits: {},
          snapshot_id: snapshotIdentity(Buffer.from(step.snapshotBase64, 'base64')),
          snapshot_key_base64: EXPLICIT_SNAPSHOT_KEY_BASE64,
          snapshot_key_digest: snapshotKeyDigest(Buffer.from(EXPLICIT_SNAPSHOT_KEY, 'utf8')),
          snapshot_token: snapshotToken(
            Buffer.from(step.snapshotBase64, 'base64'),
            EXPLICIT_SNAPSHOT_KEY,
          ),
        },
        payload,
      });
      assert.equal(resume.ok, true, `case \`${entry.id}\` failed to resume via sidecar`);
      step = decodeSidecarStep(resume.result.step);
      index += 1;
    }

    assert.equal(
      index,
      entry.steps.length,
      `case \`${entry.id}\` completed after ${index} host resumes but corpus defines ${entry.steps.length}`,
    );
    return normalizeValue(step.value);
  });
}

for (const entry of CORPUS) {
  test(`equivalence corpus ${entry.id} keeps addon and sidecar aligned`, async () => {
    const runOutcome = await runAddon(entry);
    const startedOutcome = driveAddonProgress(entry, { reloadSnapshots: false });
    const reloadedOutcome = driveAddonProgress(entry, { reloadSnapshots: true });
    const sidecarOutcome = await runSidecar(entry);

    assert.deepEqual(startedOutcome, runOutcome);
    assert.deepEqual(reloadedOutcome, runOutcome);

    const allowlisted = ALLOWLISTED_MODE_DIFFERENCES.get(entry.id);
    if (allowlisted !== undefined) {
      assert.deepEqual(sidecarOutcome, allowlisted);
      return;
    }

    assert.deepEqual(sidecarOutcome, runOutcome);
  });
}
