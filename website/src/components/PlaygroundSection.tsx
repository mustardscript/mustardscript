import { useCallback, useMemo, useRef, useState } from 'react'

import type { PlaygroundRunResult } from '../lib/playground/iframe-protocol'
import { runMustardScenario } from '../lib/playground/mustard-wasm'
import { getScenarioById, playgroundScenarios } from '../lib/playground/scenarios'
import { runVanillaScenario } from '../lib/playground/vanilla-iframe'
import { highlightCode } from './highlight'
import { PlaygroundOutput } from './PlaygroundOutput'

const CODE_FONT =
  'font-mono text-[12.5px] leading-5 tracking-[0]'

function longestCommonSubstring(a: string, b: string) {
  const m = a.length
  const n = b.length
  if (m === 0 || n === 0) return { aStart: 0, bStart: 0, length: 0 }
  let best = 0
  let bestEndA = 0
  let bestEndB = 0
  let prev = new Array(n + 1).fill(0) as number[]
  let curr = new Array(n + 1).fill(0) as number[]
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      if (a[i - 1] === b[j - 1]) {
        curr[j] = prev[j - 1] + 1
        if (curr[j] > best) {
          best = curr[j]
          bestEndA = i
          bestEndB = j
        }
      } else {
        curr[j] = 0
      }
    }
    const tmp = prev
    prev = curr
    curr = tmp
    for (let j = 0; j <= n; j++) curr[j] = 0
  }
  return {
    aStart: bestEndA - best,
    bStart: bestEndB - best,
    length: best,
  }
}

interface DiffLineProps {
  value: string
  sameStart: number
  sameLength: number
  accent: string
  ariaLabel: string
  onChange: (v: string) => void
}

function DiffLine({ value, sameStart, sameLength, accent, ariaLabel, onChange }: DiffLineProps) {
  const preRef = useRef<HTMLPreElement>(null)
  const start = sameLength > 0 ? Math.min(sameStart, value.length) : value.length
  const end = Math.min(start + sameLength, value.length)
  const prefix = value.slice(0, start)
  const same = value.slice(start, end)
  const suffix = value.slice(end)

  function handleScroll(event: React.UIEvent<HTMLInputElement>) {
    if (preRef.current) preRef.current.scrollLeft = event.currentTarget.scrollLeft
  }

  return (
    <div className="relative min-w-0 flex-1 overflow-hidden">
      <pre
        ref={preRef}
        aria-hidden
        className={`${CODE_FONT} pointer-events-none m-0 whitespace-pre px-2 py-1.5`}
      >
        <span style={{ color: accent }}>{prefix}</span>
        <span style={{ color: '#F8F2DE', opacity: 0.38 }}>{same}</span>
        <span style={{ color: accent }}>{suffix}</span>
      </pre>
      <input
        aria-label={ariaLabel}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onScroll={handleScroll}
        spellCheck={false}
        type="text"
        className={`${CODE_FONT} absolute inset-0 block w-full bg-transparent text-transparent caret-[#F5D563] outline-none px-2 py-1.5`}
        style={{ WebkitTextFillColor: 'transparent' }}
      />
    </div>
  )
}

interface HighlightedCodeProps {
  value: string
  onChange: (v: string) => void
  ariaLabel: string
  rows?: number
  singleLine?: boolean
  paddingClass?: string
}

function HighlightedCode({
  value,
  onChange,
  ariaLabel,
  rows,
  singleLine,
  paddingClass = 'px-4 py-2',
}: HighlightedCodeProps) {
  const preRef = useRef<HTMLPreElement>(null)
  const html = highlightCode(value, 'js')
  const safeHtml = html.length > 0 ? html : '&nbsp;'

  function handleScroll(event: React.UIEvent<HTMLTextAreaElement | HTMLInputElement>) {
    if (!preRef.current) return
    const target = event.currentTarget
    preRef.current.scrollLeft = target.scrollLeft
    if (!singleLine && target instanceof HTMLTextAreaElement) {
      preRef.current.scrollTop = target.scrollTop
    }
  }

  if (singleLine) {
    return (
      <div className="relative min-w-0 flex-1 overflow-hidden">
        <pre
          ref={preRef}
          aria-hidden
          className={`${CODE_FONT} ${paddingClass} pointer-events-none m-0 whitespace-pre text-[#F8F2DE]`}
          dangerouslySetInnerHTML={{ __html: safeHtml }}
        />
        <input
          aria-label={ariaLabel}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onScroll={handleScroll}
          spellCheck={false}
          type="text"
          className={`${CODE_FONT} ${paddingClass} absolute inset-0 block w-full bg-transparent text-transparent caret-[#F5D563] outline-none`}
          style={{ WebkitTextFillColor: 'transparent' }}
        />
      </div>
    )
  }

  return (
    <div className="relative">
      <pre
        ref={preRef}
        aria-hidden
        className={`${CODE_FONT} ${paddingClass} pointer-events-none absolute inset-0 m-0 overflow-hidden whitespace-pre text-[#F8F2DE]`}
        dangerouslySetInnerHTML={{ __html: safeHtml }}
      />
      <textarea
        aria-label={ariaLabel}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onScroll={handleScroll}
        spellCheck={false}
        rows={rows}
        wrap="off"
        className={`${CODE_FONT} ${paddingClass} relative block max-h-[22rem] min-h-[11rem] w-full resize-none overflow-auto bg-transparent text-transparent caret-[#F5D563] outline-none`}
        style={{ WebkitTextFillColor: 'transparent' }}
      />
    </div>
  )
}

function flattenTail(tail: string) {
  return tail.replace(/\s+/g, ' ').trim()
}

function splitSharedTail(a: string, b: string) {
  const aLines = a.split('\n')
  const bLines = b.split('\n')
  const max = Math.min(aLines.length, bLines.length)
  let i = 0
  while (i < max && aLines[i] === bLines[i]) i++
  return {
    shared: aLines.slice(0, i).join('\n'),
    mustardTail: flattenTail(aLines.slice(i).join('\n')),
    vanillaTail: flattenTail(bLines.slice(i).join('\n')),
  }
}

function stitch(shared: string, tail: string) {
  if (!shared) return tail
  if (!tail) return shared
  return `${shared}\n${tail}`
}

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
  const scenario = getScenarioById(activeScenarioId)

  const parts = useMemo(
    () => splitSharedTail(scenario.mustardTemplate, scenario.vanillaTemplate),
    [scenario.mustardTemplate, scenario.vanillaTemplate],
  )

  const [shared, setShared] = useState(parts.shared)
  const [mustardTail, setMustardTail] = useState(parts.mustardTail)
  const [vanillaTail, setVanillaTail] = useState(parts.vanillaTail)
  const [mustardResult, setMustardResult] = useState<PlaygroundRunResult | null>(null)
  const [vanillaResult, setVanillaResult] = useState<PlaygroundRunResult | null>(null)
  const [isRunning, setIsRunning] = useState(false)
  const [inputsOpen, setInputsOpen] = useState(false)

  const iframeRef = useRef<HTMLIFrameElement>(null)
  const frameRef = useRef<HTMLDivElement>(null)

  function handleSelectScenario(nextId: string) {
    if (nextId === activeScenarioId) return
    const next = getScenarioById(nextId)
    const nextParts = splitSharedTail(next.mustardTemplate, next.vanillaTemplate)
    setActiveScenarioId(nextId)
    setShared(nextParts.shared)
    setMustardTail(nextParts.mustardTail)
    setVanillaTail(nextParts.vanillaTail)
    setMustardResult(null)
    setVanillaResult(null)
    setInputsOpen(false)
  }

  const handleRun = useCallback(async () => {
    const iframe = iframeRef.current
    if (!iframe) return
    setIsRunning(true)
    const mustardCode = stitch(shared, mustardTail)
    const vanillaCode = stitch(shared, vanillaTail)
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
    setIsRunning(false)
  }, [scenario, shared, mustardTail, vanillaTail])

  function handleReset() {
    setShared(parts.shared)
    setMustardTail(parts.mustardTail)
    setVanillaTail(parts.vanillaTail)
    setMustardResult(null)
    setVanillaResult(null)
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
      event.preventDefault()
      void handleRun()
    }
  }

  const speedup =
    mustardResult?.ok && vanillaResult?.ok && mustardResult.elapsedMs > 0
      ? vanillaResult.elapsedMs / mustardResult.elapsedMs
      : null

  const sharedRows = Math.max(10, shared.split('\n').length + 1)
  const tailLcs = useMemo(
    () => longestCommonSubstring(mustardTail, vanillaTail),
    [mustardTail, vanillaTail],
  )

  return (
    <section className="relative scroll-mt-20 px-6 py-20" id="playground">
      <div className="mx-auto max-w-6xl">
        <div className="mb-6">
          <p className="font-mono text-xs uppercase tracking-[0.28em] text-black/50">
            Live Playground
          </p>
          <h2 className="mt-2 font-heading text-3xl font-bold tracking-tight text-black sm:text-4xl">
            Taste the MustardScript
          </h2>
          <p className="mt-2 text-black/60">
            AI knows MustardScript because it's just JavaScript but safer.
          </p>
        </div>

        <div
          ref={frameRef}
          onKeyDown={handleKeyDown}
          className="flex flex-col overflow-hidden rounded-2xl border border-[#78530938] bg-[#F6EFD9] shadow-[0_24px_80px_rgba(0,0,0,0.08)]"
        >
          <div className="bg-[#1B1505] px-4 pb-3 pt-3">
            <div className="flex flex-wrap items-center gap-x-2 gap-y-2">
              <div className="flex flex-wrap gap-1.5">
                {playgroundScenarios.map((s) => {
                  const isActive = s.id === activeScenarioId
                  return (
                    <button
                      key={s.id}
                      type="button"
                      onClick={() => handleSelectScenario(s.id)}
                      className={`rounded-full px-3 py-1 text-sm font-semibold transition ${
                        isActive
                          ? 'bg-[#F5D563] text-black'
                          : 'text-[#F8F2DE]/70 hover:bg-white/[0.06] hover:text-[#F8F2DE]'
                      }`}
                    >
                      {s.label}
                    </button>
                  )
                })}
              </div>
              <button
                type="button"
                onClick={() => setInputsOpen((o) => !o)}
                className="ml-auto rounded-full border border-white/10 bg-white/[0.04] px-3 py-1 font-mono text-xs text-[#F8F2DE]/70 transition hover:bg-white/[0.08]"
              >
                {inputsOpen ? '▾ inputs' : '▸ inputs'}
              </button>
            </div>
            <p className="mt-2 text-[13px] text-[#F8F2DE]/60">{scenario.description}</p>
            {inputsOpen && (
              <pre className="mt-3 w-full overflow-x-auto rounded-lg border border-white/10 bg-black/30 p-3 font-mono text-xs leading-6 text-[#F8F2DE]/80">
                {JSON.stringify(scenario.inputs, null, 2)}
              </pre>
            )}
          </div>

          <div className="flex flex-col border-b border-[#78530938] bg-[#1B1505]">
            <div className="flex items-center justify-between border-y border-white/5 px-4 py-2 text-[11px]">
              <span className="font-mono text-[#F8F2DE]/55">{scenario.mustardFilename}</span>
              <span className="font-mono text-[#F8F2DE]/40">
                shared · <span className="text-[#F5D563]">mustard</span> +{' '}
                <span className="text-[#8EB5FF]">vanilla</span>
              </span>
            </div>
            <HighlightedCode
              ariaLabel="Scenario code"
              value={shared}
              onChange={setShared}
              rows={sharedRows}
              paddingClass="px-4 py-2"
            />
            <div className="border-t border-white/5">
              <div className="flex items-center border-l-[3px] border-[#F5D563]/85 bg-[#F5D563]/[0.06] pl-3 pr-1">
                <span className="w-14 shrink-0 font-mono text-[10px] uppercase tracking-[0.18em] text-[#F5D563]/85">
                  mustard
                </span>
                <DiffLine
                  ariaLabel="MustardScript"
                  value={mustardTail}
                  sameStart={tailLcs.aStart}
                  sameLength={tailLcs.length}
                  accent="#F5D563"
                  onChange={setMustardTail}
                />
              </div>
              <div className="flex items-center border-l-[3px] border-[#8EB5FF]/75 bg-[#8EB5FF]/[0.06] pl-3 pr-1">
                <span className="w-14 shrink-0 font-mono text-[10px] uppercase tracking-[0.18em] text-[#8EB5FF]/85">
                  js
                </span>
                <DiffLine
                  ariaLabel="Vanilla JavaScript"
                  value={vanillaTail}
                  sameStart={tailLcs.bStart}
                  sameLength={tailLcs.length}
                  accent="#8EB5FF"
                  onChange={setVanillaTail}
                />
              </div>
            </div>
          </div>

          <div className="grid grid-cols-1 border-b border-[#78530938] md:grid-cols-2">
            <PlaygroundOutput
              label="Mustard"
              accent="mustard"
              result={mustardResult}
              expected={scenario.expectedResult}
              isRunning={isRunning}
              testId="playground-output-mustard"
            />
            <PlaygroundOutput
              label="Vanilla"
              accent="vanilla"
              result={vanillaResult}
              expected={scenario.expectedResult}
              isRunning={isRunning}
              testId="playground-output-vanilla"
              leftDivider
            />
          </div>

          <div className="flex flex-wrap items-center gap-3 bg-[#1B1505] px-4 py-3">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-xs text-[#F8F2DE]/70">
              <span>
                <span className="text-[#F8F2DE]/45">mustard</span>{' '}
                {mustardResult?.ok ? `${mustardResult.elapsedMs.toFixed(2)} ms` : '—'}
              </span>
              <span className="text-[#F8F2DE]/25">·</span>
              <span>
                <span className="text-[#F8F2DE]/45">vanilla</span>{' '}
                {vanillaResult?.ok ? `${vanillaResult.elapsedMs.toFixed(2)} ms` : '—'}
              </span>
              {speedup !== null && (
                <>
                  <span className="text-[#F8F2DE]/25">·</span>
                  <span className="rounded-full bg-[#F5D563] px-2 py-0.5 text-black">
                    {speedup >= 1
                      ? `${speedup.toFixed(1)}× faster`
                      : `${(1 / speedup).toFixed(1)}× slower`}
                  </span>
                </>
              )}
            </div>
            <div className="ml-auto flex gap-2">
              <button
                type="button"
                onClick={handleReset}
                className="rounded-full border border-white/15 bg-white/[0.04] px-4 py-2 text-sm font-semibold text-[#F8F2DE]/75 transition hover:border-white/30 hover:bg-white/[0.08]"
              >
                Reset
              </button>
              <button
                type="button"
                onClick={() => void handleRun()}
                disabled={isRunning}
                className="flex items-center gap-2 rounded-full bg-[#F5D563] px-4 py-2 text-sm font-semibold text-black shadow-lg shadow-black/40 transition hover:-translate-y-0.5 disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:translate-y-0"
              >
                {isRunning ? 'Running…' : 'Run Comparison'}
                <span className="rounded bg-black/15 px-1.5 py-0.5 font-mono text-[10px] text-black/60">
                  ⌘↵
                </span>
              </button>
            </div>
          </div>
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
