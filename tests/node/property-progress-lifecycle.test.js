'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { Mustard, MustardError, Progress } = require('../../index.ts');
const { PROPERTY_RUNS, fc } = require('./property-generators.js');
const { captureOutcome } = require('./runtime-oracle.js');

const SNAPSHOT_KEY = Buffer.from('progress-lifecycle-property-key');
const explicitLoadOptions = Object.freeze({
  snapshotKey: SNAPSHOT_KEY,
  capabilities: {
    fetch_data() {},
  },
  limits: {},
});

const lifecycleTransportArbitrary = fc.constantFrom('direct', 'explicit-load');
const lifecycleOutcomeArbitrary = fc.constantFrom('value', 'error');
const replayLoadActionArbitrary = fc.constantFrom('load-explicit');
const replayConsumeActionArbitrary = fc.constantFrom('resume', 'resumeError', 'cancel');
const replayLifecycleActionArbitrary = fc.constantFrom(
  'load-explicit',
  'resume',
  'resumeError',
  'cancel',
);

function renderActionHistory(actions) {
  return actions.map((action, index) => `${index + 1}. ${action}`).join('\n');
}

function renderStepHistory(steps) {
  return steps
    .map((step, index) => `${index + 1}. ${step.transport} -> ${step.outcome}`)
    .join('\n');
}

async function assertLifecycleProperty({ arbitrary, label, renderCase, runCase, numRuns }) {
  const details = await fc.check(
    fc.asyncProperty(arbitrary, async (entry) => {
      await runCase(entry);
    }),
    {
      numRuns: numRuns ?? Math.max(25, Math.floor(PROPERTY_RUNS / 2)),
      interruptAfterTimeLimit: 20_000,
    },
  );

  if (!details.failed) {
    return;
  }

  if (details.counterexample === null) {
    assert.fail(`${label} failed without a minimized counterexample`);
  }

  const [entry] = details.counterexample;
  const sections = [
    `${label} failed after ${details.numRuns} run(s).`,
    `{ seed: ${details.seed}, path: "${details.counterexamplePath}" }`,
    `Shrunk ${details.numShrinks} time(s).`,
    renderCase(entry),
  ];
  if (details.errorInstance instanceof Error) {
    sections.push(details.errorInstance.message);
  }
  assert.fail(sections.join('\n\n'));
}

function loadProgress(progress, transport) {
  if (transport === 'direct') {
    return progress;
  }
  if (transport === 'explicit-load') {
    return Progress.load(progress.dump(), explicitLoadOptions);
  }
  throw new Error(`unsupported transport: ${transport}`);
}

function lifecycleValueForArg(arg) {
  return arg + 2;
}

function lifecycleErrorForArg(arg) {
  return new Error(`boom ${arg}`);
}

function isRuntimeBoom(error) {
  return (
    error instanceof MustardError &&
    error.kind === 'Runtime' &&
    error.message.includes('boom')
  );
}

function isCancelledLimit(error) {
  return (
    error instanceof MustardError &&
    error.kind === 'Limit' &&
    error.message.includes('execution cancelled')
  );
}

function isSingleUseRuntimeError(error) {
  return (
    error instanceof MustardError &&
    error.kind === 'Runtime' &&
    error.message.includes('single-use')
  );
}

async function runLifecycleScriptWithRun(steps) {
  let index = 0;
  const runtime = new Mustard(`
    const first = fetch_data(1);
    const second = fetch_data(first + 1);
    ({ first, second, total: first + second });
  `);
  return runtime.run({
    capabilities: {
      fetch_data(arg) {
        const step = steps[index];
        index += 1;
        if (step.outcome === 'value') {
          return lifecycleValueForArg(arg);
        }
        throw lifecycleErrorForArg(arg);
      },
    },
  });
}

function runLifecycleScriptWithStart(steps) {
  const runtime = new Mustard(`
    const first = fetch_data(1);
    const second = fetch_data(first + 1);
    ({ first, second, total: first + second });
  `);
  let current = runtime.start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
    limits: {},
  });
  let index = 0;
  while (current instanceof Progress) {
    const step = steps[index];
    current = loadProgress(current, step.transport);
    if (step.outcome === 'value') {
      current = current.resume(lifecycleValueForArg(current.args[0]));
    } else {
      current = current.resumeError(lifecycleErrorForArg(current.args[0]));
    }
    index += 1;
  }
  return current;
}

test('property: run and start/load/resume flows agree across lifecycle transports', async () => {
  const lifecycleScriptArbitrary = fc.tuple(
    fc.record({
      transport: lifecycleTransportArbitrary,
      outcome: lifecycleOutcomeArbitrary,
    }),
    fc.record({
      transport: lifecycleTransportArbitrary,
      outcome: lifecycleOutcomeArbitrary,
    }),
  );

  await assertLifecycleProperty({
    arbitrary: lifecycleScriptArbitrary,
    label: 'progress lifecycle transport script',
    renderCase: (steps) => `Minimized action history:\n${renderStepHistory(steps)}`,
    runCase: async (steps) => {
      const [runOutcome, startOutcome] = await Promise.all([
        captureOutcome(() => runLifecycleScriptWithRun(steps)),
        captureOutcome(() => runLifecycleScriptWithStart(steps)),
      ]);
      assert.deepEqual(startOutcome, runOutcome);
    },
  });
});

function performReplayAction(progress, action) {
  if (action === 'resume') {
    return progress.resume(4);
  }
  if (action === 'resumeError') {
    return progress.resumeError(new Error('boom 4'));
  }
  if (action === 'cancel') {
    return progress.cancel();
  }
  if (action === 'load-explicit') {
    return Progress.load(progress.dump(), explicitLoadOptions);
  }
  throw new Error(`unsupported replay action: ${action}`);
}

function replaySequenceArbitrary({ prefixMaxLength, suffixMinLength, suffixMaxLength }) {
  return fc
    .tuple(
      fc.array(replayLoadActionArbitrary, { maxLength: prefixMaxLength }),
      replayConsumeActionArbitrary,
      fc.array(replayLifecycleActionArbitrary, {
        minLength: suffixMinLength,
        maxLength: suffixMaxLength,
      }),
    )
    .map(([prefix, consume, suffix]) => [...prefix, consume, ...suffix]);
}

async function assertReplayLifecycleSequence(actions) {
  const runtime = new Mustard('fetch_data(4);');
  let current = runtime.start({
    snapshotKey: SNAPSHOT_KEY,
    capabilities: {
      fetch_data() {},
    },
    limits: {},
  });
  assert.ok(current instanceof Progress);

  let consumed = false;
  let claimedByLoad = false;
  for (const action of actions) {
    if (!consumed && action === 'load-explicit') {
      if (claimedByLoad) {
        assert.throws(() => performReplayAction(current, action), isSingleUseRuntimeError);
      } else {
        current = performReplayAction(current, action);
        assert.ok(current instanceof Progress);
        claimedByLoad = true;
      }
      continue;
    }

    if (consumed) {
      assert.throws(() => performReplayAction(current, action), isSingleUseRuntimeError);
      continue;
    }

    if (action === 'resume') {
      assert.equal(performReplayAction(current, action), 4);
    } else if (action === 'resumeError') {
      assert.throws(() => performReplayAction(current, action), isRuntimeBoom);
    } else {
      assert.throws(() => performReplayAction(current, action), isCancelledLimit);
    }
    consumed = true;
  }
}

test('property: progress lifecycle sequences preserve single-use across replay paths', async () => {
  await assertLifecycleProperty({
    arbitrary: replaySequenceArbitrary({
      prefixMaxLength: 3,
      suffixMinLength: 1,
      suffixMaxLength: 5,
    }),
    label: 'progress replay lifecycle',
    renderCase: (actions) => `Minimized action history:\n${renderActionHistory(actions)}`,
    runCase: assertReplayLifecycleSequence,
  });
});

test(
  'property: extended progress replay sequences run outside the presubmit lane',
  { skip: !process.env.MUSTARD_LONG_TESTS },
  async () => {
    await assertLifecycleProperty({
      arbitrary: replaySequenceArbitrary({
        prefixMaxLength: 6,
        suffixMinLength: 4,
        suffixMaxLength: 12,
      }),
      label: 'extended progress replay lifecycle',
      renderCase: (actions) => `Minimized action history:\n${renderActionHistory(actions)}`,
      runCase: assertReplayLifecycleSequence,
      numRuns: process.env.CI ? 160 : 80,
    });
  },
);
