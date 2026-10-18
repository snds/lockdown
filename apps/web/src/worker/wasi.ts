/**
 * The smallest WASI preview1 the core needs, over a file system that lives
 * in this worker's memory and dies with the tab (docs/plan/17-web-vault.md
 * §3). The core is `lockdown-app`, compiled for `wasm32-wasip1`: it keeps
 * its identity root and data directory as files, reads the clock and the
 * random source, and writes panics to stderr. Nothing here reaches the
 * network or the disk.
 *
 * Every byte a file held is overwritten with zeros when the file is
 * truncated, grown into a new buffer, replaced by a rename, or removed:
 * the device key file is one of them while the vault is open.
 *
 * Calls the core never makes answer `ENOSYS`.
 */

export const ERRNO = {
  SUCCESS: 0,
  ACCES: 2,
  BADF: 8,
  EXIST: 20,
  INVAL: 28,
  IO: 29,
  ISDIR: 31,
  NOENT: 44,
  NOSYS: 52,
  NOTDIR: 54,
  NOTEMPTY: 55,
} as const

const FILETYPE_CHARACTER_DEVICE = 2
const FILETYPE_DIRECTORY = 3
const FILETYPE_REGULAR_FILE = 4

const OFLAGS_CREAT = 1
const OFLAGS_DIRECTORY = 2
const OFLAGS_EXCL = 4
const OFLAGS_TRUNC = 8
const FDFLAGS_APPEND = 1

const ALL_RIGHTS = 0xffff_ffff_ffff_ffffn

/** A file's bytes: `data` is the buffer, `size` how much of it is the file. */
export interface FileNode {
  kind: "file"
  data: Uint8Array
  size: number
  ino: bigint
}

export interface DirNode {
  kind: "dir"
  entries: Map<string, Node>
  ino: bigint
}

export type Node = FileNode | DirNode

interface OpenFile {
  node: Node
  pos: number
  append: boolean
}

/** Overwrites a buffer with zeros. */
function wipe(bytes: Uint8Array) {
  bytes.fill(0)
}

/** Overwrites every file under `node` with zeros. */
function wipeTree(node: Node) {
  if (node.kind === "file") wipe(node.data)
  else for (const child of node.entries.values()) wipeTree(child)
}

/** The in-memory file system: one root directory. */
export class MemoryFs {
  private next = 1n
  readonly root: DirNode = this.dir()

  dir(): DirNode {
    return { kind: "dir", entries: new Map(), ino: this.next++ }
  }

  file(): FileNode {
    return { kind: "file", data: new Uint8Array(0), size: 0, ino: this.next++ }
  }

  /** The node at `path` under `from` (no `..` above `from`), and its parent. */
  lookup(from: DirNode, path: string): { parent: DirNode | null; name: string; node: Node | undefined } | number {
    const parts = path.split("/").filter((p) => p !== "" && p !== ".")
    if (parts.includes("..")) return ERRNO.ACCES
    if (parts.length === 0) return { parent: null, name: "", node: from }
    let dir: DirNode = from
    for (let i = 0; i < parts.length - 1; i++) {
      const next = dir.entries.get(parts[i]!)
      if (!next) return ERRNO.NOENT
      if (next.kind !== "dir") return ERRNO.NOTDIR
      dir = next
    }
    const name = parts[parts.length - 1]!
    return { parent: dir, name, node: dir.entries.get(name) }
  }

  /** The whole file at `path` (absolute), or `null`. */
  readFile(path: string): Uint8Array | null {
    const found = this.lookup(this.root, path)
    if (typeof found === "number" || !found.node || found.node.kind !== "file") return null
    return found.node.data.slice(0, found.node.size)
  }

  /** Writes a whole file at `path` (absolute), making its directories. */
  writeFile(path: string, bytes: Uint8Array) {
    const parts = path.split("/").filter((p) => p !== "")
    let dir = this.root
    for (const part of parts.slice(0, -1)) {
      let next = dir.entries.get(part)
      if (!next) {
        next = this.dir()
        dir.entries.set(part, next)
      }
      if (next.kind !== "dir") throw new Error(`${part} isn't a directory`)
      dir = next
    }
    const name = parts[parts.length - 1]!
    const old = dir.entries.get(name)
    if (old) wipeTree(old)
    const f = this.file()
    f.data = bytes.slice()
    f.size = bytes.length
    dir.entries.set(name, f)
  }

  /** Overwrites and forgets everything. */
  clear() {
    wipeTree(this.root)
    this.root.entries.clear()
  }
}

/** Grows a file's buffer to hold `size` bytes, wiping the old buffer. */
function ensure(file: FileNode, size: number) {
  if (size <= file.data.length) return
  const grown = new Uint8Array(Math.max(size, file.data.length * 2, 256))
  grown.set(file.data.subarray(0, file.size))
  wipe(file.data)
  file.data = grown
}

export interface WasiOptions {
  fs: MemoryFs
  env: Record<string, string>
  /** Where stdout and stderr lines go. */
  log: (line: string) => void
}

/** Thrown by `proc_exit`. */
export class WasiExit extends Error {
  readonly code: number
  constructor(code: number) {
    super(`the core exited with ${code}`)
    this.code = code
  }
}

/**
 * The preview1 imports over `options.fs`. Call `bind(memory)` once the
 * instance exists, before the first export runs.
 */
export class Wasi {
  private memory: WebAssembly.Memory | null = null
  private fds = new Map<number, OpenFile>()
  private nextFd = 4
  private out: Record<number, string> = { 1: "", 2: "" }
  private readonly encoder = new TextEncoder()
  private readonly decoder = new TextDecoder()
  private readonly options: WasiOptions

  constructor(options: WasiOptions) {
    this.options = options
    // 0–2 are the standard streams; 3 is the one preopened directory, "/".
    this.fds.set(3, { node: options.fs.root, pos: 0, append: false })
  }

  bind(memory: WebAssembly.Memory) {
    this.memory = memory
  }

  private view(): DataView {
    if (!this.memory) throw new Error("the WASI memory isn't bound yet")
    return new DataView(this.memory.buffer)
  }

  private bytes(ptr: number, len: number): Uint8Array {
    if (!this.memory) throw new Error("the WASI memory isn't bound yet")
    return new Uint8Array(this.memory.buffer, ptr, len)
  }

  private str(ptr: number, len: number): string {
    return this.decoder.decode(this.bytes(ptr, len).slice())
  }

  private dirOf(fd: number): DirNode | number {
    const open = this.fds.get(fd)
    if (!open) return ERRNO.BADF
    if (open.node.kind !== "dir") return ERRNO.NOTDIR
    return open.node
  }

  private writeFilestat(ptr: number, node: Node) {
    const v = this.view()
    v.setBigUint64(ptr, 0n, true)
    v.setBigUint64(ptr + 8, node.ino, true)
    v.setUint8(ptr + 16, node.kind === "dir" ? FILETYPE_DIRECTORY : FILETYPE_REGULAR_FILE)
    v.setBigUint64(ptr + 24, 1n, true)
    v.setBigUint64(ptr + 32, BigInt(node.kind === "file" ? node.size : 0), true)
    v.setBigUint64(ptr + 40, 0n, true)
    v.setBigUint64(ptr + 48, 0n, true)
    v.setBigUint64(ptr + 56, 0n, true)
  }

  private flush(fd: number, text: string) {
    const all = (this.out[fd] ?? "") + text
    const lines = all.split("\n")
    this.out[fd] = lines.pop() ?? ""
    for (const line of lines) this.options.log(line)
  }

  /** The import object for `wasi_snapshot_preview1`. */
  imports(): Record<string, (...args: never[]) => number | void> {
    const impl: Record<string, (...args: never[]) => number | void> = {
      args_sizes_get: (argc: number, size: number) => {
        const v = this.view()
        v.setUint32(argc, 0, true)
        v.setUint32(size, 0, true)
        return ERRNO.SUCCESS
      },
      args_get: () => ERRNO.SUCCESS,
      environ_sizes_get: (count: number, size: number) => {
        const entries = Object.entries(this.options.env).map(([k, val]) => this.encoder.encode(`${k}=${val}\0`))
        const v = this.view()
        v.setUint32(count, entries.length, true)
        v.setUint32(size, entries.reduce((n, e) => n + e.length, 0), true)
        return ERRNO.SUCCESS
      },
      environ_get: (environ: number, buf: number) => {
        const v = this.view()
        let at = buf
        Object.entries(this.options.env).forEach(([k, val], i) => {
          const e = this.encoder.encode(`${k}=${val}\0`)
          this.bytes(at, e.length).set(e)
          v.setUint32(environ + i * 4, at, true)
          at += e.length
        })
        return ERRNO.SUCCESS
      },
      clock_time_get: (id: number, _precision: bigint, out: number) => {
        const ns = id === 0 ? BigInt(Date.now()) * 1_000_000n : BigInt(Math.round(performance.now() * 1_000_000))
        this.view().setBigUint64(out, ns, true)
        return ERRNO.SUCCESS
      },
      clock_res_get: (_id: number, out: number) => {
        this.view().setBigUint64(out, 1_000n, true)
        return ERRNO.SUCCESS
      },
      random_get: (ptr: number, len: number) => {
        for (let at = 0; at < len; at += 65536) {
          crypto.getRandomValues(this.bytes(ptr + at, Math.min(65536, len - at)))
        }
        return ERRNO.SUCCESS
      },
      sched_yield: () => ERRNO.SUCCESS,
      proc_exit: (code: number) => {
        throw new WasiExit(code)
      },
      fd_prestat_get: (fd: number, buf: number) => {
        if (fd !== 3) return ERRNO.BADF
        const v = this.view()
        v.setUint8(buf, 0)
        v.setUint32(buf + 4, 1, true)
        return ERRNO.SUCCESS
      },
      fd_prestat_dir_name: (fd: number, path: number, len: number) => {
        if (fd !== 3) return ERRNO.BADF
        if (len < 1) return ERRNO.INVAL
        this.bytes(path, 1).set(this.encoder.encode("/"))
        return ERRNO.SUCCESS
      },
      fd_fdstat_get: (fd: number, buf: number) => {
        const v = this.view()
        let type = FILETYPE_CHARACTER_DEVICE
        let flags = 0
        if (fd > 2) {
          const open = this.fds.get(fd)
          if (!open) return ERRNO.BADF
          type = open.node.kind === "dir" ? FILETYPE_DIRECTORY : FILETYPE_REGULAR_FILE
          flags = open.append ? FDFLAGS_APPEND : 0
        }
        v.setUint8(buf, type)
        v.setUint16(buf + 2, flags, true)
        v.setBigUint64(buf + 8, ALL_RIGHTS, true)
        v.setBigUint64(buf + 16, ALL_RIGHTS, true)
        return ERRNO.SUCCESS
      },
      fd_fdstat_set_flags: () => ERRNO.SUCCESS,
      fd_filestat_get: (fd: number, buf: number) => {
        const open = this.fds.get(fd)
        if (!open) return ERRNO.BADF
        this.writeFilestat(buf, open.node)
        return ERRNO.SUCCESS
      },
      fd_filestat_set_size: (fd: number, size: bigint) => {
        const open = this.fds.get(fd)
        if (!open) return ERRNO.BADF
        if (open.node.kind !== "file") return ERRNO.ISDIR
        const n = Number(size)
        ensure(open.node, n)
        if (n < open.node.size) wipe(open.node.data.subarray(n, open.node.size))
        open.node.size = n
        return ERRNO.SUCCESS
      },
      fd_filestat_set_times: () => ERRNO.SUCCESS,
      fd_advise: () => ERRNO.SUCCESS,
      fd_allocate: () => ERRNO.SUCCESS,
      fd_sync: () => ERRNO.SUCCESS,
      fd_datasync: () => ERRNO.SUCCESS,
      fd_close: (fd: number) => {
        if (fd <= 3) return ERRNO.SUCCESS
        return this.fds.delete(fd) ? ERRNO.SUCCESS : ERRNO.BADF
      },
      fd_seek: (fd: number, offset: bigint, whence: number, out: number) => {
        const open = this.fds.get(fd)
        if (!open) return ERRNO.BADF
        if (open.node.kind !== "file") return ERRNO.ISDIR
        const base = whence === 0 ? 0 : whence === 1 ? open.pos : open.node.size
        const pos = base + Number(offset)
        if (pos < 0) return ERRNO.INVAL
        open.pos = pos
        this.view().setBigUint64(out, BigInt(pos), true)
        return ERRNO.SUCCESS
      },
      fd_tell: (fd: number, out: number) => {
        const open = this.fds.get(fd)
        if (!open) return ERRNO.BADF
        this.view().setBigUint64(out, BigInt(open.pos), true)
        return ERRNO.SUCCESS
      },
      fd_read: (fd: number, iovs: number, count: number, nread: number) => {
        const open = this.fds.get(fd)
        const v = this.view()
        if (fd === 0) {
          v.setUint32(nread, 0, true)
          return ERRNO.SUCCESS
        }
        if (!open) return ERRNO.BADF
        if (open.node.kind !== "file") return ERRNO.ISDIR
        let total = 0
        for (let i = 0; i < count; i++) {
          const ptr = v.getUint32(iovs + i * 8, true)
          const len = v.getUint32(iovs + i * 8 + 4, true)
          const n = Math.max(0, Math.min(len, open.node.size - open.pos))
          this.bytes(ptr, n).set(open.node.data.subarray(open.pos, open.pos + n))
          open.pos += n
          total += n
          if (n < len) break
        }
        v.setUint32(nread, total, true)
        return ERRNO.SUCCESS
      },
      fd_pread: () => ERRNO.NOSYS,
      fd_pwrite: () => ERRNO.NOSYS,
      fd_write: (fd: number, iovs: number, count: number, nwritten: number) => {
        const v = this.view()
        let total = 0
        if (fd === 1 || fd === 2) {
          for (let i = 0; i < count; i++) {
            const ptr = v.getUint32(iovs + i * 8, true)
            const len = v.getUint32(iovs + i * 8 + 4, true)
            this.flush(fd, this.str(ptr, len))
            total += len
          }
          v.setUint32(nwritten, total, true)
          return ERRNO.SUCCESS
        }
        const open = this.fds.get(fd)
        if (!open) return ERRNO.BADF
        if (open.node.kind !== "file") return ERRNO.ISDIR
        const file = open.node
        if (open.append) open.pos = file.size
        for (let i = 0; i < count; i++) {
          const ptr = v.getUint32(iovs + i * 8, true)
          const len = v.getUint32(iovs + i * 8 + 4, true)
          ensure(file, open.pos + len)
          file.data.set(this.bytes(ptr, len), open.pos)
          open.pos += len
          file.size = Math.max(file.size, open.pos)
          total += len
        }
        v.setUint32(nwritten, total, true)
        return ERRNO.SUCCESS
      },
      fd_readdir: (fd: number, buf: number, len: number, cookie: bigint, used: number) => {
        const dir = this.dirOf(fd)
        if (typeof dir === "number") return dir
        const names = [...dir.entries.keys()].sort()
        const v = this.view()
        let at = 0
        for (let i = Number(cookie); i < names.length && at < len; i++) {
          const name = this.encoder.encode(names[i]!)
          const node = dir.entries.get(names[i]!)!
          const entry = new Uint8Array(24 + name.length)
          const ev = new DataView(entry.buffer)
          ev.setBigUint64(0, BigInt(i + 1), true)
          ev.setBigUint64(8, node.ino, true)
          ev.setUint32(16, name.length, true)
          ev.setUint8(20, node.kind === "dir" ? FILETYPE_DIRECTORY : FILETYPE_REGULAR_FILE)
          entry.set(name, 24)
          const n = Math.min(entry.length, len - at)
          this.bytes(buf + at, n).set(entry.subarray(0, n))
          at += n
        }
        v.setUint32(used, at, true)
        return ERRNO.SUCCESS
      },
      path_open: (
        dirfd: number,
        _dirflags: number,
        pathPtr: number,
        pathLen: number,
        oflags: number,
        _rightsBase: bigint,
        _rightsInheriting: bigint,
        fdflags: number,
        fdOut: number,
      ) => {
        const from = this.dirOf(dirfd)
        if (typeof from === "number") return from
        const found = this.options.fs.lookup(from, this.str(pathPtr, pathLen))
        if (typeof found === "number") return found
        let node = found.node
        if (node && oflags & OFLAGS_EXCL && oflags & OFLAGS_CREAT) return ERRNO.EXIST
        if (!node) {
          if (!(oflags & OFLAGS_CREAT) || !found.parent) return ERRNO.NOENT
          node = this.options.fs.file()
          found.parent.entries.set(found.name, node)
        }
        if (oflags & OFLAGS_DIRECTORY && node.kind !== "dir") return ERRNO.NOTDIR
        if (node.kind === "file" && oflags & OFLAGS_TRUNC) {
          wipe(node.data)
          node.size = 0
        }
        const fd = this.nextFd++
        this.fds.set(fd, { node, pos: 0, append: (fdflags & FDFLAGS_APPEND) !== 0 })
        this.view().setUint32(fdOut, fd, true)
        return ERRNO.SUCCESS
      },
      path_filestat_get: (dirfd: number, _flags: number, pathPtr: number, pathLen: number, buf: number) => {
        const from = this.dirOf(dirfd)
        if (typeof from === "number") return from
        const found = this.options.fs.lookup(from, this.str(pathPtr, pathLen))
        if (typeof found === "number") return found
        if (!found.node) return ERRNO.NOENT
        this.writeFilestat(buf, found.node)
        return ERRNO.SUCCESS
      },
      path_filestat_set_times: () => ERRNO.SUCCESS,
      path_create_directory: (dirfd: number, pathPtr: number, pathLen: number) => {
        const from = this.dirOf(dirfd)
        if (typeof from === "number") return from
        const found = this.options.fs.lookup(from, this.str(pathPtr, pathLen))
        if (typeof found === "number") return found
        if (found.node) return ERRNO.EXIST
        if (!found.parent) return ERRNO.INVAL
        found.parent.entries.set(found.name, this.options.fs.dir())
        return ERRNO.SUCCESS
      },
      path_remove_directory: (dirfd: number, pathPtr: number, pathLen: number) => {
        const from = this.dirOf(dirfd)
        if (typeof from === "number") return from
        const found = this.options.fs.lookup(from, this.str(pathPtr, pathLen))
        if (typeof found === "number") return found
        if (!found.node) return ERRNO.NOENT
        if (found.node.kind !== "dir") return ERRNO.NOTDIR
        if (found.node.entries.size > 0) return ERRNO.NOTEMPTY
        if (!found.parent) return ERRNO.ACCES
        found.parent.entries.delete(found.name)
        return ERRNO.SUCCESS
      },
      path_unlink_file: (dirfd: number, pathPtr: number, pathLen: number) => {
        const from = this.dirOf(dirfd)
        if (typeof from === "number") return from
        const found = this.options.fs.lookup(from, this.str(pathPtr, pathLen))
        if (typeof found === "number") return found
        if (!found.node) return ERRNO.NOENT
        if (found.node.kind === "dir") return ERRNO.ISDIR
        wipe(found.node.data)
        found.node.size = 0
        found.parent?.entries.delete(found.name)
        return ERRNO.SUCCESS
      },
      path_rename: (fromFd: number, oldPtr: number, oldLen: number, toFd: number, newPtr: number, newLen: number) => {
        const fromDir = this.dirOf(fromFd)
        const toDir = this.dirOf(toFd)
        if (typeof fromDir === "number") return fromDir
        if (typeof toDir === "number") return toDir
        const src = this.options.fs.lookup(fromDir, this.str(oldPtr, oldLen))
        const dst = this.options.fs.lookup(toDir, this.str(newPtr, newLen))
        if (typeof src === "number") return src
        if (typeof dst === "number") return dst
        if (!src.node || !src.parent) return ERRNO.NOENT
        if (!dst.parent) return ERRNO.INVAL
        if (dst.node && dst.node !== src.node) {
          if (dst.node.kind === "dir" && (src.node.kind !== "dir" || dst.node.entries.size > 0)) return ERRNO.NOTEMPTY
          wipeTree(dst.node)
        }
        src.parent.entries.delete(src.name)
        dst.parent.entries.set(dst.name, src.node)
        return ERRNO.SUCCESS
      },
      path_readlink: () => ERRNO.INVAL,
      path_symlink: () => ERRNO.NOSYS,
      path_link: () => ERRNO.NOSYS,
      poll_oneoff: () => ERRNO.NOSYS,
    }
    return new Proxy(impl, {
      get: (target, name: string) => target[name] ?? (() => ERRNO.NOSYS),
    })
  }
}
