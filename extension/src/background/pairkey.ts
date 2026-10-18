// The pairing key (docs/plan/16-browser-extension.md §3). Made once, sent
// to the desktop once in `pair_begin`, then kept only as a non-extractable
// WebCrypto HMAC key in this profile's IndexedDB: the extension can sign a
// challenge with it but cannot read it back, and neither can anything that
// reads the profile's storage through the extension's APIs.

const DB = "lockdown"
const STORE = "pairing"
const KEY = "client"

export interface Pairing {
  clientId: string
  key: CryptoKey
}

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1)
    req.onupgradeneeded = () => req.result.createObjectStore(STORE)
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

async function tx<T>(mode: IDBTransactionMode, run: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const db = await open()
  try {
    return await new Promise<T>((resolve, reject) => {
      const t = db.transaction(STORE, mode)
      const req = run(t.objectStore(STORE))
      req.onsuccess = () => resolve(req.result)
      req.onerror = () => reject(req.error)
    })
  } finally {
    db.close()
  }
}

export function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")
}

export function unhex(text: string): Uint8Array {
  const out = new Uint8Array(text.length / 2)
  for (let i = 0; i < out.length; i++) out[i] = parseInt(text.slice(i * 2, i * 2 + 2), 16)
  return out
}

/** The pairing this profile holds, if any. */
export async function load(): Promise<Pairing | null> {
  const v = await tx<Pairing | undefined>("readonly", (s) => s.get(KEY) as IDBRequest<Pairing | undefined>)
  return v ?? null
}

/** Makes a fresh client id and key, stores the key non-extractable, and
 *  returns the raw key once (hex) for `pair_begin`. The caller sends it and
 *  drops it; the bytes here are zeroed before returning. */
export async function create(): Promise<{ pairing: Pairing; keyHex: string }> {
  const id = crypto.getRandomValues(new Uint8Array(16))
  const raw = crypto.getRandomValues(new Uint8Array(32))
  const key = await crypto.subtle.importKey("raw", raw, { name: "HMAC", hash: "SHA-256" }, false, ["sign"])
  const keyHex = hex(raw)
  raw.fill(0)
  const pairing = { clientId: hex(id), key }
  await tx("readwrite", (s) => s.put(pairing, KEY))
  return { pairing, keyHex }
}

/** Forgets the pairing (the desktop refused it or forgot it). */
export async function forget(): Promise<void> {
  await tx("readwrite", (s) => s.delete(KEY))
}

const PREFIX = new TextEncoder().encode("lockdown-browser-auth/v1\0")

/** HMAC-SHA256(key, prefix ‖ challenge ‖ client id), hex. */
export async function proof(p: Pairing, challengeHex: string): Promise<string> {
  const challenge = unhex(challengeHex)
  const id = new TextEncoder().encode(p.clientId)
  const msg = new Uint8Array(PREFIX.length + challenge.length + id.length)
  msg.set(PREFIX, 0)
  msg.set(challenge, PREFIX.length)
  msg.set(id, PREFIX.length + challenge.length)
  const tag = new Uint8Array(await crypto.subtle.sign("HMAC", p.key, msg))
  return hex(tag)
}
