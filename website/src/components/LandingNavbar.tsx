import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'

const SECTIONS = [
  { id: 'what', label: 'What is it?' },
  { id: 'playground', label: 'Playground' },
  { id: 'examples', label: 'Examples' },
] as const

export function LandingNavbar() {
  const [active, setActive] = useState<string | null>(null)

  useEffect(() => {
    if (typeof window === 'undefined' || typeof IntersectionObserver === 'undefined') return
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries.filter((e) => e.isIntersecting)
        if (visible.length === 0) return
        const top = visible.reduce((best, e) =>
          e.intersectionRatio > best.intersectionRatio ? e : best,
        )
        setActive(top.target.id)
      },
      { rootMargin: '-80px 0px -55% 0px', threshold: [0, 0.15, 0.35, 0.6, 0.85, 1] },
    )
    SECTIONS.forEach((s) => {
      const el = document.getElementById(s.id)
      if (el) observer.observe(el)
    })
    return () => observer.disconnect()
  }, [])

  return (
    <header className="sticky top-0 z-30 border-b border-black/8 bg-mustard/80 backdrop-blur-md">
      <div className="mx-auto flex h-14 max-w-[1400px] items-center justify-between gap-4 px-6">
        <Link
          to="/"
          className="flex shrink-0 items-center gap-2 font-heading text-lg font-bold transition-opacity hover:opacity-80"
        >
          <img src="/logo.png" alt="" aria-hidden="true" className="h-8 w-8 shrink-0" />
          <span className="text-black">Mustard</span>
          <span className="text-black/50">Script</span>
        </Link>

        <nav className="hidden items-center gap-1 md:flex">
          {SECTIONS.map((s) => {
            const isActive = active === s.id
            return (
              <a
                key={s.id}
                href={`#${s.id}`}
                aria-current={isActive ? 'true' : undefined}
                className={`rounded-full px-3 py-1.5 text-sm font-semibold transition-colors ${
                  isActive
                    ? 'bg-black/10 text-black'
                    : 'text-black/55 hover:bg-black/[0.06] hover:text-black/85'
                }`}
              >
                {s.label}
              </a>
            )
          })}
        </nav>

        <div className="flex items-center gap-2">
          <Link
            to="/docs"
            className="px-3 py-1.5 text-sm font-semibold text-black/70 transition-colors hover:text-black"
          >
            Docs
          </Link>
          <a
            href="https://github.com/mustardscript/mustardscript"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-2 rounded-lg bg-black/10 px-4 py-1.5 text-sm font-semibold text-black/80 transition-colors hover:bg-black/15 hover:text-black"
          >
            <svg className="h-4 w-4" fill="currentColor" viewBox="0 0 24 24">
              <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
            </svg>
            GitHub
          </a>
        </div>
      </div>
    </header>
  )
}
