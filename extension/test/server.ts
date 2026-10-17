// A static server for the fixtures and the wasm package, reachable as two
// origins (`http://localhost:<port>` and `http://127.0.0.1:<port>`) so one
// process can play both the page and a cross-origin frame.
import { createServer, type Server } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const fixtures = join(here, "fixtures");
const pkg = join(here, "..", "pkg");

const types: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".json": "application/json",
  ".css": "text/css",
};

export interface Served {
  server: Server;
  port: number;
  close(): Promise<void>;
}

export async function serve(): Promise<Served> {
  const server = createServer(async (req, res) => {
    const url = new URL(req.url ?? "/", "http://x");
    let path = normalize(decodeURIComponent(url.pathname));
    if (path.includes("..")) {
      res.writeHead(400).end();
      return;
    }
    let file: string;
    if (path.startsWith("/pkg/")) file = join(pkg, path.slice("/pkg/".length));
    else file = join(fixtures, path === "/" ? "top.html" : path);
    try {
      const body = await readFile(file);
      res.writeHead(200, {
        "content-type": types[extname(file)] ?? "application/octet-stream",
        "cache-control": "no-store",
      });
      res.end(body);
    } catch {
      res.writeHead(404, { "content-type": "text/plain" }).end(`not found: ${path}`);
    }
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("no port");
  return {
    server,
    port: address.port,
    close: () => new Promise((resolve) => server.close(() => resolve())),
  };
}
