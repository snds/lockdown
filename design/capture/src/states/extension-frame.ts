// Inside an in-page frame's iframe: the extension's own frame script
// (extension/src/pages/frame.ts), unchanged, over the fake background. The
// frame reads its mode and nonce from the address, as it does when the
// content script opens `frame.html#menu:<nonce>`.

import tokensCss from "../../../../extension/static/tokens.css?url"
import uiCss from "../../../../extension/static/ui.css?url"
// The frames' fonts. The extension ships these files (scripts/build.mjs, from
// the font packages beside @unicron/ui); the harness takes the same files from
// its own install of those packages, at the version both lockfiles pin.
import geistLatin from "../../node_modules/.pnpm/@fontsource-variable+geist@5.3.0/node_modules/@fontsource-variable/geist/files/geist-latin-wght-normal.woff2?url"
import geistLatinExt from "../../node_modules/.pnpm/@fontsource-variable+geist@5.3.0/node_modules/@fontsource-variable/geist/files/geist-latin-ext-wght-normal.woff2?url"
import monoLatin from "../../node_modules/.pnpm/@fontsource-variable+geist-mono@5.3.0/node_modules/@fontsource-variable/geist-mono/files/geist-mono-latin-wght-normal.woff2?url"
import monoLatinExt from "../../node_modules/.pnpm/@fontsource-variable+geist-mono@5.3.0/node_modules/@fontsource-variable/geist-mono/files/geist-mono-latin-ext-wght-normal.woff2?url"

import { background, installChrome } from "../host/chrome"
import { scenario } from "./extension-scenarios"

/** tokens.css names each font as `fonts/<file>`, beside frame.html in the extension (the
 *  review build's copy of the sheet is minified, so the quotes may be gone). */
const FONTS: Record<string, string> = {
  "geist-latin-wght-normal.woff2": geistLatin,
  "geist-latin-ext-wght-normal.woff2": geistLatinExt,
  "geist-mono-latin-wght-normal.woff2": monoLatin,
  "geist-mono-latin-ext-wght-normal.woff2": monoLatinExt,
}

/** Any 32 hex digits: the frame accepts only a nonce of that shape. */
const NONCE = "5f2c9e1ab7d04c3e8f6a1b2c3d4e5f60"

/**
 * The frame's sheets, as the extension's frame.html links them: unicron's
 * roles (tokens.css), then the frame's own (ui.css). They pick light or
 * dark by `prefers-color-scheme`, which a frame inside the review build
 * can't be told, so each goes in with its dark block always on, or always
 * off, as the test page chose.
 */
async function installSheets(scheme: "light" | "dark") {
  for (const href of [tokensCss, uiCss]) {
    const css = await fetch(new URL(href, document.baseURI)).then((r) => r.text())
    const fonts = css.replace(/url\(\s*["']?fonts\/([a-z0-9-]+\.woff2)["']?\s*\)/g, (_match, file: string) => {
      const href = FONTS[file]
      if (!href) throw new Error(`tokens.css names a font the harness lacks: ${file}`)
      return `url("${new URL(href, document.baseURI).href}")`
    })
    const style = document.createElement("style")
    style.textContent = fonts.replace(/@media\s*\(prefers-color-scheme:\s*dark\)/g, scheme === "dark" ? "@media all" : "@media not all")
    document.head.append(style)
  }
  await document.fonts.ready
}

/**
 * The extension's host draws the frame's iframe with a 2 px corner and its
 * shadow (extension/src/content/host.ts, brief B2). The test page's iframe
 * still has the old 10 px corner (harness/test-page.tsx, the foundation's),
 * which would clip the frame's own 2 px hairline corner, so the frame gives
 * its iframe the host's corner until the test page does.
 */
function hostCorner() {
  const host = window.frameElement as HTMLElement | null
  if (host) host.style.borderRadius = "2px"
}

export async function mountExtensionFrame(frame: { mode: string; scenario: string; scheme: "light" | "dark" }): Promise<void> {
  installChrome(background(scenario(frame.scenario)))
  await installSheets(frame.scheme)
  hostCorner()
  // The mode and nonce in the frame's own address, as `frame.html#menu:<nonce>` has them.
  location.hash = `${frame.mode}:${NONCE}`
  await import("../../../../extension/src/pages/frame")
}
