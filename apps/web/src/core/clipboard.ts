import type { Copied } from "@/core/types"

/**
 * Copy and clear in the browser (docs/plan/17-web-vault.md §5). On the
 * desktop a copy goes core → clipboard on the Rust side and the webview
 * never holds the value (D85, D105); a web page has no side but the page,
 * so the value comes to it as bytes, is written with the Clipboard API,
 * and only its SHA-256 is kept to recognise it later. After the timer
 * (30 s by default, 10–90) it is cleared if the clipboard still holds it;
 * when the browser won't let the page read the clipboard to check, it is
 * cleared anyway, unless another copy of ours came since.
 */

const MIN_SECS = 10
const MAX_SECS = 90
const DEFAULT_SECS = 30

export function clearAfter(secs: number | undefined): number {
  if (secs === undefined || !Number.isFinite(secs)) return DEFAULT_SECS
  return Math.min(MAX_SECS, Math.max(MIN_SECS, Math.round(secs)))
}

async function digest(text: string): Promise<string> {
  const bytes = new TextEncoder().encode(text)
  const hash = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))
  bytes.fill(0)
  return Array.from(hash, (b) => b.toString(16).padStart(2, "0")).join("")
}

/** The clipboard as the page can reach it. Tests give a fake one. */
export interface ClipboardLike {
  writeText(text: string): Promise<void>
  readText(): Promise<string>
}

interface Held {
  copy: number
  digest: string
  timer: ReturnType<typeof setTimeout>
}

export class WebClipboard {
  private generation = 0
  private held: Held | null = null
  private readonly clipboard: ClipboardLike

  constructor(clipboard: ClipboardLike) {
    this.clipboard = clipboard
  }

  /** Writes `bytes` (wiped here) and starts the clear timer. */
  async copy(bytes: Uint8Array, clearAfterSecs?: number): Promise<Copied> {
    const text = new TextDecoder().decode(bytes)
    bytes.fill(0)
    await this.clipboard.writeText(text)
    const hash = await digest(text)
    const secs = clearAfter(clearAfterSecs)
    if (this.held) clearTimeout(this.held.timer)
    const copy = ++this.generation
    this.held = { copy, digest: hash, timer: setTimeout(() => void this.clear(copy), secs * 1000) }
    return { clears_in_secs: secs, copy }
  }

  /** Whether copy `copy` is still ours to clear (the last one, not cleared). */
  holds(copy: number): boolean {
    return this.held?.copy === copy
  }

  /** Clears for copy `copy`, only while it is the last and still on the clipboard. */
  async clear(copy: number): Promise<boolean> {
    const held = this.held
    if (!held || held.copy !== copy) return false
    let current: string | null = null
    try {
      current = await this.clipboard.readText()
    } catch {
      current = null
    }
    if (current !== null && (await digest(current)) !== held.digest) {
      // The person copied something else since: theirs, left alone.
      this.forget()
      return false
    }
    await this.clipboard.writeText("")
    this.forget()
    return true
  }

  /** Forgets the copy (on lock): the timer stops, nothing is compared later. */
  forget() {
    if (this.held) clearTimeout(this.held.timer)
    this.held = null
  }
}
