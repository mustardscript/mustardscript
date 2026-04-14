import type {
  JsonValue,
  PlaygroundError,
  PlaygroundRunResult,
  PlaygroundTraceEntry,
} from './iframe-protocol'
import type { PlaygroundScenario } from './scenarios'

interface WasmExports {
  memory: WebAssembly.Memory
  mustard_wasm_alloc(len: number): number
  mustard_wasm_free(ptr: number, len: number): void
  mustard_wasm_buffer_free(ptr: number): void
  mustard_wasm_start_json(ptr: number, len: number): number
  mustard_wasm_resume_json(ptr: number, len: number): number
}

interface WasmSuccessStep {
  status: 'completed' | 'suspended'
  value?: JsonValue
  handle?: number
  capability?: string
  args?: JsonValue[]
  metrics?: Record<string, number>
}

interface WasmFailureStep {
  status: 'error'
  error: PlaygroundError
}

type WasmStep = WasmSuccessStep | WasmFailureStep

const encoder = new TextEncoder()
const decoder = new TextDecoder()

let wasmExportsPromise: Promise<WasmExports> | null = null

function loadPlaygroundWasm(): Promise<WasmExports> {
  if (!wasmExportsPromise) {
    wasmExportsPromise = (async () => {
      const response = await fetch('/mustard-playground.wasm')
      if (!response.ok) {
        throw new Error(`failed to load mustard playground wasm: ${response.status}`)
      }
      const bytes = await response.arrayBuffer()
      const module = await WebAssembly.instantiate(bytes, {
        env: {
          mustard_now_millis: () => Date.now(),
          mustard_random_f64: () => Math.random(),
        },
      })
      return module.instance.exports as unknown as WasmExports
    })()
  }
  return wasmExportsPromise
}

function buildHelpers(scenario: PlaygroundScenario) {
  return Object.fromEntries(
    Object.entries(scenario.helperSources).map(([name, source]) => [
      name,
      (...args: JsonValue[]) =>
        new Function(
          'args',
          'context',
          `"use strict";\n${source}`,
        )(args, scenario.context) as JsonValue,
    ]),
  ) as Record<string, (...args: JsonValue[]) => JsonValue>
}

function decodeResponse(exports: WasmExports, ptr: number): WasmStep {
  if (ptr === 0) {
    throw new Error('wasm returned a null response pointer')
  }
  const view = new DataView(exports.memory.buffer, ptr, 4)
  const len = view.getUint32(0, true)
  const bytes = new Uint8Array(exports.memory.buffer, ptr + 4, len)
  const response = JSON.parse(decoder.decode(bytes)) as WasmStep
  exports.mustard_wasm_buffer_free(ptr)
  return response
}

function callWasmJson(
  exports: WasmExports,
  method: 'mustard_wasm_start_json' | 'mustard_wasm_resume_json',
  payload: unknown,
): WasmStep {
  const bytes = encoder.encode(JSON.stringify(payload))
  const ptr = exports.mustard_wasm_alloc(bytes.length)
  if (ptr === 0) {
    throw new Error('failed to allocate wasm request buffer')
  }
  new Uint8Array(exports.memory.buffer, ptr, bytes.length).set(bytes)
  try {
    const responsePtr = exports[method](ptr, bytes.length)
    return decodeResponse(exports, responsePtr)
  } finally {
    exports.mustard_wasm_free(ptr, bytes.length)
  }
}

function serializeHostError(error: unknown): PlaygroundError {
  if (error instanceof Error) {
    return {
      name: error.name || 'Error',
      message: error.message || 'unknown host error',
    }
  }
  return {
    name: 'Error',
    message: typeof error === 'string' ? error : JSON.stringify(error),
  }
}

export async function runMustardScenario(
  scenario: PlaygroundScenario,
  code: string,
): Promise<PlaygroundRunResult> {
  const exports = await loadPlaygroundWasm()
  const helpers = buildHelpers(scenario)
  const trace: PlaygroundTraceEntry[] = []
  const startedAt = performance.now()

  let step = callWasmJson(exports, 'mustard_wasm_start_json', {
    code,
    inputs: scenario.inputs,
    capabilities: Object.keys(scenario.helperSources),
    limits: {
      instruction_budget: 150_000,
      heap_limit_bytes: 4 * 1024 * 1024,
      allocation_budget: 40_000,
      call_depth_limit: 128,
      max_outstanding_host_calls: 8,
    },
  })

  while (step.status === 'suspended') {
    const capability = step.capability ?? ''
    const args = step.args ?? []
    const handle = step.handle
    trace.push({
      capability,
      args,
    })

    const helper = helpers[capability]
    if (!helper) {
      return {
        ok: false,
        elapsedMs: performance.now() - startedAt,
        error: {
          name: 'MustardHostError',
          message: `missing helper for capability \`${capability}\``,
        },
        trace,
      }
    }

    try {
      const value = helper(...args)
      step = callWasmJson(exports, 'mustard_wasm_resume_json', {
        handle,
        payload: {
          type: 'value',
          value,
        },
      })
    } catch (error) {
      step = callWasmJson(exports, 'mustard_wasm_resume_json', {
        handle,
        payload: {
          type: 'error',
          error: serializeHostError(error),
        },
      })
    }
  }

  if (step.status === 'error') {
    return {
      ok: false,
      elapsedMs: performance.now() - startedAt,
      error: step.error,
      trace,
    }
  }

  return {
    ok: true,
    elapsedMs: performance.now() - startedAt,
    result: step.value,
    trace,
  }
}
