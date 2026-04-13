import fs from 'fs'
import nodePath from 'path'
import type { Plugin } from 'vite'
import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkGfm from 'remark-gfm'
import remarkRehype from 'remark-rehype'
import rehypeStringify from 'rehype-stringify'
import rehypeSlug from 'rehype-slug'
import type { Root, Element, Text } from 'hast'
import { visit } from 'unist-util-visit'
import { highlightCode } from '../src/components/highlight.js'

let slugMap: Map<string, string> | null = null

function buildSlugMap(docsDir: string) {
  if (slugMap) return slugMap
  slugMap = new Map()
  const files = fs.readdirSync(docsDir, { recursive: true }) as string[]
  for (const f of files) {
    if (!f.endsWith('.md')) continue
    const content = fs.readFileSync(nodePath.join(docsDir, f), 'utf-8')
    const fm = parseFrontmatter(content)
    if (fm.slug) {
      const basename = nodePath.basename(f)
      slugMap.set(basename, fm.slug)
      slugMap.set(f, fm.slug)
    }
  }
  return slugMap
}

interface Frontmatter {
  title: string
  description: string
  category: string
  order: number
  slug: string
  lastUpdated: string
  [key: string]: string | number
}

interface Heading {
  level: 2 | 3
  text: string
  id: string
}

function parseFrontmatter(raw: string): Frontmatter {
  const fm: Record<string, string | number> = {}
  if (!raw.startsWith('---')) return fm as Frontmatter
  const end = raw.indexOf('\n---', 3)
  if (end < 0) return fm as Frontmatter
  const block = raw.slice(4, end)
  for (const line of block.split('\n')) {
    const idx = line.indexOf(':')
    if (idx < 0) continue
    const key = line.slice(0, idx).trim()
    let val: string | number = line.slice(idx + 1).trim()
    if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
      val = val.slice(1, -1)
    }
    if (/^\d+$/.test(String(val))) val = Number(val)
    fm[key] = val
  }
  return fm as Frontmatter
}

function stripFrontmatter(raw: string): string {
  if (!raw.startsWith('---')) return raw
  const end = raw.indexOf('\n---', 3)
  if (end < 0) return raw
  return raw.slice(end + 4).trimStart()
}

function rehypeLinkRewrite(docsDir: string) {
  return () => (tree: Root) => {
    const map = buildSlugMap(docsDir)
    visit(tree, 'element', (node: Element) => {
      if (node.tagName !== 'a') return
      const href = node.properties?.href as string | undefined
      if (!href) return

      if (href.startsWith('http://') || href.startsWith('https://')) {
        node.properties = node.properties || {}
        node.properties['target'] = '_blank'
        node.properties['rel'] = 'noopener noreferrer'
        return
      }

      if (!href.endsWith('.md')) return

      if (href.startsWith('../')) {
        const ghBase = 'https://github.com/mustardscript/mustardscript/blob/main/'
        node.properties!['href'] = ghBase + href.replace('../', '')
        node.properties!['target'] = '_blank'
        node.properties!['rel'] = 'noopener noreferrer'
        return
      }

      const filename = href.split('/').pop() || href
      const slug = map.get(filename)
      if (slug) {
        node.properties!['href'] = `/docs/${slug}`
      }
    })
  }
}

function rehypeHighlight() {
  return () => (tree: Root) => {
    visit(tree, 'element', (node: Element) => {
      if (node.tagName !== 'pre') return
      const codeNode = node.children[0] as Element | undefined
      if (!codeNode || codeNode.tagName !== 'code') return

      const classes = (codeNode.properties?.className as string[]) || []
      const langClass = classes.find(c => c.startsWith('language-'))
      const lang = langClass?.replace('language-', '')

      const text = getTextContent(codeNode)
      const highlighted = highlightCode(text, lang)

      codeNode.children = [{ type: 'raw', value: highlighted } as unknown as Text]
    })
  }
}

function getTextContent(node: Element | Text): string {
  if (node.type === 'text') return (node as Text).value
  if ('children' in node) {
    return (node.children as (Element | Text)[]).map(getTextContent).join('')
  }
  return ''
}

function extractHeadings(markdown: string): Heading[] {
  const headings: Heading[] = []
  for (const line of markdown.split('\n')) {
    const m2 = line.match(/^## (.+)/)
    const m3 = line.match(/^### (.+)/)
    if (m2) {
      const text = m2[1].trim()
      headings.push({ level: 2, text, id: slugify(text) })
    } else if (m3) {
      const text = m3[1].trim()
      headings.push({ level: 3, text, id: slugify(text) })
    }
  }
  return headings
}

function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, '')
    .replace(/\s+/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
}

function processMarkdown(raw: string, docsDir: string) {
  const frontmatter = parseFrontmatter(raw)
  const content = stripFrontmatter(raw)

  const processor = unified()
    .use(remarkParse)
    .use(remarkGfm)
    .use(remarkRehype, { allowDangerousHtml: true })
    .use(rehypeSlug)
    .use(rehypeLinkRewrite(docsDir))
    .use(rehypeHighlight())
    .use(rehypeStringify, { allowDangerousHtml: true })

  const html = processor.processSync(content).toString()
  const headings = extractHeadings(content)

  return { frontmatter, html, headings }
}

/** Build the full docs manifest by reading all .md files from the docs directory */
function buildManifest(docsDir: string): string {
  const files = fs.readdirSync(docsDir, { recursive: true }) as string[]
  const entries: Array<{ frontmatter: Frontmatter; html: string; headings: Heading[] }> = []

  for (const f of files) {
    if (!f.endsWith('.md')) continue
    const raw = fs.readFileSync(nodePath.join(docsDir, f), 'utf-8')
    const result = processMarkdown(raw, docsDir)
    if (result.frontmatter.slug) {
      entries.push(result)
    }
  }

  return `export default ${JSON.stringify(entries)};`
}

const VIRTUAL_ID = 'virtual:docs-manifest'
const RESOLVED_VIRTUAL_ID = '\0' + VIRTUAL_ID

export default function markdown(): Plugin {
  let docsDir: string

  return {
    name: 'vite-plugin-md',
    configResolved(config) {
      docsDir = nodePath.resolve(config.root, '../docs')
    },
    resolveId(id) {
      if (id === VIRTUAL_ID) return RESOLVED_VIRTUAL_ID
    },
    load(id) {
      if (id === RESOLVED_VIRTUAL_ID) {
        return buildManifest(docsDir)
      }
    },
    transform(raw, id) {
      if (!id.endsWith('.md')) return null

      const { frontmatter, html, headings } = processMarkdown(raw, docsDir)

      return {
        code: `
export const frontmatter = ${JSON.stringify(frontmatter)};
export const html = ${JSON.stringify(html)};
export const headings = ${JSON.stringify(headings)};
`,
        map: null,
      }
    },
  }
}
