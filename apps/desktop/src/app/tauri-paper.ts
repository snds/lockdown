import { invoke } from "@tauri-apps/api/core"

import type { PaperClient } from "@/features/paper/paper-client"

/** The body `kit_save_pdf` takes: the notes as a NUL-terminated JSON list, then the page's bytes (`kit_create`'s own shape). */
export function kitBody(lines: string[], page: string): Uint8Array {
  const encoder = new TextEncoder()
  const notes = encoder.encode(JSON.stringify(lines))
  const text = encoder.encode(page)
  const body = new Uint8Array(notes.length + 1 + text.length)
  body.set(notes, 0)
  body.set(text, notes.length + 1)
  text.fill(0)
  return body
}

/**
 * The desktop's paper client (apps/desktop/src-tauri/src/paper.rs): the
 * shell opens the webview's own print panel, and lays the kit out and
 * writes it where the person picks in the OS's save dialog. The webview
 * never holds the PDF; the page's bytes it sends are wiped once sent.
 * Only `App.tsx` imports this; feature code reaches it through
 * `PaperClientProvider`.
 */
export const tauriPaperClient: PaperClient = {
  print: async () => {
    await invoke("print_page")
    return "open"
  },
  saveKitPdf: async (lines, page) => {
    const body = kitBody(lines, page)
    try {
      return (await invoke<boolean>("kit_save_pdf", body)) ? "saved" : "cancelled"
    } finally {
      body.fill(0)
    }
  },
}
