import { useRef, useState } from 'react'

import type { PlaygroundRunResult } from '../lib/playground/iframe-protocol'
import { runMustardScenario } from '../lib/playground/mustard-wasm'
import { getScenarioById, playgroundScenarios } from '../lib/playground/scenarios'
import { runVanillaScenario } from '../lib/playground/vanilla-iframe'
import { PlaygroundEditor } from './PlaygroundEditor'
import { PlaygroundOutput } from './PlaygroundOutput'
import { PlaygroundScenarioTabs } from './PlaygroundScenarioTabs'

function thrownErrorToResult(error: unknown): PlaygroundRunResult {
  const message =
    error instanceof Error
      ? error.message
      : typeof error === 'string'
        ? error
        : 'unknown execution error'

  return {
    ok: false,
    elapsedMs: 0,
    error: {
      name: error instanceof Error ? error.name : 'Error',
      message,
    },
    trace: [],
  }
}

export function PlaygroundSection() {
  const [activeScenarioId, setActiveScenarioId] = useState(playgroundScenarios[0].id)
  const [mustardCode, setMustardCode] = useState(playgroundScenarios[0].mustardTemplate)
  const [vanillaCode, setVanillaCode] = useState(playgroundScenarios[0].vanillaTemplate)
  const [mustardResult, setMustardResult] = useState<PlaygroundRunResult | null>(null)
  const [vanillaResult, setVanillaResult] = useState<PlaygroundRunResult | null>(null)
  const [isMustardRunning, setIsMustardRunning] = useState(false)
  const [isVanillaRunning, setIsVanillaRunning] = useState(false)

  const iframeRef = useRef<HTMLIFrameElement>(null)
  const scenario = getScenarioById(activeScenarioId)

  async function handleRunComparison() {
    const iframe = iframeRef.current
    if (!iframe) {
      return
    }

    setIsMustardRunning(true)
    setIsVanillaRunning(true)

    const [mustardOutcome, vanillaOutcome] = await Promise.allSettled([
      runMustardScenario(scenario, mustardCode),
      runVanillaScenario(iframe, scenario, vanillaCode),
    ])

    setMustardResult(
      mustardOutcome.status === 'fulfilled'
        ? mustardOutcome.value
        : thrownErrorToResult(mustardOutcome.reason),
    )
    setVanillaResult(
      vanillaOutcome.status === 'fulfilled'
        ? vanillaOutcome.value
        : thrownErrorToResult(vanillaOutcome.reason),
    )
    setIsMustardRunning(false)
    setIsVanillaRunning(false)
  }

  function handleReset() {
    setMustardCode(scenario.mustardTemplate)
    setVanillaCode(scenario.vanillaTemplate)
    setMustardResult(null)
    setVanillaResult(null)
  }

  function handleSelectScenario(nextScenarioId: string) {
    const nextScenario = getScenarioById(nextScenarioId)
    setActiveScenarioId(nextScenarioId)
    setMustardCode(nextScenario.mustardTemplate)
    setVanillaCode(nextScenario.vanillaTemplate)
    setMustardResult(null)
    setVanillaResult(null)
  }

  return (
    <section className="relative overflow-hidden px-6 py-24" id="playground">
      <div className="mx-auto max-w-7xl">
        <div className="mb-10 flex flex-col gap-5 lg:flex-row lg:items-end lg:justify-between">
          <div className="max-w-2xl">
            <p className="mb-3 font-mono text-xs uppercase tracking-[0.28em] text-black/50">
              Live Playground
            </p>
            <h2 className="font-heading text-4xl font-bold tracking-tight text-black sm:text-5xl">
              MustardScript vs vanilla JavaScript
            </h2>
            <p className="mt-4 text-lg leading-8 text-black/60">
              Same scenario, same inputs, same helpers. Mustard runs through the Rust core in
              browser WASM. Vanilla JavaScript runs inside a sandboxed iframe.
            </p>
          </div>

          <div className="flex flex-wrap gap-3">
            <button
              type="button"
              onClick={() => void handleRunComparison()}
              className="rounded-full bg-black px-5 py-3 text-sm font-semibold text-[#F5D563] shadow-lg shadow-black/20 transition hover:-translate-y-0.5"
            >
              Run Comparison
            </button>
            <button
              type="button"
              onClick={handleReset}
              className="rounded-full border border-black/15 bg-white/35 px-5 py-3 text-sm font-semibold text-black/70 transition hover:border-black/35 hover:bg-white/55"
            >
              Reset
            </button>
          </div>
        </div>

        <div className="mb-8 rounded-[28px] border border-black/10 bg-white/40 p-6 shadow-[0_24px_80px_rgba(0,0,0,0.08)] backdrop-blur">
          <div className="mb-5">
            <PlaygroundScenarioTabs
              scenarios={playgroundScenarios}
              activeScenarioId={activeScenarioId}
              onSelect={handleSelectScenario}
            />
          </div>
          <div className="grid gap-6 lg:grid-cols-[1.4fr_1fr]">
            <div>
              <h3 className="font-heading text-xl font-bold text-black">{scenario.label}</h3>
              <p className="mt-2 text-black/60">{scenario.description}</p>
            </div>
            <div className="rounded-[20px] border border-black/10 bg-black/5 p-4">
              <p className="mb-2 font-semibold text-black/65">Shared Inputs</p>
              <pre className="overflow-x-auto font-mono text-xs leading-6 text-black/75">
                {JSON.stringify(scenario.inputs, null, 2)}
              </pre>
            </div>
          </div>
        </div>

        <div className="grid gap-6 xl:grid-cols-2">
          <PlaygroundEditor
            title="MustardScript"
            subtitle="Sandboxed guest code executed by the Rust runtime compiled to browser WASM."
            filename={scenario.mustardFilename}
            value={mustardCode}
            onChange={setMustardCode}
          />
          <PlaygroundEditor
            title="Vanilla JavaScript"
            subtitle="Reference implementation executed inside a sandboxed iframe."
            filename={scenario.vanillaFilename}
            value={vanillaCode}
            onChange={setVanillaCode}
          />
        </div>

        <div className="mt-6 grid gap-6 xl:grid-cols-[1fr_1fr_0.8fr]">
          <PlaygroundOutput
            title="Mustard Output"
            testId="playground-output-mustard"
            tone="mustard"
            result={mustardResult}
            isRunning={isMustardRunning}
          />
          <PlaygroundOutput
            title="Vanilla Output"
            testId="playground-output-vanilla"
            tone="vanilla"
            result={vanillaResult}
            isRunning={isVanillaRunning}
          />
          <section className="rounded-[24px] border border-black/10 bg-white/35 p-5 shadow-xl">
            <h3 className="font-heading text-lg font-bold text-black">Expected Result</h3>
            <p className="mt-2 text-sm text-black/55">
              Both panes should converge on this shared output when the scenario logic is correct.
            </p>
            <pre className="mt-4 overflow-x-auto rounded-2xl bg-black/85 p-4 font-mono text-xs leading-6 text-[#F8F2DE]">
              {JSON.stringify(scenario.expectedResult, null, 2)}
            </pre>
          </section>
        </div>
      </div>

      <iframe
        ref={iframeRef}
        title="Vanilla playground sandbox"
        src="/playground-iframe.html"
        sandbox="allow-scripts"
        className="hidden"
      />
    </section>
  )
}
