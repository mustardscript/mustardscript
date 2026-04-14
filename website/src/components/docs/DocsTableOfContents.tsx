import { useEffect, useState } from 'react'

interface Heading {
  level: 2 | 3
  text: string
  id: string
}

export function DocsTableOfContents({ headings }: { headings: Heading[] }) {
  const [activeId, setActiveId] = useState<string>('')

  useEffect(() => {
    if (typeof IntersectionObserver === 'undefined') return

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            setActiveId(entry.target.id)
          }
        }
      },
      { rootMargin: '-80px 0px -60% 0px', threshold: 0 },
    )

    for (const h of headings) {
      const el = document.getElementById(h.id)
      if (el) observer.observe(el)
    }

    return () => observer.disconnect()
  }, [headings])

  if (headings.length === 0) return null

  return (
    <nav className="py-6 pr-4">
      <h4 className="font-heading font-semibold text-xs uppercase tracking-wider text-black/40 dark:text-white/40 mb-3">
        On this page
      </h4>
      <ul className="space-y-1 text-sm">
        {headings.map((h) => (
          <li key={h.id}>
            <a
              href={`#${h.id}`}
              className={`block py-0.5 transition-colors ${
                h.level === 3 ? 'pl-4' : ''
              } ${
                activeId === h.id
                  ? 'text-[#A67C17] dark:text-[#F5D563] font-medium'
                  : 'text-black/45 dark:text-white/45 hover:text-black/70 dark:hover:text-white/70'
              }`}
            >
              {h.text}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  )
}
