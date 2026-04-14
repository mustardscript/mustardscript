'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { once } = require('node:events');
const { performance } = require('node:perf_hooks');
const { spawn, execFileSync } = require('node:child_process');

const ivm = require('isolated-vm');

const { ExecutionContext, Mustard, Progress } = require('../index.ts');
const { loadNative } = require('../native-loader.ts');
const { callNative } = require('../lib/errors.ts');
const { createBinarySidecarClient } = require('../lib/sidecar.ts');
const {
  decodeStructured,
  encodeResumePayloadError,
  encodeResumePayloadErrorBuffer,
  encodeResumePayloadValue,
  encodeResumePayloadValueBuffer,
  encodeStartOptionsBuffer,
  encodeStructuredInputs,
  encodeStructuredInputsBuffer,
} = require('../lib/structured.ts');
const {
  encodeSnapshotPolicy,
  resolveExecutionContext,
  resolveProgressLoadContext,
  snapshotKeyDigest,
  snapshotToken,
} = require('../lib/policy.ts');
const {
  DEFAULT_MEASURE_OPTIONS,
  measure,
  measureSamples,
  machineMetadata,
  summarize,
  writeBenchmarkArtifact,
} = require('./support.ts');
const {
  PTC_WEIGHTS,
  createCapabilityTransferProbe,
  createDurablePtcScenarios,
  createPtcScenarios,
  structuredByteLength,
  summarizePtcWeightedScore,
} = require('./ptc-fixtures.ts');
const {
  BROAD_USE_CASE_IDS,
  HEADLINE_USE_CASE_IDS,
  HOLDOUT_USE_CASE_IDS,
  averageMetric,
  buildPhase2Scorecards,
  metricNameForUseCase,
} = require('./ptc-portfolio.ts');
const { createGalleryScenarios } = require('./ptc-gallery.ts');
const { createHeadlineSeedScenarios } = require('./ptc-headline-seeds.ts');
const {
  createSentinelScenarios,
  summarizeSentinelFamilyScores,
} = require('./ptc-sentinels.ts');
const { annotateCollectionCallSites } = require('./ptc-attribution.ts');

const REPO_ROOT = path.join(__dirname, '..');
const FIXTURE_VERSION = 10;
const SNAPSHOT_KEY = 'benchmark-workloads-snapshot-key';
const SNAPSHOT_KEY_BASE64 = Buffer.from(SNAPSHOT_KEY, 'utf8').toString('base64');
const WEBSITE_PTC_EXPORT_PATH = path.join(
  REPO_ROOT,
  'website',
  'src',
  'generated',
  'benchmarkData.ts',
);
const BOUNDARY_VALUE_SIZES = Object.freeze([
  { name: 'small', itemCount: 6, weightCount: 6, tagCount: 3 },
  { name: 'medium', itemCount: 24, weightCount: 16, tagCount: 6 },
  { name: 'large', itemCount: 96, weightCount: 32, tagCount: 12 },
]);

const DEFAULT_OPTIONS = DEFAULT_MEASURE_OPTIONS;
const COLD_OPTIONS = Object.freeze({ warmup: 0, iterations: 2 });
const STABLE_PTC_RELEASE_OPTIONS = Object.freeze({ warmup: 1, iterations: 5, batch: 5 });
const MEMORY_RUNS = 20;
const SIDECAR_PROTOCOL_VERSION = 2;
const PHASE2_REPRESENTATIVE_PTC_METRICS = HEADLINE_USE_CASE_IDS.map((id) => metricNameForUseCase(id));
const ADDON_PTC_BREAKDOWN_METRICS = new Set([
  'ptc_website_demo_small',
  'ptc_incident_triage_medium',
  'ptc_fraud_investigation_medium',
  'ptc_vendor_review_medium',
  ...PHASE2_REPRESENTATIVE_PTC_METRICS,
]);
const WORKLOAD_MODES = new Set([
  'full',
  'ptc_public',
  'ptc_headline_release',
  'ptc_broad_release',
  'ptc_holdout_release',
  'ptc_gallery_canary',
  'ptc_sentinel_release',
]);
const GALLERY_CANARY_OPTIONS = Object.freeze({ warmup: 0, iterations: 1 });

function parseArgs(argv) {
  let profile = 'release';
  let mode = 'full';
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === '--profile') {
      profile = argv[index + 1];
      index += 1;
      continue;
    }
    if (value === '--mode') {
      mode = argv[index + 1];
      index += 1;
      continue;
    }
    throw new Error(`Unknown benchmark argument: ${value}`);
  }
  if (profile !== 'dev' && profile !== 'release') {
    throw new Error(`Unsupported workloads profile: ${profile}`);
  }
  if (!WORKLOAD_MODES.has(mode)) {
    throw new Error(`Unsupported workloads mode: ${mode}`);
  }
  return { profile, mode };
}

function sidecarPath(profile) {
  return path.join(
    REPO_ROOT,
    'target',
    profile === 'release' ? 'release' : 'debug',
    process.platform === 'win32' ? 'mustard-sidecar.exe' : 'mustard-sidecar',
  );
}

function wrapIife(body) {
  return `(() => {\n${body}\n})()`;
}

function createSmallComputeSource() {
  return wrapIife(`
    const values = [1, 2, 3, 4, 5, 6, 7, 8];
    let total = 0;
    for (let round = 0; round < 200; round += 1) {
      for (let index = 0; index < values.length; index += 1) {
        total += values[index] * (round + 1);
      }
    }
    return total;
  `);
}

function createCodeModeSource(operationCount = 200) {
  const operations = [];
  for (let i = 0; i < operationCount; i += 1) {
    operations.push({
      path: `/v1/${i % 4 === 0 ? 'accounts' : 'users'}/${i}/actions/${i % 7}`,
      method: i % 2 === 0 ? 'GET' : 'POST',
      tagA: i % 2 === 0 ? 'billing' : 'identity',
      tagB: i % 3 === 0 ? 'search' : 'mutate',
      tagC: i % 5 === 0 ? 'enterprise' : 'self-serve',
      schemaWeight: (i % 11) + 1,
    });
  }
  return wrapIife(`
    const operations = ${JSON.stringify(operations)};
    const matches = [];
    let schemaTotal = 0;
    for (let i = 0; i < operations.length; i += 1) {
      const operation = operations[i];
      const isBilling = operation.tagA === 'billing';
      const isAccountPath = operation.path.indexOf('/accounts/') !== -1;
      const supportsSearch = operation.tagB === 'search';
      if (isBilling && isAccountPath && supportsSearch) {
        matches.push(operation.method + ':' + operation.path);
        schemaTotal += operation.schemaWeight;
      }
    }
    return {
      count: matches.length,
      top: matches.slice(0, 8),
      schemaTotal,
    };
  `);
}

function createFanoutSource(callCount) {
  return wrapIife(`
    let total = 0;
    for (let i = 0; i < ${callCount}; i += 1) {
      total += fetch_value(i);
    }
    return total;
  `);
}

function createSuspendResumeSource(boundaryCount) {
  return `
    let total = 0;
    for (let i = 0; i < ${boundaryCount}; i += 1) {
      total += checkpoint(i + 1);
    }
    total;
  `;
}

function createRuntimeInitSource() {
  return '0;';
}

function createImmediateSuspendSource() {
  return 'checkpoint(1);';
}

function createBoundaryStartInputsSource() {
  return `
    checkpoint(payload.items.length + payload.meta.weights.length);
  `;
}

function createBoundaryResumeValueSource() {
  return `
    const payload = checkpoint(0);
    payload.items.length + payload.meta.weights.length;
  `;
}

function createBoundaryResumeErrorSource() {
  return `
    let total = 0;
    try {
      checkpoint(0);
    } catch (error) {
      total = error.details.items.length + error.details.meta.weights.length;
    }
    total;
  `;
}

function createExecutionOnlySource() {
  return `
    const seed = checkpoint(1);
    const values = [1, 2, 3, 4, 5, 6, 7, 8];
    let total = seed;
    for (let round = 0; round < 200; round += 1) {
      for (let index = 0; index < values.length; index += 1) {
        total += values[index] * (round + 1);
      }
    }
    total;
  `;
}

function createFailureSource() {
  return wrapIife(`
    const values = [];
    for (let i = 0; i < 10000; i += 1) {
      values.push(i);
    }
    return values.length;
  `);
}

function createIsolateLimitFailureSource() {
  return wrapIife(`
    while (true) {
    }
  `);
}

function createHostFailureSource() {
  return wrapIife(`
    let total = 0;
    total += fetch_value(1);
    total += explode(2);
    return total;
  `);
}

function createWorkflowDataset() {
  const members = [];
  const expenses = [];
  const levels = ['l1', 'l2', 'l3', 'l4'];
  for (let i = 0; i < 48; i += 1) {
    const level = levels[i % levels.length];
    members.push({
      id: `m${i}`,
      level,
      teamId: `t${i % 6}`,
      active: i % 7 !== 0,
    });
    for (let j = 0; j < 6; j += 1) {
      expenses.push({
        memberId: `m${i}`,
        amount: 80 + ((i * 17 + j * 23) % 210),
      });
    }
  }
  const budgets = {
    l1: 600,
    l2: 800,
    l3: 980,
    l4: 1150,
  };
  return { members, budgets, expenses };
}

function createBoundaryPayload(size) {
  const weights = Array.from({ length: size.weightCount }, (_, index) => (index + 1) * 3);
  const tags = Array.from({ length: size.tagCount }, (_, index) => `${size.name}-tag-${index}`);
  const items = Array.from({ length: size.itemCount }, (_, index) => ({
    id: `${size.name}-item-${index}`,
    metrics: {
      score: ((index * 7) % 19) + 11,
      cost: 100 + index * 17,
      ratio: Number((((index % 9) + 1) / 10).toFixed(2)),
    },
    flags: {
      active: index % 2 === 0,
      stale: index % 3 === 1,
      remote: index % 5 === 0,
    },
    tags: tags.slice(0, (index % tags.length) + 1),
  }));
  return {
    meta: {
      region: 'us-west-2',
      owner: `benchmark-${size.name}`,
      weights,
      tags,
    },
    items,
  };
}

function createBoundarySuspendArgsSource(payload) {
  return `checkpoint(${JSON.stringify(payload)});`;
}

function createWorkflowSource() {
  return wrapIife(`
    const members = get_team_members('org-1');
    const flagged = [];
    for (let i = 0; i < members.length; i += 1) {
      const member = members[i];
      if (!member.active) {
        continue;
      }
      const budget = get_budget_by_level(member.level);
      const expenses = get_expenses(member.id);
      let spent = 0;
      for (let j = 0; j < expenses.length; j += 1) {
        spent += expenses[j].amount;
      }
      if (spent > budget) {
        flagged.push({
          memberId: member.id,
          teamId: member.teamId,
          level: member.level,
          budget,
          spent,
          over: spent - budget,
        });
      }
    }
    for (let i = 0; i < flagged.length; i += 1) {
      for (let j = i + 1; j < flagged.length; j += 1) {
        if (flagged[j].over > flagged[i].over) {
          const current = flagged[i];
          flagged[i] = flagged[j];
          flagged[j] = current;
        }
      }
    }
    let totalOver = 0;
    for (let i = 0; i < flagged.length; i += 1) {
      totalOver += flagged[i].over;
    }
    return {
      count: flagged.length,
      totalOver,
      top: flagged.slice(0, 5),
    };
  `);
}

function createIsolateResumeClosure() {
  return `
    let total = $0.total;
    const boundaryCount = $0.boundaryCount;
    for (let index = $0.nextIndex; index < boundaryCount; index += 1) {
      total += checkpoint(index + 1);
      return {
        done: false,
        boundaryCount,
        nextIndex: index + 1,
        total,
      };
    }
    return {
      done: true,
      boundaryCount,
      nextIndex: boundaryCount,
      total,
    };
  `;
}

function expectedFanoutTotal(callCount) {
  return (callCount * (callCount - 1)) / 2;
}

function expectedSuspendTotal(boundaryCount) {
  return (boundaryCount * (boundaryCount + 1)) / 2;
}

function sidecarStepValue(step, result = undefined, blob = undefined) {
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
    snapshotId: result?.snapshot_id ?? null,
    policyId: result?.policy_id ?? null,
  };
}

function rssBytesForPid(pid) {
  try {
    const rssKb = execFileSync('ps', ['-o', 'rss=', '-p', String(pid)], {
      encoding: 'utf8',
    }).trim();
    return Number.parseInt(rssKb, 10) * 1024;
  } catch {
    return null;
  }
}

function processMemorySnapshot() {
  const usage = process.memoryUsage();
  return {
    heapUsedBytes: usage.heapUsed,
    rssBytes: usage.rss,
  };
}

function subtractMemory(after, before) {
  return {
    heapUsedDeltaBytes: after.heapUsedBytes - before.heapUsedBytes,
    rssDeltaBytes: after.rssBytes - before.rssBytes,
  };
}

async function withSidecar(profile, run) {
  const child = spawn(sidecarPath(profile), [], {
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
      child,
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

async function compileSidecarSource(request, source) {
  const { payload, blob } = await request({
    protocol_version: SIDECAR_PROTOCOL_VERSION,
    method: 'compile',
    id: 1,
    source,
  });
  assert.equal(payload.ok, true, `sidecar compile failed: ${payload.error}`);
  return {
    program: Buffer.from(blob),
    programId: payload.result.program_id ?? null,
  };
}

function sidecarStartRequestPayload(program, requestId, options, profile = false) {
  const payload = {
    protocol_version: SIDECAR_PROTOCOL_VERSION,
    method: 'start',
    id: requestId,
    options,
    profile,
  };
  if (typeof program.programId === 'string' && program.programId.length > 0) {
    payload.program_id = program.programId;
  }
  return payload;
}

function sidecarCapabilityNames(capabilities = undefined) {
  return capabilities ? Object.keys(capabilities) : [];
}

function sidecarEncodedInputs(inputs = {}) {
  return JSON.parse(encodeStructuredInputs(inputs));
}

function sidecarResumeAuth(snapshot) {
  return {
    snapshot_key_base64: SNAPSHOT_KEY_BASE64,
    snapshot_key_digest: snapshotKeyDigest(Buffer.from(SNAPSHOT_KEY, 'utf8')),
    snapshot_token: snapshotToken(snapshot, SNAPSHOT_KEY),
  };
}

function parseSidecarExecutionProfile(payload, requestMetrics) {
  const executionNs = payload.profile?.execution_ns;
  const responsePrepareNs = payload.profile?.response_prepare_ns;
  if (!Number.isFinite(executionNs) || !Number.isFinite(responsePrepareNs)) {
    return null;
  }
  const executionMs = executionNs / 1e6;
  const responseMaterializationMs = (responsePrepareNs / 1e6) + (requestMetrics.responseDecodeMs ?? 0);
  return {
    executionMs,
    responseMaterializationMs,
    requestTransportMs: Math.max(
      0,
      (requestMetrics.roundTripMs ?? 0) - executionMs - responseMaterializationMs,
    ),
  };
}

async function startSidecarProgram(request, program, options = {}) {
  const capabilities = options.capabilities;
  const capabilityNames = sidecarCapabilityNames(capabilities);
  const { payload, blob } = await request(
    sidecarStartRequestPayload(program, 2, {
      inputs: sidecarEncodedInputs(options.inputs ?? {}),
      capabilities: capabilityNames,
      limits: options.limits ?? {},
    }),
    typeof program.programId === 'string' && program.programId.length > 0 ? undefined : program.program,
  );
  assert.equal(payload.ok, true, `sidecar start failed: ${payload.error}`);
  return {
    capabilityNames,
    step: sidecarStepValue(payload.result.step, payload.result, blob),
  };
}

async function startProfiledSidecarProgram(request, program, options = {}) {
  const capabilities = options.capabilities;
  const capabilityNames = sidecarCapabilityNames(capabilities);
  const response = await request(
    sidecarStartRequestPayload(program, 2, {
      inputs: sidecarEncodedInputs(options.inputs ?? {}),
      capabilities: capabilityNames,
      limits: options.limits ?? {},
    }, true),
    typeof program.programId === 'string' && program.programId.length > 0 ? undefined : program.program,
  );
  assert.equal(response.payload.ok, true, `sidecar start failed: ${response.payload.error}`);
  return {
    capabilityNames,
    step: sidecarStepValue(response.payload.result.step, response.payload.result, response.blob),
    profile: parseSidecarExecutionProfile(response.payload, response),
  };
}

async function resumeSidecarSnapshot(
  request,
  step,
  payloadValue,
  requestId = 3,
) {
  assert.equal(typeof step.snapshotId, 'string', 'sidecar resume requires a cached snapshotId');
  assert.equal(typeof step.policyId, 'string', 'sidecar resume requires a cached policyId');
  const { payload, blob } = await request({
    protocol_version: SIDECAR_PROTOCOL_VERSION,
    method: 'resume',
    id: requestId,
    snapshot_id: step.snapshotId,
    policy_id: step.policyId,
    auth: sidecarResumeAuth(step.snapshot),
    payload: JSON.parse(encodeResumePayloadValue(payloadValue)),
  });
  assert.equal(payload.ok, true, `sidecar resume failed: ${payload.error}`);
  return sidecarStepValue(payload.result.step, payload.result, blob);
}

async function resumeProfiledSidecarSnapshot(
  request,
  step,
  payloadValue,
  requestId = 3,
) {
  assert.equal(typeof step.snapshotId, 'string', 'sidecar resume requires a cached snapshotId');
  assert.equal(typeof step.policyId, 'string', 'sidecar resume requires a cached policyId');
  const response = await request({
    protocol_version: SIDECAR_PROTOCOL_VERSION,
    method: 'resume',
    id: requestId,
    snapshot_id: step.snapshotId,
    policy_id: step.policyId,
    auth: sidecarResumeAuth(step.snapshot),
    payload: JSON.parse(encodeResumePayloadValue(payloadValue)),
    profile: true,
  });
  assert.equal(response.payload.ok, true, `sidecar resume failed: ${response.payload.error}`);
  return {
    step: sidecarStepValue(response.payload.result.step, response.payload.result, response.blob),
    profile: parseSidecarExecutionProfile(response.payload, response),
  };
}

async function runSidecarProgram(request, program, options = {}, resumeValue = undefined) {
  let { step } = await startSidecarProgram(request, program, options);
  let requestId = 3;
  while (step.type === 'suspended') {
    const payloadValue =
      typeof resumeValue === 'function'
        ? resumeValue(step)
        : options.capabilities?.[step.capability]?.(...step.args) ?? step.args[0];
    const resolvedPayload =
      payloadValue && typeof payloadValue.then === 'function'
        ? await payloadValue
        : payloadValue;
    step = await resumeSidecarSnapshot(request, step, resolvedPayload, requestId);
    requestId += 1;
  }
  return step.value;
}

async function runSidecarPtcBreakdownSample(request, program, scenario) {
  const capabilities = scenario.createCapabilities();
  const totals = {
    executionMs: 0,
    requestTransportMs: 0,
    responseMaterializationMs: 0,
  };
  let { step, profile } = await startProfiledSidecarProgram(request, program, {
    capabilities,
    inputs: scenario.inputs,
  });
  assert.ok(profile, `sidecar start profile missing for ${scenario.metricName}`);
  totals.executionMs += profile.executionMs;
  totals.requestTransportMs += profile.requestTransportMs;
  totals.responseMaterializationMs += profile.responseMaterializationMs;
  let requestId = 3;
  while (step.type === 'suspended') {
    const handler = capabilities[step.capability];
    assert.equal(
      typeof handler,
      'function',
      `PTC sidecar breakdown is missing capability \`${step.capability}\` for ${scenario.metricName}`,
    );
    const value = handler(...step.args);
    const resolved = value && typeof value.then === 'function' ? await value : value;
    ({ step, profile } = await resumeProfiledSidecarSnapshot(
      request,
      step,
      resolved,
      requestId,
    ));
    requestId += 1;
    assert.ok(profile, `sidecar resume profile missing for ${scenario.metricName}`);
    totals.executionMs += profile.executionMs;
    totals.requestTransportMs += profile.requestTransportMs;
    totals.responseMaterializationMs += profile.responseMaterializationMs;
  }
  scenario.assertResult(step.value);
  return totals;
}

async function captureSidecarPtcBreakdown(
  request,
  program,
  scenario,
  options = DEFAULT_OPTIONS,
) {
  for (let iteration = 0; iteration < options.warmup; iteration += 1) {
    await runSidecarPtcBreakdownSample(request, program, scenario);
  }
  const samples = {
    requestTransport: [],
    execution: [],
    responseMaterialization: [],
  };
  for (let iteration = 0; iteration < options.iterations; iteration += 1) {
    const sample = await runSidecarPtcBreakdownSample(request, program, scenario);
    samples.requestTransport.push(sample.requestTransportMs);
    samples.execution.push(sample.executionMs);
    samples.responseMaterialization.push(sample.responseMaterializationMs);
  }
  return {
    requestTransport: summarize(samples.requestTransport),
    execution: summarize(samples.execution),
    responseMaterialization: summarize(samples.responseMaterialization),
  };
}

function createDurableLoadOptions(capabilities) {
  return {
    capabilities,
    limits: {},
    snapshotKey: SNAPSHOT_KEY,
  };
}

function createDurableCheckpointContext(capabilities) {
  return new ExecutionContext({
    capabilities,
    limits: {},
    snapshotKey: SNAPSHOT_KEY,
  });
}

function durableCheckpointArgsBytes(args) {
  return structuredByteLength(args.length === 1 ? args[0] : args);
}

async function resumeAddonProgressToCompletion(progress, capabilities, scenario) {
  let step = progress;
  while (step instanceof Progress) {
    const handler = capabilities[step.capability];
    assert.equal(
      typeof handler,
      'function',
      `${scenario.metricName} is missing capability \`${step.capability}\` during durable resume`,
    );
    const value = handler(...step.args);
    const resolved = value && typeof value.then === 'function' ? await value : value;
    step = step.resume(resolved);
  }
  scenario.assertResult(step);
  return step;
}

async function takeAddonDurableCheckpoint(runtime, scenario, capabilities = scenario.createCapabilities()) {
  assert.equal(
    typeof scenario.checkpointCapability,
    'string',
    `${scenario.metricName} is missing durable checkpoint metadata`,
  );
  const context = createDurableCheckpointContext(capabilities);
  let progress = runtime.start({
    context,
    inputs: scenario.inputs,
  });
  while (progress instanceof Progress && progress.capability !== scenario.checkpointCapability) {
    const handler = capabilities[progress.capability];
    assert.equal(
      typeof handler,
      'function',
      `${scenario.metricName} is missing capability \`${progress.capability}\` before the durable checkpoint`,
    );
    const value = handler(...progress.args);
    const resolved = value && typeof value.then === 'function' ? await value : value;
    progress = progress.resume(resolved);
  }
  assert.ok(progress instanceof Progress, `${scenario.metricName} should suspend at the durable checkpoint`);
  assert.equal(
    progress.capability,
    scenario.checkpointCapability,
    `${scenario.metricName} should suspend on ${scenario.checkpointCapability}`,
  );
  return { progress, capabilities };
}

async function createAddonDurableCheckpointState(runtime, scenario) {
  const { progress, capabilities } = await takeAddonDurableCheckpoint(runtime, scenario);
  const checkpointArgs = structuredClone(progress.args);
  const dumped = progress.dump();
  return {
    capabilities,
    checkpointArgs,
    dumped,
    snapshotBytes: dumped.snapshot.length,
    detachedManifestBytes: Buffer.byteLength(dumped.suspended_manifest, 'utf8'),
    checkpointArgsBytes: durableCheckpointArgsBytes(checkpointArgs),
  };
}

async function runAddonDurableResumeOnlySample(runtime, scenario) {
  const checkpoint = await createAddonDurableCheckpointState(runtime, scenario);
  const started = performance.now();
  const restored = Progress.load(
    checkpoint.dumped,
    createDurableLoadOptions(checkpoint.capabilities),
  );
  assert.equal(
    restored.capability,
    scenario.checkpointCapability,
    `${scenario.metricName} restore should resume from the durable checkpoint`,
  );
  await resumeAddonProgressToCompletion(restored, checkpoint.capabilities, scenario);
  return performance.now() - started;
}

async function captureAddonDurableResumeOnly(runtime, scenario, options = DEFAULT_OPTIONS) {
  for (let iteration = 0; iteration < options.warmup; iteration += 1) {
    await runAddonDurableResumeOnlySample(runtime, scenario);
  }
  const samples = [];
  for (let iteration = 0; iteration < options.iterations; iteration += 1) {
    samples.push(await runAddonDurableResumeOnlySample(runtime, scenario));
  }
  return summarize(samples);
}

async function takeSidecarDurableCheckpoint(request, program, scenario) {
  assert.equal(
    typeof scenario.checkpointCapability,
    'string',
    `${scenario.metricName} is missing durable checkpoint metadata`,
  );
  const capabilities = scenario.createCapabilities();
  let { step } = await startSidecarProgram(request, program, {
    capabilities,
    inputs: scenario.inputs,
  });
  let requestId = 3;
  while (step.type === 'suspended' && step.capability !== scenario.checkpointCapability) {
    const handler = capabilities[step.capability];
    assert.equal(
      typeof handler,
      'function',
      `${scenario.metricName} is missing capability \`${step.capability}\` before the durable checkpoint`,
    );
    const value = handler(...step.args);
    const resolved = value && typeof value.then === 'function' ? await value : value;
    step = await resumeSidecarSnapshot(request, step, resolved, requestId);
    requestId += 1;
  }
  assert.equal(step.type, 'suspended');
  assert.equal(
    step.capability,
    scenario.checkpointCapability,
    `${scenario.metricName} should suspend on ${scenario.checkpointCapability}`,
  );
  return { step, capabilities };
}

async function captureSidecarDurableCheckpointState(profile, scenario) {
  return withSidecar(profile, async ({ request }) => {
    const program = await compileSidecarSource(request, scenario.source);
    const { step, capabilities } = await takeSidecarDurableCheckpoint(request, program, scenario);
    const resumePlan = await captureScenarioResumePlan(scenario);
    const checkpointIndex = resumePlan.findIndex(
      (entry) => entry.capability === scenario.checkpointCapability,
    );
    assert.ok(checkpointIndex >= 0, `${scenario.metricName} should have a replayable durable checkpoint`);
    const capabilityNames = sidecarCapabilityNames(capabilities);
    const policy = {
      capabilities: capabilityNames,
      limits: {},
      snapshot_id: step.snapshotId,
      snapshot_key_base64: SNAPSHOT_KEY_BASE64,
      snapshot_key_digest: snapshotKeyDigest(Buffer.from(SNAPSHOT_KEY, 'utf8')),
      snapshot_token: snapshotToken(step.snapshot, SNAPSHOT_KEY),
    };
    return {
      checkpointArgs: structuredClone(step.args),
      snapshot: Buffer.from(step.snapshot),
      snapshotBytes: step.snapshot.length,
      checkpointArgsBytes: durableCheckpointArgsBytes(step.args),
      resumePlan: resumePlan.slice(checkpointIndex),
      policy,
      policyBytes: structuredByteLength(policy),
    };
  });
}

async function runSidecarDurableResumeOnlySample(request, persisted, scenario, requestBaseId = 7000) {
  const capabilities = createQueuedCapabilities(
    persisted.resumePlan,
    `${scenario.metricName} durable sidecar replay`,
  );
  const checkpointValue = capabilities[scenario.checkpointCapability](...persisted.checkpointArgs);
  const started = performance.now();
  const restored = await request({
    protocol_version: SIDECAR_PROTOCOL_VERSION,
    method: 'resume',
    id: requestBaseId,
    policy: persisted.policy,
    payload: JSON.parse(encodeResumePayloadValue(checkpointValue)),
  }, persisted.snapshot);
  assert.equal(restored.payload.ok, true, `${scenario.metricName} durable raw resume failed: ${restored.payload.error}`);
  let step = sidecarStepValue(restored.payload.result.step, restored.payload.result, restored.blob);
  let requestId = requestBaseId + 1;
  while (step.type === 'suspended') {
    const handler = capabilities[step.capability];
    assert.equal(
      typeof handler,
      'function',
      `${scenario.metricName} is missing capability \`${step.capability}\` during durable raw resume`,
    );
    const value = handler(...step.args);
    const resolved = value && typeof value.then === 'function' ? await value : value;
    const completion = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'resume',
      id: requestId,
      snapshot_id: step.snapshotId,
      policy_id: step.policyId,
      auth: sidecarResumeAuth(step.snapshot),
      payload: JSON.parse(encodeResumePayloadValue(resolved)),
    });
    assert.equal(completion.payload.ok, true, `${scenario.metricName} durable completion failed: ${completion.payload.error}`);
    step = sidecarStepValue(completion.payload.result.step, completion.payload.result, completion.blob);
    requestId += 1;
  }
  assert.equal(step.type, 'completed');
  scenario.assertResult(step.value);
  return performance.now() - started;
}

async function captureSidecarDurableResumeOnly(profile, scenario, options = DEFAULT_OPTIONS) {
  const persisted = await captureSidecarDurableCheckpointState(profile, scenario);
  const samples = [];
  await withSidecar(profile, async ({ request }) => {
    for (let iteration = 0; iteration < options.warmup; iteration += 1) {
      await runSidecarDurableResumeOnlySample(request, persisted, scenario, 7000 + iteration * 10);
    }
    for (let iteration = 0; iteration < options.iterations; iteration += 1) {
      samples.push(
        await runSidecarDurableResumeOnlySample(
          request,
          persisted,
          scenario,
          8000 + iteration * 10,
        ),
      );
    }
  });
  return {
    resumeOnly: summarize(samples),
    state: {
      snapshotBytes: persisted.snapshotBytes,
      fullPolicyBytes: persisted.policyBytes,
      checkpointArgsBytes: persisted.checkpointArgsBytes,
    },
  };
}

async function captureIsolateDurableCheckpointState(scenario) {
  const resumePlan = await captureScenarioResumePlan(scenario);
  const checkpointIndex = resumePlan.findIndex(
    (entry) => entry.capability === scenario.checkpointCapability,
  );
  assert.ok(checkpointIndex >= 0, `${scenario.metricName} should suspend on its durable checkpoint`);
  const checkpointArgs = structuredClone(resumePlan[checkpointIndex].args);
  return {
    checkpointArgs,
    checkpointArgsBytes: durableCheckpointArgsBytes(checkpointArgs),
    resumePlan: resumePlan.map((entry, index) => ({
      ...entry,
      checkpoint: index === checkpointIndex,
    })),
  };
}

async function captureIsolateDurableResumeOnly(scenario, options = DEFAULT_OPTIONS) {
  const persisted = await captureIsolateDurableCheckpointState(scenario);
  const samples = [];
  for (let iteration = 0; iteration < options.warmup; iteration += 1) {
    const isolate = new ivm.Isolate({ memoryLimit: 128 });
    const script = isolate.compileScriptSync(scenario.source);
    const context = isolate.createContextSync();
    let started = null;
    installIsolateCapabilities(
      context,
      createQueuedCapabilities(
        persisted.resumePlan,
        `${scenario.metricName} isolate durable replay`,
        (entry) => {
          if (entry.checkpoint && started === null) {
            started = performance.now();
          }
        },
      ),
      scenario.inputs,
    );
    const result = await runIsolateScript(context, script);
    assert.ok(started !== null, `${scenario.metricName} isolate durable replay never reached its checkpoint`);
    scenario.assertResult(result);
  }
  for (let iteration = 0; iteration < options.iterations; iteration += 1) {
    const isolate = new ivm.Isolate({ memoryLimit: 128 });
    const script = isolate.compileScriptSync(scenario.source);
    const context = isolate.createContextSync();
    let started = null;
    installIsolateCapabilities(
      context,
      createQueuedCapabilities(
        persisted.resumePlan,
        `${scenario.metricName} isolate durable replay`,
        (entry) => {
          if (entry.checkpoint && started === null) {
            started = performance.now();
          }
        },
      ),
      scenario.inputs,
    );
    const result = await runIsolateScript(context, script);
    assert.ok(started !== null, `${scenario.metricName} isolate durable replay never reached its checkpoint`);
    scenario.assertResult(result);
    samples.push(performance.now() - started);
  }
  return {
    resumeOnly: summarize(samples),
    state: {
      carriedStateBytes: persisted.checkpointArgsBytes,
    },
  };
}

function installIsolateCapabilities(context, capabilities = {}, inputs = {}) {
  const jail = context.global;
  jail.setSync('global', jail.derefInto());
  for (const [name, handler] of Object.entries(capabilities)) {
    if (handler?.constructor?.name === 'AsyncFunction') {
      jail.setSync(
        `__host_${name}`,
        new ivm.Reference((...args) =>
          Promise.resolve(handler(...args)).then((value) => JSON.stringify(value)),
        ),
      );
      continue;
    }
    jail.setSync(`__host_${name}`, new ivm.Reference(handler));
  }
  if (Object.keys(capabilities).length > 0) {
    context.evalSync(`
      ${Object.keys(capabilities)
        .map(
          (name) => capabilities[name]?.constructor?.name === 'AsyncFunction'
            ? `global.${name} = function(...args) {
        const raw = __host_${name}.applySyncPromise(undefined, args, {
          arguments: { copy: true },
        });
        return JSON.parse(raw);
      };`
            : `global.${name} = function(...args) {
        return __host_${name}.applySync(undefined, args, {
          arguments: { copy: true },
          result: { copy: true }
        });
      };`,
        )
        .join('\n')}
    `);
  }
  if (Object.keys(inputs).length > 0) {
    context.evalSync(
      Object.entries(inputs)
        .map(([name, value]) => `global.${name} = ${JSON.stringify(value)};`)
        .join('\n'),
    );
  }
}

function createIsolateContext(capabilities = {}) {
  const isolate = new ivm.Isolate({ memoryLimit: 128 });
  const context = isolate.createContextSync();
  installIsolateCapabilities(context, capabilities);
  return { isolate, context };
}

function runIsolateScript(context, script) {
  return script.run(context, { promise: true, copy: true });
}

function benchmarkFixtureSet() {
  return {
    smallSource: createSmallComputeSource(),
    codeModeSource: createCodeModeSource(),
    workflowSource: createWorkflowSource(),
    failureSource: createFailureSource(),
    isolateLimitFailureSource: createIsolateLimitFailureSource(),
    hostFailureSource: createHostFailureSource(),
    workflowData: createWorkflowDataset(),
    ptcScenarios: createPtcScenarios(),
    durablePtcScenarios: createDurablePtcScenarios(),
    galleryScenarios: createGalleryScenarios(),
    headlineSeedScenarios: createHeadlineSeedScenarios(),
    sentinelScenarios: createSentinelScenarios(),
  };
}

function isFullMode(mode) {
  return mode === 'full';
}

function syntheticPtcMetricNamesForMode(mode) {
  if (mode === 'ptc_public') {
    return ['ptc_website_demo_small'];
  }
  if (mode === 'full') {
    return null;
  }
  return [];
}

function galleryMetricNamesForMode(mode) {
  if (mode === 'ptc_headline_release') {
    return HEADLINE_USE_CASE_IDS.map((id) => metricNameForUseCase(id));
  }
  if (mode === 'ptc_broad_release') {
    return BROAD_USE_CASE_IDS.map((id) => metricNameForUseCase(id));
  }
  if (mode === 'ptc_holdout_release') {
    return HOLDOUT_USE_CASE_IDS.map((id) => metricNameForUseCase(id));
  }
  if (mode === 'ptc_gallery_canary') {
    return [
      ...BROAD_USE_CASE_IDS.map((id) => metricNameForUseCase(id)),
      ...HOLDOUT_USE_CASE_IDS.map((id) => metricNameForUseCase(id)),
    ];
  }
  if (mode === 'ptc_sentinel_release' || mode === 'ptc_public') {
    return [];
  }
  return [
    ...BROAD_USE_CASE_IDS.map((id) => metricNameForUseCase(id)),
    ...HOLDOUT_USE_CASE_IDS.map((id) => metricNameForUseCase(id)),
  ];
}

function shouldBenchmarkSentinels(mode) {
  return mode === 'full' || mode === 'ptc_sentinel_release';
}

function shouldBenchmarkDurablePtc(mode) {
  return mode !== 'ptc_public' && mode !== 'ptc_sentinel_release';
}

function headlineSeedMetricNamesForMode(mode) {
  if (
    mode === 'full' ||
    mode === 'ptc_headline_release' ||
    mode === 'ptc_broad_release' ||
    mode === 'ptc_gallery_canary'
  ) {
    return HEADLINE_USE_CASE_IDS.map((id) => metricNameForUseCase(id, 'medium', 'skewed'));
  }
  return [];
}

function galleryMeasureOptionsForMode(mode) {
  if (mode === 'ptc_gallery_canary') {
    return GALLERY_CANARY_OPTIONS;
  }
  if (mode === 'ptc_headline_release' || mode === 'ptc_broad_release' || mode === 'ptc_holdout_release') {
    return STABLE_PTC_RELEASE_OPTIONS;
  }
  return DEFAULT_OPTIONS;
}

function headlineSeedMeasureOptionsForMode(mode) {
  if (mode === 'ptc_gallery_canary') {
    return GALLERY_CANARY_OPTIONS;
  }
  if (mode === 'ptc_headline_release' || mode === 'ptc_broad_release') {
    return STABLE_PTC_RELEASE_OPTIONS;
  }
  return DEFAULT_OPTIONS;
}

function ptcMeasureOptionsForMode(mode) {
  if (mode === 'ptc_public') {
    return STABLE_PTC_RELEASE_OPTIONS;
  }
  return DEFAULT_OPTIONS;
}

function durablePtcMeasureOptionsForMode(mode) {
  if (mode === 'ptc_headline_release' || mode === 'ptc_broad_release' || mode === 'ptc_holdout_release') {
    return STABLE_PTC_RELEASE_OPTIONS;
  }
  return DEFAULT_OPTIONS;
}

function sentinelMeasureOptionsForMode(mode) {
  if (mode === 'ptc_sentinel_release') {
    return STABLE_PTC_RELEASE_OPTIONS;
  }
  return DEFAULT_OPTIONS;
}

function representativePhase2AttributionMetricNamesForMode(mode) {
  if (mode === 'ptc_headline_release' || mode === 'ptc_broad_release') {
    return PHASE2_REPRESENTATIVE_PTC_METRICS;
  }
  return [];
}

function createWorkflowCapabilities(data) {
  const expensesByMember = new Map();
  for (const entry of data.expenses) {
    let bucket = expensesByMember.get(entry.memberId);
    if (!bucket) {
      bucket = [];
      expensesByMember.set(entry.memberId, bucket);
    }
    bucket.push({ amount: entry.amount });
  }
  return {
    get_team_members() {
      return data.members.map((member) => ({ ...member }));
    },
    get_budget_by_level(level) {
      return data.budgets[level];
    },
    get_expenses(memberId) {
      return (expensesByMember.get(memberId) ?? []).map((entry) => ({ ...entry }));
    },
  };
}

function assertWorkflowResult(result) {
  assert.equal(typeof result.count, 'number');
  assert.equal(typeof result.totalOver, 'number');
  assert.ok(Array.isArray(result.top));
  assert.ok(result.count >= result.top.length);
}

async function capturePtcTransfer(runScenario, scenario) {
  const probe = createCapabilityTransferProbe(scenario.createCapabilities());
  const result = await runScenario(probe.capabilities);
  scenario.assertResult(result);
  return {
    laneId: scenario.laneId,
    sizeName: scenario.sizeName,
    ...scenario.shape,
    ...probe.finalize(result),
  };
}

function createPtcCounterResumeResolver(scenario) {
  const capabilities = scenario.createCapabilities();
  return async (step) => {
    const handler = capabilities[step.capability];
    assert.equal(
      typeof handler,
      'function',
      `PTC counter collection is missing capability \`${step.capability}\` for ${scenario.metricName}`,
    );
    const value = handler(...step.args);
    return value && typeof value.then === 'function' ? await value : value;
  };
}

function createQueuedCapabilities(entries, label, onUse = undefined) {
  const queue = entries.map((entry) => ({
    capability: entry.capability,
    args: structuredClone(entry.args),
    value: structuredClone(entry.value),
    checkpoint: entry.checkpoint === true,
    used: false,
  }));
  const capabilityNames = [...new Set(queue.map((entry) => entry.capability))];
  return Object.fromEntries(
    capabilityNames.map((name) => [
      name,
      (...args) => {
        let next = queue.find((entry) => {
          if (entry.used || entry.capability !== name) {
            return false;
          }
          try {
            assert.deepStrictEqual(entry.args, args);
            return true;
          } catch {
            return false;
          }
        });
        if (!next) {
          next = queue.find((entry) => !entry.used && entry.capability === name);
        }
        assert.ok(next, `${label} exhausted its planned resume queue before completion`);
        next.used = true;
        if (typeof onUse === 'function') {
          onUse(next, args);
        }
        return structuredClone(next.value);
      },
    ]),
  );
}

async function captureScenarioResumePlan(scenario) {
  const capabilities = scenario.createCapabilities();
  const runtime = new Mustard(scenario.source);
  let step = runtime.start({
    context: new ExecutionContext({
      capabilities,
      snapshotKey: SNAPSHOT_KEY,
    }),
    inputs: scenario.inputs,
  });
  const entries = [];
  while (step instanceof Progress) {
    const handler = capabilities[step.capability];
    assert.equal(
      typeof handler,
      'function',
      `${scenario.metricName} is missing capability \`${step.capability}\` while materializing the resume plan`,
    );
    const value = handler(...step.args);
    const resolved = value && typeof value.then === 'function' ? await value : value;
    entries.push({
      capability: step.capability,
      args: structuredClone(step.args),
      value: structuredClone(resolved),
    });
    step = step.resume(resolved);
  }
  scenario.assertResult(step);
  return entries;
}

async function measureRetainedMemory(runMany, options = {}) {
  const sample = options.sample ?? processMemorySnapshot;
  global.gc();
  const before = sample();
  await runMany();
  global.gc();
  const after = sample();
  return subtractMemory(after, before);
}

async function measureRetainedLiveMemory(createLiveState, options = {}) {
  const sample = options.sample ?? processMemorySnapshot;
  global.gc();
  const before = sample();
  const liveState = await createLiveState();
  void liveState;
  global.gc();
  const after = sample();
  return subtractMemory(after, before);
}

async function measureFailureCleanup(name, failThenRecover, options = DEFAULT_OPTIONS) {
  const [, summary] = await measure(name, async () => {
    await failThenRecover();
  }, options);
  return summary;
}

function createPhaseLoadOptions() {
  return {
    context: new ExecutionContext({
      snapshotKey: SNAPSHOT_KEY,
      capabilities: {
        checkpoint() {},
      },
      limits: {},
    }),
  };
}

function takeProgress(step, label) {
  assert.ok(step instanceof Progress, `${label} should suspend`);
  return step;
}

function createPhaseProgressFactory(runtime, context) {
  return () => takeProgress(runtime.start({ context }), 'phase benchmark');
}

function createAuthenticatedPolicyJson(dumped, loadOptions) {
  const context = resolveProgressLoadContext(dumped, dumped.snapshot, loadOptions);
  return encodeSnapshotPolicy(context.policy, {
    snapshotId: dumped.snapshot_id,
    snapshotKey: context.snapshotKey,
    snapshotKeyBase64: context.snapshotKeyBase64,
    snapshotKeyDigest: context.snapshotKeyDigest,
    snapshotToken: dumped.token,
  });
}

function parseNativeStepObjectWithMetrics(step) {
  const metrics = step.metrics ?? null;
  if (step.type === 'completed') {
    return {
      type: 'completed',
      value: decodeStructured(step.value),
      metrics,
    };
  }
  return {
    type: 'suspended',
    capability: step.capability,
    args: step.args.map(decodeStructured),
    snapshotHandle:
      typeof step.snapshot_handle === 'string' && step.snapshot_handle.length > 0
        ? step.snapshot_handle
        : null,
    metrics,
  };
}

function parseNativeStepWithMetrics(stepJson) {
  return parseNativeStepObjectWithMetrics(JSON.parse(stepJson));
}

function parseNativeInspectionWithMetrics(inspectionJson) {
  const inspection = JSON.parse(inspectionJson);
  return {
    capability: inspection.capability,
    args: inspection.args.map(decodeStructured),
    metrics: inspection.metrics ?? null,
  };
}

function parseNativeProfiledStepWithMetrics(stepJson) {
  const profiled = JSON.parse(stepJson);
  return {
    step: parseNativeStepObjectWithMetrics(profiled.step),
    profile: {
      parseMs: profiled.profile.parse_ns / 1e6,
      executeMs: profiled.profile.execute_ns / 1e6,
      encodeMs: profiled.profile.encode_ns / 1e6,
    },
  };
}

async function collectAddonStepCounters(runtime, options, resumeValueForStep = undefined) {
  const native = loadNative();
  const { policy, nativeContextHandle } = resolveExecutionContext(
    options,
    'benchmark counter options',
  );
  const programHandle = runtime._ensureProgramHandle();
  const startProgram =
    typeof nativeContextHandle === 'string' && nativeContextHandle.length > 0
      ? native.startProgramWithExecutionContextHandleBuffer
      : native.startProgramWithSnapshotHandleBuffer;
  const startArgs =
    typeof nativeContextHandle === 'string' && nativeContextHandle.length > 0
      ? [programHandle, nativeContextHandle, encodeStructuredInputsBuffer(options.inputs)]
      : [programHandle, encodeStartOptionsBuffer(options.inputs, policy)];
  let step = parseNativeStepWithMetrics(
    callNative(startProgram, ...startArgs),
  );
  let metrics = step.metrics;
  while (step.type === 'suspended') {
    const snapshotHandle = step.snapshotHandle;
    assert.ok(snapshotHandle, 'counter collection step should retain a snapshot handle');
    try {
      const nextValue =
        resumeValueForStep === undefined ? step.args[0] : await resumeValueForStep(step);
      step = parseNativeStepWithMetrics(
        callNative(
          native.resumeSnapshotHandleBuffer,
          snapshotHandle,
          encodeResumePayloadValueBuffer(nextValue),
        ),
      );
      metrics = step.metrics;
    } finally {
      try {
        callNative(native.releaseSnapshotHandle, snapshotHandle);
      } catch {
        // Best-effort cleanup only; the handle may already have been consumed.
      }
    }
  }
  return metrics;
}

async function invokeAddonCapability(hostHandlers, step, totals, scenario) {
  const handler = hostHandlers[step.capability];
  assert.equal(
    typeof handler,
    'function',
    `PTC boundary breakdown is missing capability \`${step.capability}\` for ${scenario.metricName}`,
  );
  const started = performance.now();
  try {
    const value = handler(...step.args);
    return value && typeof value.then === 'function' ? await value : value;
  } finally {
    totals.hostCallbacksMs += performance.now() - started;
  }
}

async function runAddonPtcBoundaryBreakdownSample(runtime, scenario) {
  const native = loadNative();
  const context = new ExecutionContext({
    capabilities: scenario.createCapabilities(),
    limits: {},
    snapshotKey: SNAPSHOT_KEY,
  });
  const { hostHandlers, policy, nativeContextHandle } = resolveExecutionContext(
    {
      context,
      inputs: scenario.inputs,
    },
    `${scenario.metricName} boundary breakdown options`,
  );
  const programHandle = runtime._ensureProgramHandle();
  const startProgram =
    typeof nativeContextHandle === 'string' && nativeContextHandle.length > 0
      ? native.profileStartProgramWithExecutionContextHandleBuffer
      : native.profileStartProgramWithSnapshotHandleBuffer;
  const startArgs =
    typeof nativeContextHandle === 'string' && nativeContextHandle.length > 0
      ? [programHandle, nativeContextHandle, encodeStructuredInputsBuffer(scenario.inputs)]
      : [programHandle, encodeStartOptionsBuffer(scenario.inputs, policy)];
  const totals = {
    hostCallbacksMs: 0,
    guestExecutionMs: 0,
    boundaryParseMs: 0,
    boundaryEncodeMs: 0,
  };
  let { step, profile } = parseNativeProfiledStepWithMetrics(callNative(startProgram, ...startArgs));
  totals.guestExecutionMs += profile.executeMs;
  totals.boundaryParseMs += profile.parseMs;
  totals.boundaryEncodeMs += profile.encodeMs;

  while (step.type === 'suspended') {
    const snapshotHandle = step.snapshotHandle;
    assert.ok(snapshotHandle, `${scenario.metricName} profiled steps should retain a snapshot handle`);
    try {
      let payload;
      try {
        const value = await invokeAddonCapability(hostHandlers, step, totals, scenario);
        payload = encodeResumePayloadValueBuffer(value);
      } catch (error) {
        payload = encodeResumePayloadErrorBuffer(error);
      }
      ({ step, profile } = parseNativeProfiledStepWithMetrics(
        callNative(native.profileResumeSnapshotHandleBuffer, snapshotHandle, payload),
      ));
      totals.guestExecutionMs += profile.executeMs;
      totals.boundaryParseMs += profile.parseMs;
      totals.boundaryEncodeMs += profile.encodeMs;
    } finally {
      try {
        callNative(native.releaseSnapshotHandle, snapshotHandle);
      } catch {
        // Best-effort cleanup only; the handle may already have been consumed.
      }
    }
  }

  scenario.assertResult(step.value);
  return {
    hostCallbacksMs: totals.hostCallbacksMs,
    guestExecutionMs: totals.guestExecutionMs,
    boundaryParseMs: totals.boundaryParseMs,
    boundaryEncodeMs: totals.boundaryEncodeMs,
    boundaryCodecMs: totals.boundaryParseMs + totals.boundaryEncodeMs,
  };
}

async function captureAddonPtcBoundaryBreakdown(
  runtime,
  scenario,
  options = DEFAULT_OPTIONS,
) {
  for (let iteration = 0; iteration < options.warmup; iteration += 1) {
    await runAddonPtcBoundaryBreakdownSample(runtime, scenario);
  }
  const samples = {
    hostCallbacks: [],
    guestExecution: [],
    boundaryParse: [],
    boundaryEncode: [],
    boundaryCodec: [],
  };
  for (let iteration = 0; iteration < options.iterations; iteration += 1) {
    const sample = await runAddonPtcBoundaryBreakdownSample(runtime, scenario);
    samples.hostCallbacks.push(sample.hostCallbacksMs);
    samples.guestExecution.push(sample.guestExecutionMs);
    samples.boundaryParse.push(sample.boundaryParseMs);
    samples.boundaryEncode.push(sample.boundaryEncodeMs);
    samples.boundaryCodec.push(sample.boundaryCodecMs);
  }
  return {
    hostCallbacks: summarize(samples.hostCallbacks),
    guestExecution: summarize(samples.guestExecution),
    boundaryParse: summarize(samples.boundaryParse),
    boundaryEncode: summarize(samples.boundaryEncode),
    boundaryCodec: summarize(samples.boundaryCodec),
  };
}

function queuedFactory(factory) {
  const queue = [];
  return () => {
    if (queue.length === 0) {
      queue.push(factory());
    }
    return queue.shift();
  };
}

async function benchmarkAddonPhases() {
  const native = loadNative();
  const loadOptions = createPhaseLoadOptions();
  const defaultContext = new ExecutionContext();
  const checkpointContext = loadOptions.context;
  const runtimeInitRuntime = new Mustard(createRuntimeInitSource());
  const suspendRuntime = new Mustard(createImmediateSuspendSource());
  const executionRuntime = new Mustard(createExecutionOnlySource());

  const phaseLatency = {};
  const phaseProgressFactory = createPhaseProgressFactory(executionRuntime, checkpointContext);
  const suspendProgressFactory = createPhaseProgressFactory(suspendRuntime, checkpointContext);

  const runtimeInit = await measureSamples('runtime_init_only', async () => {
    const start = performance.now();
    const result = await runtimeInitRuntime.run({ context: defaultContext });
    const duration = performance.now() - start;
    assert.equal(result, 0);
    return duration;
  }, DEFAULT_OPTIONS);
  phaseLatency[runtimeInit[0]] = runtimeInit[1];

  const executionOnly = await measureSamples('execution_only_small', async () => {
    const progress = phaseProgressFactory();
    const start = performance.now();
    const result = progress.resume(1);
    const duration = performance.now() - start;
    assert.equal(typeof result, 'number');
    return duration;
  }, DEFAULT_OPTIONS);
  phaseLatency[executionOnly[0]] = executionOnly[1];

  const suspendOnly = await measureSamples('suspend_only', async () => {
    const start = performance.now();
    const progress = suspendRuntime.start({ context: checkpointContext });
    const duration = performance.now() - start;
    assert.ok(progress instanceof Progress);
    return duration;
  }, DEFAULT_OPTIONS);
  phaseLatency[suspendOnly[0]] = suspendOnly[1];

  const dumpOnly = await measureSamples('snapshot_dump_only', async () => {
    const progress = suspendProgressFactory();
    const start = performance.now();
    const dumped = progress.dump();
    const duration = performance.now() - start;
    assert.ok(Buffer.isBuffer(dumped.snapshot));
    return duration;
  }, DEFAULT_OPTIONS);
  phaseLatency[dumpOnly[0]] = dumpOnly[1];

  const applyPolicyOnlyDump = queuedFactory(() => suspendProgressFactory().dump());
  const applyPolicyOnly = await measureSamples('apply_snapshot_policy_only', async () => {
    const dumped = applyPolicyOnlyDump();
    const start = performance.now();
    const context = resolveProgressLoadContext(dumped, dumped.snapshot, loadOptions);
    const duration = performance.now() - start;
    assert.equal(typeof context.policy, 'object');
    return duration;
  }, DEFAULT_OPTIONS);
  phaseLatency[applyPolicyOnly[0]] = applyPolicyOnly[1];

  const inspectProgramHandle = callNative(
    native.loadProgram,
    suspendProgressFactory().dump().program,
  );
  const inspectDump = queuedFactory(() => suspendProgressFactory().dump());
  const snapshotLoadOnly = await measureSamples('snapshot_load_only', async () => {
    const dumped = inspectDump();
    const policyJson = createAuthenticatedPolicyJson(dumped, loadOptions);
    const start = performance.now();
    const inspection = JSON.parse(
      callNative(native.inspectDetachedSnapshot, inspectProgramHandle, dumped.snapshot, policyJson),
    );
    const duration = performance.now() - start;
    assert.equal(inspection.capability, 'checkpoint');
    return duration;
  }, DEFAULT_OPTIONS);
  phaseLatency[snapshotLoadOnly[0]] = snapshotLoadOnly[1];

  const progressLoadDump = queuedFactory(() => suspendProgressFactory().dump());
  const progressLoadOnly = await measureSamples('Progress.load_only', async () => {
    const dumped = progressLoadDump();
    const start = performance.now();
    const progress = Progress.load(dumped, loadOptions);
    const duration = performance.now() - start;
    assert.equal(progress.capability, 'checkpoint');
    assert.equal(progress.resume(1), 1);
    return duration;
  }, DEFAULT_OPTIONS);
  phaseLatency[progressLoadOnly[0]] = progressLoadOnly[1];

  callNative(native.releaseProgram, inspectProgramHandle);

  return phaseLatency;
}

async function benchmarkAddonBoundary() {
  const boundary = {
    startInputs: {},
    suspendedArgs: {},
    resumeValues: {},
    resumeErrors: {},
  };
  const context = new ExecutionContext({
    capabilities: { checkpoint() {} },
    limits: {},
    snapshotKey: SNAPSHOT_KEY,
  });
  const startInputsRuntime = new Mustard(createBoundaryStartInputsSource());
  const resumeValuesRuntime = new Mustard(createBoundaryResumeValueSource());
  const resumeErrorsRuntime = new Mustard(createBoundaryResumeErrorSource());

  for (const size of BOUNDARY_VALUE_SIZES) {
    const payload = createBoundaryPayload(size);
    const expectedCount = size.itemCount + size.weightCount;
    const suspendedArgsRuntime = new Mustard(createBoundarySuspendArgsSource(payload));
    const resumeValueFactory = createPhaseProgressFactory(resumeValuesRuntime, context);
    const resumeErrorFactory = createPhaseProgressFactory(resumeErrorsRuntime, context);

    const startInputs = await measureSamples(`start_inputs_${size.name}`, async () => {
      const start = performance.now();
      const progress = startInputsRuntime.start({
        context,
        inputs: { payload },
      });
      const duration = performance.now() - start;
      assert.ok(progress instanceof Progress);
      assert.equal(progress.args[0], expectedCount);
      return duration;
    }, DEFAULT_OPTIONS);
    boundary.startInputs[size.name] = startInputs[1];

    const suspendedArgs = await measureSamples(`suspended_args_${size.name}`, async () => {
      const start = performance.now();
      const progress = suspendedArgsRuntime.start({ context });
      const duration = performance.now() - start;
      assert.ok(progress instanceof Progress);
      assert.equal(progress.args[0].items.length, size.itemCount);
      assert.equal(progress.args[0].meta.weights.length, size.weightCount);
      return duration;
    }, DEFAULT_OPTIONS);
    boundary.suspendedArgs[size.name] = suspendedArgs[1];

    const resumeValues = await measureSamples(`resume_values_${size.name}`, async () => {
      const progress = resumeValueFactory();
      const start = performance.now();
      const result = progress.resume(payload);
      const duration = performance.now() - start;
      assert.equal(result, expectedCount);
      return duration;
    }, DEFAULT_OPTIONS);
    boundary.resumeValues[size.name] = resumeValues[1];

    const resumeErrors = await measureSamples(`resume_errors_${size.name}`, async () => {
      const progress = resumeErrorFactory();
      const start = performance.now();
      const result = progress.resumeError({
        name: 'Error',
        message: `boundary-${size.name}`,
        code: `E_BOUNDARY_${size.name.toUpperCase()}`,
        details: payload,
      });
      const duration = performance.now() - start;
      assert.equal(result, expectedCount);
      return duration;
    }, DEFAULT_OPTIONS);
    boundary.resumeErrors[size.name] = resumeErrors[1];
  }

  return boundary;
}

async function benchmarkAddon(fixtures, mode = 'full') {
  console.log('Running addon benchmarks...');
  const latency = {};
  const suspendState = {};
  const {
    smallSource,
    codeModeSource,
    workflowSource,
    workflowData,
    ptcScenarios,
    durablePtcScenarios,
    galleryScenarios,
    headlineSeedScenarios,
    sentinelScenarios,
  } = fixtures;
  const ptc = {
    transfer: {},
    breakdown: {},
    phase2: {
      gallery: {
        transfer: {},
      },
      headlineSeeds: {
        transfer: {},
      },
      canary: {},
      sentinel: {
        transfer: {},
        familyScore: {},
      },
      scorecards: {},
    },
  };
  const durablePtc = {
    resumeOnly: {},
    state: {},
  };
  const defaultContext = new ExecutionContext();
  const warmSmallRuntime = new Mustard(smallSource);
  const warmCodeModeRuntime = new Mustard(codeModeSource);
  const workflowRuntime = new Mustard(workflowSource);
  const workflowCapabilities = createWorkflowCapabilities(workflowData);
  const workflowContext = new ExecutionContext({
    capabilities: workflowCapabilities,
    snapshotKey: SNAPSHOT_KEY,
  });

  let phases = {};
  let boundary = {};
  let counters = {};
  let memory = {};
  let failureCleanup = {};

  if (isFullMode(mode)) {
    const native = loadNative();
    phases = await benchmarkAddonPhases();
    boundary = await benchmarkAddonBoundary();

    Object.assign(latency, Object.fromEntries([
      await measure('cold_start_small', async () => {
        const result = await new Mustard(smallSource).run({ context: defaultContext });
        assert.equal(typeof result, 'number');
      }, { warmup: 1, iterations: 3 }),
      await measure('warm_run_small', async () => {
        const result = await warmSmallRuntime.run({ context: defaultContext });
        assert.equal(typeof result, 'number');
      }),
      await measure('cold_start_code_mode_search', async () => {
        const result = await new Mustard(codeModeSource).run({ context: defaultContext });
        assert.equal(result.count > 0, true);
      }, { warmup: 1, iterations: 2 }),
      await measure('warm_run_code_mode_search', async () => {
        const result = await warmCodeModeRuntime.run({ context: defaultContext });
        assert.equal(result.count > 0, true);
      }),
      await measure('programmatic_tool_workflow', async () => {
        const result = await workflowRuntime.run({ context: workflowContext });
        assertWorkflowResult(result);
      }, DEFAULT_OPTIONS),
    ]));

    for (const scenario of Object.values(ptcScenarios)) {
      const runtime = new Mustard(scenario.source);
      const metric = await measure(scenario.metricName, async () => {
        const result = await runtime.run({
          context: new ExecutionContext({
            capabilities: scenario.createCapabilities(),
            snapshotKey: SNAPSHOT_KEY,
          }),
          inputs: scenario.inputs,
        });
        scenario.assertResult(result);
      }, DEFAULT_OPTIONS);
      latency[metric[0]] = metric[1];
      ptc.transfer[scenario.metricName] = await capturePtcTransfer(
        (capabilities) => runtime.run({
          context: new ExecutionContext({
            capabilities,
            snapshotKey: SNAPSHOT_KEY,
          }),
          inputs: scenario.inputs,
        }),
        scenario,
      );
      if (ADDON_PTC_BREAKDOWN_METRICS.has(scenario.metricName)) {
        ptc.breakdown[scenario.metricName] = await captureAddonPtcBoundaryBreakdown(
          runtime,
          scenario,
        );
      }
    }
    ptc.weightedScore = {
      medium: summarizePtcWeightedScore(latency),
    };

    for (const callCount of [1, 10, 50, 100]) {
      const runtime = new Mustard(createFanoutSource(callCount));
      const context = new ExecutionContext({
        capabilities: {
          fetch_value(value) {
            return value;
          },
        },
        snapshotKey: SNAPSHOT_KEY,
      });
      const metric = await measure(`host_fanout_${callCount}`, async () => {
        const result = await runtime.run({ context });
        assert.equal(result, expectedFanoutTotal(callCount));
      }, DEFAULT_OPTIONS);
      latency[metric[0]] = metric[1];
    }

    for (const boundaryCount of [1, 5, 20]) {
      const source = createSuspendResumeSource(boundaryCount);
      const runtime = new Mustard(source);
      const context = new ExecutionContext({
        capabilities: { checkpoint() {} },
        limits: {},
        snapshotKey: SNAPSHOT_KEY,
      });
      const metric = await measure(`suspend_resume_${boundaryCount}`, async () => {
        let step = runtime.start({ context });
        let expected = 0;
        while (step instanceof Progress) {
          expected += step.args[0];
          step = Progress.load(step.dump(), { context }).resume(step.args[0]);
        }
        assert.equal(step, expected);
      }, DEFAULT_OPTIONS);
      latency[metric[0]] = metric[1];

      const dumpedProgram = runtime.dump();
      const dumpedSnapshot = takeProgress(runtime.start({ context }), 'suspend state').dump();
      const retainedLiveMemory = await measureRetainedLiveMemory(() => {
        const retainedProgress = [];
        for (let index = 0; index < MEMORY_RUNS; index += 1) {
          const retainedRuntime = new Mustard(source);
          retainedProgress.push(
            takeProgress(retainedRuntime.start({ context }), 'retained suspend state'),
          );
        }
        return retainedProgress;
      });
      suspendState[metric[0]] = {
        serializedProgramBytes: dumpedProgram.length,
        snapshotBytes: dumpedSnapshot.snapshot.length,
        retainedLiveProgressCount: MEMORY_RUNS,
        retainedLiveHeapBytes: retainedLiveMemory.heapUsedDeltaBytes,
        retainedLiveRssBytes: retainedLiveMemory.rssDeltaBytes,
      };
    }

    memory = await measureRetainedMemory(async () => {
      for (let i = 0; i < MEMORY_RUNS; i += 1) {
        const result = await workflowRuntime.run({ context: workflowContext });
        assertWorkflowResult(result);
      }
    });

    failureCleanup = {
      limitFailure: await measureFailureCleanup('limit_failure', async () => {
        await assert.rejects(
          new Mustard(fixtures.failureSource).run({
            context: new ExecutionContext({
              limits: {
                heapLimitBytes: 512,
              },
            }),
          }),
        );
        const recovered = await warmSmallRuntime.run({ context: defaultContext });
        assert.equal(typeof recovered, 'number');
      }, DEFAULT_OPTIONS),
      hostFailure: await measureFailureCleanup('host_failure', async () => {
        await assert.rejects(
          new Mustard(fixtures.hostFailureSource).run({
            context: new ExecutionContext({
              capabilities: {
                fetch_value(value) {
                  return value;
                },
                explode() {
                  throw new Error('explode');
                },
              },
            }),
          }),
        );
        const recovered = await warmSmallRuntime.run({ context: defaultContext });
        assert.equal(typeof recovered, 'number');
      }, DEFAULT_OPTIONS),
    };

    const executionCounterRuntime = new Mustard(createExecutionOnlySource());
    const phaseCounterOptions = createPhaseLoadOptions();
    const snapshotCounterProgress = takeProgress(
      executionCounterRuntime.start(phaseCounterOptions),
      'snapshot counter progress',
    );
    const dumpedCounterProgress = snapshotCounterProgress.dump();
    const snapshotCounterInspection = parseNativeInspectionWithMetrics(
      callNative(
        native.inspectDetachedSnapshot,
        executionCounterRuntime._ensureProgramHandle(),
        dumpedCounterProgress.snapshot,
        createAuthenticatedPolicyJson(dumpedCounterProgress, phaseCounterOptions),
      ),
    );

    counters = {
      warm_run_small: await collectAddonStepCounters(warmSmallRuntime, { context: defaultContext }),
      programmatic_tool_workflow: await collectAddonStepCounters(workflowRuntime, {
        context: workflowContext,
      }),
      host_fanout_100: await collectAddonStepCounters(new Mustard(createFanoutSource(100)), {
        context: new ExecutionContext({
          capabilities: {
            fetch_value(value) {
              return value;
            },
          },
          snapshotKey: SNAPSHOT_KEY,
        }),
      }),
      execution_only_small: await collectAddonStepCounters(
        executionCounterRuntime,
        phaseCounterOptions,
        () => 1,
      ),
      suspend_resume_20: await collectAddonStepCounters(
        new Mustard(createSuspendResumeSource(20)),
        {
          context: new ExecutionContext({
            capabilities: { checkpoint() {} },
            limits: {},
            snapshotKey: SNAPSHOT_KEY,
          }),
        },
        (step) => step.args[0],
      ),
      snapshot_load_only: snapshotCounterInspection.metrics,
    };

    for (const metricName of [
      'ptc_incident_triage_medium',
      'ptc_fraud_investigation_medium',
      'ptc_vendor_review_medium',
    ]) {
      const scenario = ptcScenarios[metricName];
      counters[metricName] = await collectAddonStepCounters(
        new Mustard(scenario.source),
        {
          inputs: scenario.inputs,
          context: new ExecutionContext({
            capabilities: scenario.createCapabilities(),
            limits: {},
          }),
        },
        createPtcCounterResumeResolver(scenario),
      );
    }
  }

  const syntheticMetricNames = syntheticPtcMetricNamesForMode(mode);
  if (Array.isArray(syntheticMetricNames)) {
    const measureOptions = ptcMeasureOptionsForMode(mode);
    for (const metricName of syntheticMetricNames) {
      const scenario = ptcScenarios[metricName];
      const runtime = new Mustard(scenario.source);
      const metric = await measure(scenario.metricName, async () => {
        const result = await runtime.run({
          context: new ExecutionContext({
            capabilities: scenario.createCapabilities(),
            snapshotKey: SNAPSHOT_KEY,
          }),
          inputs: scenario.inputs,
        });
        scenario.assertResult(result);
      }, measureOptions);
      latency[metric[0]] = metric[1];
      ptc.transfer[scenario.metricName] = await capturePtcTransfer(
        (capabilities) => runtime.run({
          context: new ExecutionContext({
            capabilities,
            snapshotKey: SNAPSHOT_KEY,
          }),
          inputs: scenario.inputs,
        }),
        scenario,
      );
    }
  }

  const galleryMetricNames = galleryMetricNamesForMode(mode);
  if (galleryMetricNames.length > 0) {
    const measureOptions = galleryMeasureOptionsForMode(mode);
    for (const metricName of galleryMetricNames) {
      const scenario = galleryScenarios[metricName];
      const runtime = new Mustard(scenario.source);
      const metric = await measure(scenario.metricName, async () => {
        const result = await runtime.run({
          context: new ExecutionContext({
            capabilities: scenario.createCapabilities(),
            snapshotKey: SNAPSHOT_KEY,
          }),
          inputs: scenario.inputs,
        });
        scenario.assertResult(result);
      }, measureOptions);
      latency[metric[0]] = metric[1];
      ptc.phase2.gallery.transfer[scenario.metricName] = await capturePtcTransfer(
        (capabilities) => runtime.run({
          context: new ExecutionContext({
            capabilities,
            snapshotKey: SNAPSHOT_KEY,
          }),
          inputs: scenario.inputs,
        }),
        scenario,
      );
      if (ADDON_PTC_BREAKDOWN_METRICS.has(scenario.metricName)) {
        ptc.breakdown[scenario.metricName] = await captureAddonPtcBoundaryBreakdown(
          runtime,
          scenario,
        );
      }
    }

    ptc.phase2.canary = {
      laneCount: galleryMetricNames.length,
      categories: Object.fromEntries(
        ['analytics', 'operations', 'workflows'].map((category) => [
          category,
          galleryMetricNames.filter((metricName) => galleryScenarios[metricName].category === category)
            .length,
        ]),
      ),
    };
  }

  const phase2AttributionMetricNames = representativePhase2AttributionMetricNamesForMode(mode);
  if (phase2AttributionMetricNames.length > 0) {
    for (const metricName of phase2AttributionMetricNames) {
      const scenario = galleryScenarios[metricName];
      counters[metricName] = annotateCollectionCallSites(
        await collectAddonStepCounters(
          new Mustard(scenario.source),
          {
            inputs: scenario.inputs,
            context: new ExecutionContext({
              capabilities: scenario.createCapabilities(),
              limits: {},
            }),
          },
          createPtcCounterResumeResolver(scenario),
        ),
        scenario,
      );
    }
  }

  const headlineSeedMetricNames = headlineSeedMetricNamesForMode(mode);
  if (headlineSeedMetricNames.length > 0) {
    const measureOptions = headlineSeedMeasureOptionsForMode(mode);
    for (const metricName of headlineSeedMetricNames) {
      const scenario = headlineSeedScenarios[metricName];
      const runtime = new Mustard(scenario.source);
      const metric = await measure(scenario.metricName, async () => {
        const result = await runtime.run({
          context: new ExecutionContext({
            capabilities: scenario.createCapabilities(),
            snapshotKey: SNAPSHOT_KEY,
          }),
          inputs: scenario.inputs,
        });
        scenario.assertResult(result);
      }, measureOptions);
      latency[metric[0]] = metric[1];
      ptc.phase2.headlineSeeds.transfer[scenario.metricName] = await capturePtcTransfer(
        (capabilities) => runtime.run({
          context: new ExecutionContext({
            capabilities,
            snapshotKey: SNAPSHOT_KEY,
          }),
          inputs: scenario.inputs,
        }),
        scenario,
      );
    }

    ptc.phase2.headlineSeeds.laneCount = headlineSeedMetricNames.length;
    ptc.phase2.headlineSeeds.categories = Object.fromEntries(
      ['analytics', 'operations', 'workflows'].map((category) => [
        category,
        headlineSeedMetricNames.filter(
          (metricName) => headlineSeedScenarios[metricName].category === category,
        ).length,
      ]),
    );
    ptc.phase2.headlineSeeds.patterns = Object.fromEntries(
      headlineSeedMetricNames.map((metricName) => [
        metricName,
        [...headlineSeedScenarios[metricName].skewPatterns],
      ]),
    );
  }

  if (shouldBenchmarkDurablePtc(mode)) {
    const measureOptions = durablePtcMeasureOptionsForMode(mode);
    for (const scenario of Object.values(durablePtcScenarios)) {
      const runtime = new Mustard(scenario.source);
      const checkpoint = await createAddonDurableCheckpointState(runtime, scenario);
      durablePtc.state[scenario.metricName] = {
        snapshotBytes: checkpoint.snapshotBytes,
        detachedManifestBytes: checkpoint.detachedManifestBytes,
        checkpointArgsBytes: checkpoint.checkpointArgsBytes,
      };
      durablePtc.resumeOnly[scenario.metricName] = await captureAddonDurableResumeOnly(
        runtime,
        scenario,
        measureOptions,
      );
    }
  }

  if (shouldBenchmarkSentinels(mode)) {
    const measureOptions = sentinelMeasureOptionsForMode(mode);
    for (const familyScenarios of Object.values(sentinelScenarios)) {
      for (const scenario of Object.values(familyScenarios)) {
        const runtime = new Mustard(scenario.source);
        const metric = await measure(scenario.metricName, async () => {
          const result = await runtime.run({
            context: new ExecutionContext({
              capabilities: scenario.createCapabilities(),
              snapshotKey: SNAPSHOT_KEY,
            }),
            inputs: scenario.inputs,
          });
          scenario.assertResult(result);
        }, measureOptions);
        latency[metric[0]] = metric[1];
        ptc.phase2.sentinel.transfer[scenario.metricName] = await capturePtcTransfer(
          (capabilities) => runtime.run({
            context: new ExecutionContext({
              capabilities,
              snapshotKey: SNAPSHOT_KEY,
            }),
            inputs: scenario.inputs,
          }),
          scenario,
        );
      }
    }
    ptc.phase2.sentinel.familyScore = summarizeSentinelFamilyScores(latency, sentinelScenarios);
  }

  if (
    latency.ptc_incident_triage_medium &&
    latency.ptc_fraud_investigation_medium &&
    latency.ptc_vendor_review_medium
  ) {
    ptc.weightedScore = {
      medium: summarizePtcWeightedScore(latency),
    };
  }

  return {
    latency,
    phases,
    boundary,
    counters,
    ptc,
    durablePtc,
    suspendState,
    memory,
    failureCleanup,
  };
}

async function benchmarkSidecar(fixtures, profile, mode = 'full') {
  console.log('Running sidecar benchmarks...');
  const latency = {};
  const phases = {};
  const {
    smallSource,
    codeModeSource,
    workflowSource,
    workflowData,
    ptcScenarios,
    durablePtcScenarios,
    galleryScenarios,
    headlineSeedScenarios,
    sentinelScenarios,
  } = fixtures;
  const ptc = {
    transfer: {},
    breakdown: {},
    phase2: {
      gallery: {
        transfer: {},
      },
      headlineSeeds: {
        transfer: {},
      },
      canary: {},
      sentinel: {
        transfer: {},
        familyScore: {},
      },
      scorecards: {},
    },
  };
  const durablePtc = {
    resumeOnly: {},
    state: {},
  };
  let memory = {};
  let failureCleanup = {};

  if (isFullMode(mode)) {
    const startupOnly = await measure('startup_only', async () => {
      await withSidecar(profile, async () => {});
    }, COLD_OPTIONS);
    phases[startupOnly[0]] = startupOnly[1];
    ptc.breakdown.processStartup = startupOnly[1];

    Object.assign(latency, Object.fromEntries([
      await measure('cold_start_small', async () => {
        await withSidecar(profile, async ({ request }) => {
          const program = await compileSidecarSource(request, smallSource);
          const result = await runSidecarProgram(request, program);
          assert.equal(typeof result, 'number');
        });
      }, COLD_OPTIONS),
      await measure('cold_start_code_mode_search', async () => {
        await withSidecar(profile, async ({ request }) => {
          const program = await compileSidecarSource(request, codeModeSource);
          const result = await runSidecarProgram(request, program);
          assert.equal(result.count > 0, true);
        });
      }, COLD_OPTIONS),
    ]));

    const workflowCapabilities = createWorkflowCapabilities(workflowData);

    await withSidecar(profile, async ({ child, request }) => {
      const smallProgram = await compileSidecarSource(request, smallSource);
      const codeModeProgram = await compileSidecarSource(request, codeModeSource);
      const workflowProgram = await compileSidecarSource(request, workflowSource);
      const transportProgram = await compileSidecarSource(request, createImmediateSuspendSource());
      const ptcPrograms = new Map();
      const transportProbe = await startSidecarProgram(request, transportProgram, {
        capabilities: { checkpoint() {} },
      });
      assert.equal(transportProbe.step.type, 'suspended');
      assert.equal(transportProbe.step.capability, 'checkpoint');

      const warmSmall = await measure('warm_run_small', async () => {
        const result = await runSidecarProgram(request, smallProgram);
        assert.equal(typeof result, 'number');
      }, DEFAULT_OPTIONS);
      latency[warmSmall[0]] = warmSmall[1];
      phases.execution_only_small = warmSmall[1];

      const transportResume = await measure('transport_resume_only', async () => {
        const step = await resumeSidecarSnapshot(request, transportProbe.step, 1);
        assert.equal(step.type, 'completed');
        assert.equal(step.value, 1);
      }, DEFAULT_OPTIONS);
      phases[transportResume[0]] = transportResume[1];

      const warmCode = await measure('warm_run_code_mode_search', async () => {
        const result = await runSidecarProgram(request, codeModeProgram);
        assert.equal(result.count > 0, true);
      }, DEFAULT_OPTIONS);
      latency[warmCode[0]] = warmCode[1];

      const workflowMetric = await measure('programmatic_tool_workflow', async () => {
        const result = await runSidecarProgram(request, workflowProgram, {
          capabilities: workflowCapabilities,
        });
        assertWorkflowResult(result);
      }, DEFAULT_OPTIONS);
      latency[workflowMetric[0]] = workflowMetric[1];

      for (const scenario of Object.values(ptcScenarios)) {
        let program = ptcPrograms.get(scenario.laneId);
        if (!program) {
          program = await compileSidecarSource(request, scenario.source);
          ptcPrograms.set(scenario.laneId, program);
        }
        const metric = await measure(scenario.metricName, async () => {
          const result = await runSidecarProgram(request, program, {
            capabilities: scenario.createCapabilities(),
            inputs: scenario.inputs,
          });
          scenario.assertResult(result);
        }, DEFAULT_OPTIONS);
        latency[metric[0]] = metric[1];
        ptc.transfer[scenario.metricName] = await capturePtcTransfer(
          (capabilities) => runSidecarProgram(request, program, {
            capabilities,
            inputs: scenario.inputs,
          }),
          scenario,
        );
        if (ADDON_PTC_BREAKDOWN_METRICS.has(scenario.metricName)) {
          ptc.breakdown[scenario.metricName] = await captureSidecarPtcBreakdown(
            request,
            program,
            scenario,
          );
        }
      }

      for (const callCount of [1, 10, 50, 100]) {
        const program = await compileSidecarSource(request, createFanoutSource(callCount));
        const metric = await measure(`host_fanout_${callCount}`, async () => {
          const result = await runSidecarProgram(request, program, {
            capabilities: {
              fetch_value(value) {
                return value;
              },
            },
          });
          assert.equal(result, expectedFanoutTotal(callCount));
        }, COLD_OPTIONS);
        latency[metric[0]] = metric[1];
      }

      for (const boundaryCount of [1, 5, 20]) {
        const program = await compileSidecarSource(request, createSuspendResumeSource(boundaryCount));
        const metric = await measure(`suspend_resume_${boundaryCount}`, async () => {
          const result = await runSidecarProgram(request, program, {
            capabilities: {
              checkpoint(value) {
                return value;
              },
            },
          });
          assert.equal(result, expectedSuspendTotal(boundaryCount));
        }, COLD_OPTIONS);
        latency[metric[0]] = metric[1];
      }

      memory = await measureRetainedMemory(
        async () => {
          for (let i = 0; i < MEMORY_RUNS; i += 1) {
            const result = await runSidecarProgram(request, workflowProgram, {
              capabilities: workflowCapabilities,
            });
            assertWorkflowResult(result);
          }
        },
        {
          sample() {
            const parent = processMemorySnapshot();
            const childRssBytes = rssBytesForPid(child.pid);
            return {
              heapUsedBytes: parent.heapUsedBytes,
              rssBytes: parent.rssBytes + (childRssBytes ?? 0),
            };
          },
        },
      );

      failureCleanup = {
        limitFailure: await measureFailureCleanup('limit_failure', async () => {
          const failingProgram = await compileSidecarSource(request, fixtures.failureSource);
          const { payload: failure } = await request(
            sidecarStartRequestPayload(failingProgram, 4000, {
              inputs: {},
              capabilities: [],
              limits: {
                heap_limit_bytes: 512,
              },
            }),
          );
          assert.equal(failure.ok, false);
          const recovered = await runSidecarProgram(request, smallProgram);
          assert.equal(typeof recovered, 'number');
        }, COLD_OPTIONS),
        hostFailure: await measureFailureCleanup('host_failure', async () => {
          const failingProgram = await compileSidecarSource(request, fixtures.hostFailureSource);
          const { payload: start, blob: startBlob } = await request(
            sidecarStartRequestPayload(failingProgram, 5000, {
              inputs: {},
              capabilities: ['fetch_value', 'explode'],
              limits: {},
            }),
            typeof failingProgram.programId === 'string' && failingProgram.programId.length > 0
              ? undefined
              : failingProgram.program,
          );
          assert.equal(start.ok, true);
          let step = sidecarStepValue(start.result.step, start.result, startBlob);
          assert.equal(step.capability, 'fetch_value');
          let { payload: response, blob: responseBlob } = await request({
            protocol_version: SIDECAR_PROTOCOL_VERSION,
            method: 'resume',
            id: 5001,
            snapshot_id: step.snapshotId,
            policy_id: step.policyId,
            auth: {
              snapshot_key_base64: SNAPSHOT_KEY_BASE64,
              snapshot_key_digest: snapshotKeyDigest(Buffer.from(SNAPSHOT_KEY, 'utf8')),
              snapshot_token: snapshotToken(step.snapshot, SNAPSHOT_KEY),
            },
            payload: JSON.parse(encodeResumePayloadValue(1)),
          });
          assert.equal(response.ok, true);
          step = sidecarStepValue(response.result.step, response.result, responseBlob);
          assert.equal(step.capability, 'explode');
          ({ payload: response, blob: responseBlob } = await request({
            protocol_version: SIDECAR_PROTOCOL_VERSION,
            method: 'resume',
            id: 5002,
            snapshot_id: step.snapshotId,
            policy_id: step.policyId,
            auth: {
              snapshot_key_base64: SNAPSHOT_KEY_BASE64,
              snapshot_key_digest: snapshotKeyDigest(Buffer.from(SNAPSHOT_KEY, 'utf8')),
              snapshot_token: snapshotToken(step.snapshot, SNAPSHOT_KEY),
            },
            payload: JSON.parse(encodeResumePayloadValue({ __host_error__: true })),
          }));
          assert.equal(response.ok, false);
          const recovered = await runSidecarProgram(request, smallProgram);
          assert.equal(typeof recovered, 'number');
        }, COLD_OPTIONS),
      };
    });

  }

  const syntheticMetricNames = syntheticPtcMetricNamesForMode(mode);
  if (Array.isArray(syntheticMetricNames) && syntheticMetricNames.length > 0) {
    const measureOptions = ptcMeasureOptionsForMode(mode);
    await withSidecar(profile, async ({ request }) => {
      const programs = new Map();
      for (const metricName of syntheticMetricNames) {
        const scenario = ptcScenarios[metricName];
        let program = programs.get(scenario.laneId);
        if (!program) {
          program = await compileSidecarSource(request, scenario.source);
          programs.set(scenario.laneId, program);
        }
        const metric = await measure(scenario.metricName, async () => {
          const result = await runSidecarProgram(request, program, {
            capabilities: scenario.createCapabilities(),
            inputs: scenario.inputs,
          });
          scenario.assertResult(result);
        }, measureOptions);
        latency[metric[0]] = metric[1];
        ptc.transfer[scenario.metricName] = await capturePtcTransfer(
          (capabilities) => runSidecarProgram(request, program, {
            capabilities,
            inputs: scenario.inputs,
          }),
          scenario,
        );
      }
    });
  }

  const galleryMetricNames = galleryMetricNamesForMode(mode);
  if (galleryMetricNames.length > 0) {
    const measureOptions = galleryMeasureOptionsForMode(mode);
    await withSidecar(profile, async ({ request }) => {
      const programs = new Map();
      for (const metricName of galleryMetricNames) {
        const scenario = galleryScenarios[metricName];
        let program = programs.get(scenario.laneId);
        if (!program) {
          program = await compileSidecarSource(request, scenario.source);
          programs.set(scenario.laneId, program);
        }
        const metric = await measure(scenario.metricName, async () => {
          const result = await runSidecarProgram(request, program, {
            capabilities: scenario.createCapabilities(),
            inputs: scenario.inputs,
          });
          scenario.assertResult(result);
        }, measureOptions);
        latency[metric[0]] = metric[1];
        ptc.phase2.gallery.transfer[scenario.metricName] = await capturePtcTransfer(
          (capabilities) => runSidecarProgram(request, program, {
            capabilities,
            inputs: scenario.inputs,
          }),
          scenario,
        );
        if (ADDON_PTC_BREAKDOWN_METRICS.has(scenario.metricName)) {
          ptc.breakdown[scenario.metricName] = await captureSidecarPtcBreakdown(
            request,
            program,
            scenario,
          );
        }
      }
    });

    ptc.phase2.canary = {
      laneCount: galleryMetricNames.length,
      categories: Object.fromEntries(
        ['analytics', 'operations', 'workflows'].map((category) => [
          category,
          galleryMetricNames.filter((metricName) => galleryScenarios[metricName].category === category)
            .length,
        ]),
      ),
    };
  }

  const headlineSeedMetricNames = headlineSeedMetricNamesForMode(mode);
  if (headlineSeedMetricNames.length > 0) {
    const measureOptions = headlineSeedMeasureOptionsForMode(mode);
    await withSidecar(profile, async ({ request }) => {
      const programs = new Map();
      for (const metricName of headlineSeedMetricNames) {
        const scenario = headlineSeedScenarios[metricName];
        let program = programs.get(scenario.metricName);
        if (!program) {
          program = await compileSidecarSource(request, scenario.source);
          programs.set(scenario.metricName, program);
        }
        const metric = await measure(scenario.metricName, async () => {
          const result = await runSidecarProgram(request, program, {
            capabilities: scenario.createCapabilities(),
            inputs: scenario.inputs,
          });
          scenario.assertResult(result);
        }, measureOptions);
        latency[metric[0]] = metric[1];
        ptc.phase2.headlineSeeds.transfer[scenario.metricName] = await capturePtcTransfer(
          (capabilities) => runSidecarProgram(request, program, {
            capabilities,
            inputs: scenario.inputs,
          }),
          scenario,
        );
      }
    });

    ptc.phase2.headlineSeeds.laneCount = headlineSeedMetricNames.length;
    ptc.phase2.headlineSeeds.categories = Object.fromEntries(
      ['analytics', 'operations', 'workflows'].map((category) => [
        category,
        headlineSeedMetricNames.filter(
          (metricName) => headlineSeedScenarios[metricName].category === category,
        ).length,
      ]),
    );
    ptc.phase2.headlineSeeds.patterns = Object.fromEntries(
      headlineSeedMetricNames.map((metricName) => [
        metricName,
        [...headlineSeedScenarios[metricName].skewPatterns],
      ]),
    );
  }

  if (shouldBenchmarkDurablePtc(mode)) {
    const measureOptions = durablePtcMeasureOptionsForMode(mode);
    for (const scenario of Object.values(durablePtcScenarios)) {
      const metrics = await captureSidecarDurableResumeOnly(profile, scenario, measureOptions);
      durablePtc.resumeOnly[scenario.metricName] = metrics.resumeOnly;
      durablePtc.state[scenario.metricName] = metrics.state;
    }
  }

  if (shouldBenchmarkSentinels(mode)) {
    const measureOptions = sentinelMeasureOptionsForMode(mode);
    await withSidecar(profile, async ({ request }) => {
      const programs = new Map();
      for (const familyScenarios of Object.values(sentinelScenarios)) {
        for (const scenario of Object.values(familyScenarios)) {
          let program = programs.get(scenario.metricName);
          if (!program) {
            program = await compileSidecarSource(request, scenario.source);
            programs.set(scenario.metricName, program);
          }
          const metric = await measure(scenario.metricName, async () => {
            const result = await runSidecarProgram(request, program, {
              capabilities: scenario.createCapabilities(),
              inputs: scenario.inputs,
            });
            scenario.assertResult(result);
          }, measureOptions);
          latency[metric[0]] = metric[1];
          ptc.phase2.sentinel.transfer[scenario.metricName] = await capturePtcTransfer(
            (capabilities) => runSidecarProgram(request, program, {
              capabilities,
              inputs: scenario.inputs,
            }),
            scenario,
          );
        }
      }
    });
    ptc.phase2.sentinel.familyScore = summarizeSentinelFamilyScores(latency, sentinelScenarios);
  }

  if (
    latency.ptc_incident_triage_medium &&
    latency.ptc_fraud_investigation_medium &&
    latency.ptc_vendor_review_medium
  ) {
    ptc.weightedScore = {
      medium: summarizePtcWeightedScore(latency),
    };
  }

  return { latency, phases, ptc, durablePtc, memory, failureCleanup };
}

async function benchmarkIsolate(fixtures, mode = 'full') {
  console.log('Running V8 isolate benchmarks...');
  const latency = {};
  const {
    smallSource,
    codeModeSource,
    workflowSource,
    workflowData,
    ptcScenarios,
    durablePtcScenarios,
    galleryScenarios,
    headlineSeedScenarios,
    sentinelScenarios,
  } = fixtures;
  const ptc = {
    transfer: {},
    phase2: {
      gallery: {
        transfer: {},
      },
      headlineSeeds: {
        transfer: {},
      },
      canary: {},
      sentinel: {
        transfer: {},
        familyScore: {},
      },
      scorecards: {},
    },
  };
  const durablePtc = {
    resumeOnly: {},
    state: {},
  };
  const workflowCapabilities = createWorkflowCapabilities(workflowData);
  let memory = {};
  let failureCleanup = {};

  if (isFullMode(mode)) {
    Object.assign(latency, Object.fromEntries([
      await measure('cold_start_small', async () => {
        const { isolate, context } = createIsolateContext();
        const script = isolate.compileScriptSync(smallSource);
        const result = await runIsolateScript(context, script);
        assert.equal(typeof result, 'number');
      }, DEFAULT_OPTIONS),
      await measure('cold_start_code_mode_search', async () => {
        const { isolate, context } = createIsolateContext();
        const script = isolate.compileScriptSync(codeModeSource);
        const result = await runIsolateScript(context, script);
        assert.equal(result.count > 0, true);
      }, DEFAULT_OPTIONS),
    ]));

    {
      const isolate = new ivm.Isolate({ memoryLimit: 128 });
      const smallScript = isolate.compileScriptSync(smallSource);
      const codeModeScript = isolate.compileScriptSync(codeModeSource);
      const workflowScript = isolate.compileScriptSync(workflowSource);

      const warmSmall = await measure('warm_run_small', async () => {
        const context = isolate.createContextSync();
        installIsolateCapabilities(context);
        const result = await runIsolateScript(context, smallScript);
        assert.equal(typeof result, 'number');
      }, DEFAULT_OPTIONS);
      latency[warmSmall[0]] = warmSmall[1];

      const warmCode = await measure('warm_run_code_mode_search', async () => {
        const context = isolate.createContextSync();
        installIsolateCapabilities(context);
        const result = await runIsolateScript(context, codeModeScript);
        assert.equal(result.count > 0, true);
      }, DEFAULT_OPTIONS);
      latency[warmCode[0]] = warmCode[1];

      const workflowMetric = await measure('programmatic_tool_workflow', async () => {
        const context = isolate.createContextSync();
        installIsolateCapabilities(context, workflowCapabilities);
        const result = await runIsolateScript(context, workflowScript);
        assertWorkflowResult(result);
      }, DEFAULT_OPTIONS);
      latency[workflowMetric[0]] = workflowMetric[1];

      for (const scenario of Object.values(ptcScenarios)) {
        const script = isolate.compileScriptSync(scenario.source);
        const metric = await measure(scenario.metricName, async () => {
          const context = isolate.createContextSync();
          installIsolateCapabilities(context, scenario.createCapabilities(), scenario.inputs);
          const result = await runIsolateScript(context, script);
          scenario.assertResult(result);
        }, DEFAULT_OPTIONS);
        latency[metric[0]] = metric[1];
        ptc.transfer[scenario.metricName] = await capturePtcTransfer((capabilities) => {
          const context = isolate.createContextSync();
          installIsolateCapabilities(context, capabilities, scenario.inputs);
          return runIsolateScript(context, script);
        }, scenario);
      }
    }

    for (const callCount of [1, 10, 50, 100]) {
      const source = createFanoutSource(callCount);
      const isolate = new ivm.Isolate({ memoryLimit: 128 });
      const script = isolate.compileScriptSync(source);
      const metric = await measure(`host_fanout_${callCount}`, async () => {
        const context = isolate.createContextSync();
        installIsolateCapabilities(context, {
          fetch_value(value) {
            return value;
          },
        });
        const result = await runIsolateScript(context, script);
        assert.equal(result, expectedFanoutTotal(callCount));
      }, DEFAULT_OPTIONS);
      latency[metric[0]] = metric[1];
    }

    const resumeClosure = createIsolateResumeClosure();
    for (const boundaryCount of [1, 5, 20]) {
      const metric = await measure(`suspend_resume_${boundaryCount}`, async () => {
        let state = {
          boundaryCount,
          nextIndex: 0,
          total: 0,
        };
        while (!state.done) {
          const { context } = createIsolateContext({
            checkpoint(value) {
              return value;
            },
          });
          state = context.evalClosureSync(resumeClosure, [state], {
            arguments: { copy: true },
            result: { copy: true },
          });
        }
        assert.equal(state.total, expectedSuspendTotal(boundaryCount));
      }, DEFAULT_OPTIONS);
      latency[metric[0]] = metric[1];
    }

    memory = await measureRetainedMemory(async () => {
      const isolate = new ivm.Isolate({ memoryLimit: 128 });
      const script = isolate.compileScriptSync(workflowSource);
      for (let i = 0; i < MEMORY_RUNS; i += 1) {
        const context = isolate.createContextSync();
        installIsolateCapabilities(context, workflowCapabilities);
        const result = await runIsolateScript(context, script);
        assertWorkflowResult(result);
      }
    });

    failureCleanup = {
      limitFailure: await measureFailureCleanup('limit_failure', async () => {
        const isolate = new ivm.Isolate({ memoryLimit: 128 });
        const script = isolate.compileScriptSync(fixtures.isolateLimitFailureSource);
        const context = isolate.createContextSync();
        installIsolateCapabilities(context);
        assert.throws(() => {
          script.runSync(context, { timeout: 1, copy: true });
        });
        const recoveryIsolate = new ivm.Isolate({ memoryLimit: 128 });
        const recoveryContext = recoveryIsolate.createContextSync();
        installIsolateCapabilities(recoveryContext);
        const recoveryScript = recoveryIsolate.compileScriptSync(fixtures.smallSource);
        const recovered = await runIsolateScript(recoveryContext, recoveryScript);
        assert.equal(typeof recovered, 'number');
      }, DEFAULT_OPTIONS),
      hostFailure: await measureFailureCleanup('host_failure', async () => {
        const isolate = new ivm.Isolate({ memoryLimit: 128 });
        const script = isolate.compileScriptSync(fixtures.hostFailureSource);
        const context = isolate.createContextSync();
        installIsolateCapabilities(context, {
          fetch_value(value) {
            return value;
          },
          explode() {
            throw new Error('explode');
          },
        });
        await assert.rejects(runIsolateScript(context, script));
        const recoveryContext = isolate.createContextSync();
        installIsolateCapabilities(recoveryContext);
        const recoveryScript = isolate.compileScriptSync(fixtures.smallSource);
        const recovered = await runIsolateScript(recoveryContext, recoveryScript);
        assert.equal(typeof recovered, 'number');
      }, DEFAULT_OPTIONS),
    };

  }

  const syntheticMetricNames = syntheticPtcMetricNamesForMode(mode);
  if (Array.isArray(syntheticMetricNames) && syntheticMetricNames.length > 0) {
    const measureOptions = ptcMeasureOptionsForMode(mode);
    const isolate = new ivm.Isolate({ memoryLimit: 128 });
    for (const metricName of syntheticMetricNames) {
      const scenario = ptcScenarios[metricName];
      const script = isolate.compileScriptSync(scenario.source);
      const metric = await measure(scenario.metricName, async () => {
        const context = isolate.createContextSync();
        installIsolateCapabilities(context, scenario.createCapabilities(), scenario.inputs);
        const result = await runIsolateScript(context, script);
        scenario.assertResult(result);
      }, measureOptions);
      latency[metric[0]] = metric[1];
      ptc.transfer[scenario.metricName] = await capturePtcTransfer((capabilities) => {
        const context = isolate.createContextSync();
        installIsolateCapabilities(context, capabilities, scenario.inputs);
        return runIsolateScript(context, script);
      }, scenario);
    }
  }

  const galleryMetricNames = galleryMetricNamesForMode(mode);
  if (galleryMetricNames.length > 0) {
    const isolate = new ivm.Isolate({ memoryLimit: 128 });
    const measureOptions = galleryMeasureOptionsForMode(mode);
    for (const metricName of galleryMetricNames) {
      const scenario = galleryScenarios[metricName];
      const resumePlan = await captureScenarioResumePlan(scenario);
      const script = isolate.compileScriptSync(scenario.source);
      const metric = await measure(scenario.metricName, async () => {
        const context = isolate.createContextSync();
        installIsolateCapabilities(
          context,
          createQueuedCapabilities(resumePlan, scenario.metricName),
          scenario.inputs,
        );
        const result = await runIsolateScript(context, script);
        scenario.assertResult(result);
      }, measureOptions);
      latency[metric[0]] = metric[1];
      ptc.phase2.gallery.transfer[scenario.metricName] = await capturePtcTransfer(
        (capabilities) => {
          const context = isolate.createContextSync();
          installIsolateCapabilities(context, capabilities, scenario.inputs);
          return runIsolateScript(context, script);
        },
        {
          ...scenario,
          createCapabilities() {
            return createQueuedCapabilities(resumePlan, scenario.metricName);
          },
        },
      );
    }

    ptc.phase2.canary = {
      laneCount: galleryMetricNames.length,
      categories: Object.fromEntries(
        ['analytics', 'operations', 'workflows'].map((category) => [
          category,
          galleryMetricNames.filter((metricName) => galleryScenarios[metricName].category === category)
            .length,
        ]),
      ),
    };
  }

  const headlineSeedMetricNames = headlineSeedMetricNamesForMode(mode);
  if (headlineSeedMetricNames.length > 0) {
    const isolate = new ivm.Isolate({ memoryLimit: 128 });
    const measureOptions = headlineSeedMeasureOptionsForMode(mode);
    for (const metricName of headlineSeedMetricNames) {
      const scenario = headlineSeedScenarios[metricName];
      const script = isolate.compileScriptSync(scenario.source);
      const metric = await measure(scenario.metricName, async () => {
        const context = isolate.createContextSync();
        installIsolateCapabilities(context, scenario.createCapabilities(), scenario.inputs);
        const result = await runIsolateScript(context, script);
        scenario.assertResult(result);
      }, measureOptions);
      latency[metric[0]] = metric[1];
      ptc.phase2.headlineSeeds.transfer[scenario.metricName] = await capturePtcTransfer(
        (capabilities) => {
          const context = isolate.createContextSync();
          installIsolateCapabilities(context, capabilities, scenario.inputs);
          return runIsolateScript(context, script);
        },
        scenario,
      );
    }

    ptc.phase2.headlineSeeds.laneCount = headlineSeedMetricNames.length;
    ptc.phase2.headlineSeeds.categories = Object.fromEntries(
      ['analytics', 'operations', 'workflows'].map((category) => [
        category,
        headlineSeedMetricNames.filter(
          (metricName) => headlineSeedScenarios[metricName].category === category,
        ).length,
      ]),
    );
    ptc.phase2.headlineSeeds.patterns = Object.fromEntries(
      headlineSeedMetricNames.map((metricName) => [
        metricName,
        [...headlineSeedScenarios[metricName].skewPatterns],
      ]),
    );
  }

  if (shouldBenchmarkDurablePtc(mode)) {
    const measureOptions = durablePtcMeasureOptionsForMode(mode);
    for (const scenario of Object.values(durablePtcScenarios)) {
      const metrics = await captureIsolateDurableResumeOnly(scenario, measureOptions);
      durablePtc.resumeOnly[scenario.metricName] = metrics.resumeOnly;
      durablePtc.state[scenario.metricName] = metrics.state;
    }
  }

  if (shouldBenchmarkSentinels(mode)) {
    const measureOptions = sentinelMeasureOptionsForMode(mode);
    const isolate = new ivm.Isolate({ memoryLimit: 128 });
    for (const familyScenarios of Object.values(sentinelScenarios)) {
      for (const scenario of Object.values(familyScenarios)) {
        const script = isolate.compileScriptSync(scenario.source);
        const metric = await measure(scenario.metricName, async () => {
          const context = isolate.createContextSync();
          installIsolateCapabilities(context, scenario.createCapabilities(), scenario.inputs);
          const result = await runIsolateScript(context, script);
          scenario.assertResult(result);
        }, measureOptions);
        latency[metric[0]] = metric[1];
        ptc.phase2.sentinel.transfer[scenario.metricName] = await capturePtcTransfer((capabilities) => {
          const context = isolate.createContextSync();
          installIsolateCapabilities(context, capabilities, scenario.inputs);
          return runIsolateScript(context, script);
        }, scenario);
      }
    }
    ptc.phase2.sentinel.familyScore = summarizeSentinelFamilyScores(latency, sentinelScenarios);
  }

  if (
    latency.ptc_incident_triage_medium &&
    latency.ptc_fraud_investigation_medium &&
    latency.ptc_vendor_review_medium
  ) {
    ptc.weightedScore = {
      medium: summarizePtcWeightedScore(latency),
    };
  }

  return { latency, ptc, durablePtc, memory, failureCleanup };
}

function ratioTable(left, right) {
  const ratios = {};
  for (const [name, leftMetric] of Object.entries(left)) {
    const rightMetric = right[name];
    if (!rightMetric) {
      continue;
    }
    ratios[name] = {
      medianRatio: rightMetric.medianMs / leftMetric.medianMs,
      p95Ratio: rightMetric.p95Ms / leftMetric.p95Ms,
    };
  }
  return ratios;
}

function failureRatioTable(left, right) {
  const ratios = {};
  for (const [name, leftMetric] of Object.entries(left)) {
    const rightMetric = right[name];
    ratios[name] = {
      medianRatio: rightMetric.medianMs / leftMetric.medianMs,
      p95Ratio: rightMetric.p95Ms / leftMetric.p95Ms,
    };
  }
  return ratios;
}

function writeWebsitePtcExport(results) {
  const ptcMetric = results.addon.latency.ptc_website_demo_small;
  if (!ptcMetric) {
    return null;
  }
  const exportSource = `'use strict';\n\n` +
    `export const benchmarkData = ${JSON.stringify({
      sourceArtifact: path.basename(results.reportPath),
      benchmarkKind: 'ptc_website_demo_small',
      machine: {
        cpuModel: results.machine.cpuModel,
        nodeVersion: results.machine.nodeVersion,
        platform: results.machine.platform,
      },
      addon: {
        medianMs: Number(ptcMetric.medianMs.toFixed(3)),
        p95Ms: Number(ptcMetric.p95Ms.toFixed(3)),
      },
      note: 'Representative 4-tool orchestration workflow derived from the audited programmatic tool-call gallery.',
    }, null, 2)} as const;\n`;
  fs.mkdirSync(path.dirname(WEBSITE_PTC_EXPORT_PATH), { recursive: true });
  fs.writeFileSync(WEBSITE_PTC_EXPORT_PATH, exportSource);
  return WEBSITE_PTC_EXPORT_PATH;
}

function benchmarkKindForMode(mode) {
  return mode === 'full' ? 'workloads' : mode;
}

function hasTimeMetric(metric) {
  return metric && typeof metric.medianMs === 'number' && typeof metric.p95Ms === 'number';
}

function hasRatioMetric(metric) {
  return metric && typeof metric.medianRatio === 'number' && typeof metric.p95Ratio === 'number';
}

function printSummary(results) {
  console.log(`Machine: ${results.machine.cpuModel} (${results.machine.cpuCount} cores)`);
  console.log(
    `Node: ${results.machine.nodeVersion} on ${results.machine.platform} [${results.machine.buildProfile}]`,
  );
  console.log(`Benchmark kind: ${results.machine.benchmarkKind}`);
  if (results.machine.gitSha) {
    console.log(`Git SHA: ${results.machine.gitSha}`);
  }
  console.log('');
  for (const name of Object.keys(results.addon.latency).filter(
    (metricName) => results.sidecar.latency[metricName] && results.isolate.latency[metricName],
  )) {
    const addon = results.addon.latency[name];
    const sidecar = results.sidecar.latency[name];
    const isolate = results.isolate.latency[name];
    console.log(
      `${name}: addon ${addon.medianMs.toFixed(2)}ms, sidecar ${sidecar.medianMs.toFixed(2)}ms, isolate ${isolate.medianMs.toFixed(2)}ms`,
    );
  }

  if (Object.keys(results.addon.phases).length > 0) {
    console.log('');
    console.log('Addon phase splits:');
    for (const [name, metric] of Object.entries(results.addon.phases)) {
      console.log(`${name}: ${metric.medianMs.toFixed(2)}ms median, ${metric.p95Ms.toFixed(2)}ms p95`);
    }
  }

  if (Object.keys(results.sidecar.phases).length > 0) {
    console.log('');
    console.log('Sidecar phase splits:');
    for (const [name, metric] of Object.entries(results.sidecar.phases)) {
      console.log(`${name}: ${metric.medianMs.toFixed(2)}ms median, ${metric.p95Ms.toFixed(2)}ms p95`);
    }
  }

  if (Object.keys(results.addon.boundary).length > 0) {
    console.log('');
    console.log('Addon boundary-only metrics:');
    for (const [surface, sizes] of Object.entries(results.addon.boundary)) {
      for (const [size, metric] of Object.entries(sizes)) {
        console.log(
          `${surface}.${size}: ${metric.medianMs.toFixed(2)}ms median, ${metric.p95Ms.toFixed(2)}ms p95`,
        );
      }
    }
  }

  if (Object.keys(results.addon.suspendState).length > 0) {
    console.log('');
    console.log('Addon suspend-state sizes:');
    for (const [name, metric] of Object.entries(results.addon.suspendState)) {
      console.log(
        `${name}: program ${metric.serializedProgramBytes}B snapshot ${metric.snapshotBytes}B liveHeap ${metric.retainedLiveHeapBytes}B liveRss ${metric.retainedLiveRssBytes}B`,
      );
    }
  }

  if (Object.keys(results.addon.counters).length > 0) {
    console.log('');
    console.log('Addon runtime counters:');
    for (const [name, metric] of Object.entries(results.addon.counters)) {
      const microtaskSummary = [
        `microtasks ${metric.executed_microtasks}/${metric.queued_microtasks} executed/queued`,
        `peak queued ${metric.peak_microtask_queue_len}`,
        `resume ${metric.executed_resume_async_microtasks}/${metric.queued_resume_async_microtasks}`,
        `reactions ${metric.executed_promise_reactions}/${metric.queued_promise_reactions}`,
        `combinators ${metric.executed_promise_combinators}/${metric.queued_promise_combinators}`,
      ].join(', ');
      const operationSummary = [
        `dynamic instructions ${metric.dynamic_instructions}`,
        `static/computed props ${metric.static_property_reads}/${metric.computed_property_reads}`,
        `property IC hit/miss/deopt ${metric.property_ic_hits}/${metric.property_ic_misses}/${metric.property_ic_deopts}`,
        `object/array allocs ${metric.object_allocations}/${metric.array_allocations}`,
        `Map get/set ${metric.map_get_calls}/${metric.map_set_calls}`,
        `Set add/has ${metric.set_add_calls}/${metric.set_has_calls}`,
        `string case ${metric.string_case_conversions}`,
        `ASCII case hit/fallback ${metric.ascii_case_fast_path_hits}/${metric.ascii_case_fast_path_fallbacks}`,
        `literal search ${metric.literal_string_searches}`,
        `ASCII substring hit/fallback ${metric.ascii_substring_fast_path_hits}/${metric.ascii_substring_fast_path_fallbacks}`,
        `regex search/replace ${metric.regex_search_or_replacements}`,
        `ASCII token regex hit/fallback ${metric.ascii_token_regex_fast_path_hits}/${metric.ascii_token_regex_fast_path_fallbacks}`,
        `ASCII cleanup hit/fallback ${metric.ascii_cleanup_fast_path_hits}/${metric.ascii_cleanup_fast_path_fallbacks}`,
        `comparator sorts ${metric.comparator_sort_invocations}`,
      ].join(', ');
      console.log(
        `${name}: gc collections ${metric.gc_collections}, gc time ${(metric.gc_total_time_ns / 1e6).toFixed(3)}ms, reclaimed ${metric.gc_reclaimed_bytes}B/${metric.gc_reclaimed_allocations} allocs, accounting refreshes ${metric.accounting_refreshes}, ${microtaskSummary}, ${operationSummary}`,
      );
      if (Array.isArray(metric.collection_hotspots) && metric.collection_hotspots.length > 0) {
        for (const hotspot of metric.collection_hotspots.slice(0, 3)) {
          console.log(
            `  hotspot ${hotspot.source_file}:${hotspot.start_line}:${hotspot.start_column} total ${hotspot.total_calls}, Map get/set ${hotspot.map_get_calls}/${hotspot.map_set_calls}, Set add/has ${hotspot.set_add_calls}/${hotspot.set_has_calls} :: ${hotspot.snippet}`,
          );
        }
      }
    }
  }

  if (hasTimeMetric(results.addon.ptc.weightedScore?.medium)) {
    console.log('');
    console.log('Representative PTC weighted score (medium lanes):');
    console.log(
      `addon ${results.addon.ptc.weightedScore.medium.medianMs.toFixed(2)}ms median, sidecar ${results.sidecar.ptc.weightedScore.medium.medianMs.toFixed(2)}ms median, isolate ${results.isolate.ptc.weightedScore.medium.medianMs.toFixed(2)}ms median`,
    );
  }

  if (Object.keys(results.addon.ptc.transfer).length > 0) {
    console.log('');
    console.log('Representative PTC transfer summaries:');
    for (const [name, metric] of Object.entries(results.addon.ptc.transfer)) {
      console.log(
        `${name}: calls ${metric.toolCallCount}, families ${metric.toolFamilyCount}, toolBytes ${metric.toolBytesIn}B, resultBytes ${metric.resultBytesOut}B, reduction ${metric.reductionRatio.toFixed(2)}x`,
      );
    }
  }

  if (Object.keys(results.addon.ptc.breakdown).length > 0) {
    console.log('');
    console.log('Addon representative PTC boundary breakdowns:');
    for (const [name, metric] of Object.entries(results.addon.ptc.breakdown)) {
      console.log(
        `${name}: host callbacks ${metric.hostCallbacks.medianMs.toFixed(2)}ms, guest execution ${metric.guestExecution.medianMs.toFixed(2)}ms, boundary parse ${metric.boundaryParse.medianMs.toFixed(2)}ms, boundary encode ${metric.boundaryEncode.medianMs.toFixed(2)}ms, boundary codec ${metric.boundaryCodec.medianMs.toFixed(2)}ms`,
      );
    }
  }

  if (Object.keys(results.sidecar.ptc.breakdown).length > 0) {
    console.log('');
    console.log('Sidecar representative PTC breakdowns:');
    const startupMetric = results.sidecar.ptc.breakdown.processStartup;
    if (startupMetric) {
      console.log(
        `processStartup: ${startupMetric.medianMs.toFixed(2)}ms median, ${startupMetric.p95Ms.toFixed(2)}ms p95`,
      );
    }
    for (const [name, metric] of Object.entries(results.sidecar.ptc.breakdown)) {
      if (name === 'processStartup') {
        continue;
      }
      console.log(
        `${name}: request transport ${metric.requestTransport.medianMs.toFixed(2)}ms, execution ${metric.execution.medianMs.toFixed(2)}ms, response materialization ${metric.responseMaterialization.medianMs.toFixed(2)}ms`,
      );
    }
  }

  const phase2Scorecards = results.addon.ptc.phase2.scorecards;
  if (hasTimeMetric(phase2Scorecards.headlineScore?.medium) || hasTimeMetric(phase2Scorecards.broadScore?.medium)) {
    console.log('');
    console.log('Phase-2 PTC scorecards:');
    for (const [label, metric] of [
      ['headline', phase2Scorecards.headlineScore?.medium],
      ['broad', phase2Scorecards.broadScore?.medium],
      ['holdout', phase2Scorecards.holdoutScore?.medium],
      ['durable', phase2Scorecards.durableScore?.medium],
    ]) {
      if (!hasTimeMetric(metric)) {
        continue;
      }
      console.log(
        `${label}: addon ${metric.medianMs.toFixed(2)}ms median, sidecar ${results.sidecar.ptc.phase2.scorecards[`${label}Score`]?.medium?.medianMs?.toFixed?.(2) ?? 'n/a'}ms median, isolate ${results.isolate.ptc.phase2.scorecards[`${label}Score`]?.medium?.medianMs?.toFixed?.(2) ?? 'n/a'}ms median`,
      );
    }
    if (hasRatioMetric(phase2Scorecards.p90LaneRatio?.medium)) {
      console.log(
        `p90 lane ratio: addon ${phase2Scorecards.p90LaneRatio.medium.medianRatio.toFixed(2)}x, sidecar ${results.sidecar.ptc.phase2.scorecards.p90LaneRatio.medium.medianRatio.toFixed(2)}x, isolate ${results.isolate.ptc.phase2.scorecards.p90LaneRatio.medium.medianRatio.toFixed(2)}x`,
      );
    }
    if (hasRatioMetric(phase2Scorecards.worstLaneRatio?.medium)) {
      console.log(
        `worst lane ratio: addon ${phase2Scorecards.worstLaneRatio.medium.p95Ratio.toFixed(2)}x, sidecar ${results.sidecar.ptc.phase2.scorecards.worstLaneRatio.medium.p95Ratio.toFixed(2)}x, isolate ${results.isolate.ptc.phase2.scorecards.worstLaneRatio.medium.p95Ratio.toFixed(2)}x`,
      );
    }
    for (const [familyId, metric] of Object.entries(phase2Scorecards.sentinelFamily ?? {})) {
      if (!hasTimeMetric(metric)) {
        continue;
      }
      console.log(
        `sentinel ${familyId}: addon ${metric.medianMs.toFixed(2)}ms, sidecar ${results.sidecar.ptc.phase2.scorecards.sentinelFamily[familyId].medianMs.toFixed(2)}ms, isolate ${results.isolate.ptc.phase2.scorecards.sentinelFamily[familyId].medianMs.toFixed(2)}ms`,
      );
    }
  }

  if (Object.keys(results.addon.ptc.phase2.gallery.transfer).length > 0) {
    console.log('');
    console.log('Phase-2 gallery transfer summaries:');
    for (const [name, metric] of Object.entries(results.addon.ptc.phase2.gallery.transfer)) {
      console.log(
        `${name}: calls ${metric.toolCallCount}, families ${metric.toolFamilyCount}, toolBytes ${metric.toolBytesIn}B, resultBytes ${metric.resultBytesOut}B, reduction ${metric.reductionRatio.toFixed(2)}x`,
      );
    }
  }

  if (Object.keys(results.addon.ptc.phase2.headlineSeeds.transfer).length > 0) {
    console.log('');
    console.log('Headline skew-seed transfer summaries:');
    for (const [name, metric] of Object.entries(results.addon.ptc.phase2.headlineSeeds.transfer)) {
      console.log(
        `${name}: calls ${metric.toolCallCount}, families ${metric.toolFamilyCount}, toolBytes ${metric.toolBytesIn}B, resultBytes ${metric.resultBytesOut}B, reduction ${metric.reductionRatio.toFixed(2)}x`,
      );
    }
  }

  if (Object.keys(results.addon.durablePtc.resumeOnly).length > 0) {
    console.log('');
    console.log('Durable PTC resume-only checkpoints:');
    for (const [name, metric] of Object.entries(results.addon.durablePtc.resumeOnly)) {
      const addonState = results.addon.durablePtc.state[name];
      const sidecarMetric = results.sidecar.durablePtc.resumeOnly[name];
      const sidecarState = results.sidecar.durablePtc.state[name];
      const isolateMetric = results.isolate.durablePtc.resumeOnly[name];
      const isolateState = results.isolate.durablePtc.state[name];
      console.log(
        `${name}: addon ${metric.medianMs.toFixed(2)}ms (snapshot ${addonState.snapshotBytes}B, manifest ${addonState.detachedManifestBytes}B), sidecar ${sidecarMetric.medianMs.toFixed(2)}ms (snapshot ${sidecarState.snapshotBytes}B, policy ${sidecarState.fullPolicyBytes}B), isolate ${isolateMetric.medianMs.toFixed(2)}ms (carried state ${isolateState.carriedStateBytes}B)`,
      );
    }
  }

  if (results.addon.memory.heapUsedDeltaBytes !== undefined) {
    console.log('');
    console.log(`Memory retained after ${MEMORY_RUNS} workflow runs:`);
    console.log(
      `addon heap ${results.addon.memory.heapUsedDeltaBytes}B rss ${results.addon.memory.rssDeltaBytes}B`,
    );
    console.log(
      `sidecar heap ${results.sidecar.memory.heapUsedDeltaBytes}B rss ${results.sidecar.memory.rssDeltaBytes}B`,
    );
    console.log(
      `isolate heap ${results.isolate.memory.heapUsedDeltaBytes}B rss ${results.isolate.memory.rssDeltaBytes}B`,
    );
  }

  if (results.addon.failureCleanup.limitFailure) {
    console.log('');
    console.log(
      `Failure cleanup limitFailure median ms: addon ${results.addon.failureCleanup.limitFailure.medianMs.toFixed(2)}, sidecar ${results.sidecar.failureCleanup.limitFailure.medianMs.toFixed(2)}, isolate ${results.isolate.failureCleanup.limitFailure.medianMs.toFixed(2)}`,
    );
    console.log(
      `Failure cleanup hostFailure median ms: addon ${results.addon.failureCleanup.hostFailure.medianMs.toFixed(2)}, sidecar ${results.sidecar.failureCleanup.hostFailure.medianMs.toFixed(2)}, isolate ${results.isolate.failureCleanup.hostFailure.medianMs.toFixed(2)}`,
    );
  }
  console.log('');
  console.log(`Wrote JSON report to ${results.reportPath}`);
  if (results.websiteExportPath) {
    console.log(`Wrote website benchmark export to ${results.websiteExportPath}`);
  }
}

async function main() {
  if (typeof global.gc !== 'function') {
    throw new Error('benchmarks/workloads.ts requires node --expose-gc');
  }

  const { profile, mode } = parseArgs(process.argv.slice(2));
  const fixtures = benchmarkFixtureSet();

  global.gc();
  const addon = await benchmarkAddon(fixtures, mode);
  global.gc();
  const sidecar = await benchmarkSidecar(fixtures, profile, mode);
  global.gc();
  const isolate = await benchmarkIsolate(fixtures, mode);
  const durableMediumMetricNames = Object.keys(fixtures.durablePtcScenarios).filter(
    (metricName) => metricName.endsWith('_medium'),
  );

  addon.ptc.phase2.scorecards = buildPhase2Scorecards(
    addon.latency,
    isolate.latency,
    addon.ptc.phase2.sentinel.familyScore,
  );
  sidecar.ptc.phase2.scorecards = buildPhase2Scorecards(
    sidecar.latency,
    isolate.latency,
    sidecar.ptc.phase2.sentinel.familyScore,
  );
  isolate.ptc.phase2.scorecards = buildPhase2Scorecards(
    isolate.latency,
    isolate.latency,
    isolate.ptc.phase2.sentinel.familyScore,
  );
  addon.ptc.phase2.scorecards.durableScore = {
    medium: averageMetric(addon.durablePtc.resumeOnly, durableMediumMetricNames),
  };
  sidecar.ptc.phase2.scorecards.durableScore = {
    medium: averageMetric(sidecar.durablePtc.resumeOnly, durableMediumMetricNames),
  };
  isolate.ptc.phase2.scorecards.durableScore = {
    medium: averageMetric(isolate.durablePtc.resumeOnly, durableMediumMetricNames),
  };

  const results = {
    machine: machineMetadata({
      fixtureVersion: FIXTURE_VERSION,
      benchmarkKind: benchmarkKindForMode(mode),
      buildProfile: profile,
    }),
    ptc: {
      websiteMetric: 'addon.latency.ptc_website_demo_small',
      weightedScoreMetric: 'addon.ptc.weightedScore.medium',
      phase2Mode: mode,
      weights: PTC_WEIGHTS,
      scenarios: Object.fromEntries(
        Object.entries(fixtures.ptcScenarios).map(([metricName, scenario]) => [
          metricName,
          {
            laneId: scenario.laneId,
            sizeName: scenario.sizeName,
            ...scenario.shape,
          },
        ]),
      ),
      durableScenarios: Object.fromEntries(
        Object.entries(fixtures.durablePtcScenarios).map(([metricName, scenario]) => [
          metricName,
          {
            laneId: scenario.laneId,
            sizeName: scenario.sizeName,
            checkpointCapability: scenario.checkpointCapability,
            ...scenario.shape,
          },
        ]),
      ),
      phase2: {
        headlineUseCaseIds: HEADLINE_USE_CASE_IDS,
        broadUseCaseIds: BROAD_USE_CASE_IDS,
        holdoutUseCaseIds: HOLDOUT_USE_CASE_IDS,
        galleryScenarios: Object.fromEntries(
          Object.entries(fixtures.galleryScenarios).map(([metricName, scenario]) => [
            metricName,
            {
              laneId: scenario.laneId,
              category: scenario.category,
              sizeName: scenario.sizeName,
              ...scenario.shape,
            },
          ]),
        ),
        headlineSeedScenarios: Object.fromEntries(
          Object.entries(fixtures.headlineSeedScenarios).map(([metricName, scenario]) => [
            metricName,
            {
              laneId: scenario.laneId,
              nominalMetricName: scenario.nominalMetricName,
              category: scenario.category,
              sizeName: scenario.sizeName,
              seedName: scenario.seedName,
              skewPatterns: [...scenario.skewPatterns],
              ...scenario.shape,
            },
          ]),
        ),
        sentinelFamilies: Object.fromEntries(
          Object.entries(fixtures.sentinelScenarios).map(([familyId, variants]) => [
            familyId,
            Object.fromEntries(
              Object.entries(variants).map(([variantId, scenario]) => [
                variantId,
                {
                  metricName: scenario.metricName,
                  ...scenario.shape,
                },
              ]),
            ),
          ]),
        ),
      },
    },
    notes: {
      suspendResumeIsolate:
        'isolated-vm cannot snapshot continuations here; suspend_resume_* is measured as repeated isolate re-entry with explicit host-carried state rebuild.',
      sidecarMemory:
        'sidecar memory deltas include parent Node process RSS plus the live child sidecar RSS sampled via ps.',
      phaseSplitDefinitions:
        'addon.phases isolates compile-free runtime slices: runtime_init_only uses a precompiled trivial program, execution_only_small resumes pre-created suspended progress, snapshot_load_only uses raw native detached-snapshot inspection, and Progress.load_only measures the public JS wrapper before cleanup.',
      sidecarPhaseDefinitions:
        'sidecar.phases separates process startup from warm execution and transport-dominated resume work: startup_only measures spawn plus clean shutdown, execution_only_small reuses a precompiled program in a warm sidecar, and transport_resume_only replays an already-suspended minimal snapshot so snapshot bytes, auth metadata, and stdio round-trips dominate the timed region.',
      boundaryDefinitions:
        'addon.boundary isolates structured host-boundary work for start inputs, suspended args, resume values, and resume errors across small/medium/large nested payloads while keeping compile and unrelated guest execution out of the timed region.',
      counterDefinitions:
        'addon.counters records untimed cumulative runtime counters from representative addon executions: GC collection count, total GC time, reclaimed bytes/allocations, accounting refresh counts, dynamic instruction dispatch count, static/computed property reads, object/array allocations, Map.get/Map.set, Set.add/Set.has, string case conversion, ASCII string case fast-path hit/fallback counts, literal string search, ASCII substring-search hit/fallback counts, regex search or replacement, ASCII cleanup replaceAll hit/fallback counts, comparator-based sort invocations, and line/column-resolved collection call-site hotspots for representative phase-2 gallery lanes.',
      suspendStateDefinitions:
        'addon.suspendState records serialized program bytes, dumped snapshot bytes, and retained live Progress memory deltas for the suspend_resume_* fixtures while holding a batch of suspended Progress objects live.',
      ptcDefinitions:
        'Representative PTC lanes are sourced from the real programmatic-tool-call gallery: ptc_website_demo_* uses operations/triage-production-incident.js, ptc_incident_triage_* uses operations/triage-multi-region-auth-outage.js, ptc_fraud_investigation_* uses analytics/investigate-fraud-ring.js, and ptc_vendor_review_* uses workflows/vendor-compliance-renewal.js. addon/sidecar/isolate all run the same guest source with deterministic synthetic tool fixtures.',
      ptcWeightedScoreDefinitions:
        'runtime.ptc.weightedScore.medium is a weighted median/p95 rollup of ptc_incident_triage_medium (40%), ptc_fraud_investigation_medium (35%), and ptc_vendor_review_medium (25%). runtime.ptc.transfer records actual tool call counts plus JSON-encoded tool/result payload sizes for one untimed representative run of each PTC lane.',
      ptcBreakdownDefinitions:
        'addon.ptc.breakdown records representative profiled addon runs for the website-small lane, the original medium primary lanes, and the phase-2 headline gallery lanes. hostCallbacks measures JS time spent inside host tool handlers, guestExecution measures Rust runtime execution between boundaries, boundaryParse measures native addon decode/parse of live start/resume payloads, boundaryEncode measures native addon encoding of step results, and boundaryCodec is parse+encode combined.',
      sidecarPtcBreakdownDefinitions:
        'sidecar.ptc.breakdown separates sidecar startup from representative lane-level request flow for the same website and headline gallery lanes. processStartup reuses sidecar.phases.startup_only, while the lane entries split client-observed requestTransport from sidecar execution and combined response materialization (sidecar response preparation plus client response decode/copy).',
      durablePtcDefinitions:
        'runtime.durablePtc.resumeOnly measures restore/resume from persisted checkpoints on the synthetic vendor-review durable lane plus the real audited plan-database-failover and privacy-erasure-orchestration workflows. addon.durablePtc.state records dumped snapshot bytes, detached suspended-manifest bytes, and the checkpoint payload size; sidecar.durablePtc.state records raw snapshot bytes, full raw-resume policy bytes, and the same checkpoint payload size; isolate.durablePtc.state records the explicit carried-state bytes required to emulate the same pause without continuation snapshots.',
      phase2PtcDefinitions:
        'runtime.ptc.phase2 adds real audited gallery lanes, skewed headline seed companions, balanced headline/broad/holdout panels, a multi-lane durable panel, a full-gallery canary summary, and separate sentinel-family scores. The gallery lanes execute the checked-in audited examples from examples/programmatic-tool-calls/*/catalog.ts with exact addon-generated expected outputs reused across addon, sidecar, and isolate runs.',
      phase2PtcMeasurementDefinitions:
        'The targeted phase-2 release modes (`ptc_public`, `ptc_headline_release`, `ptc_broad_release`, `ptc_holdout_release`, and `ptc_sentinel_release`) batch 5 inner executions into each reported warm sample and keep 5 reported samples per lane so sub-millisecond scorecards are less sensitive to timer and scheduler noise.',
    },
    addon,
    sidecar,
    isolate,
    ratios: {
      latency: {
        sidecarVsAddon: ratioTable(addon.latency, sidecar.latency),
        isolateVsAddon: ratioTable(addon.latency, isolate.latency),
        sidecarVsIsolate: ratioTable(isolate.latency, sidecar.latency),
      },
      ptcWeightedScore: {
        sidecarVsAddon: addon.ptc.weightedScore
          ? ratioTable(addon.ptc.weightedScore, sidecar.ptc.weightedScore)
          : {},
        isolateVsAddon: addon.ptc.weightedScore
          ? ratioTable(addon.ptc.weightedScore, isolate.ptc.weightedScore)
          : {},
        sidecarVsIsolate: addon.ptc.weightedScore
          ? ratioTable(isolate.ptc.weightedScore, sidecar.ptc.weightedScore)
          : {},
      },
      failureCleanup: {
        sidecarVsAddon: addon.failureCleanup.limitFailure
          ? failureRatioTable(addon.failureCleanup, sidecar.failureCleanup)
          : {},
        isolateVsAddon: addon.failureCleanup.limitFailure
          ? failureRatioTable(addon.failureCleanup, isolate.failureCleanup)
          : {},
        sidecarVsIsolate: addon.failureCleanup.limitFailure
          ? failureRatioTable(isolate.failureCleanup, sidecar.failureCleanup)
          : {},
      },
    },
  };

  const reportPath = writeBenchmarkArtifact(results);
  results.reportPath = reportPath;
  results.websiteExportPath = writeWebsitePtcExport(results);
  printSummary(results);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
