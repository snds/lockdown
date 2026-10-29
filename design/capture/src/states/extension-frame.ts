// Inside an in-page frame's iframe: the extension's own frame script
// (extension/src/pages/frame.ts), unchanged, over the fake background. The
// frame reads its mode and nonce from the address, as it does when the
// content script opens `frame.html#menu:<nonce>`.

import { background, installChrome } from "../host/chrome"
import { scenario } from "./extension-scenarios"

/** Any 32 hex digits: the frame accepts only a nonce of that shape. */
const NONCE = "5f2c9e1ab7d04c3e8f6a1b2c3d4e5f60"

/**
 * The frame's sheet picks light or dark by `prefers-color-scheme`, which a
 * frame inside the review build can't be told. The same sheet goes in with
 * its dark block always on, or always off, as the test page chose.
 */
async function matchScheme(scheme: "light" | "dark") {
  const link = document.querySelector<HTMLLinkElement>("link[rel=stylesheet]")
  if (!link) return
  const css = await fetch(link.href).then((r) => r.text())
  const style = document.createElement("style")
  style.textContent = css.replace(/@media\s*\(prefers-color-scheme:\s*dark\)/g, scheme === "dark" ? "@media all" : "@media not all")
  link.replaceWith(style)
}

export async function mountExtensionFrame(frame: { mode: string; scenario: string; scheme: "light" | "dark" }): Promise<void> {
  installChrome(background(scenario(frame.scenario)))
  await matchScheme(frame.scheme)
  // A fragment change on the frame's own document (about:srcdoc), as `frame.html#menu:<nonce>` has.
  location.hash = `${frame.mode}:${NONCE}`
  await import("../../../../extension/src/pages/frame")
}
