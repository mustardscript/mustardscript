import { useParams, Link } from 'react-router-dom'
import { useEffect } from 'react'
import { getDocBySlug, getAdjacentDocs } from '../../lib/docs'
import { DocsTableOfContents } from './DocsTableOfContents'

export function DocPage() {
  const { slug } = useParams<{ slug: string }>()
  const doc = slug ? getDocBySlug(slug) : undefined
  const { prev, next } = slug ? getAdjacentDocs(slug) : {}

  useEffect(() => {
    if (doc) {
      document.title = `${doc.title} — MustardScript Docs`
    }
    return () => {
      document.title = 'MustardScript — Like JavaScript, but smaller'
    }
  }, [doc])

  if (!doc) {
    return (
      <div className="max-w-3xl mx-auto text-center py-20">
        <h1 className="font-heading text-2xl font-bold text-black mb-4">Page not found</h1>
        <Link to="/docs" className="text-[#A67C17] underline underline-offset-2 hover:text-[#854D0E]">
          Back to docs
        </Link>
      </div>
    )
  }

  return (
    <div className="flex gap-8">
      {/* Main content */}
      <article className="min-w-0 flex-1 max-w-3xl">
        <header className="mb-8">
          <p className="text-xs text-black/40 font-medium uppercase tracking-wider mb-2">
            {doc.category}
          </p>
          <h1 className="font-heading text-4xl font-bold text-black mb-2">{doc.title}</h1>
          <p className="text-sm text-black/40">Last updated {doc.lastUpdated}</p>
        </header>

        <div
          className="docs-prose"
          dangerouslySetInnerHTML={{ __html: doc.html }}
        />

        {/* Prev / Next */}
        <nav className="mt-16 pt-8 border-t border-black/8 flex justify-between gap-4">
          {prev ? (
            <Link
              to={`/docs/${prev.slug}`}
              className="group flex-1 p-4 rounded-xl border border-black/8 hover:border-[#E8B931]/40 hover:bg-[#FEF3C7]/40 transition-all"
            >
              <span className="text-xs text-black/40 group-hover:text-[#A67C17] transition-colors">Previous</span>
              <span className="block font-heading font-semibold text-black mt-1">{prev.title}</span>
            </Link>
          ) : (
            <div className="flex-1" />
          )}
          {next ? (
            <Link
              to={`/docs/${next.slug}`}
              className="group flex-1 p-4 rounded-xl border border-black/8 hover:border-[#E8B931]/40 hover:bg-[#FEF3C7]/40 transition-all text-right"
            >
              <span className="text-xs text-black/40 group-hover:text-[#A67C17] transition-colors">Next</span>
              <span className="block font-heading font-semibold text-black mt-1">{next.title}</span>
            </Link>
          ) : (
            <div className="flex-1" />
          )}
        </nav>
      </article>

      {/* Table of contents — desktop only */}
      <aside className="hidden xl:block w-52 shrink-0 sticky top-14 max-h-[calc(100vh-3.5rem)] overflow-y-auto">
        <DocsTableOfContents headings={doc.headings} />
      </aside>
    </div>
  )
}
