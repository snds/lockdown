// Packs the built extension (dist/chromium, dist/firefox; run `pnpm build`
// first) into store-ready, reproducible archives under dist/packages/:
//
//   lockdown-chromium-<version>.zip   Chrome Web Store and Edge Add-ons
//   lockdown-firefox-<version>.zip    addons.mozilla.org (an .xpi is this zip)
//
// The Chromium store package leaves out the manifest's `key`: the stores
// assign their own id, and the development key fixes only the unpacked
// build's (docs/plan/24-release.md §6). Everything else is the tested build,
// byte for byte.
//
// Reproducible (docs/plan/24-release.md §5): entries sorted by path, every
// timestamp set from SOURCE_DATE_EPOCH (else the last commit's time, else
// 1980-01-01), one file mode, no extra fields, and entries stored rather
// than deflated, so the bytes depend on the files alone and not on the zlib
// a given Node carries. The same dist/ gives the same archive on any machine.
import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import { mkdir, readdir, readFile, writeFile } from "node:fs/promises"
import { dirname, join, relative, sep } from "node:path"
import { fileURLToPath } from "node:url"

const here = join(dirname(fileURLToPath(import.meta.url)), "..")
const dist = join(here, "dist")
const out = join(dist, "packages")

/** Seconds since the epoch every entry is stamped with. */
export function sourceDateEpoch() {
  const env = process.env.SOURCE_DATE_EPOCH
  if (env && /^\d+$/.test(env)) return Number(env)
  try {
    const t = execFileSync("git", ["log", "-1", "--format=%ct"], { cwd: here, encoding: "utf8" }).trim()
    if (/^\d+$/.test(t)) return Number(t)
  } catch {
    // Not a git checkout: fall through to the zip format's earliest date.
  }
  return 315532800
}

/** A DOS date and time (UTC; the zip format has no zone) for `epoch`. */
function dosTime(epoch) {
  const d = new Date(Math.max(epoch, 315532800) * 1000)
  const time = (d.getUTCHours() << 11) | (d.getUTCMinutes() << 5) | Math.floor(d.getUTCSeconds() / 2)
  const date = ((d.getUTCFullYear() - 1980) << 9) | ((d.getUTCMonth() + 1) << 5) | d.getUTCDate()
  return { time, date }
}

const CRC_TABLE = (() => {
  const t = new Uint32Array(256)
  for (let n = 0; n < 256; n++) {
    let c = n
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
    t[n] = c >>> 0
  }
  return t
})()

export function crc32(bytes) {
  let c = 0xffffffff
  for (const b of bytes) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8)
  return (c ^ 0xffffffff) >>> 0
}

/** Every file under `dir`, as forward-slash paths relative to it, sorted. */
async function walk(dir, base = dir) {
  const found = []
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const p = join(dir, entry.name)
    if (entry.isDirectory()) found.push(...(await walk(p, base)))
    else if (entry.isFile()) found.push(relative(base, p).split(sep).join("/"))
  }
  return found.sort((a, b) => (Buffer.from(a) < Buffer.from(b) ? -1 : a === b ? 0 : 1))
}

/** A stored (uncompressed) zip of `files` ([name, bytes], in order). */
export function zip(files, epoch) {
  const { time, date } = dosTime(epoch)
  const locals = []
  const centrals = []
  let offset = 0
  for (const [name, data] of files) {
    const nameBytes = Buffer.from(name, "utf8")
    const crc = crc32(data)
    const local = Buffer.alloc(30)
    local.writeUInt32LE(0x04034b50, 0)
    local.writeUInt16LE(20, 4) // version needed: 2.0
    local.writeUInt16LE(0x0800, 6) // UTF-8 names
    local.writeUInt16LE(0, 8) // stored
    local.writeUInt16LE(time, 10)
    local.writeUInt16LE(date, 12)
    local.writeUInt32LE(crc, 14)
    local.writeUInt32LE(data.length, 18)
    local.writeUInt32LE(data.length, 22)
    local.writeUInt16LE(nameBytes.length, 26)
    local.writeUInt16LE(0, 28)
    locals.push(local, nameBytes, data)
    const central = Buffer.alloc(46)
    central.writeUInt32LE(0x02014b50, 0)
    central.writeUInt16LE((3 << 8) | 20, 4) // made by: Unix, 2.0
    central.writeUInt16LE(20, 6)
    central.writeUInt16LE(0x0800, 8)
    central.writeUInt16LE(0, 10)
    central.writeUInt16LE(time, 12)
    central.writeUInt16LE(date, 14)
    central.writeUInt32LE(crc, 16)
    central.writeUInt32LE(data.length, 20)
    central.writeUInt32LE(data.length, 24)
    central.writeUInt16LE(nameBytes.length, 28)
    central.writeUInt16LE(0, 30) // extra
    central.writeUInt16LE(0, 32) // comment
    central.writeUInt16LE(0, 34) // disk
    central.writeUInt16LE(0, 36) // internal attributes
    central.writeUInt32LE(((0o100644 << 16) >>> 0) >>> 0, 38) // -rw-r--r--
    central.writeUInt32LE(offset, 42)
    centrals.push(central, nameBytes)
    offset += 30 + nameBytes.length + data.length
  }
  const centralSize = centrals.reduce((n, b) => n + b.length, 0)
  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0)
  end.writeUInt16LE(files.length, 8)
  end.writeUInt16LE(files.length, 10)
  end.writeUInt32LE(centralSize, 12)
  end.writeUInt32LE(offset, 16)
  return Buffer.concat([...locals, ...centrals, end])
}

/** The files of one family's build, with the store manifest swapped in. */
async function familyFiles(family) {
  const dir = join(dist, family)
  const names = await walk(dir)
  const files = []
  for (const name of names) {
    let data = await readFile(join(dir, name))
    if (name === "manifest.json") {
      const manifest = JSON.parse(data.toString("utf8"))
      if (family === "chromium") delete manifest.key
      data = Buffer.from(JSON.stringify(manifest, null, 2) + "\n", "utf8")
    }
    files.push([name, data])
  }
  return files
}

async function main() {
  const epoch = sourceDateEpoch()
  await mkdir(out, { recursive: true })
  const lines = []
  for (const family of ["chromium", "firefox"]) {
    const files = await familyFiles(family)
    const manifest = JSON.parse(files.find(([n]) => n === "manifest.json")[1].toString("utf8"))
    const name = `lockdown-${family}-${manifest.version}.zip`
    const bytes = zip(files, epoch)
    await writeFile(join(out, name), bytes)
    const sha = createHash("sha256").update(bytes).digest("hex")
    lines.push(`${sha}  ${name}`)
    console.log(`pack: ${name} (${files.length} files, ${bytes.length} bytes) ${sha}`)
  }
  await writeFile(join(out, "SHA256SUMS"), lines.join("\n") + "\n")
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  await main()
}
