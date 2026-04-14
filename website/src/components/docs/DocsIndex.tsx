import { Link } from 'react-router-dom'
import { getCategories } from '../../lib/docs'

export function DocsIndex() {
  const categories = getCategories()

  return (
    <div className="max-w-3xl mx-auto">
      <h1 className="font-heading text-4xl font-bold text-black dark:text-white mb-3">Documentation</h1>
      <p className="text-black/60 dark:text-white/60 text-lg mb-10">
        Everything you need to integrate MustardScript into your AI agent stack.
      </p>

      <div className="space-y-10">
        {categories.map((cat) => (
          <section key={cat.name}>
            <h2 className="font-heading text-lg font-semibold text-black/70 dark:text-white/70 mb-4">{cat.name}</h2>
            <div className="grid gap-3 sm:grid-cols-2">
              {cat.docs.map((doc) => (
                <Link
                  key={doc.slug}
                  to={`/docs/${doc.slug}`}
                  className="group block p-4 rounded-xl bg-[#FEF3C7]/60 dark:bg-white/5 hover:bg-[#FDE68A]/60 dark:hover:bg-[#E8B931]/10 border border-black/5 dark:border-white/8 hover:border-[#E8B931]/30 dark:hover:border-[#E8B931]/40 transition-all"
                >
                  <h3 className="font-heading font-semibold text-black dark:text-white group-hover:text-[#A67C17] dark:group-hover:text-[#F5D563] transition-colors mb-1">
                    {doc.title}
                  </h3>
                  <p className="text-sm text-black/50 dark:text-white/50 leading-relaxed">{doc.description}</p>
                </Link>
              ))}
            </div>
          </section>
        ))}
      </div>
    </div>
  )
}
