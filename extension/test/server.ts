// A static server for the fixtures and the wasm package, reachable as two
// origins (`http://localhost:<port>` and `http://127.0.0.1:<port>`) so one
// process can play both the page and a cross-origin frame. The extension
// suite maps more names to it (`--host-resolver-rules`), and `/basic/`
// answers only with the fixture's Basic Auth credentials.
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
  /** What the stand-in alias service was asked (bridges.spec.ts). */
  aliasCalls: { hostname: string; key: string }[];
  close(): Promise<void>;
}

export async function serve(): Promise<Served> {
  const aliasCalls: Served["aliasCalls"] = [];
  const server = createServer(async (req, res) => {
    const url = new URL(req.url ?? "/", "http://x");
    if (url.pathname === "/.well-known/change-password") {
      // A site with a change-password page (W3C well-known URL).
      res.writeHead(302, { location: "/ext/change.html" }).end();
      return;
    }
    if (req.method === "POST" && url.pathname === "/api/alias/random/new") {
      // A stand-in for SimpleLogin's API, for the alias rows.
      const key = String(req.headers.authentication ?? "");
      aliasCalls.push({ hostname: url.searchParams.get("hostname") ?? "", key });
      if (key !== "fixture-alias-key") {
        res.writeHead(401, { "content-type": "application/json" }).end('{"error":"wrong key"}');
        return;
      }
      res.writeHead(201, { "content-type": "application/json" }).end(JSON.stringify({ alias: `fixture.${aliasCalls.length}@aliases.test` }));
      return;
    }
    let path = normalize(decodeURIComponent(url.pathname));
    if (path.includes("..")) {
      res.writeHead(400).end();
      return;
    }
    if (path.startsWith("/basic/")) {
      // HTTP Basic Auth, for the extension's onAuthRequired answer.
      const want = "Basic " + Buffer.from("basic-user:basic-pass-4").toString("base64");
      if (req.headers.authorization === want) {
        res.writeHead(200, { "content-type": "text/html; charset=utf-8" }).end('<!doctype html><title>In</title><h1 id="in">Welcome, basic-user</h1>');
      } else {
        res.writeHead(401, { "www-authenticate": 'Basic realm="fixture"', "content-type": "text/plain" }).end("sign in");
      }
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
    aliasCalls,
    close: () => new Promise((resolve) => server.close(() => resolve())),
  };
}
