// The host for Lockdown's in-page frames (the menu, the save prompt, the
// passkey sheet): shared by the content script and the passkey bridge.

/** A host element the page cannot look into (closed shadow root) holding
 *  an extension-origin frame the page cannot read, raised into the top
 *  layer where the browser supports popovers so page z-indexes cannot
 *  cover it. */
export function makeHost(src: string, label: string, width: number, height: number): { host: HTMLElement; frame: HTMLIFrameElement } {
  const host = document.createElement("div")
  host.style.cssText = `all: initial; position: fixed; z-index: 2147483647; width: ${width}px; height: ${height}px; margin: 0; padding: 0; border: 0; inset: auto; overflow: visible; background: transparent; display: block;`
  const root = host.attachShadow({ mode: "closed" })
  const frame = document.createElement("iframe")
  frame.src = src
  frame.title = label
  frame.setAttribute("scrolling", "no")
  frame.style.cssText =
    "all: initial; display: block; width: 100%; height: 100%; border: 0; border-radius: 10px; box-shadow: 0 6px 24px rgba(0,0,0,.28); color-scheme: normal; background: transparent;"
  root.append(frame)
  document.documentElement.append(host)
  try {
    host.setAttribute("popover", "manual")
    ;(host as HTMLElement & { showPopover(): void }).showPopover()
  } catch {
    // No popover support: the fixed, top-z-index host stands.
  }
  return { host, frame }
}
