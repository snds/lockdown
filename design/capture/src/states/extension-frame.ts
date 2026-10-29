// Inside an in-page frame's iframe: the extension's own frame script
// (extension/src/pages/frame.ts), unchanged, over the fake background. The
// frame reads its mode and nonce from the address, as it does when the
// content script opens `frame.html#menu:<nonce>`.

import { background, installChrome } from "../host/chrome"
import { scenario } from "./extension-scenarios"

/** Any 32 hex digits: the frame accepts only a nonce of that shape. */
const NONCE = "5f2c9e1ab7d04c3e8f6a1b2c3d4e5f60"

export async function mountExtensionFrame(frame: { mode: string; scenario: string }): Promise<void> {
  installChrome(background(scenario(frame.scenario)))
  // A fragment change on the frame's own document (about:srcdoc), as `frame.html#menu:<nonce>` has.
  location.hash = `${frame.mode}:${NONCE}`
  await import("../../../../extension/src/pages/frame")
}
