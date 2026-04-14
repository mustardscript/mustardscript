import { useState } from 'react'

import type { JsonValue, PlaygroundRunResult } from '../lib/playground/iframe-protocol'

interface PlaygroundOutputProps {
  label: string
  accent: 'mustard' | 'vanilla'
  result: PlaygroundRunResult | null
  expected: JsonValue
  isRunning: boolean
  testId?: string
  leftDivider?: boolean
}

function formatValue(value: unknown) {
  return JSON.stringify(value, null, 2)
}

function jsonEqual(a: unknown, b: unknown) {
  return JSON.stringify(a) === JSON.stringify(b)
}

export function PlaygroundOutput({
  label,
  accent,
  result,
  expected,
  isRunning,
  testId,
  leftDivider,
}: PlaygroundOutputProps) {
  const [traceOpen, setTraceOpen] = useState(false)
  const [expectedOpen, setExpectedOpen] = useState(false)

  const railColor = accent === 'mustard' ? 'bg-[#F5D563]' : 'bg-[#8EB5FF]'
  const labelColor = accent === 'mustard' ? 'text-[#F5D563]' : 'text-[#8EB5FF]'

  const matches = result?.ok ? jsonEqual(result.result, expected) : false

  let statusChip: React.ReactNode
  if (isRunning) {
    statusChip = <span className="text-white/60">running…</span>
  } else if (!result) {
    statusChip = <span className="text-white/40">idle</span>
  } else if (!result.ok) {
    statusChip = (
      <span className="rounded-full bg-[#FCA5A5]/15 px-2 py-0.5 text-[#FCA5A5]">
        {result.error?.name ?? 'error'}
      </span>
    )
  } else if (matches) {
    statusChip = (
      <span className="rounded-full bg-[#86EFAC]/15 px-2 py-0.5 text-[#86EFAC]">
        ✓ matches expected
      </span>
    )
  } else {
    statusChip = (
      <span className="rounded-full bg-[#FCA5A5]/15 px-2 py-0.5 text-[#FCA5A5]">
        ✗ diverges
      </span>
    )
  }

  return (
    <section
      data-testid={testId}
      className={`relative flex min-h-[14rem] flex-col bg-[#111110] ${
        leftDivider ? 'md:border-l md:border-white/5' : ''
      }`}
    >
      <div className={`absolute inset-y-0 left-0 w-[3px] ${railColor}`} aria-hidden />
      <div className="flex items-center justify-between gap-3 px-5 py-3">
        <div className="flex items-center gap-3">
          <span className={`font-mono text-[11px] uppercase tracking-[0.22em] ${labelColor}`}>
            {label}
          </span>
          <span className="font-mono text-[11px] text-white/45">
            {result && !isRunning ? `${result.elapsedMs.toFixed(2)} ms` : ''}
          </span>
        </div>
        <div className="text-[11px]">{statusChip}</div>
      </div>

      <div className="flex-1 space-y-3 px-5 pb-4 text-sm">
        {!result && !isRunning && (
          <p className="text-white/40">Run the scenario to see output.</p>
        )}

        {result?.ok && (
          <pre className="overflow-x-auto rounded-lg bg-black/40 p-3 font-mono text-[12px] leading-6 text-white/90">
            {formatValue(result.result)}
          </pre>
        )}

        {result && !result.ok && (
          <pre className="overflow-x-auto rounded-lg bg-black/40 p-3 font-mono text-[12px] leading-6 text-[#FED7D7]">
            {formatValue(result.error)}
          </pre>
        )}

        {result && (
          <div className="flex flex-col gap-2 text-[11px]">
            <button
              type="button"
              onClick={() => setExpectedOpen((o) => !o)}
              className="self-start rounded-full border border-white/10 bg-white/5 px-2 py-0.5 font-mono text-white/60 transition hover:bg-white/10"
            >
              {expectedOpen ? '▾ hide expected' : '▸ show expected'}
            </button>
            {expectedOpen && (
              <pre className="overflow-x-auto rounded-lg bg-black/30 p-3 font-mono text-[12px] leading-6 text-white/70">
                {formatValue(expected)}
              </pre>
            )}
          </div>
        )}

        {result && result.trace.length > 0 && (
          <div className="flex flex-col gap-2 text-[11px]">
            <button
              type="button"
              onClick={() => setTraceOpen((o) => !o)}
              className="self-start rounded-full border border-white/10 bg-white/5 px-2 py-0.5 font-mono text-white/60 transition hover:bg-white/10"
            >
              {traceOpen
                ? `▾ Capability Trace (${result.trace.length})`
                : `▸ Capability Trace (${result.trace.length})`}
            </button>
            {traceOpen && (
              <pre className="overflow-x-auto rounded-lg bg-black/30 p-3 font-mono text-[12px] leading-6 text-white/70">
                {formatValue(result.trace)}
              </pre>
            )}
          </div>
        )}
      </div>
    </section>
  )
}
