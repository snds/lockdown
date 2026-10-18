// Messages inside the extension: between the content script, the
// extension-owned frames (menu and save prompt), the popup and the
// background. The background checks who sent each one (docs/plan/
// 16-browser-extension.md §5): a content script's sender URL is the page,
// a frame's and the popup's is the extension's own origin, and a frame may
// only act for a nonce the content script in the same tab registered.

import type { FillValue, FillWhat } from "./protocol"

/** What a menu or prompt is for. */
export type MenuKind = FillWhat | "prompt"

/** What a pick in the menu fills: a login's parts, or a passkey sign-in
 *  (its credential id rides in `field`). */
export type PickWhat = FillWhat | "passkey"

export type ToBackground =
  // from a content script
  | { t: "register"; nonce: string; kind: MenuKind; new_password?: boolean }
  | { t: "unregister"; nonce: string }
  | { t: "offer_save"; username: string; password: string }
  | { t: "top_ready" }
  // from the menu or prompt frame
  | { t: "menu_data"; nonce: string }
  | { t: "menu_pick"; nonce: string; item_id: string; what: PickWhat; field?: string | null; confirmed: boolean; frame_visible: boolean | null }
  | { t: "menu_passkey_other"; nonce: string }
  | { t: "menu_close"; nonce: string }
  | { t: "menu_suggest"; nonce: string; frame_visible: boolean | null }
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
  // from the passkey bridge in a tab's top frame (docs/plan/19-passkeys.md)
  | { t: "pk_begin"; op: "create" | "get"; mediation: string; request: Record<string, unknown> }
  | { t: "pk_abort"; rid: string }
  | { t: "pk_status" }
  | { t: "pk_signal"; signal: Record<string, unknown> }
  // from the passkey sheet frame
  | { t: "pk_sheet"; nonce: string }
  | { t: "pk_sheet_pick"; nonce: string; item_id: string; credential_id: string; confirmed: boolean; frame_visible: boolean | null }
  | { t: "pk_sheet_create"; nonce: string; frame_visible: boolean | null }
  | { t: "pk_sheet_handoff"; nonce: string }
  | { t: "pk_sheet_cancel"; nonce: string }
  | { t: "pk_sheet_retry"; nonce: string }

export type ToContent =
  | { t: "measure"; nonce: string }
  | { t: "apply"; nonce: string; what: FillWhat; values: FillValue[] }
  | { t: "close"; nonce: string }
  | { t: "apply_new"; nonce: string; value: string }
  | { t: "hotkey" }
  | { t: "target_login" }
  | { t: "show_prompt" }
  | { t: "open_menu"; nonce: string }
  // to the passkey bridge
  | { t: "pk_done"; rid: string; credential?: unknown; error?: { name: string; message: string }; handoff?: "same" | "modal" }
  | { t: "pk_measure"; rid: string }

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
