import type {
  Capability,
  CapabilityError,
  ExecutionOptions,
  Progress as ProgressType,
  StructuredValue,
} from 'jslite';

const { Jslite, Progress } = require('jslite') as typeof import('jslite');

const runtime = new Jslite('const response = fetch_data(seed); response + 1;', {
  inputs: ['seed'],
});

const executionOptions: ExecutionOptions = {
  inputs: {
    seed: 1,
    nested: { values: [1, 2, 3] },
  },
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

const capability: Capability = async (...args) => args[0] ?? structured;

async function typecheck(): Promise<void> {
  const result: StructuredValue = await runtime.run({
    ...executionOptions,
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
    const resumed: StructuredValue | ProgressType = step.resume(1);
    const hostError: CapabilityError = Object.assign(new Error('failed'), {
      name: 'CapabilityError',
      code: 'E_FAIL',
      details: { retriable: false },
    });
    const resumedError: StructuredValue | ProgressType = step.resumeError(hostError);
    void capabilityName;
    void args;
    void snapshot;
    void resumed;
    void resumedError;
  }

  void result;
}

void typecheck();

// @ts-expect-error symbols are not structured values
runtime.run({ inputs: { bad: Symbol('nope') } });

// @ts-expect-error capabilities must return structured values
const invalidCapability: Capability = () => Symbol('nope');

void invalidCapability;
