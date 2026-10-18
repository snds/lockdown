// The store packages (scripts/pack.mjs, docs/plan/24-release.md §5–§6):
// each holds the tested build file for file, the Chromium one without the
// development key, and two packings of one build are the same bytes.
import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readdir, readFile } from "node:fs/promises";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const version = JSON.parse(execFileSync("node", ["-p", "JSON.stringify(require('./manifest.base.json'))"], { cwd: root, encoding: "utf8" })).version as string;

function pack(epoch: string) {
  execFileSync("node", ["scripts/pack.mjs"], { cwd: root, env: { ...process.env, SOURCE_DATE_EPOCH: epoch } });
}

async function packageBytes(family: string) {
  return readFile(join(root, "dist", "packages", `lockdown-${family}-${version}.zip`));
}

/** Reads a stored zip's central directory: name → [bytes, dos time, dos date]. */
function unzip(buf: Buffer) {
  const end = buf.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  expect(end).toBeGreaterThan(0);
  const count = buf.readUInt16LE(end + 10);
  let at = buf.readUInt32LE(end + 16);
  const entries = new Map<string, { data: Buffer; time: number; date: number; method: number }>();
  const order: string[] = [];
  for (let i = 0; i < count; i++) {
    expect(buf.readUInt32LE(at)).toBe(0x02014b50);
    const method = buf.readUInt16LE(at + 10);
    const time = buf.readUInt16LE(at + 12);
    const date = buf.readUInt16LE(at + 14);
    const size = buf.readUInt32LE(at + 24);
    const nameLen = buf.readUInt16LE(at + 28);
    const extra = buf.readUInt16LE(at + 30);
    const comment = buf.readUInt16LE(at + 32);
    const local = buf.readUInt32LE(at + 42);
    const name = buf.subarray(at + 46, at + 46 + nameLen).toString("utf8");
    const localName = buf.readUInt16LE(local + 26);
    const localExtra = buf.readUInt16LE(local + 28);
    const start = local + 30 + localName + localExtra;
    entries.set(name, { data: buf.subarray(start, start + size), time, date, method });
    order.push(name);
    at += 46 + nameLen + extra + comment;
  }
  return { entries, order };
}

async function distFiles(family: string): Promise<string[]> {
  const base = join(root, "dist", family);
  const out: string[] = [];
  async function walk(dir: string) {
    for (const e of await readdir(dir, { withFileTypes: true })) {
      const p = join(dir, e.name);
      if (e.isDirectory()) await walk(p);
      else out.push(relative(base, p).split(sep).join("/"));
    }
  }
  await walk(base);
  return out.sort();
}

test("two packings of one build are the same bytes, and the date comes from SOURCE_DATE_EPOCH", async () => {
  pack("1760000000");
  const first = await Promise.all(["chromium", "firefox"].map(packageBytes));
  pack("1760000000");
  const second = await Promise.all(["chromium", "firefox"].map(packageBytes));
  for (const i of [0, 1]) {
    expect(createHash("sha256").update(first[i]).digest("hex")).toBe(createHash("sha256").update(second[i]).digest("hex"));
  }
  pack("1700000000");
  const other = await packageBytes("chromium");
  expect(other.equals(first[0])).toBe(false);
  // 1760000000 is 2025-10-09T08:53:20Z.
  const { entries } = unzip(first[0]);
  const any = [...entries.values()][0];
  expect(any.date).toBe(((2025 - 1980) << 9) | (10 << 5) | 9);
  expect(any.time).toBe((8 << 11) | (53 << 5) | 10);
  const sums = await readFile(join(root, "dist", "packages", "SHA256SUMS"), "utf8");
  expect(sums).toContain(`lockdown-chromium-${version}.zip`);
  expect(sums).toContain(`lockdown-firefox-${version}.zip`);
});

test("each package is the tested build, sorted and stored; the Chromium one leaves the dev key out", async () => {
  pack("1760000000");
  for (const family of ["chromium", "firefox"]) {
    const { entries, order } = unzip(await packageBytes(family));
    expect(order).toEqual([...order].sort((a, b) => (Buffer.from(a) < Buffer.from(b) ? -1 : 1)));
    expect(new Set(order)).toEqual(new Set(await distFiles(family)));
    for (const [name, e] of entries) {
      expect(e.method, name).toBe(0);
      if (name === "manifest.json") continue;
      expect(e.data.equals(await readFile(join(root, "dist", family, name))), name).toBe(true);
    }
    const manifest = JSON.parse(entries.get("manifest.json")!.data.toString("utf8"));
    const built = JSON.parse(await readFile(join(root, "dist", family, "manifest.json"), "utf8"));
    expect(manifest.key).toBeUndefined();
    delete built.key;
    expect(manifest).toEqual(built);
    if (family === "firefox") expect(manifest.browser_specific_settings.gecko.id).toBe("lockdown@snds.design");
  }
});
