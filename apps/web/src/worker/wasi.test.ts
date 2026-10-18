// @vitest-environment node
import { describe, expect, it } from "vitest"

import { ERRNO, MemoryFs, Wasi } from "./wasi"

/** A WASI instance over a fresh memory, and helpers to call it as the core would. */
function setup() {
  const fs = new MemoryFs()
  const lines: string[] = []
  const wasi = new Wasi({ fs, env: { LOCKDOWN_DEVICE_NAME: "test" }, log: (l) => lines.push(l) })
  const memory = new WebAssembly.Memory({ initial: 1 })
  wasi.bind(memory)
  const imp = wasi.imports() as unknown as Record<string, (...a: unknown[]) => number>
  const bytes = () => new Uint8Array(memory.buffer)
  const view = () => new DataView(memory.buffer)
  const put = (at: number, text: string) => {
    const b = new TextEncoder().encode(text)
    bytes().set(b, at)
    return b.length
  }
  const open = (path: string, oflags: number, fdflags = 0): number | { errno: number } => {
    const len = put(1000, path)
    const errno = imp.path_open!(3, 0, 1000, len, oflags, 0n, 0n, fdflags, 2000)
    return errno === ERRNO.SUCCESS ? view().getUint32(2000, true) : { errno }
  }
  const write = (fd: number, text: string) => {
    const len = put(3000, text)
    view().setUint32(100, 3000, true)
    view().setUint32(104, len, true)
    return imp.fd_write!(fd, 100, 1, 200)
  }
  const read = (fd: number, max = 64): string => {
    view().setUint32(100, 4000, true)
    view().setUint32(104, max, true)
    imp.fd_read!(fd, 100, 1, 200)
    const n = view().getUint32(200, true)
    return new TextDecoder().decode(bytes().slice(4000, 4000 + n))
  }
  const path = (fn: string, p: string) => {
    const len = put(1000, p)
    return imp[fn]!(3, 1000, len)
  }
  return { fs, lines, imp, open, write, read, path, view, put }
}

describe("the worker's WASI file system", () => {
  it("creates, writes, reads back and truncates a file", () => {
    const t = setup()
    expect(t.path("path_create_directory", "lockdown")).toBe(ERRNO.SUCCESS)
    expect(t.path("path_create_directory", "lockdown")).toBe(ERRNO.EXIST)
    const fd = t.open("lockdown/a.cbor", 1 /* CREAT */) as number
    expect(t.write(fd, "hello")).toBe(ERRNO.SUCCESS)
    const again = t.open("lockdown/a.cbor", 0) as number
    expect(t.read(again)).toBe("hello")
    expect(t.fs.readFile("/lockdown/a.cbor")).toEqual(new TextEncoder().encode("hello"))
    t.open("lockdown/a.cbor", 8 /* TRUNC */)
    expect(t.fs.readFile("/lockdown/a.cbor")).toEqual(new Uint8Array(0))
  })

  it("refuses a missing file, a missing directory and a climb out of the root", () => {
    const t = setup()
    expect(t.open("nope.cbor", 0)).toEqual({ errno: ERRNO.NOENT })
    expect(t.open("no/such/dir.cbor", 1)).toEqual({ errno: ERRNO.NOENT })
    expect(t.open("../escape", 1)).toEqual({ errno: ERRNO.ACCES })
    expect(t.open("a", 1 | 4 /* EXCL */)).not.toHaveProperty("errno")
    expect(t.open("a", 1 | 4)).toEqual({ errno: ERRNO.EXIST })
  })

  it("wipes the bytes of a file it removes or replaces", () => {
    const t = setup()
    t.fs.writeFile("/k/device-key.cbor", new Uint8Array([1, 2, 3, 4]))
    const node = t.fs.lookup(t.fs.root, "/k/device-key.cbor")
    if (typeof node === "number" || !node.node || node.node.kind !== "file") throw new Error("no file")
    const held = node.node.data
    expect(t.path("path_unlink_file", "k/device-key.cbor")).toBe(ERRNO.SUCCESS)
    expect(held.every((b) => b === 0)).toBe(true)
    expect(t.fs.readFile("/k/device-key.cbor")).toBeNull()

    t.fs.writeFile("/k/old", new Uint8Array([9, 9]))
    t.fs.writeFile("/k/new.tmp", new Uint8Array([7]))
    const old = t.fs.lookup(t.fs.root, "/k/old")
    if (typeof old === "number" || !old.node || old.node.kind !== "file") throw new Error("no file")
    const oldBytes = old.node.data
    const a = t.put(1000, "k/new.tmp")
    const b = t.put(1100, "k/old")
    expect(t.imp.path_rename!(3, 1000, a, 3, 1100, b)).toBe(ERRNO.SUCCESS)
    expect(oldBytes.every((x) => x === 0)).toBe(true)
    expect(t.fs.readFile("/k/old")).toEqual(new Uint8Array([7]))
    expect(t.fs.readFile("/k/new.tmp")).toBeNull()
  })

  it("lists a directory in name order and removes only empty ones", () => {
    const t = setup()
    t.fs.writeFile("/d/b", new Uint8Array([1]))
    t.fs.writeFile("/d/a", new Uint8Array([1]))
    expect(t.path("path_remove_directory", "d")).toBe(ERRNO.NOTEMPTY)
    const fd = t.open("d", 2 /* DIRECTORY */) as number
    expect(t.imp.fd_readdir!(fd, 5000, 512, 0n, 200)).toBe(ERRNO.SUCCESS)
    const used = t.view().getUint32(200, true)
    const raw = new Uint8Array(t.view().buffer.slice(5000, 5000 + used))
    const names: string[] = []
    for (let at = 0; at < raw.length; ) {
      const len = new DataView(raw.buffer).getUint32(at + 16, true)
      names.push(new TextDecoder().decode(raw.slice(at + 24, at + 24 + len)))
      at += 24 + len
    }
    expect(names).toEqual(["a", "b"])
    t.path("path_unlink_file", "d/a")
    t.path("path_unlink_file", "d/b")
    expect(t.path("path_remove_directory", "d")).toBe(ERRNO.SUCCESS)
  })

  it("gives the clock, the environment and stderr; refuses what the core never asks for", () => {
    const t = setup()
    expect(t.imp.clock_time_get!(0, 0n, 300)).toBe(ERRNO.SUCCESS)
    const ns = t.view().getBigUint64(300, true)
    expect(Number(ns / 1_000_000n)).toBeGreaterThan(Date.now() - 5000)
    expect(t.imp.environ_sizes_get!(400, 404)).toBe(ERRNO.SUCCESS)
    expect(t.view().getUint32(400, true)).toBe(1)
    const len = t.put(3000, "panicked here\n")
    t.view().setUint32(100, 3000, true)
    t.view().setUint32(104, len, true)
    t.imp.fd_write!(2, 100, 1, 200)
    expect(t.lines).toEqual(["panicked here"])
    expect(t.imp.sock_accept!()).toBe(ERRNO.NOSYS)
  })
})
