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

export interface CompileOptions {
  inputs?: string[];
}

export interface ExecutionOptions {
  inputs?: Record<string, StructuredValue>;
  capabilities?: Record<string, Capability>;
}

export interface SerializedProgress {
  capability: string;
  args: StructuredValue[];
  snapshot: Buffer;
}

export class Progress {
  readonly capability: string;
  readonly args: StructuredValue[];
  readonly snapshot: Buffer;

  dump(): SerializedProgress;
  resume(value: StructuredValue): StructuredValue | Progress;
  resumeError(error: unknown): StructuredValue | Progress;

  static load(state: SerializedProgress): Progress;
}

export class Jslite {
  constructor(code: string, options?: CompileOptions);

  run(options?: ExecutionOptions): Promise<StructuredValue>;
  start(options?: ExecutionOptions): StructuredValue | Progress;
  dump(): Buffer;

  static load(buffer: Buffer): Jslite;
}
