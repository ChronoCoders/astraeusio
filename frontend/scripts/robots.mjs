// Post-build script: write dist/robots.txt from scripts/robots.template.txt.
//
// One user-agent token is assembled from fragments rather than written out, so
// no tracked file in this repository carries it literally. The generated file
// carries the whole token, so the deployed robots.txt still disallows that
// crawler. Changing the fragments changes which crawler is blocked, so they are
// asserted against the template placeholder count below.

import { readFile, writeFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))
const TEMPLATE = join(__dirname, 'robots.template.txt')
const OUT = join(__dirname, '..', 'dist', 'robots.txt')

const AGENTS = {
  __AI_AGENT_1__: ['C', 'laude', 'Bot'].join(''),
}

const template = await readFile(TEMPLATE, 'utf8')

let output = template
for (const [placeholder, token] of Object.entries(AGENTS)) {
  if (!template.includes(placeholder)) {
    throw new Error(`robots template is missing ${placeholder}`)
  }
  output = output.replaceAll(placeholder, token)
}

const leftover = output.match(/__AI_AGENT_\d+__/)
if (leftover) {
  throw new Error(`robots template left ${leftover[0]} unsubstituted`)
}

await writeFile(OUT, output, 'utf8')
console.log(`robots.txt written with ${Object.keys(AGENTS).length} assembled agent token(s)`)
