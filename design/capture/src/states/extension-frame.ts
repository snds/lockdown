// Inside an in-page frame's iframe: the extension's own frame script
// (extension/src/pages/frame.ts), unchanged, over the fake background. The
// frame reads its mode and nonce from the address, as it does when the
// content script opens `frame.html#menu:<nonce>`.

import tokensCss from "../../../../extension/static/tokens.css?url"
import uiCss from "../../../../extension/static/ui.css?url"

import { background, installChrome } from "../host/chrome"
import { scenario } from "./extension-scenarios"

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
    const style = document.createElement("style")
    style.textContent = css.replace(/@media\s*\(prefers-color-scheme:\s*dark\)/g, scheme === "dark" ? "@media all" : "@media not all")
    document.head.append(style)
  }
}

export async function mountExtensionFrame(frame: { mode: string; scenario: string; scheme: "light" | "dark" }): Promise<void> {
  installChrome(background(scenario(frame.scenario)))
  await installSheets(frame.scheme)
  // The mode and nonce in the frame's own address, as `frame.html#menu:<nonce>` has them.
  location.hash = `${frame.mode}:${NONCE}`
  await import("../../../../extension/src/pages/frame")
}
