'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const {
  PLAN_BLOCKED_MARKER,
  PLAN_COMPLETED_MARKER,
  PLAN_DONE_AT_MARKER_PREFIX,
  DEFAULT_DELAY_MS,
  DEFAULT_MODEL,
  DEFAULT_REASONING_EFFORT,
  UsageError,
  buildCodexExecArgs,
  buildPlanPrompt,
  getPlanStateFromContent,
  parseArgs,
  runLoop,
} = require('../../scripts/ralph-loop.ts');

function createTempPlan(initialContent) {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'jslite-ralph-loop-'));
  const planPath = path.join(tempRoot, 'plan.md');
  fs.writeFileSync(planPath, initialContent, 'utf8');
  return { tempRoot, planPath };
}

test('parseArgs accepts a required plan path and defaults to an unbounded loop', () => {
  assert.deepEqual(parseArgs(['plans/sample.md']), {
    help: false,
    planPath: 'plans/sample.md',
    maxIterations: null,
    delayMs: DEFAULT_DELAY_MS,
  });
});

test('parseArgs validates flag values and unknown options', () => {
  assert.throws(() => parseArgs([]), UsageError);
  assert.throws(() => parseArgs(['plan.md', '--max-iterations', '0']), UsageError);
  assert.throws(() => parseArgs(['plan.md', '--delay-ms', '-1']), UsageError);
  assert.throws(() => parseArgs(['plan.md', '--wat']), UsageError);
});

test('plan helpers detect completion, blocked state, and done-at hash', () => {
  const content = [
    '# Plan',
    PLAN_COMPLETED_MARKER,
    `${PLAN_DONE_AT_MARKER_PREFIX}abc123]`,
    '',
    '## Iteration Log',
  ].join('\n');
  assert.deepEqual(getPlanStateFromContent(content), {
    completed: true,
    blocked: false,
    doneAtCommit: 'abc123',
  });

  assert.deepEqual(getPlanStateFromContent('## Status: Blocked\n'), {
    completed: false,
    blocked: true,
    doneAtCommit: null,
  });
});

test('buildPlanPrompt and buildCodexExecArgs target gpt-5.4 xhigh and add plan dir when needed', () => {
  const cwd = path.join('/tmp', 'repo');
  const planPath = path.join('/tmp', 'plans', 'queue.md');
  const prompt = buildPlanPrompt(planPath, 3, cwd);
  const args = buildCodexExecArgs({ prompt, cwd, planPath });

  assert.match(prompt, /This is iteration 3\.$/);
  assert.ok(args.includes('--dangerously-bypass-approvals-and-sandbox'));
  assert.deepEqual(args.slice(0, 6), [
    'exec',
    '--dangerously-bypass-approvals-and-sandbox',
    '--model',
    DEFAULT_MODEL,
    '-c',
    `model_reasoning_effort="${DEFAULT_REASONING_EFFORT}"`,
  ]);
  assert.deepEqual(args.slice(6, 8), ['--add-dir', path.dirname(planPath)]);
});

test('runLoop exits immediately when the plan is already complete', async () => {
  const { tempRoot, planPath } = createTempPlan(`${PLAN_COMPLETED_MARKER}\n`);
  let calls = 0;

  try {
    const result = await runLoop({
      cwd: tempRoot,
      planPath,
      maxIterations: 3,
      delayMs: 0,
      runner: async () => {
        calls += 1;
        return 0;
      },
      logger: {
        log() {},
        error() {},
      },
      sleepFn: async () => {},
    });

    assert.equal(result.status, 'completed');
    assert.equal(result.iterations, 0);
    assert.equal(calls, 0);
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
});

test('runLoop retries until the plan is marked complete', async () => {
  const { tempRoot, planPath } = createTempPlan('# Plan\n## Status: In Progress\n');
  let calls = 0;

  try {
    const result = await runLoop({
      cwd: tempRoot,
      planPath,
      maxIterations: 5,
      delayMs: 0,
      runner: async () => {
        calls += 1;
        if (calls === 2) {
          fs.writeFileSync(
            planPath,
            [
              '# Plan',
              PLAN_COMPLETED_MARKER,
              `${PLAN_DONE_AT_MARKER_PREFIX}deadbee]`,
              '## Status: Done',
            ].join('\n'),
            'utf8',
          );
        }
        return calls === 1 ? 1 : 0;
      },
      logger: {
        log() {},
        error() {},
      },
      sleepFn: async () => {},
    });

    assert.equal(result.status, 'completed');
    assert.equal(result.iterations, 2);
    assert.equal(result.doneAtCommit, 'deadbee');
    assert.equal(calls, 2);
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
});

test('runLoop stops when the plan is marked blocked', async () => {
  const { tempRoot, planPath } = createTempPlan('# Plan\n');

  try {
    const result = await runLoop({
      cwd: tempRoot,
      planPath,
      maxIterations: 5,
      delayMs: 0,
      runner: async () => {
        fs.writeFileSync(planPath, `${PLAN_BLOCKED_MARKER}\n## Status: Blocked\n`, 'utf8');
        return 0;
      },
      logger: {
        log() {},
        error() {},
      },
      sleepFn: async () => {},
    });

    assert.equal(result.status, 'blocked');
    assert.equal(result.iterations, 1);
    assert.equal(result.exitCode, 1);
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
});
