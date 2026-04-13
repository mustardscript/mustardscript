import { NavLink } from 'react-router-dom'
import { getCategories } from '../../lib/docs'

export function DocsSidebar({ onNavigate }: { onNavigate?: () => void }) {
  const categories = getCategories()

  return (
    <nav className="py-6 px-4 space-y-6">
      {categories.map((cat) => (
        <div key={cat.name}>
          <h3 className="font-heading font-semibold text-xs uppercase tracking-wider text-black/40 px-3 mb-2">
            {cat.name}
          </h3>
          <ul className="space-y-0.5">
            {cat.docs.map((doc) => (
              <li key={doc.slug}>
                <NavLink
                  to={`/docs/${doc.slug}`}
                  onClick={onNavigate}
                  className={({ isActive }) =>
                    `block px-3 py-1.5 rounded-lg text-sm transition-colors ${
                      isActive
                        ? 'bg-[#FEF3C7] text-black font-medium border-l-2 border-[#E8B931]'
                        : 'text-black/60 hover:text-black hover:bg-black/5'
                    }`
                  }
                >
                  {doc.title}
                </NavLink>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </nav>
  )
}
