'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const { once } = require('node:events');
const { performance } = require('node:perf_hooks');
const { spawn, execFileSync } = require('node:child_process');

const ivm = require('isolated-vm');

const { ExecutionContext, Mustard, Progress } = require('../index.ts');
const { loadNative } = require('../native-loader.ts');
const { callNative } = require('../lib/errors.ts');
const { decodeStructured, encodeResumePayloadValue } = require('../lib/structured.ts');
const {
  encodeSnapshotPolicy,
  resolveProgressLoadContext,
  snapshotIdentity,
  snapshotKeyDigest,
  snapshotToken,
} = require('../lib/policy.ts');
const {
  DEFAULT_MEASURE_OPTIONS,
  measure,
  measureSamples,
  machineMetadata,
  writeBenchmarkArtifact,
} = require('./support.ts');

const REPO_ROOT = path.join(__dirname, '..');
const FIXTURE_VERSION = 4;
const SNAPSHOT_KEY = 'benchmark-workloads-snapshot-key';
const SNAPSHOT_KEY_BASE64 = Buffer.from(SNAPSHOT_KEY, 'utf8').toString('base64');

const DEFAULT_OPTIONS = DEFAULT_MEASURE_OPTIONS;
const COLD_OPTIONS = Object.freeze({ warmup: 0, iterations: 2 });
const MEMORY_RUNS = 20;
const SIDECAR_PROTOCOL_VERSION = 1;

function parseArgs(argv) {
  let profile = 'release';
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === '--profile') {
      profile = argv[index + 1];
      index += 1;
      continue;
    }
    throw new Error(`Unknown benchmark argument: ${value}`);
  }
  if (profile !== 'dev' && profile !== 'release') {
    throw new Error(`Unsupported workloads profile: ${profile}`);
  }
  return { profile };
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

function sidecarStepValue(step) {
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
    return await run({ child, request });
  } finally {
    child.stdin.end();
    reader.close();
    const [code] = await once(child, 'close');
    assert.equal(code, 0, `sidecar exited unsuccessfully\nstderr:\n${stderr.join('')}`);
  }
}

async function compileSidecarSource(request, source) {
  const compile = await request({
    protocol_version: SIDECAR_PROTOCOL_VERSION,
    method: 'compile',
    id: 1,
    source,
  });
  assert.equal(compile.ok, true, `sidecar compile failed: ${compile.error}`);
  return compile.result.program_base64;
}

async function runSidecarProgram(request, programBase64, capabilities = undefined, resumeValue = undefined) {
  const capabilityNames = capabilities ? Object.keys(capabilities) : [];
  const start = await request({
    protocol_version: SIDECAR_PROTOCOL_VERSION,
    method: 'start',
    id: 2,
    program_base64: programBase64,
    options: {
      inputs: {},
      capabilities: capabilityNames,
      limits: {},
    },
  });
  assert.equal(start.ok, true, `sidecar start failed: ${start.error}`);
  let step = sidecarStepValue(start.result.step);
  let requestId = 3;
  while (step.type === 'suspended') {
    const snapshot = Buffer.from(step.snapshotBase64, 'base64');
    const payloadValue =
      typeof resumeValue === 'function'
        ? resumeValue(step)
        : capabilities?.[step.capability]?.(...step.args) ?? step.args[0];
    const resume = await request({
      protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'resume',
      id: requestId,
      snapshot_base64: step.snapshotBase64,
      policy: {
        capabilities: capabilityNames,
        limits: {},
        snapshot_id: snapshotIdentity(snapshot),
        snapshot_key_base64: SNAPSHOT_KEY_BASE64,
        snapshot_key_digest: snapshotKeyDigest(Buffer.from(SNAPSHOT_KEY, 'utf8')),
        snapshot_token: snapshotToken(snapshot, SNAPSHOT_KEY),
      },
      payload: JSON.parse(encodeResumePayloadValue(payloadValue)),
    });
    assert.equal(resume.ok, true, `sidecar resume failed: ${resume.error}`);
    step = sidecarStepValue(resume.result.step);
    requestId += 1;
  }
  return step.value;
}

function installIsolateCapabilities(context, capabilities = {}) {
  const jail = context.global;
  jail.setSync('global', jail.derefInto());
  for (const [name, handler] of Object.entries(capabilities)) {
    jail.setSync(`__host_${name}`, new ivm.Reference(handler));
  }
  if (Object.keys(capabilities).length > 0) {
    context.evalSync(`
      ${Object.keys(capabilities)
        .map(
          (name) => `global.${name} = function(...args) {
        return __host_${name}.applySync(undefined, args, {
          arguments: { copy: true },
          result: { copy: true }
        });
      };`,
        )
        .join('\n')}
    `);
  }
}

function createIsolateContext(capabilities = {}) {
  const isolate = new ivm.Isolate({ memoryLimit: 128 });
  const context = isolate.createContextSync();
  installIsolateCapabilities(context, capabilities);
  return { isolate, context };
}

function runIsolateScript(context, script) {
  return script.runSync(context, { copy: true });
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
  };
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

async function measureRetainedMemory(runMany, options = {}) {
  const sample = options.sample ?? processMemorySnapshot;
  global.gc();
  const before = sample();
  await runMany();
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
    snapshotToken: dumped.token,
  });
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

  const inspectDump = queuedFactory(() => suspendProgressFactory().dump());
  const snapshotLoadOnly = await measureSamples('snapshot_load_only', async () => {
    const dumped = inspectDump();
    const policyJson = createAuthenticatedPolicyJson(dumped, loadOptions);
    const start = performance.now();
    const inspection = JSON.parse(callNative(native.inspectSnapshot, dumped.snapshot, policyJson));
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

  return phaseLatency;
}

async function benchmarkAddon(fixtures) {
  console.log('Running addon benchmarks...');
  const latency = {};
  const { smallSource, codeModeSource, workflowSource, workflowData } = fixtures;
  const defaultContext = new ExecutionContext();
  const warmSmallRuntime = new Mustard(smallSource);
  const warmCodeModeRuntime = new Mustard(codeModeSource);
  const workflowRuntime = new Mustard(workflowSource);
  const workflowCapabilities = createWorkflowCapabilities(workflowData);
  const workflowContext = new ExecutionContext({
    capabilities: workflowCapabilities,
    snapshotKey: SNAPSHOT_KEY,
  });
  const phases = await benchmarkAddonPhases();

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
    const runtime = new Mustard(createSuspendResumeSource(boundaryCount));
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
  }

  const memory = await measureRetainedMemory(async () => {
    for (let i = 0; i < MEMORY_RUNS; i += 1) {
      const result = await workflowRuntime.run({ context: workflowContext });
      assertWorkflowResult(result);
    }
  });

  const failureCleanup = {
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

  return { latency, phases, memory, failureCleanup };
}

async function benchmarkSidecar(fixtures, profile) {
  console.log('Running sidecar benchmarks...');
  const latency = {};
  const { smallSource, codeModeSource, workflowSource, workflowData } = fixtures;

  Object.assign(latency, Object.fromEntries([
    await measure('cold_start_small', async () => {
      await withSidecar(profile, async ({ request }) => {
        const programBase64 = await compileSidecarSource(request, smallSource);
        const result = await runSidecarProgram(request, programBase64);
        assert.equal(typeof result, 'number');
      });
    }, COLD_OPTIONS),
    await measure('cold_start_code_mode_search', async () => {
      await withSidecar(profile, async ({ request }) => {
        const programBase64 = await compileSidecarSource(request, codeModeSource);
        const result = await runSidecarProgram(request, programBase64);
        assert.equal(result.count > 0, true);
      });
    }, COLD_OPTIONS),
  ]));

  const workflowCapabilities = createWorkflowCapabilities(workflowData);

  await withSidecar(profile, async ({ child, request }) => {
    const smallProgram = await compileSidecarSource(request, smallSource);
    const codeModeProgram = await compileSidecarSource(request, codeModeSource);
    const workflowProgram = await compileSidecarSource(request, workflowSource);

    const warmSmall = await measure('warm_run_small', async () => {
      const result = await runSidecarProgram(request, smallProgram);
      assert.equal(typeof result, 'number');
    }, DEFAULT_OPTIONS);
    latency[warmSmall[0]] = warmSmall[1];

    const warmCode = await measure('warm_run_code_mode_search', async () => {
      const result = await runSidecarProgram(request, codeModeProgram);
      assert.equal(result.count > 0, true);
    }, DEFAULT_OPTIONS);
    latency[warmCode[0]] = warmCode[1];

    const workflowMetric = await measure('programmatic_tool_workflow', async () => {
      const result = await runSidecarProgram(request, workflowProgram, workflowCapabilities);
      assertWorkflowResult(result);
    }, DEFAULT_OPTIONS);
    latency[workflowMetric[0]] = workflowMetric[1];

    for (const callCount of [1, 10, 50, 100]) {
      const programBase64 = await compileSidecarSource(request, createFanoutSource(callCount));
      const metric = await measure(`host_fanout_${callCount}`, async () => {
        const result = await runSidecarProgram(request, programBase64, {
          fetch_value(value) {
            return value;
          },
        });
        assert.equal(result, expectedFanoutTotal(callCount));
      }, COLD_OPTIONS);
      latency[metric[0]] = metric[1];
    }

    for (const boundaryCount of [1, 5, 20]) {
      const programBase64 = await compileSidecarSource(request, createSuspendResumeSource(boundaryCount));
      const metric = await measure(`suspend_resume_${boundaryCount}`, async () => {
        const result = await runSidecarProgram(request, programBase64, {
          checkpoint(value) {
            return value;
          },
        });
        assert.equal(result, expectedSuspendTotal(boundaryCount));
      }, COLD_OPTIONS);
      latency[metric[0]] = metric[1];
    }

    const memory = await measureRetainedMemory(
      async () => {
        for (let i = 0; i < MEMORY_RUNS; i += 1) {
          const result = await runSidecarProgram(request, workflowProgram, workflowCapabilities);
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

    const failureCleanup = {
      limitFailure: await measureFailureCleanup('limit_failure', async () => {
        const failingProgram = await compileSidecarSource(request, fixtures.failureSource);
        const failure = await request({
          protocol_version: SIDECAR_PROTOCOL_VERSION,
    method: 'start',
    id: 4000,
          program_base64: failingProgram,
          options: {
            inputs: {},
            capabilities: [],
            limits: {
              heap_limit_bytes: 512,
            },
          },
        });
        assert.equal(failure.ok, false);
        const recovered = await runSidecarProgram(request, smallProgram);
        assert.equal(typeof recovered, 'number');
      }, COLD_OPTIONS),
      hostFailure: await measureFailureCleanup('host_failure', async () => {
        const failingProgram = await compileSidecarSource(request, fixtures.hostFailureSource);
        const start = await request({
          protocol_version: SIDECAR_PROTOCOL_VERSION,
    method: 'start',
    id: 5000,
          program_base64: failingProgram,
          options: {
            inputs: {},
            capabilities: ['fetch_value', 'explode'],
            limits: {},
          },
        });
        assert.equal(start.ok, true);
        let step = sidecarStepValue(start.result.step);
        assert.equal(step.capability, 'fetch_value');
        const snapshot = Buffer.from(step.snapshotBase64, 'base64');
        let response = await request({
          protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'resume',
      id: 5001,
          snapshot_base64: step.snapshotBase64,
          policy: {
            capabilities: ['fetch_value', 'explode'],
            limits: {},
            snapshot_id: snapshotIdentity(snapshot),
            snapshot_key_base64: SNAPSHOT_KEY_BASE64,
            snapshot_key_digest: snapshotKeyDigest(Buffer.from(SNAPSHOT_KEY, 'utf8')),
            snapshot_token: snapshotToken(snapshot, SNAPSHOT_KEY),
          },
          payload: JSON.parse(encodeResumePayloadValue(1)),
        });
        assert.equal(response.ok, true);
        step = sidecarStepValue(response.result.step);
        assert.equal(step.capability, 'explode');
        response = await request({
          protocol_version: SIDECAR_PROTOCOL_VERSION,
      method: 'resume',
      id: 5002,
          snapshot_base64: step.snapshotBase64,
          policy: {
            capabilities: ['fetch_value', 'explode'],
            limits: {},
            snapshot_id: snapshotIdentity(Buffer.from(step.snapshotBase64, 'base64')),
            snapshot_key_base64: SNAPSHOT_KEY_BASE64,
            snapshot_key_digest: snapshotKeyDigest(Buffer.from(SNAPSHOT_KEY, 'utf8')),
            snapshot_token: snapshotToken(Buffer.from(step.snapshotBase64, 'base64'), SNAPSHOT_KEY),
          },
          payload: JSON.parse(encodeResumePayloadValue({ __host_error__: true })),
        });
        assert.equal(response.ok, false);
        const recovered = await runSidecarProgram(request, smallProgram);
        assert.equal(typeof recovered, 'number');
      }, COLD_OPTIONS),
    };

    latency.__memory = memory;
    latency.__failureCleanup = failureCleanup;
  });

  const memory = latency.__memory;
  const failureCleanup = latency.__failureCleanup;
  delete latency.__memory;
  delete latency.__failureCleanup;
  return { latency, memory, failureCleanup };
}

async function benchmarkIsolate(fixtures) {
  console.log('Running V8 isolate benchmarks...');
  const latency = {};
  const { smallSource, codeModeSource, workflowSource, workflowData } = fixtures;
  const workflowCapabilities = createWorkflowCapabilities(workflowData);

  Object.assign(latency, Object.fromEntries([
    await measure('cold_start_small', async () => {
      const { isolate, context } = createIsolateContext();
      const script = isolate.compileScriptSync(smallSource);
      const result = runIsolateScript(context, script);
      assert.equal(typeof result, 'number');
    }, DEFAULT_OPTIONS),
    await measure('cold_start_code_mode_search', async () => {
      const { isolate, context } = createIsolateContext();
      const script = isolate.compileScriptSync(codeModeSource);
      const result = runIsolateScript(context, script);
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
      const result = runIsolateScript(context, smallScript);
      assert.equal(typeof result, 'number');
    }, DEFAULT_OPTIONS);
    latency[warmSmall[0]] = warmSmall[1];

    const warmCode = await measure('warm_run_code_mode_search', async () => {
      const context = isolate.createContextSync();
      installIsolateCapabilities(context);
      const result = runIsolateScript(context, codeModeScript);
      assert.equal(result.count > 0, true);
    }, DEFAULT_OPTIONS);
    latency[warmCode[0]] = warmCode[1];

    const workflowMetric = await measure('programmatic_tool_workflow', async () => {
      const context = isolate.createContextSync();
      installIsolateCapabilities(context, workflowCapabilities);
      const result = runIsolateScript(context, workflowScript);
      assertWorkflowResult(result);
    }, DEFAULT_OPTIONS);
    latency[workflowMetric[0]] = workflowMetric[1];
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
      const result = runIsolateScript(context, script);
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

  const memory = await measureRetainedMemory(async () => {
    const isolate = new ivm.Isolate({ memoryLimit: 128 });
    const script = isolate.compileScriptSync(workflowSource);
    for (let i = 0; i < MEMORY_RUNS; i += 1) {
      const context = isolate.createContextSync();
      installIsolateCapabilities(context, workflowCapabilities);
      const result = runIsolateScript(context, script);
      assertWorkflowResult(result);
    }
  });

  const failureCleanup = {
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
      const recovered = runIsolateScript(recoveryContext, recoveryScript);
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
      assert.throws(() => {
        runIsolateScript(context, script);
      });
      const recoveryContext = isolate.createContextSync();
      installIsolateCapabilities(recoveryContext);
      const recoveryScript = isolate.compileScriptSync(fixtures.smallSource);
      const recovered = runIsolateScript(recoveryContext, recoveryScript);
      assert.equal(typeof recovered, 'number');
    }, DEFAULT_OPTIONS),
  };

  return { latency, memory, failureCleanup };
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

function printSummary(results) {
  console.log(`Machine: ${results.machine.cpuModel} (${results.machine.cpuCount} cores)`);
  console.log(`Node: ${results.machine.nodeVersion} on ${results.machine.platform} [${results.machine.buildProfile}]`);
  if (results.machine.gitSha) {
    console.log(`Git SHA: ${results.machine.gitSha}`);
  }
  console.log('');
  for (const name of Object.keys(results.addon.latency)) {
    const addon = results.addon.latency[name];
    const sidecar = results.sidecar.latency[name];
    const isolate = results.isolate.latency[name];
    console.log(`${name}: addon ${addon.medianMs.toFixed(2)}ms, sidecar ${sidecar.medianMs.toFixed(2)}ms, isolate ${isolate.medianMs.toFixed(2)}ms`);
  }
  console.log('');
  console.log('Addon phase splits:');
  for (const [name, metric] of Object.entries(results.addon.phases)) {
    console.log(`${name}: ${metric.medianMs.toFixed(2)}ms median, ${metric.p95Ms.toFixed(2)}ms p95`);
  }
  console.log('');
  console.log(`Memory retained after ${MEMORY_RUNS} workflow runs:`);
  console.log(`addon heap ${results.addon.memory.heapUsedDeltaBytes}B rss ${results.addon.memory.rssDeltaBytes}B`);
  console.log(`sidecar heap ${results.sidecar.memory.heapUsedDeltaBytes}B rss ${results.sidecar.memory.rssDeltaBytes}B`);
  console.log(`isolate heap ${results.isolate.memory.heapUsedDeltaBytes}B rss ${results.isolate.memory.rssDeltaBytes}B`);
  console.log('');
  console.log(`Failure cleanup limitFailure median ms: addon ${results.addon.failureCleanup.limitFailure.medianMs.toFixed(2)}, sidecar ${results.sidecar.failureCleanup.limitFailure.medianMs.toFixed(2)}, isolate ${results.isolate.failureCleanup.limitFailure.medianMs.toFixed(2)}`);
  console.log(`Failure cleanup hostFailure median ms: addon ${results.addon.failureCleanup.hostFailure.medianMs.toFixed(2)}, sidecar ${results.sidecar.failureCleanup.hostFailure.medianMs.toFixed(2)}, isolate ${results.isolate.failureCleanup.hostFailure.medianMs.toFixed(2)}`);
  console.log('');
  console.log(`Wrote JSON report to ${results.reportPath}`);
}

async function main() {
  if (typeof global.gc !== 'function') {
    throw new Error('benchmarks/workloads.ts requires node --expose-gc');
  }

  const { profile } = parseArgs(process.argv.slice(2));
  const fixtures = benchmarkFixtureSet();

  global.gc();
  const addon = await benchmarkAddon(fixtures);
  global.gc();
  const sidecar = await benchmarkSidecar(fixtures, profile);
  global.gc();
  const isolate = await benchmarkIsolate(fixtures);

  const results = {
    machine: machineMetadata({
      fixtureVersion: FIXTURE_VERSION,
      benchmarkKind: 'workloads',
      buildProfile: profile,
    }),
    notes: {
      suspendResumeIsolate:
        'isolated-vm cannot snapshot continuations here; suspend_resume_* is measured as repeated isolate re-entry with explicit host-carried state rebuild.',
      sidecarMemory:
        'sidecar memory deltas include parent Node process RSS plus the live child sidecar RSS sampled via ps.',
      phaseSplitDefinitions:
        'addon.phases isolates compile-free runtime slices: runtime_init_only uses a precompiled trivial program, execution_only_small resumes pre-created suspended progress, snapshot_load_only uses raw native inspectSnapshot, and Progress.load_only measures the public JS wrapper before cleanup.',
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
      failureCleanup: {
        sidecarVsAddon: failureRatioTable(addon.failureCleanup, sidecar.failureCleanup),
        isolateVsAddon: failureRatioTable(addon.failureCleanup, isolate.failureCleanup),
        sidecarVsIsolate: failureRatioTable(isolate.failureCleanup, sidecar.failureCleanup),
      },
    },
  };

  const reportPath = writeBenchmarkArtifact(results);
  results.reportPath = reportPath;
  printSummary(results);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
