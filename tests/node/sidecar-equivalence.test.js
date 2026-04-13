'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { once } = require('node:events');
const { spawn, spawnSync } = require('node:child_process');

const { Mustard, Progress } = require('../../index.ts');
const { createBinarySidecarClient } = require('../../lib/sidecar.ts');
const {
  decodeStructured,
  encodeResumePayloadError,
  encodeResumePayloadValue,
} = require('../../lib/structured.ts');
const { snapshotKeyDigest, snapshotToken } = require('../../lib/policy.ts');
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
const SIDECAR_PROTOCOL_VERSION = 2;

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
  const runtime = new Mustard(entry.source);
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
  let current = new Mustard(entry.source).start(baseExecutionOptions(capabilities));
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
  const result = spawnSync('cargo', ['build', '-q', '-p', 'mustard-sidecar'], {
    cwd: REPO_ROOT,
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    throw new Error(
      `failed to build mustard-sidecar for sidecar equivalence tests\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
    );
  }
}

function sidecarExecutablePath() {
  return path.join(
    REPO_ROOT,
    'target',
    'debug',
    process.platform === 'win32' ? 'mustard-sidecar.exe' : 'mustard-sidecar',
  );
}

function decodeSidecarStep(step, blob = undefined) {
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
    snapshot: Buffer.from(blob ?? Buffer.alloc(0)),
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
  const client = createBinarySidecarClient(child);

  try {
    return await run({
      async request(payload, blob = undefined) {
        try {
          return await client.request(payload, blob);
        } catch (error) {
          throw new Error(`sidecar closed early\nstderr:\n${stderr.join('')}\n${error.message}`);
        }
      },
    });
  } finally {
    child.stdin.end();
    const [code] = await once(child, 'close');
    assert.equal(code, 0, `sidecar exited unsuccessfully\nstderr:\n${stderr.join('')}`);
  }
}

async function runSidecar(entry) {
  return withSidecar(async ({ request }) => {
    const { payload: compile } = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'compile',
      id: 1,
      source: entry.source,
    });
    assert.equal(compile.ok, true, `case \`${entry.id}\` failed to compile via sidecar`);
    assert.equal(compile.protocol_version, SIDECAR_PROTOCOL_VERSION);

    const { payload: start, blob: startBlob } = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'start',
      id: 2,
      program_id: compile.result.program_id,
      options: {
        inputs: {},
        capabilities: entry.capabilities,
        limits: {},
      },
    });
    assert.equal(start.ok, true, `case \`${entry.id}\` failed to start via sidecar`);
    assert.equal(start.protocol_version, SIDECAR_PROTOCOL_VERSION);

    let step = decodeSidecarStep(start.result.step, startBlob);
    let snapshotId = start.result.snapshot_id ?? null;
    let policyId = start.result.policy_id ?? null;
    let index = 0;
    while (step.type === 'suspended') {
      assert.ok(
        entry.capabilities.includes(step.capability),
        `case \`${entry.id}\` suspended on unexpected capability \`${step.capability}\``,
      );
      assert.equal(typeof snapshotId, 'string', `case \`${entry.id}\` missing sidecar snapshot_id`);
      assert.equal(typeof policyId, 'string', `case \`${entry.id}\` missing sidecar policy_id`);
      const corpusStep = entry.steps[index];
      assert.ok(corpusStep, `case \`${entry.id}\` suspended more often than the corpus defines`);
      const payload =
        corpusStep.type === 'error'
          ? JSON.parse(encodeResumePayloadError(makeHostError(corpusStep)))
          : JSON.parse(encodeResumePayloadValue(corpusStep.value));
      const { payload: resume, blob: resumeBlob } = await request({
        protocol_version: SIDECAR_PROTOCOL_VERSION,
        method: 'resume',
        id: 3 + index,
        snapshot_id: snapshotId,
        policy_id: policyId,
        auth: {
          snapshot_key_base64: EXPLICIT_SNAPSHOT_KEY_BASE64,
          snapshot_key_digest: snapshotKeyDigest(Buffer.from(EXPLICIT_SNAPSHOT_KEY, 'utf8')),
          snapshot_token: snapshotToken(step.snapshot, EXPLICIT_SNAPSHOT_KEY),
        },
        payload,
      });
      assert.equal(resume.ok, true, `case \`${entry.id}\` failed to resume via sidecar`);
      assert.equal(resume.protocol_version, SIDECAR_PROTOCOL_VERSION);
      step = decodeSidecarStep(resume.result.step, resumeBlob);
      snapshotId = resume.result.snapshot_id ?? null;
      policyId = resume.result.policy_id ?? policyId;
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
