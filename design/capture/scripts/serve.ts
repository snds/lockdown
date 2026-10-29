// A static server for the built harness, on 127.0.0.1 only, for the
// capture to drive. Nothing but files from `root`.

import { createReadStream, existsSync, statSync } from "node:fs"
import { createServer, type Server } from "node:http"
import path from "node:path"

const TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".woff2": "font/woff2",
  ".woff": "font/woff",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".json": "application/json",
}

export function serve(root: string): Promise<{ url: string; close: () => Promise<void> }> {
  const server: Server = createServer((req, res) => {
    const url = new URL(req.url ?? "/", "http://127.0.0.1")
    const rel = decodeURIComponent(url.pathname).replace(/^\/+/, "") || "index.html"
    const file = path.resolve(root, rel)
    if (!file.startsWith(path.resolve(root)) || !existsSync(file) || !statSync(file).isFile()) {
      res.writeHead(404).end()
      return
    }
    res.writeHead(200, { "content-type": TYPES[path.extname(file)] ?? "application/octet-stream", "cache-control": "max-age=600" })
    createReadStream(file).pipe(res)
  })
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      const addr = server.address()
      const port = typeof addr === "object" && addr ? addr.port : 0
      resolve({ url: `http://127.0.0.1:${port}/`, close: () => new Promise((r) => server.close(() => r())) })
    })
  })
}
