// Stable ids that look like the core's (UUIDv7 text), made from a small
// number so every capture names the same item the same way.

function hex(seed: number, len: number): string {
  let x = (seed * 2654435761) >>> 0
  let out = ""
  while (out.length < len) {
    x = (Math.imul(x ^ (x >>> 15), 2246822507) + 0x9e3779b9) >>> 0
    out += x.toString(16).padStart(8, "0")
  }
  return out.slice(0, len)
}

/** An id in the core's shape: `0192f3a1-…-7…-8…-…`. */
export function uid(kind: string, n: number): string {
  const salt = [...kind].reduce((a, c) => (a * 31 + c.charCodeAt(0)) >>> 0, 7)
  const h = hex(n * 977 + salt, 28)
  return `0192${h.slice(0, 4)}-${h.slice(4, 8)}-7${h.slice(8, 11)}-8${h.slice(11, 14)}-${h.slice(14, 26)}`
}
