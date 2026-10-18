import type { Client } from "@/core/client"
import type { GateInput, Status } from "@/core/types"

import type { WebClipboard } from "./clipboard"
import type { Link } from "./link"
import type { GetAnswer, GetRequest } from "./webauthn"

/**
 * The desktop's `Client` over the web vault's worker (one shared
 * interface: apps/desktop/src/core/client.ts). Commands keep the desktop's
 * names and argument shapes, so the desktop's views run unchanged. Where
 * the web differs:
 *
 * - copies come to the page as bytes and go to the clipboard here
 *   (`WebClipboard`), since no Rust side sits outside the page;
 * - the lifecycle (`probe`, `openVault`, `unlock`, the account calls) is
 *   the web's own: see `WebSession`;
 * - what the web vault does not do yet refuses in words (import, export,
 *   backups, attachments, the security center's network checks, the kit
 *   and the browser pairing), and the views show those words.
 */

/** What the web vault doesn't do yet, as the app's own error shape. */
function notHere(what: string): Promise<never> {
  return Promise.reject({
    code: "refused",
    message: `${what} isn't in the web vault yet; use the desktop app for it`,
  })
}

/** Whether someone is signed in in this tab, and whether the vault is open. */
export interface WebState {
  signed_in: boolean
  locked: boolean
  server: string | null
  login: string | null
}

/** The web vault's own lifecycle: sign in, unlock, sign out. */
export interface WebSession {
  state(): Promise<WebState>
  signIn(input: { server: string; login: string; password: string; secretKey: string }): Promise<Status>
  unlock(password: string): Promise<Status>
  signOut(): Promise<void>
  /** The WebAuthn request of a sign-in waiting for a security key (D253). */
  signInWaiting(): Promise<GetRequest>
  /** The key's answer; the server checks it and the sign-in finishes. */
  signInAnswer(answer: GetAnswer): Promise<Status>
}

export function webSession(link: Link): WebSession {
  return {
    state: () => link.call("web_state"),
    signIn: (input) => link.call("web_sign_in", input),
    unlock: (password) => link.call("web_unlock", { password }),
    signOut: () => link.call("web_sign_out"),
    signInWaiting: () => link.call("web_sign_in_waiting"),
    signInAnswer: (answer) => link.call("web_sign_in_answer", { answer }),
  }
}

export function webClient(link: Link, clipboard: WebClipboard): Client {
  const call = <T,>(cmd: string, args?: Record<string, unknown>) => link.call<T>(cmd, args)
  const bytes = (cmd: string, args?: Record<string, unknown>) => link.callBytes(cmd, args)
  const g = (gate?: GateInput) => (gate ? { gate } : {})
  return {
    probe: () => notHere("Opening a vault on this device"),
    openVault: () => notHere("Opening a vault on this device"),
    unlock: () => notHere("Unlocking without the password"),
    lock: () => call("lock"),
    lockState: () => call("lock_state"),
    touch: () => call("touch"),
    setHidden: (hidden) => call("set_hidden", { hidden }),
    setPolicy: (policy) => call("set_policy", { policy }),
    getSettings: () => call("get_settings"),
    setSettings: (settings) => call("set_settings", { settings }),
    approvalRoute: (kind) => call("approval_route", { kind }),
    list: () => call("list"),
    show: (id) => call("show", { id }),
    reveal: (id, fieldId, gate) => bytes("reveal", { id, fieldId, ...g(gate) }),
    copySecret: async (id, fieldId, clearAfterSecs, gate) =>
      clipboard.copy(await bytes("reveal_to_use", { id, fieldId, ...g(gate) }), clearAfterSecs),
    copyCode: async (itemId, fieldId, clearAfterSecs, which, gate) =>
      clipboard.copy(await bytes("code_now", { itemId, fieldId, which, ...g(gate) }), clearAfterSecs),
    clipboardHolds: (copy) => Promise.resolve(clipboard.holds(copy)),
    clipboardClear: (copy) => clipboard.clear(copy),
    addLogin: (input) =>
      call("add_item", {
        kind: "login",
        title: input.title,
        fields: [
          { field_id: "user", action: "set", text: input.username },
          { field_id: "pw", action: "set", secret: input.password },
          { field_id: "url.0", action: "set", text: input.site },
        ],
      }),
    editLogin: () => notHere("The narrow login editor"),
    templates: () => call("templates"),
    passwordStrength: (candidate, context) => call("password_strength", { candidate, context }),
    addItem: (input) => call("add_item", input),
    editItem: (input, gate) => call("edit_item", { ...input, ...g(gate) }),
    trash: (id) => call("trash", { id }),
    restore: (id) => call("restore", { id }),
    codes: () => call("codes"),
    advanceCode: (itemId, fieldId, gate) => call("advance_code", { itemId, fieldId, ...g(gate) }),
    addCode: (input) => call("add_code", input),
    personas: () => call("personas"),
    status: () => call("status"),
    audit: () => call("audit"),
    accountRegister: () => notHere("Registering from inside the vault"),
    accountLogin: () => notHere("Signing in again from inside the vault"),
    recover: () => notHere("Recovery with the kit"),
    sync: () => call("sync"),
    kitCreate: () => notHere("Making a new recovery kit"),
    kitVerify: () => notHere("The recovery drill"),
    deviceList: () => call("device_list"),
    deviceRevoke: (id, gate) => call("device_revoke", { id, ...g(gate) }),
    importItems: () => notHere("Import"),
    openSite: (url) => {
      if (!/^https?:\/\//i.test(url)) return Promise.reject({ code: "refused", message: "only http and https links open from here" })
      window.open(url, "_blank", "noopener,noreferrer")
      return Promise.resolve()
    },
    openClockSettings: () => Promise.resolve(false),
    importFormats: () => notHere("Import"),
    importDetect: () => notHere("Import"),
    importPreview: () => notHere("Import"),
    importCommit: () => notHere("Import"),
    importCancel: () => Promise.resolve(),
    exportPreview: () => notHere("Export"),
    exportChoose: () => notHere("Export"),
    exportSave: () => notHere("Export"),
    setExportOptIn: () => notHere("Export"),
    backupStatus: () => notHere("Backups"),
    backupPickFolder: () => notHere("Backups"),
    backupSetup: () => notHere("Backups"),
    backupNow: () => notHere("Backups"),
    backupOff: () => notHere("Backups"),
    backupRestorePreview: () => notHere("Backups"),
    vaults: () => call("vaults"),
    createVault: (name) => call("create_vault", { name }),
    renameVault: (vaultId, name) => call("rename_vault", { vaultId, name }),
    setTrashDays: (vaultId, days) => call("set_trash_days", { vaultId, days }),
    addFolder: (vaultId, name, parent) => call("add_folder", { vaultId, name, parent: parent ?? null }),
    changeFolder: (vaultId, folder, name, parent) => call("change_folder", { vaultId, folder, name, parent: parent ?? null }),
    removeFolder: (vaultId, folder) => call("remove_folder", { vaultId, folder }),
    organize: (id, change) => call("organize", { id, ...change }),
    renameTag: (from, to) => call("rename_tag", { from, to }),
    archive: (id) => call("archive", { id }),
    unarchive: (id) => call("unarchive", { id }),
    moveItem: (id, vaultId, gate) => call("move_item", { id, vaultId, ...g(gate) }),
    setCrownJewel: (id, on, gate) => call("set_crown_jewel", { id, on, ...g(gate) }),
    purge: (id, gate) => call("purge", { id, ...g(gate) }),
    resolveConflict: (keep, other) => call("resolve_conflict", { keep, other }),
    duplicates: () => call("duplicates"),
    mergeDuplicates: (keep, others, gate) => call("merge_duplicates", { keep, others, ...g(gate) }),
    history: (id) => call("history", { id }),
    showVersion: (id, n) => call("show_version", { id, n }),
    restoreVersion: (id, n, gate) => call("restore_version", { id, n, ...g(gate) }),
    attach: () => notHere("Attaching a file"),
    attachment: () => notHere("Saving an attached file"),
    detach: () => notHere("Removing an attached file"),
    noteUse: (id) => call("note_use", { id }),
    expiryReminders: () => call("expiry_reminders"),
    addItemIn: (vaultId, input) => call("add_item", { ...input, vault: vaultId }),
    securityReport: () => call("security_report"),
    securityCheck: () => notHere("Checking passwords against breach lists"),
    securityDismiss: () => notHere("Dismissing a finding"),
    securityRestore: () => notHere("Bringing a finding back"),
    securityDismissAlert: () => notHere("Dismissing an account alert"),
    securityChangePassword: () => notHere("Finding the change-password page"),
    securityPlaybooks: () => call("security_playbooks"),
  }
}
