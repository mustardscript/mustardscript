export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue }

export interface PlaygroundError {
  name: string
  message: string
  span?: {
    start: number
    end: number
  }
}

export interface PlaygroundTraceEntry {
  capability: string
  args: JsonValue[]
}

export interface PlaygroundRunResult {
  ok: boolean
  elapsedMs: number
  result?: JsonValue
  error?: PlaygroundError
  trace: PlaygroundTraceEntry[]
}

export interface IframeRunRequest {
  type: 'playground-run'
  requestId: string
  code: string
  helperSources: Record<string, string>
  context: JsonValue
  inputs: Record<string, JsonValue>
}

export interface IframeRunResponse {
  type: 'playground-result'
  requestId: string
  ok: boolean
  elapsedMs: number
  result?: JsonValue
  error?: PlaygroundError
}

export interface IframeReadyMessage {
  type: 'playground-ready'
}

export type IframeMessage = IframeRunResponse | IframeReadyMessage
