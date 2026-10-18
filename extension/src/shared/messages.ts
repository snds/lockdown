// Messages inside the extension: between the content script, the
// extension-owned frames (menu and save prompt), the popup and the
// background. The background checks who sent each one (docs/plan/
// 16-browser-extension.md §5): a content script's sender URL is the page,
// a frame's and the popup's is the extension's own origin, and a frame may
// only act for a nonce the content script in the same tab registered.

import type { FillValue, FillWhat } from "./protocol"

/** What a menu or prompt is for. */
export type MenuKind = FillWhat | "prompt"

export type ToBackground =
  // from a content script
  | { t: "register"; nonce: string; kind: MenuKind }
  | { t: "unregister"; nonce: string }
  | { t: "offer_save"; username: string; password: string }
  | { t: "top_ready" }
  // from the menu or prompt frame
  | { t: "menu_data"; nonce: string }
  | { t: "menu_pick"; nonce: string; item_id: string; what: FillWhat; field?: string | null; confirmed: boolean; frame_visible: boolean | null }
  | { t: "menu_close"; nonce: string }
  | { t: "prompt_data"; nonce: string }
  | { t: "prompt_answer"; nonce: string; save: boolean }
  // from the popup
  | { t: "status" }
  | { t: "pair" }
  | { t: "search"; query: string }
  | { t: "copy"; item_id: string; tab_id?: number }
  | { t: "site"; tab_id?: number }
  | { t: "fill_tab"; tab_id: number; item_id: string }
  | { t: "change_password"; origin: string }

export type ToContent =
  | { t: "measure"; nonce: string }
  | { t: "apply"; nonce: string; what: FillWhat; values: FillValue[] }
  | { t: "close"; nonce: string }
  | { t: "hotkey" }
  | { t: "target_login" }
  | { t: "show_prompt" }
  | { t: "open_menu"; nonce: string }

/** What the content script measured for a fill (D67). */
export interface Measured {
  ok: boolean
  visible: boolean
  why: string
  /** The menu frame's host element is itself on top (the Firefox stand-in
   *  for IntersectionObserver v2). `null` when there is no menu open. */
  menu_visible: boolean | null
}

/** The extension's own origin, e.g. `chrome-extension://<id>`. */
export function extensionOrigin(): string {
  return new URL(chrome.runtime.getURL("/")).origin
}
