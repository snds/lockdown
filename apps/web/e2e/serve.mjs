// Starts the real lockdown-server for the end-to-end suite: a throwaway
// data directory (removed on exit), the built web vault at /vault/, on the
// port Playwright waits for. The binary is cargo's debug build
// (`cargo build -p lockdown-server`), found through CARGO_TARGET_DIR or
// the repository's target/; LOCKDOWN_SERVER_BIN overrides both.
import { spawn } from "node:child_process"
import { existsSync, mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import path from "node:path"

const here = import.meta.dirname
const root = path.resolve(here, "../../..")
const target = process.env.CARGO_TARGET_DIR ?? path.join(root, "target")
const bin = process.env.LOCKDOWN_SERVER_BIN ?? path.join(target, "debug", "lockdown-server")
const dist = path.resolve(here, "../dist")
const port = process.env.LOCKDOWN_E2E_PORT ?? "8797"

if (!existsSync(bin)) {
  console.error(`serve: no server binary at ${bin}; run: cargo build -p lockdown-server`)
  process.exit(2)
}
if (!existsSync(path.join(dist, "index.html"))) {
  console.error("serve: no built web vault; run: pnpm build:wasm && pnpm build")
  process.exit(2)
}

const data = mkdtempSync(path.join(tmpdir(), "lockdown-web-e2e-"))
const server = spawn(bin, ["--listen", `127.0.0.1:${port}`, "--data", data, "--web-vault", dist], {
  stdio: "inherit",
})
const stop = () => {
  server.kill("SIGINT")
}
server.on("exit", (code) => {
  rmSync(data, { recursive: true, force: true })
  process.exit(code ?? 0)
})
process.on("SIGINT", stop)
process.on("SIGTERM", stop)
