import type {
  Capability,
  CapabilityError,
  ConsoleCallbacks,
  ExecutionOptions,
  JsliteError as JsliteErrorType,
  JsliteErrorKind,
  ProgressLoadOptions,
  Progress as ProgressType,
  ResumeOptions,
  RuntimeLimits,
  SerializedProgress,
  StructuredValue,
} from '@keppoai/jslite';

const { Jslite, JsliteError, Progress } = require('@keppoai/jslite') as typeof import('@keppoai/jslite');

const runtime = new Jslite('const response = fetch_data(seed); response + 1;', {
  inputs: ['seed'],
});

const executionOptions: ExecutionOptions = {
  inputs: {
    seed: 1,
    nested: { values: [1, 2, 3] },
  },
  limits: {
    instructionBudget: 1000,
  },
  signal: new AbortController().signal,
  snapshotKey: Buffer.from('snapshot-key'),
  capabilities: {
    fetch_data(value) {
      return value;
    },
    async fetch_async(value) {
      return value;
    },
  },
};

const structured: StructuredValue = {
  ok: true,
  count: 1,
  values: ['ready'],
};

const runtimeLimits: RuntimeLimits = {
  instructionBudget: 10,
};
const resumeOptions: ResumeOptions = {
  signal: new AbortController().signal,
};
const progressLoadOptions: ProgressLoadOptions = {
  snapshotKey: Buffer.from('snapshot-key'),
  capabilities: {
    fetch_data(value) {
      return value;
    },
  },
  limits: {
    instructionBudget: 1000,
  },
};

const errorKind: JsliteErrorKind = 'Runtime';
const capability: Capability = async (...args) => args[0] ?? structured;
const consoleCallbacks: ConsoleCallbacks = {
  log(...args) {
    return args[0];
  },
};

async function typecheck(): Promise<void> {
  const result: StructuredValue = await runtime.run({
    ...executionOptions,
    console: consoleCallbacks,
    capabilities: {
      ...executionOptions.capabilities,
      fetch_data: capability,
    },
  });

  const dumped: Buffer = runtime.dump();
  const loaded: typeof runtime = Jslite.load(dumped);
  const step: StructuredValue | ProgressType = loaded.start(executionOptions);

  if (step instanceof Progress) {
    const capabilityName: string = step.capability;
    const args: StructuredValue[] = step.args;
    const snapshot: Buffer = step.snapshot;
    const dumpedProgress: SerializedProgress = step.dump();
    const restored: ProgressType = Progress.load(dumpedProgress);
    const restoredWithPolicy: ProgressType = Progress.load(dumpedProgress, progressLoadOptions);
    const resumed: StructuredValue | ProgressType = step.resume(1, resumeOptions);
    const hostError: CapabilityError = Object.assign(new Error('failed'), {
      name: 'CapabilityError',
      code: 'E_FAIL',
      details: { retriable: false },
    });
    const resumedError: StructuredValue | ProgressType = step.resumeError(hostError, resumeOptions);
    const cancelled: StructuredValue | ProgressType = step.cancel();
    void capabilityName;
    void args;
    void snapshot;
    void restored;
    void restoredWithPolicy;
    void resumed;
    void resumedError;
    void cancelled;
  }

  void result;
}

void typecheck();

const typedError: JsliteErrorType = new JsliteError(errorKind, 'boom', new Error('cause'));
void typedError;
void runtimeLimits;
void resumeOptions;
void progressLoadOptions;
void errorKind;
void consoleCallbacks;

// @ts-expect-error symbols are not structured values
runtime.run({ inputs: { bad: Symbol('nope') } });

// @ts-expect-error capabilities must return structured values
const invalidCapability: Capability = () => Symbol('nope');

void invalidCapability;
