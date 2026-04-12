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

export interface ExecutionOptions {
  inputs?: Record<string, StructuredValue>;
  capabilities?: Record<string, Capability>;
  console?: ConsoleCallbacks;
  limits?: RuntimeLimits;
  signal?: AbortSignal;
}

export interface ResumeOptions {
  signal?: AbortSignal;
}

export interface ProgressLoadOptions {
  capabilities?: Record<string, Capability>;
  console?: ConsoleCallbacks;
  limits?: RuntimeLimits;
}

export interface RuntimeLimits {
  instructionBudget?: number;
  heapLimitBytes?: number;
  allocationBudget?: number;
  callDepthLimit?: number;
  maxOutstandingHostCalls?: number;
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
  token?: string;
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
