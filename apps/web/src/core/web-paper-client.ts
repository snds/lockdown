import type { PaperClient } from "@/features/paper/paper-client"

import type { Link } from "./link"

/** The name the browser's download suggests, the desktop's save dialog's own. */
export const KIT_FILE_NAME = "Lockdown Recovery Kit.pdf"

/** Hands `bytes` to the browser as a file download named `name`. */
export type Download = (bytes: Uint8Array, name: string) => void

/**
 * A download through a link to an object URL. The browser copies the bytes
 * into its own blob, which the page cannot wipe; the URL is revoked once
 * the download has started, so nothing in the page can fetch it again.
 */
export const browserDownload: Download = (bytes, name) => {
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: "application/pdf" }))
  const a = document.createElement("a")
  a.href = url
  a.download = name
  a.rel = "noopener"
  a.hidden = true
  document.body.append(a)
  try {
    a.click()
  } finally {
    a.remove()
    window.setTimeout(() => URL.revokeObjectURL(url), 1000)
  }
}

/**
 * The web vault's paper (docs/plan/38-mac-fixes.md §3–§4): the browser's
 * own print, which blocks until its dialog closes, and the recovery kit
 * as the core's PDF (`kit_pdf`, the bytes the desktop writes), handed to
 * the browser as a download. The PDF's bytes are wiped once handed over.
 */
export function webPaperClient(link: Link, download: Download = browserDownload): PaperClient {
  return {
    print: async () => {
      window.print()
      return "done"
    },
    saveKitPdf: async (lines, page) => {
      const bytes = await link.callBytes("kit_pdf", { lines, page })
      try {
        download(bytes, KIT_FILE_NAME)
      } finally {
        bytes.fill(0)
      }
      return "saved"
    },
  }
}
