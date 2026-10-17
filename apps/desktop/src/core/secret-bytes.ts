/**
 * Secrets arrive from the shell as bytes (never a JSON string). The webview
 * cannot draw text from bytes without making a string, so a string exists
 * only while a field is revealed, inside the component that shows it, and
 * the bytes are zeroed as soon as they are decoded. Copy never comes here
 * at all: it goes from the core to the clipboard on the Rust side.
 */

const decoder = new TextDecoder()

/** Decodes once and wipes the buffer. The returned string is the caller's to drop. */
export function decodeAndWipe(bytes: Uint8Array): string {
  const text = decoder.decode(bytes)
  bytes.fill(0)
  return text
}

/** Encodes a typed secret for the bridge; the caller clears its input right after. */
export function encodeSecret(text: string): number[] {
  return Array.from(new TextEncoder().encode(text))
}

/** Splits a once-page body: a NUL-terminated JSON list of notes, then the page's bytes. */
export function splitOncePage(body: Uint8Array): { lines: string[]; page: Uint8Array } {
  const nul = body.indexOf(0)
  if (nul < 0) return { lines: [], page: body }
  const lines = JSON.parse(decoder.decode(body.subarray(0, nul))) as string[]
  return { lines, page: body.slice(nul + 1) }
}
