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

export interface ProgressLoadOptions {
  capabilities?: Record<string, Capability>;
  console?: ConsoleCallbacks;
  limits?: RuntimeLimits;
  snapshotKey?: SnapshotKey;
}

export interface RuntimeLimits {
  instructionBudget?: number;
  heapLimitBytes?: number;
  allocationBudget?: number;
  callDepthLimit?: number;
  maxOutstandingHostCalls?: number;
}

export type JsliteJobId = string;

export type JsliteJobState =
  | 'queued'
  | 'running'
  | 'waiting'
  | 'completed'
  | 'failed'
  | 'cancelled';

export interface JsliteJobError {
  name: string;
  message: string;
  code?: string;
  details?: StructuredValue;
}

export interface JsliteJobRecord<
  TInput extends Record<string, StructuredValue> = Record<string, StructuredValue>,
  TResult extends StructuredValue = StructuredValue,
> {
  jobId: JsliteJobId;
  state: JsliteJobState;
  input: TInput;
  capability?: string;
  args?: StructuredValue[];
  result?: TResult;
  error?: JsliteJobError;
  attempts: number;
  createdAt: number;
  updatedAt: number;
}

export interface PersistedProgress extends SerializedProgress {}

export interface JsliteExecutorStore<
  TInput extends Record<string, StructuredValue> = Record<string, StructuredValue>,
  TResult extends StructuredValue = StructuredValue,
> {
  enqueue(record: JsliteJobRecord<TInput, TResult>): Promise<{
    jobId: JsliteJobId;
    inserted: boolean;
  }>;
  get(jobId: JsliteJobId): Promise<JsliteJobRecord<TInput, TResult> | null>;
  claimRunnable(limit: number, workerId: string, now: number): Promise<JsliteJobId[]>;
  releaseClaim(jobId: JsliteJobId, workerId: string): Promise<void>;
  update(
    jobId: JsliteJobId,
    patch: Partial<JsliteJobRecord<TInput, TResult>>,
  ): Promise<void>;
  saveProgress(jobId: JsliteJobId, progress: PersistedProgress): Promise<void>;
  loadProgress(jobId: JsliteJobId): Promise<PersistedProgress | null>;
  deleteProgress(jobId: JsliteJobId): Promise<void>;
  requestCancel(jobId: JsliteJobId): Promise<'cancelled' | 'requested' | 'ignored'>;
  consumeCancel(jobId: JsliteJobId): Promise<boolean>;
}

export interface JsliteExecutorOptions<
  TInput extends Record<string, StructuredValue> = Record<string, StructuredValue>,
  TResult extends StructuredValue = StructuredValue,
> {
  program: Jslite;
  capabilities: Record<string, Capability>;
  snapshotKey?: SnapshotKey;
  store: JsliteExecutorStore<TInput, TResult>;
  limits?: RuntimeLimits;
}

export interface JsliteExecutorRunWorkerOptions {
  maxConcurrentJobs?: number;
  signal?: AbortSignal;
  drain?: boolean;
}

export type JsliteErrorKind =
  | 'Parse'
  | 'Validation'
  | 'Runtime'
  | 'Limit'
  | 'Serialization';

export class JsliteError extends Error {
  constructor(kind: JsliteErrorKind, message: string, cause?: unknown);

  readonly kind: JsliteErrorKind;
  readonly cause?: unknown;
}

export interface SerializedProgress {
  capability: string;
  args: StructuredValue[];
  snapshot: Buffer;
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

  static load(state: SerializedProgress, options?: ProgressLoadOptions): Progress;
}

export class Jslite {
  constructor(code: string, options?: CompileOptions);

  run(options?: ExecutionOptions): Promise<StructuredValue>;
  start(options?: ExecutionOptions): StructuredValue | Progress;
  dump(): Buffer;

  static load(buffer: Buffer): Jslite;
}

export class InMemoryJsliteExecutorStore<
  TInput extends Record<string, StructuredValue> = Record<string, StructuredValue>,
  TResult extends StructuredValue = StructuredValue,
> implements JsliteExecutorStore<TInput, TResult> {
  enqueue(record: JsliteJobRecord<TInput, TResult>): Promise<{
    jobId: JsliteJobId;
    inserted: boolean;
  }>;
  get(jobId: JsliteJobId): Promise<JsliteJobRecord<TInput, TResult> | null>;
  claimRunnable(limit: number, workerId: string, now: number): Promise<JsliteJobId[]>;
  releaseClaim(jobId: JsliteJobId, workerId: string): Promise<void>;
  update(
    jobId: JsliteJobId,
    patch: Partial<JsliteJobRecord<TInput, TResult>>,
  ): Promise<void>;
  saveProgress(jobId: JsliteJobId, progress: PersistedProgress): Promise<void>;
  loadProgress(jobId: JsliteJobId): Promise<PersistedProgress | null>;
  deleteProgress(jobId: JsliteJobId): Promise<void>;
  requestCancel(jobId: JsliteJobId): Promise<'cancelled' | 'requested' | 'ignored'>;
  consumeCancel(jobId: JsliteJobId): Promise<boolean>;
}

export class JsliteExecutor<
  TInput extends Record<string, StructuredValue> = Record<string, StructuredValue>,
  TResult extends StructuredValue = StructuredValue,
> {
  constructor(options: JsliteExecutorOptions<TInput, TResult>);

  enqueue(input: TInput, options?: { jobId?: JsliteJobId }): Promise<JsliteJobId>;
  get(jobId: JsliteJobId): Promise<JsliteJobRecord<TInput, TResult> | null>;
  cancel(jobId: JsliteJobId): Promise<void>;
  runWorker(options?: JsliteExecutorRunWorkerOptions): Promise<void>;
}
