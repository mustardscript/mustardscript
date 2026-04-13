declare module '*.md' {
  export const frontmatter: {
    title: string
    description: string
    category: string
    order: number
    slug: string
    lastUpdated: string
  }
  export const html: string
  export const headings: Array<{ level: 2 | 3; text: string; id: string }>
}

declare module 'virtual:docs-manifest' {
  interface DocManifestEntry {
    frontmatter: {
      title: string
      description: string
      category: string
      order: number
      slug: string
      lastUpdated: string
    }
    html: string
    headings: Array<{ level: 2 | 3; text: string; id: string }>
  }
  const manifest: DocManifestEntry[]
  export default manifest
}
