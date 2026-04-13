import fs from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'
import { createServer } from 'vite'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(__dirname, '..')
const distDir = path.resolve(root, 'dist')

async function prerender() {
  const vite = await createServer({
    root,
    server: { middlewareMode: true },
    appType: 'custom',
    logLevel: 'warn',
  })

  try {
    // Load docs module first to ensure glob is populated
    const docs = await vite.ssrLoadModule('/src/lib/docs.ts')
    const slugs = docs.getAllSlugs()
    console.log(`Found ${slugs.length} doc slugs`)

    // Load the server entry
    const { render } = await vite.ssrLoadModule('/src/entry-server.tsx')

    // Read the client-built HTML template
    const template = fs.readFileSync(path.resolve(distDir, 'index.html'), 'utf-8')

    // Build route list
    const routes = [
      '/',
      '/docs',
      ...slugs.map((s) => `/docs/${s}`),
    ]

    console.log(`Pre-rendering ${routes.length} routes...`)

    for (const route of routes) {
      const html = render(route)
      const page = template.replace('<!--ssr-outlet-->', html)

      const filePath =
        route === '/'
          ? path.resolve(distDir, 'index.html')
          : path.resolve(distDir, route.slice(1), 'index.html')

      fs.mkdirSync(path.dirname(filePath), { recursive: true })
      fs.writeFileSync(filePath, page)
      console.log(`  ${route} -> ${path.relative(distDir, filePath)}`)
    }

    console.log('Done!')
  } finally {
    await vite.close()
  }
}

prerender().catch((err) => {
  console.error('Pre-render failed:', err)
  process.exit(1)
})
