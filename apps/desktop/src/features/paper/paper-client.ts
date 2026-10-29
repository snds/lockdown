import { createContext, useCallback, useContext, useState } from "react"

/**
 * Paper: printing a page, and saving the recovery kit as a file
 * (docs/plan/38-mac-fixes.md §3–§4). Feature code reaches the host only
 * through this interface: the desktop asks the shell (`window.print()`
 * does nothing in WKWebView under Tauri, so the shell opens the webview's
 * own print panel, and the kit's PDF is laid out and written by the core
 * where the person picks); the web vault prints with the browser and
 * downloads the core's PDF. With no provider, the browser's print and no
 * file.
 */
export interface PaperClient {
  /**
   * Prints the page as drawn now. `"done"` when printing finished before
   * this resolved (a browser's `window.print()` blocks until its dialog
   * closes); `"open"` when the host's print panel is still up (the
   * desktop's native panel runs on its own).
   */
  print(): Promise<"done" | "open">
  /**
   * Saves the kit page being shown as a PDF: the notes above it and the
   * page as written. `"cancelled"` when the person closed the save dialog.
   * Absent where there is no way to keep a file.
   */
  saveKitPdf?(lines: string[], page: string): Promise<"saved" | "cancelled">
}

/** The browser's own print, and no file. */
export const browserPaper: PaperClient = {
  print: async () => {
    window.print()
    return "done"
  },
}

const PaperContext = createContext<PaperClient>(browserPaper)

/** The host's paper client: the desktop's shell, or the web vault's. */
export const PaperClientProvider = PaperContext.Provider

export function usePaper(): PaperClient {
  return useContext(PaperContext)
}

/**
 * Printing with its end in view. `print` resolves once printing is over as
 * far as the page can tell: at once after a blocking print, and after a
 * native panel when the window has the person back (it regains focus, or a
 * pointer goes down in the page, which the panel's sheet would have
 * stopped). `printing` is true until then, so a view that draws something
 * only for the paper keeps it, and one that hides itself on blur waits.
 */
export function usePrinting(): { print: () => Promise<void>; printing: boolean } {
  const paper = usePaper()
  const [printing, setPrinting] = useState(false)
  const print = useCallback(async () => {
    setPrinting(true)
    try {
      if ((await paper.print()) === "open") await backFromPanel()
    } finally {
      setPrinting(false)
    }
  }, [paper])
  return { print, printing }
}

function backFromPanel(): Promise<void> {
  return new Promise((resolve) => {
    const back = () => {
      window.removeEventListener("focus", back)
      document.removeEventListener("pointerdown", back, true)
      resolve()
    }
    window.addEventListener("focus", back)
    document.addEventListener("pointerdown", back, true)
  })
}
