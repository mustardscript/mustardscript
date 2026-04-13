import manifest from 'virtual:docs-manifest'

interface Heading {
  level: 2 | 3
  text: string
  id: string
}

export interface DocEntry {
  slug: string
  title: string
  description: string
  category: string
  order: number
  lastUpdated: string
  html: string
  headings: Heading[]
}

export interface CategoryGroup {
  name: string
  docs: DocEntry[]
}

const CATEGORY_ORDER = [
  'Getting Started',
  'Language & Runtime',
  'API Reference',
  'Security',
  'Development',
  'Architecture',
]

function buildDocs(): DocEntry[] {
  const entries: DocEntry[] = manifest.map((m) => ({
    slug: m.frontmatter.slug,
    title: m.frontmatter.title,
    description: m.frontmatter.description,
    category: m.frontmatter.category,
    order: m.frontmatter.order,
    lastUpdated: m.frontmatter.lastUpdated,
    html: m.html,
    headings: m.headings,
  }))

  entries.sort((a, b) => {
    const catDiff = CATEGORY_ORDER.indexOf(a.category) - CATEGORY_ORDER.indexOf(b.category)
    if (catDiff !== 0) return catDiff
    return a.order - b.order
  })
  return entries
}

let _docs: DocEntry[] | null = null
function docs(): DocEntry[] {
  if (!_docs) _docs = buildDocs()
  return _docs
}

export function getAllDocs(): DocEntry[] {
  return docs()
}

export function getDocBySlug(slug: string): DocEntry | undefined {
  return docs().find(d => d.slug === slug)
}

export function getCategories(): CategoryGroup[] {
  const all = docs()
  const groups: CategoryGroup[] = []
  for (const cat of CATEGORY_ORDER) {
    const catDocs = all.filter(d => d.category === cat)
    if (catDocs.length > 0) {
      groups.push({ name: cat, docs: catDocs })
    }
  }
  return groups
}

export function getAdjacentDocs(slug: string): { prev?: DocEntry; next?: DocEntry } {
  const all = docs()
  const idx = all.findIndex(d => d.slug === slug)
  if (idx < 0) return {}
  return {
    prev: idx > 0 ? all[idx - 1] : undefined,
    next: idx < all.length - 1 ? all[idx + 1] : undefined,
  }
}

export function getAllSlugs(): string[] {
  return docs().map(d => d.slug)
}
