'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { once } = require('node:events');
const { spawn, spawnSync } = require('node:child_process');

const { Mustard, Progress } = require('../../index.ts');
const { createBinarySidecarClient } = require('../../lib/sidecar.ts');
const {
  decodeStructured,
  encodeResumePayloadError,
  encodeResumePayloadValue,
  encodeStructuredInputs,
} = require('../../lib/structured.ts');
const { snapshotKeyDigest, snapshotToken } = require('../../lib/policy.ts');
const { createDurablePtcScenarios } = require('../../benchmarks/ptc-fixtures.ts');

const REPO_ROOT = path.join(__dirname, '../..');
const SNAPSHOT_KEY = 'durable-ptc-equivalence-snapshot-key';
const SNAPSHOT_KEY_BASE64 = Buffer.from(SNAPSHOT_KEY, 'utf8').toString('base64');
const SIDECAR_PROTOCOL_VERSION = 2;
const DURABLE_CHECKPOINT_CAPABILITY = 'checkpoint_vendor_review';
const DURABLE_FINAL_ACTION_CAPABILITY = 'file_vendor_review';
const SCENARIO = createDurablePtcScenarios().ptc_vendor_review_durable_small;

let sidecarBuildChecked = false;

function normalizeResult(result) {
  return JSON.parse(JSON.stringify(result));
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
      `failed to build mustard-sidecar for durable PTC equivalence tests\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
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
    snapshotId: step.snapshot_id ?? null,
    policyId: step.policy_id ?? null,
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

function durableCapabilities() {
  return SCENARIO.createCapabilities();
}

function durableLoadOptions(capabilities) {
  return {
    capabilities,
    limits: {},
    snapshotKey: SNAPSHOT_KEY,
  };
}

async function startAddonCheckpoint(capabilities) {
  const runtime = new Mustard(SCENARIO.source);
  let progress = runtime.start({
    inputs: SCENARIO.inputs,
    capabilities,
    snapshotKey: SNAPSHOT_KEY,
  });
  while (progress instanceof Progress && progress.capability !== DURABLE_CHECKPOINT_CAPABILITY) {
    const value = capabilities[progress.capability](...progress.args);
    const resolved = value && typeof value.then === 'function' ? await value : value;
    progress = progress.resume(resolved);
  }
  assert.ok(progress instanceof Progress);
  assert.equal(progress.capability, DURABLE_CHECKPOINT_CAPABILITY);
  return progress;
}

async function runAddonDurableSuccess() {
  const capabilities = durableCapabilities();
  const first = await startAddonCheckpoint(capabilities);
  const checkpointArgs = structuredClone(first.args);
  const restored = Progress.load(first.dump(), durableLoadOptions(capabilities));
  const second = restored.resume(capabilities[DURABLE_CHECKPOINT_CAPABILITY](...checkpointArgs));
  assert.ok(second instanceof Progress);
  assert.equal(second.capability, DURABLE_FINAL_ACTION_CAPABILITY);
  const result = second.resume(capabilities[DURABLE_FINAL_ACTION_CAPABILITY](...second.args));
  SCENARIO.assertResult(result);
  return normalizeResult(result);
}

async function runAddonDurableFailureMessage() {
  const capabilities = durableCapabilities();
  const first = await startAddonCheckpoint(capabilities);
  const checkpointArgs = structuredClone(first.args);
  const restored = Progress.load(first.dump(), durableLoadOptions(capabilities));
  const second = restored.resume(capabilities[DURABLE_CHECKPOINT_CAPABILITY](...checkpointArgs));
  assert.ok(second instanceof Progress);
  assert.equal(second.capability, DURABLE_FINAL_ACTION_CAPABILITY);
  try {
    second.resumeError(new Error('durable final action failed'));
  } catch (error) {
    assert.match(error.message, /durable final action failed/);
    return error.message;
  }
  throw new Error('expected addon durable failure to throw');
}

async function captureSidecarCheckpoint() {
  const capabilities = durableCapabilities();
  const capabilityNames = Object.keys(capabilities);
  return withSidecar(async ({ request }) => {
    const compile = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'compile',
      id: 1,
      source: SCENARIO.source,
    });
    assert.equal(compile.payload.ok, true);
    const start = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'start',
      id: 2,
      program_id: compile.payload.result.program_id,
      options: {
        inputs: JSON.parse(encodeStructuredInputs(SCENARIO.inputs)),
        capabilities: capabilityNames,
        limits: {},
      },
    });
    assert.equal(start.payload.ok, true);
    let step = {
      ...decodeSidecarStep(start.payload.result.step, start.blob),
      snapshotId: start.payload.result.snapshot_id ?? null,
      policyId: start.payload.result.policy_id ?? null,
    };
    let requestId = 3;
    while (step.type === 'suspended' && step.capability !== DURABLE_CHECKPOINT_CAPABILITY) {
      const resumed = await request({
        protocol_version: SIDECAR_PROTOCOL_VERSION,
        method: 'resume',
        id: requestId,
        snapshot_id: step.snapshotId,
        policy_id: step.policyId,
        auth: {
          snapshot_key_base64: SNAPSHOT_KEY_BASE64,
          snapshot_key_digest: snapshotKeyDigest(Buffer.from(SNAPSHOT_KEY, 'utf8')),
          snapshot_token: snapshotToken(step.snapshot, SNAPSHOT_KEY),
        },
        payload: JSON.parse(encodeResumePayloadValue(capabilities[step.capability](...step.args))),
      });
      assert.equal(resumed.payload.ok, true);
      step = {
        ...decodeSidecarStep(resumed.payload.result.step, resumed.blob),
        snapshotId: resumed.payload.result.snapshot_id ?? null,
        policyId: resumed.payload.result.policy_id ?? null,
      };
      requestId += 1;
    }
    assert.equal(step.type, 'suspended');
    assert.equal(step.capability, DURABLE_CHECKPOINT_CAPABILITY);
    return {
      capabilities,
      checkpointArgs: structuredClone(step.args),
      snapshot: Buffer.from(step.snapshot),
      policy: {
        capabilities: capabilityNames,
        limits: {},
        snapshot_id: step.snapshotId,
        snapshot_key_base64: SNAPSHOT_KEY_BASE64,
        snapshot_key_digest: snapshotKeyDigest(Buffer.from(SNAPSHOT_KEY, 'utf8')),
        snapshot_token: snapshotToken(step.snapshot, SNAPSHOT_KEY),
      },
    };
  });
}

async function runSidecarDurableSuccess() {
  const checkpoint = await captureSidecarCheckpoint();
  return withSidecar(async ({ request }) => {
    const restored = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'resume',
      id: 3,
      policy: checkpoint.policy,
      payload: JSON.parse(
        encodeResumePayloadValue(
          checkpoint.capabilities[DURABLE_CHECKPOINT_CAPABILITY](...checkpoint.checkpointArgs),
        ),
      ),
    }, checkpoint.snapshot);
    assert.equal(restored.payload.ok, true);
    let step = {
      ...decodeSidecarStep(restored.payload.result.step, restored.blob),
      snapshotId: restored.payload.result.snapshot_id ?? null,
      policyId: restored.payload.result.policy_id ?? null,
    };
    assert.equal(step.type, 'suspended');
    assert.equal(step.capability, DURABLE_FINAL_ACTION_CAPABILITY);
    const completion = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'resume',
      id: 4,
      snapshot_id: step.snapshotId,
      policy_id: step.policyId,
      auth: {
        snapshot_key_base64: SNAPSHOT_KEY_BASE64,
        snapshot_key_digest: snapshotKeyDigest(Buffer.from(SNAPSHOT_KEY, 'utf8')),
        snapshot_token: snapshotToken(step.snapshot, SNAPSHOT_KEY),
      },
      payload: JSON.parse(
        encodeResumePayloadValue(
          checkpoint.capabilities[DURABLE_FINAL_ACTION_CAPABILITY](...step.args),
        ),
      ),
    });
    assert.equal(completion.payload.ok, true);
    step = decodeSidecarStep(completion.payload.result.step, completion.blob);
    assert.equal(step.type, 'completed');
    SCENARIO.assertResult(step.value);
    return normalizeResult(step.value);
  });
}

async function runSidecarDurableFailureMessage() {
  const checkpoint = await captureSidecarCheckpoint();
  return withSidecar(async ({ request }) => {
    const restored = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'resume',
      id: 5,
      policy: checkpoint.policy,
      payload: JSON.parse(
        encodeResumePayloadValue(
          checkpoint.capabilities[DURABLE_CHECKPOINT_CAPABILITY](...checkpoint.checkpointArgs),
        ),
      ),
    }, checkpoint.snapshot);
    assert.equal(restored.payload.ok, true);
    let step = {
      ...decodeSidecarStep(restored.payload.result.step, restored.blob),
      snapshotId: restored.payload.result.snapshot_id ?? null,
      policyId: restored.payload.result.policy_id ?? null,
    };
    assert.equal(step.type, 'suspended');
    assert.equal(step.capability, DURABLE_FINAL_ACTION_CAPABILITY);
    const completion = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'resume',
      id: 6,
      snapshot_id: step.snapshotId,
      policy_id: step.policyId,
      auth: {
        snapshot_key_base64: SNAPSHOT_KEY_BASE64,
        snapshot_key_digest: snapshotKeyDigest(Buffer.from(SNAPSHOT_KEY, 'utf8')),
        snapshot_token: snapshotToken(step.snapshot, SNAPSHOT_KEY),
      },
      payload: JSON.parse(encodeResumePayloadError(new Error('durable final action failed'))),
    });
    assert.equal(completion.payload.ok, false);
    return completion.payload.error;
  });
}

test('durable vendor-review checkpoint keeps addon Progress.load and sidecar raw resume aligned', async () => {
  const addonResult = await runAddonDurableSuccess();
  const sidecarResult = await runSidecarDurableSuccess();
  assert.deepEqual(sidecarResult, addonResult);
});

test('durable vendor-review final-action failure stays aligned across addon and sidecar restore flows', async () => {
  const addonMessage = await runAddonDurableFailureMessage();
  const sidecarMessage = await runSidecarDurableFailureMessage();
  assert.match(addonMessage, /durable final action failed/);
  assert.match(sidecarMessage, /durable final action failed/);
});
