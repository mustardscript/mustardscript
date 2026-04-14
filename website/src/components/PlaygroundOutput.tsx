import type { PlaygroundRunResult } from '../lib/playground/iframe-protocol'

interface PlaygroundOutputProps {
  title: string
  testId?: string
  tone: 'mustard' | 'vanilla'
  result: PlaygroundRunResult | null
  isRunning: boolean
}

function formatValue(value: unknown) {
  return JSON.stringify(value, null, 2)
}

export function PlaygroundOutput({
  title,
  testId,
  tone,
  result,
  isRunning,
}: PlaygroundOutputProps) {
  const accentClass =
    tone === 'mustard'
      ? 'border-[#C49102]/35 bg-[#2A1E07] text-[#F8E8B1]'
      : 'border-[#2563EB]/20 bg-[#111827] text-[#D7E7FF]'

  return (
    <section data-testid={testId} className={`rounded-[24px] border ${accentClass} shadow-xl`}>
      <div className="flex items-center justify-between gap-4 border-b border-white/10 px-5 py-4">
        <h3 className="font-heading text-lg font-bold">{title}</h3>
        <span className="rounded-full border border-white/10 bg-white/5 px-3 py-1 font-mono text-xs">
          {isRunning ? 'running' : result ? `${result.elapsedMs.toFixed(2)} ms` : 'idle'}
        </span>
      </div>

      <div className="space-y-4 p-5 text-sm">
        {!result && !isRunning && (
          <p className="text-white/60">
            Run the scenario to compare output, errors, and capability trace.
          </p>
        )}

        {isRunning && <p className="text-white/70">Executing current scenario…</p>}

        {result && (
          <>
            <div>
              <p className="mb-2 font-semibold text-white/75">Status</p>
              <p className={result.ok ? 'text-[#86EFAC]' : 'text-[#FCA5A5]'}>
                {result.ok ? 'Completed' : result.error?.name ?? 'Error'}
              </p>
            </div>

            {result.ok && (
              <div>
                <p className="mb-2 font-semibold text-white/75">Result</p>
                <pre className="overflow-x-auto rounded-2xl bg-black/25 p-4 font-mono text-xs leading-6 text-white/90">
                  {formatValue(result.result)}
                </pre>
              </div>
            )}

            {!result.ok && (
              <div>
                <p className="mb-2 font-semibold text-white/75">Error</p>
                <pre className="overflow-x-auto rounded-2xl bg-black/25 p-4 font-mono text-xs leading-6 text-[#FED7D7]">
                  {formatValue(result.error)}
                </pre>
              </div>
            )}

            {result.trace.length > 0 && (
              <div>
                <p className="mb-2 font-semibold text-white/75">Capability Trace</p>
                <pre className="overflow-x-auto rounded-2xl bg-black/25 p-4 font-mono text-xs leading-6 text-white/90">
                  {formatValue(result.trace)}
                </pre>
              </div>
            )}
          </>
        )}
      </div>
    </section>
  )
}
