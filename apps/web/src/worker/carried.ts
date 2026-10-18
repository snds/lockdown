/**
 * What one visit hands the next in this browser (crates/web `carried`):
 * the server's pinned log key and newest tree head, and each vault's newest
 * accepted head. Hashes and public keys only, kept in IndexedDB under the
 * login name, so a later sign-in here holds the server to what this browser
 * saw. Never a key, a value or a title; losing it only loses those checks.
 */

const DB = "lockdown-web"
const STORE = "carried"

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB, 1)
    req.onupgradeneeded = () => req.result.createObjectStore(STORE)
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

async function run<T>(mode: IDBTransactionMode, f: (s: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const db = await open()
  try {
    return await new Promise<T>((resolve, reject) => {
      const req = f(db.transaction(STORE, mode).objectStore(STORE))
      req.onsuccess = () => resolve(req.result)
      req.onerror = () => reject(req.error)
    })
  } finally {
    db.close()
  }
}

/** What an earlier visit left for `login`, or `null` (none, or no IndexedDB here). */
export async function loadCarried(login: string): Promise<Uint8Array | null> {
  try {
    const v = await run<unknown>("readonly", (s) => s.get(login))
    return v instanceof Uint8Array ? v : null
  } catch {
    return null
  }
}

/** Keeps what this visit hands on; a browser that refuses storage just keeps nothing. */
export async function saveCarried(login: string, bytes: Uint8Array): Promise<void> {
  try {
    await run("readwrite", (s) => s.put(bytes, login))
  } catch {
    // Private windows and blocked storage: the next visit starts fresh.
  }
}
