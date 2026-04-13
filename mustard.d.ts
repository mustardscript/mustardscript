export type StructuredValue =
  | undefined
  | null
  | boolean
  | number
  | string
  | StructuredValue[]
  | { [key: string]: StructuredValue };

export interface CapabilityError extends Error {
  code?: string;
  details?: StructuredValue;
}

export type Capability = (
  ...args: StructuredValue[]
) => StructuredValue | Promise<StructuredValue>;

export interface ConsoleCallbacks {
  log?: Capability;
  warn?: Capability;
  error?: Capability;
}

export interface CompileOptions {
  inputs?: string[];
}

export type SnapshotKey = string | Buffer | Uint8Array;

export interface ExecutionOptions {
  inputs?: Record<string, StructuredValue>;
  capabilities?: Record<string, Capability>;
  console?: ConsoleCallbacks;
  limits?: RuntimeLimits;
  signal?: AbortSignal;
  snapshotKey?: SnapshotKey;
}

export interface ResumeOptions {
  signal?: AbortSignal;
}

export interface ProgressLoadOptionsBase {
  limits: RuntimeLimits;
  snapshotKey: SnapshotKey;
}

export type ProgressLoadOptions =
  | (ProgressLoadOptionsBase & {
      capabilities: Record<string, Capability>;
      console?: ConsoleCallbacks;
    })
  | (ProgressLoadOptionsBase & {
      capabilities?: Record<string, Capability>;
      console: ConsoleCallbacks;
    });

export interface RuntimeLimits {
  instructionBudget?: number;
  heapLimitBytes?: number;
  allocationBudget?: number;
  callDepthLimit?: number;
  maxOutstandingHostCalls?: number;
}

export type MustardJobId = string;

export type MustardJobState =
  | 'queued'
  | 'running'
  | 'waiting'
  | 'completed'
  | 'failed'
  | 'cancelled';

export interface MustardJobError {
  name: string;
  message: string;
  code?: string;
  details?: StructuredValue;
}

export interface MustardJobRecord<
  TInput extends Record<string, StructuredValue> = Record<string, StructuredValue>,
  TResult extends StructuredValue = StructuredValue,
> {
  jobId: MustardJobId;
  state: MustardJobState;
  input: TInput;
  capability?: string;
  args?: StructuredValue[];
  result?: TResult;
  error?: MustardJobError;
  attempts: number;
  createdAt: number;
  updatedAt: number;
}

export interface PersistedProgress extends SerializedProgress {}

export interface MustardExecutorStore<
  TInput extends Record<string, StructuredValue> = Record<string, StructuredValue>,
  TResult extends StructuredValue = StructuredValue,
> {
  enqueue(record: MustardJobRecord<TInput, TResult>): Promise<{
    jobId: MustardJobId;
    inserted: boolean;
  }>;
  get(jobId: MustardJobId): Promise<MustardJobRecord<TInput, TResult> | null>;
  claimRunnable(limit: number, workerId: string, now: number): Promise<MustardJobId[]>;
  releaseClaim(jobId: MustardJobId, workerId: string): Promise<void>;
  update(
    jobId: MustardJobId,
    patch: Partial<MustardJobRecord<TInput, TResult>>,
  ): Promise<void>;
  saveProgress(jobId: MustardJobId, progress: PersistedProgress): Promise<void>;
  loadProgress(jobId: MustardJobId): Promise<PersistedProgress | null>;
  deleteProgress(jobId: MustardJobId): Promise<void>;
  requestCancel(jobId: MustardJobId): Promise<'cancelled' | 'requested' | 'ignored'>;
  consumeCancel(jobId: MustardJobId): Promise<boolean>;
}

export interface MustardExecutorOptions<
  TInput extends Record<string, StructuredValue> = Record<string, StructuredValue>,
  TResult extends StructuredValue = StructuredValue,
> {
  program: Mustard;
  capabilities: Record<string, Capability>;
  snapshotKey?: SnapshotKey;
  store: MustardExecutorStore<TInput, TResult>;
  limits?: RuntimeLimits;
}

export interface MustardExecutorRunWorkerOptions {
  maxConcurrentJobs?: number;
  signal?: AbortSignal;
  drain?: boolean;
}

export type MustardErrorKind =
  | 'Parse'
  | 'Validation'
  | 'Runtime'
  | 'Limit'
  | 'Serialization';

export class MustardError extends Error {
  constructor(kind: MustardErrorKind, message: string, cause?: unknown);

  readonly kind: MustardErrorKind;
  readonly cause?: unknown;
}

export interface SerializedProgress {
  capability: string;
  args: StructuredValue[];
  snapshot: Buffer;
  snapshot_id: string;
  snapshot_key_digest: string;
  token: string;
}

export class Progress {
  readonly capability: string;
  readonly args: StructuredValue[];
  readonly snapshot: Buffer;

  dump(): SerializedProgress;
  resume(value: StructuredValue, options?: ResumeOptions): StructuredValue | Progress;
  resumeError(error: unknown, options?: ResumeOptions): StructuredValue | Progress;
  cancel(): StructuredValue | Progress;

  static load(state: SerializedProgress, options: ProgressLoadOptions): Progress;
}

export class Mustard {
  constructor(code: string, options?: CompileOptions);

  run(options?: ExecutionOptions): Promise<StructuredValue>;
  start(options?: ExecutionOptions): StructuredValue | Progress;
  dump(): Buffer;

  static validateProgram(code: string): void;
  static load(buffer: Buffer): Mustard;
}

export class InMemoryMustardExecutorStore<
  TInput extends Record<string, StructuredValue> = Record<string, StructuredValue>,
  TResult extends StructuredValue = StructuredValue,
> implements MustardExecutorStore<TInput, TResult> {
  enqueue(record: MustardJobRecord<TInput, TResult>): Promise<{
    jobId: MustardJobId;
    inserted: boolean;
  }>;
  get(jobId: MustardJobId): Promise<MustardJobRecord<TInput, TResult> | null>;
  claimRunnable(limit: number, workerId: string, now: number): Promise<MustardJobId[]>;
  releaseClaim(jobId: MustardJobId, workerId: string): Promise<void>;
  update(
    jobId: MustardJobId,
    patch: Partial<MustardJobRecord<TInput, TResult>>,
  ): Promise<void>;
  saveProgress(jobId: MustardJobId, progress: PersistedProgress): Promise<void>;
  loadProgress(jobId: MustardJobId): Promise<PersistedProgress | null>;
  deleteProgress(jobId: MustardJobId): Promise<void>;
  requestCancel(jobId: MustardJobId): Promise<'cancelled' | 'requested' | 'ignored'>;
  consumeCancel(jobId: MustardJobId): Promise<boolean>;
}

export class MustardExecutor<
  TInput extends Record<string, StructuredValue> = Record<string, StructuredValue>,
  TResult extends StructuredValue = StructuredValue,
> {
  constructor(options: MustardExecutorOptions<TInput, TResult>);

  enqueue(input: TInput, options?: { jobId?: MustardJobId }): Promise<MustardJobId>;
  get(jobId: MustardJobId): Promise<MustardJobRecord<TInput, TResult> | null>;
  cancel(jobId: MustardJobId): Promise<void>;
  runWorker(options?: MustardExecutorRunWorkerOptions): Promise<void>;
}
