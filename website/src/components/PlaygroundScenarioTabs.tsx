import type { PlaygroundScenario } from '../lib/playground/scenarios'

interface PlaygroundScenarioTabsProps {
  scenarios: PlaygroundScenario[]
  activeScenarioId: string
  onSelect: (scenarioId: string) => void
}

export function PlaygroundScenarioTabs({
  scenarios,
  activeScenarioId,
  onSelect,
}: PlaygroundScenarioTabsProps) {
  return (
    <div className="flex flex-wrap gap-3">
      {scenarios.map((scenario) => {
        const isActive = scenario.id === activeScenarioId
        return (
          <button
            key={scenario.id}
            type="button"
            onClick={() => onSelect(scenario.id)}
            className={`rounded-full border px-4 py-2 text-sm font-semibold transition ${
              isActive
                ? 'border-black bg-black text-[#F5D563]'
                : 'border-black/15 bg-white/35 text-black/70 hover:border-black/40 hover:bg-white/60'
            }`}
          >
            {scenario.label}
          </button>
        )
      })}
    </div>
  )
}
