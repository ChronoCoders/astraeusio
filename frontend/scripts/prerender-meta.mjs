// Post-build script: generate per-route index.html copies with route-specific
// meta tags (title, description, og:*, twitter:*, canonical). Social scrapers
// don't run JS, so this is the only way to give them route-specific previews
// in a Vite SPA without going full SSR.
//
// Routes live as static html files at dist/<route>/index.html. Nginx already
// has `try_files $uri $uri/ /index.html;` so it picks them up automatically.
// The SPA still owns rendering once the page loads in a real browser.

import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { dirname, join } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))
const DIST   = join(__dirname, '..', 'dist')
const POSTS  = join(__dirname, '..', 'src', 'blog', 'posts.js')
const SITE   = 'https://astraeusio.com'

const STATIC_ROUTES = [
  {
    route: '/products',
    title: 'Products - Astraeusio',
    desc:  'Three ways to consume Astraeusio space weather data: the live dashboard, a JSON API, and real-time alerts via webhook or email.',
  },
  {
    route: '/pricing',
    title: 'Pricing - Astraeusio',
    desc:  'Plans for monitoring, building, and integrating space weather data. Free tier, developer API, production-scale, and enterprise options with SLA.',
  },
  {
    route: '/docs',
    title: 'API Documentation - Astraeusio',
    desc:  'Reference for the Astraeusio space weather API: endpoints, authentication, rate limits, webhooks, error codes, and a glossary of terms.',
  },
  {
    route: '/about',
    title: 'About - Astraeusio',
    desc:  'How Astraeusio is built and what it stands for. Real-time space weather monitoring from NOAA, NASA, Celestrak, and Kyoto WDC open data.',
  },
  {
    route: '/status',
    title: 'Status - Astraeusio',
    desc:  'Real-time service health and component status for the Astraeusio platform. Live monitoring of backend, ML forecast, database, and upstream data feeds.',
  },
  {
    route: '/blog',
    title: 'Blog - Astraeusio',
    desc:  'Articles on space weather, geomagnetic storms, solar wind, X-ray flares, and how the Astraeusio forecasting model works.',
  },
]

function escape(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

function patch(html, { title, desc, url }) {
  const t = escape(title)
  const d = escape(desc)
  return html
    .replace(/<title>[^<]*<\/title>/, `<title>${t}</title>`)
    .replace(/(<link rel="canonical" href=")[^"]*(")/, `$1${url}$2`)
    .replace(/(<meta name="description" content=")[^"]*(")/, `$1${d}$2`)
    .replace(/(<meta property="og:url"\s*content=")[^"]*(")/, `$1${url}$2`)
    .replace(/(<meta property="og:title"\s*content=")[^"]*(")/, `$1${t}$2`)
    .replace(/(<meta property="og:description"\s*content=")[^"]*(")/, `$1${d}$2`)
    .replace(/(<meta name="twitter:title"\s*content=")[^"]*(")/, `$1${t}$2`)
    .replace(/(<meta name="twitter:description"\s*content=")[^"]*(")/, `$1${d}$2`)
}

async function writeRoute(srcHtml, { route, title, desc }) {
  const url = `${SITE}${route}`
  const html = patch(srcHtml, { title, desc, url })
  const outDir = join(DIST, route.replace(/^\//, ''))
  await mkdir(outDir, { recursive: true })
  await writeFile(join(outDir, 'index.html'), html, 'utf8')
}

async function main() {
  const indexPath = join(DIST, 'index.html')
  const baseHtml = await readFile(indexPath, 'utf8')

  // Static pages
  for (const r of STATIC_ROUTES) {
    await writeRoute(baseHtml, r)
  }

  // Blog posts: import the ESM module dynamically
  const { posts } = await import(pathToFileURL(POSTS).href)
  for (const post of posts) {
    await writeRoute(baseHtml, {
      route: `/blog/${post.slug}`,
      title: `${post.title} - Astraeusio Blog`,
      desc:  post.excerpt,
    })
  }

  const total = STATIC_ROUTES.length + posts.length
  console.log(`prerender-meta: wrote ${total} route-specific HTML files`)
}

main().catch(err => {
  console.error('prerender-meta failed:', err)
  process.exit(1)
})
