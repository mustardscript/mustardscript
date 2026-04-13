# Mustard Executor

`MustardExecutor` is a Node-side queue-oriented API built on top of the existing
`Mustard` and `Progress` primitives.

It exists for hosts that need to manage many resumable guest executions without
manually orchestrating `start()`, `Progress.dump()`, `Progress.load()`, and
`resume()` for every job.

## Scope

The executor layer:

- keeps guest/runtime semantics in Rust
- keeps capability implementations in the host
- treats progress snapshots as durable queue state
- reuses the existing snapshot policy and validation model

It does not:

- serialize host functions or live host futures
- change the same-version-only snapshot contract
- provide hard kill semantics for in-process host work

## Public API

```ts
class MustardExecutor<TInput extends Record<string, StructuredValue>, TResult extends StructuredValue> {
  constructor(options: {
    program: Mustard;
    capabilities: Record<string, Capability>;
    snapshotKey?: string | Buffer | Uint8Array;
    store: MustardExecutorStore<TInput, TResult>;
    limits?: RuntimeLimits;
  });

  enqueue(input: TInput, options?: { jobId?: string }): Promise<string>;
  get(jobId: string): Promise<MustardJobRecord<TInput, TResult> | null>;
  cancel(jobId: string): Promise<void>;
  runWorker(options?: {
    maxConcurrentJobs?: number;
    signal?: AbortSignal;
    drain?: boolean;
  }): Promise<void>;
}
```

`enqueue()` accepts the exact `inputs` object that will later be passed into
`program.start({ inputs, ... })`.

## Job States

- `queued`: durable input exists, no worker is executing it
- `running`: a worker is actively starting or resuming guest execution
- `waiting`: guest execution suspended on a host capability and durable
  progress exists
- `completed`: job produced a final structured result
- `failed`: job terminated with a guest-safe error record
- `cancelled`: host requested cancellation and the job terminated through the
  explicit cancellation path

## Store Contract

The executor depends on a pluggable store. The store is the source of truth for
durable state; worker-local memory is only transient cache.

```ts
interface MustardExecutorStore<TInput, TResult> {
  enqueue(record: MustardJobRecord<TInput, TResult>): Promise<{
    jobId: string;
    inserted: boolean;
  }>;
  get(jobId: string): Promise<MustardJobRecord<TInput, TResult> | null>;
  claimRunnable(limit: number, workerId: string, now: number): Promise<string[]>;
  releaseClaim(jobId: string, workerId: string): Promise<void>;
  update(jobId: string, patch: Partial<MustardJobRecord<TInput, TResult>>): Promise<void>;
  saveProgress(jobId: string, progress: SerializedProgress): Promise<void>;
  loadProgress(jobId: string): Promise<SerializedProgress | null>;
  deleteProgress(jobId: string): Promise<void>;
  requestCancel(jobId: string): Promise<'cancelled' | 'requested' | 'ignored'>;
  consumeCancel(jobId: string): Promise<boolean>;
}
```

Required semantics:

- `enqueue()` is idempotent by `jobId`
- `claimRunnable()` must not hand the same job to two workers at once
- `update()` must fail closed on invalid state transitions
- a `waiting` job must have matching durable progress
- terminal jobs must not remain claimable

## Worker Invariants

Workers must preserve these invariants:

- guest execution starts only from durable `input` or a validated stored
  `Progress` blob
- capability handlers always come from the current host process, never from
  durable state
- every durable restore uses explicit `capabilities`, `limits`, and
  `snapshotKey`
- stored progress must be bound to its owning `jobId`; this implementation
  derives a per-job snapshot key from the configured executor `snapshotKey`
  before every `start()` and `Progress.load(...)`
- progress is persisted before a job is marked `waiting`
- terminal state is persisted before worker-local state is discarded
- invalid snapshots, missing capabilities, and missing stored progress fail the
  job closed

## Cancellation

Cancellation stays cooperative and honest:

- `queued` jobs can move directly to `cancelled`
- `waiting` jobs cancel by resuming the stored progress through
  `Progress.cancel()`
- already-running host handlers are not hard-preempted in-process; the executor
  can only honor cancellation before or after the host handler returns

## Initial Implementation

The first implementation in this repository includes:

- `MustardExecutor`
- `InMemoryMustardExecutorStore`
- bounded worker concurrency with `maxConcurrentJobs`
- `drain` mode for tests and batch scripts
- durable queued/running/waiting/completed/failed/cancelled state in the
  in-memory store

Future durable stores can extend the same public executor API without changing
the guest runtime model.
