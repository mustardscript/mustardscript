'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { once } = require('node:events');
const { spawn, spawnSync } = require('node:child_process');

const { createBinarySidecarClient } = require('../../lib/sidecar.ts');
const {
  decodeStructured,
  encodeResumePayloadValue,
  encodeStructuredInputs,
} = require('../../lib/structured.ts');
const { snapshotKeyDigest, snapshotToken } = require('../../lib/policy.ts');

const REPO_ROOT = path.join(__dirname, '../..');
const SIDECAR_PROTOCOL_VERSION = 2;
const SNAPSHOT_KEY = 'sidecar-profile-test-snapshot-key';
const SNAPSHOT_KEY_BASE64 = Buffer.from(SNAPSHOT_KEY, 'utf8').toString('base64');

let sidecarBuildChecked = false;

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
      `failed to build mustard-sidecar for profiling tests\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
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

function assertProfileShape(profile) {
  assert.equal(typeof profile?.execution_ns, 'number');
  assert.equal(typeof profile?.response_prepare_ns, 'number');
  assert.ok(Number.isFinite(profile.execution_ns));
  assert.ok(Number.isFinite(profile.response_prepare_ns));
  assert.ok(profile.execution_ns >= 0);
  assert.ok(profile.response_prepare_ns >= 0);
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

test('sidecar profiled start/resume responses expose execution and response-preparation timings', async () => {
  await withSidecar(async ({ request }) => {
    const compile = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'compile',
      id: 1,
      source: `
        const first = checkpoint(seed);
        first + 1;
      `,
    });
    assert.equal(compile.payload.ok, true);

    const start = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'start',
      id: 2,
      program_id: compile.payload.result.program_id,
      profile: true,
      options: {
        inputs: JSON.parse(encodeStructuredInputs({ seed: 1 })),
        capabilities: ['checkpoint'],
        limits: {},
      },
    });
    assert.equal(start.payload.ok, true);
    assert.equal(typeof start.roundTripMs, 'number');
    assert.equal(typeof start.responseDecodeMs, 'number');
    assertProfileShape(start.payload.profile);
    const startStep = decodeSidecarStep(start.payload.result.step, start.blob);
    assert.equal(startStep.type, 'suspended');
    assert.equal(startStep.capability, 'checkpoint');
    assert.deepEqual(startStep.args, [1]);

    const resume = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'resume',
      id: 3,
      snapshot_id: start.payload.result.snapshot_id,
      policy_id: start.payload.result.policy_id,
      profile: true,
      auth: {
        snapshot_key_base64: SNAPSHOT_KEY_BASE64,
        snapshot_key_digest: snapshotKeyDigest(Buffer.from(SNAPSHOT_KEY, 'utf8')),
        snapshot_token: snapshotToken(startStep.snapshot, SNAPSHOT_KEY),
      },
      payload: JSON.parse(encodeResumePayloadValue(2)),
    });
    assert.equal(resume.payload.ok, true);
    assert.equal(typeof resume.roundTripMs, 'number');
    assert.equal(typeof resume.responseDecodeMs, 'number');
    assertProfileShape(resume.payload.profile);
    const resumeStep = decodeSidecarStep(resume.payload.result.step, resume.blob);
    assert.equal(resumeStep.type, 'completed');
    assert.equal(resumeStep.value, 3);
  });
});
