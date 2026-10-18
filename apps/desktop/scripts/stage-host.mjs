// Builds `lockdown-native-host` and puts it where Tauri's bundler takes
// sidecars from (`bundle.externalBin` in src-tauri/tauri.bundle.conf.json):
// src-tauri/binaries/lockdown-native-host-<target triple>[.exe]. The
// bundler then installs it beside the app's own executable, which is where
// the app looks for it (EndpointConfig::for_app, docs/plan/24-release.md).
//
//   node scripts/stage-host.mjs [--target <triple> | --universal] [--release]
//
// --universal (macOS) builds aarch64-apple-darwin and x86_64-apple-darwin,
// stages each under its own name (each architecture's build checks for it)
// and joins them with lipo as ...-universal-apple-darwin, the one
// `tauri build --target universal-apple-darwin` puts in the app. Without
// --target the host is built for this machine without a --target flag, so
// it shares the target directory's existing build.
import { execFileSync } from "node:child_process"
import { copyFileSync, mkdirSync, chmodSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const here = join(dirname(fileURLToPath(import.meta.url)), "..")
const root = join(here, "..", "..")
const args = process.argv.slice(2)
const release = args.includes("--release")
const universal = args.includes("--universal")
const at = args.indexOf("--target")
const explicit = at >= 0 ? args[at + 1] : undefined
const profile = release ? "release" : "debug"
const targetDir = process.env.CARGO_TARGET_DIR || join(root, "target")
const binaries = join(here, "src-tauri", "binaries")
mkdirSync(binaries, { recursive: true })

function hostTriple() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  const line = out.split("\n").find((l) => l.startsWith("host: "))
  if (!line) throw new Error("stage-host: rustc -vV did not name the host triple")
  return line.slice("host: ".length).trim()
}

function build(triple) {
  const cargo = ["build", "--locked", "-p", "lockdown-native-host"]
  if (release) cargo.push("--release")
  if (triple) cargo.push("--target", triple)
  execFileSync("cargo", cargo, { cwd: root, stdio: "inherit" })
  const exe = (triple ?? hostTriple()).includes("windows") ? ".exe" : ""
  const dir = triple ? join(targetDir, triple, profile) : join(targetDir, profile)
  return join(dir, `lockdown-native-host${exe}`)
}

function stage(from, triple) {
  const exe = triple.includes("windows") ? ".exe" : ""
  const to = join(binaries, `lockdown-native-host-${triple}${exe}`)
  copyFileSync(from, to)
  chmodSync(to, 0o755)
  console.log(`stage-host: ${to}`)
}

if (universal) {
  // Each architecture's own build also checks for its own sidecar name.
  const triples = ["aarch64-apple-darwin", "x86_64-apple-darwin"]
  const parts = triples.map((t) => build(t))
  triples.forEach((t, i) => stage(parts[i], t))
  const out = join(binaries, "lockdown-native-host-universal-apple-darwin")
  execFileSync("lipo", ["-create", "-output", out, ...parts], { stdio: "inherit" })
  execFileSync("lipo", ["-info", out], { stdio: "inherit" })
  console.log(`stage-host: ${out}`)
} else {
  const triple = explicit ?? hostTriple()
  stage(build(explicit), triple)
}
