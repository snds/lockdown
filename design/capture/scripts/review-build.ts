// The review build: the harness as static files a page host can serve
// (index.html, the in-page frames' frame.html, relative assets/, hash
// routing, no server, no network), copied outside the repo and checked
// against the host's limits.
//
//   pnpm review:build [--out <dir>]

import { cpSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs"
import path from "node:path"
import { parseArgs } from "node:util"

import { build } from "vite"

const here = import.meta.dirname
const { values: args } = parseArgs({ options: { out: { type: "string", default: "/home/user/design-capture/review-build" } } })

const MAX_BYTES = 16 * 1024 * 1024
const MAX_FILES = 255

function walk(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = path.join(dir, name)
    return statSync(p).isDirectory() ? walk(p) : [p]
  })
}

await build({ root: path.join(here, ".."), logLevel: "warn" })

const dist = path.join(here, "../dist")
const files = walk(dist)
const bytes = files.reduce((n, f) => n + statSync(f).size, 0)
const problems: string[] = []
if (files.length > MAX_FILES) problems.push(`${files.length} files, over ${MAX_FILES}`)
if (bytes > MAX_BYTES) problems.push(`${(bytes / 1048576).toFixed(1)} MB, over 16 MB`)
for (const f of files.filter((f) => /\.(html|css|js)$/.test(f))) {
  const text = readFileSync(f, "utf8")
  // Absolute addresses would break once the folder is served from anywhere but a root.
  if (/(?:src|href)="\/(?!\/)/.test(text) || /url\(\/(?!\/)/.test(text)) problems.push(`${path.relative(dist, f)} names an absolute path`)
  if (/serviceWorker\.register|\.wasm\b/.test(text) && !f.endsWith(".js")) problems.push(`${path.relative(dist, f)} registers a service worker or loads wasm`)
}
if (problems.length) {
  console.error(`The review build breaks the host's limits:\n  ${problems.join("\n  ")}`)
  process.exit(1)
}

const out = path.resolve(args.out)
rmSync(out, { recursive: true, force: true })
cpSync(dist, out, { recursive: true })
console.log(`Review build: ${files.length} files, ${(bytes / 1048576).toFixed(1)} MB, in ${out}`)
