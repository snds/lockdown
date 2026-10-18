import { createContext, useContext } from "react"
import { invoke } from "@tauri-apps/api/core"

import type {
  AppError,
  BackupStatus,
  Chosen,
  ExportDone,
  ExportPreview,
  ImportFormatView,
  ImportPreview,
  ApprovalKind,
  ApprovalRoute,
  ChangeLink,
  CodeAdvanced,
  CodeView,
  Copied,
  DeviceInfo,
  FieldChangeInput,
  GateInput,
  ImportReport,
  ItemDetail,
  ItemSummary,
  LockState,
  OncePage,
  PersonaInfo,
  Playbook,
  PolicyInput,
  Probe,
  SecurityReport,
  Settings,
  Status,
  SyncOutcome,
  Templates,
  DuplicateGroup,
  ExpiryReminder,
  VaultView,
  VersionView,
} from "./types"
import { splitOncePage } from "./secret-bytes"

/**
 * The command surface of lockdown-app as the UI sees it. One method per
 * Tauri command (apps/desktop/src-tauri/src/commands.rs). Secrets go out
 * as byte arrays and come back as `Uint8Array`, never as strings in JSON.
 * A gated call (D101) takes a `GateInput`; without one it may reject with
 * `approval_needed`, and `app/gate.tsx` asks and calls again.
 */
export interface Client {
  probe(): Promise<Probe>
  openVault(create: boolean): Promise<Status>
  unlock(): Promise<Status>
  lock(): Promise<void>
  lockState(): Promise<LockState>
  touch(): Promise<void>
  setHidden(hidden: boolean): Promise<LockState>
  setPolicy(policy: PolicyInput): Promise<LockState>
  getSettings(): Promise<Settings>
  setSettings(settings: Settings): Promise<Settings>
  approvalRoute(kind: ApprovalKind): Promise<ApprovalRoute>
  list(): Promise<ItemSummary[]>
  show(id: string): Promise<ItemDetail>
  reveal(id: string, fieldId: string, gate?: GateInput): Promise<Uint8Array>
  /** Copies on the Rust side; resolves to the copy's clear time and generation, never the value. */
  copySecret(id: string, fieldId: string, clearAfterSecs?: number, gate?: GateInput): Promise<Copied>
  /**
   * Copies the code valid now (or, with `which: "next"`, the one after it)
   * on the Rust side, with the same clipboard timer. A code inside a crown
   * jewel goes through the `Use` gate (D101).
   */
  copyCode(itemId: string, fieldId: string, clearAfterSecs?: number, which?: "code" | "next", gate?: GateInput): Promise<Copied>
  /**
   * Whether the clipboard still holds the shell's copy number `copy` (a
   * `Copied.copy`). It is compared with the shell's own record only, so
   * the answer is never about text the shell did not write.
   */
  clipboardHolds(copy: number): Promise<boolean>
  /**
   * Clears the clipboard now for copy number `copy` (a `Copied.copy`): only
   * while that copy is still the shell's last write and still on the
   * clipboard, so one control's countdown never clears a later copy or the
   * person's own text.
   */
  /** Clears the clipboard only for copy `copy`; resolves whether it did. */
  clipboardClear(copy: number): Promise<boolean>
  addLogin(input: { site: string; username: string; password: number[]; title?: string }): Promise<string>
  editLogin(input: { id: string; title?: string; username?: string; password?: number[]; gate?: GateInput }): Promise<number>
  /** Every kind's template and the kinds a field added by hand may take (static). */
  templates(): Promise<Templates>
  /**
   * zxcvbn's 0–4 guess score for a password the person is typing in the
   * editor (`null` when empty), with the form's plain words as context.
   * The candidate goes as bytes the caller zeroes; never a stored secret.
   */
  passwordStrength(candidate: number[], context: string[]): Promise<number | null>
  /** Makes an item of any creatable kind; resolves to its id. */
  addItem(input: { kind: string; title?: string; crownJewel?: boolean; fields: FieldChangeInput[] }): Promise<string>
  /**
   * Saves a new version from an edit naming every field (`keep` for a secret
   * not retyped). A crown jewel's edit goes through the `Use` gate (D110).
   */
  editItem(input: { id: string; title?: string; changes: FieldChangeInput[] }, gate?: GateInput): Promise<number>
  trash(id: string): Promise<void>
  /** Brings a trashed item back from Deleted, as it was. */
  restore(id: string): Promise<void>
  codes(): Promise<CodeView[]>
  /**
   * "Next code" on an HOTP code: the core moves its counter on in a new
   * item version (so it syncs) and answers with the new code and counter.
   * A crown jewel's step goes through the `Use` gate.
   */
  advanceCode(itemId: string, fieldId: string, gate?: GateInput): Promise<CodeAdvanced>
  addCode(input: { uri: string; title?: string; into?: string }): Promise<string>
  personas(): Promise<PersonaInfo[]>
  status(): Promise<Status>
  audit(): Promise<string[]>
  accountRegister(input: { server: string; login: string; password: string; secretKey?: string }): Promise<OncePage>
  accountLogin(input: { server: string; login: string; password: string; secretKey?: string }): Promise<string[]>
  recover(input: { server: string; login: string; password: string; secretKey: string; recoveryCode: string }): Promise<string[]>
  sync(): Promise<SyncOutcome>
  kitCreate(input: { replace: boolean; password?: string }): Promise<OncePage>
  kitVerify(input: { secretKey: string; recoveryCode: string; password?: string; drill: boolean }): Promise<string[]>
  deviceList(): Promise<DeviceInfo[]>
  deviceRevoke(id: string, gate?: GateInput): Promise<string[]>
  importItems(bytes: number[], format: string): Promise<ImportReport>
  openSite(url: string): Promise<void>
  /** Opens the OS date and time settings; `false` where this platform has no way in (Linux). */
  openClockSettings(): Promise<boolean>
  // --- import, export and backups (docs/plan/14-import-export.md) ---
  /** The formats the import wizard lists. */
  importFormats(): Promise<ImportFormatView[]>
  /** The format a file looks like, from its first bytes and its name; `null` when unsure. */
  importDetect(bytes: number[], fileName: string): Promise<string | null>
  /** Parses a file in the core (hostile until proven otherwise) and says what it holds; nothing is kept. */
  importPreview(input: { bytes: number[]; format: string; fileName: string; password?: number[]; vault?: string | null }): Promise<ImportPreview>
  /** Keeps the previewed import. */
  importCommit(token: number): Promise<ImportReport>
  importCancel(): Promise<void>
  /** What an export in `format` would carry and leave out. */
  exportPreview(format: string, vault?: string | null): Promise<ExportPreview>
  /** The OS's save dialog; the place stays in the shell under the returned number. */
  exportChoose(format: string, sealed: boolean): Promise<Chosen | null>
  /** The core writes the export to the chosen place, after the `Reveal` gate (D101). */
  exportSave(input: { target: number; format: string; password?: number[]; plainConfirmed: boolean; vault?: string | null }, gate?: GateInput): Promise<ExportDone>
  /** A crown jewel's "include in exports and backups" (D121); on goes through the `Reveal` gate. */
  setExportOptIn(id: string, on: boolean, gate?: GateInput): Promise<number>
  backupStatus(): Promise<BackupStatus>
  /** The OS's folder picker; the folder stays in the shell under the returned number. */
  backupPickFolder(): Promise<Chosen | null>
  /** Turns backups on or changes them, behind the `Reveal` gate; `folder` is a `Chosen.token`, or `null` for the folder in use. */
  backupSetup(input: { folder: number | null; password?: number[]; intervalHours: number; keepLast: number }, gate?: GateInput): Promise<BackupStatus>
  backupNow(): Promise<BackupStatus>
  backupOff(forget: boolean): Promise<BackupStatus>
  /** Opens one of the folder's backups for the import preview (the key kept here, or `password`). */
  backupRestorePreview(name: string, password?: number[]): Promise<ImportPreview>

  // --- organization (docs/plan/15-organization.md) ---------------------------
  /** Every vault of the persona, the first one first, with its folders. */
  vaults(): Promise<VaultView[]>
  createVault(name: string): Promise<string>
  renameVault(vaultId: string, name: string): Promise<void>
  /** Days a trashed item stays before it is purged on every device (1–365). */
  setTrashDays(vaultId: string, days: number): Promise<void>
  addFolder(vaultId: string, name: string, parent?: string | null): Promise<string>
  changeFolder(vaultId: string, folder: string, name: string, parent?: string | null): Promise<void>
  /** Removes a folder; what was in it moves up. Resolves to how many items moved. */
  removeFolder(vaultId: string, folder: string): Promise<number>
  /** Files, tags or stars an item. `folder: ""` takes it out of its folder; a part left out stays. */
  organize(id: string, change: { folder?: string; tags?: string[]; favourite?: boolean }): Promise<number>
  /** Renames a tag everywhere (nested ones too); `to: null` takes it off. */
  renameTag(from: string, to: string | null): Promise<number>
  archive(id: string): Promise<void>
  unarchive(id: string): Promise<void>
  /** Seals the item again in another vault; resolves to its new id. A crown jewel's asks first. */
  moveItem(id: string, vaultId: string, gate?: GateInput): Promise<string>
  /** Into the crown-jewel class, or out of it (asks for the password); resolves to the new id. */
  setCrownJewel(id: string, on: boolean, gate?: GateInput): Promise<string>
  /** Deletes a trashed item for good now. */
  purge(id: string, gate?: GateInput): Promise<void>
  /** Keeps one copy of a conflict and trashes the other (D58). */
  resolveConflict(keep: string, other: string): Promise<void>
  duplicates(): Promise<DuplicateGroup[]>
  mergeDuplicates(keep: string, others: string[], gate?: GateInput): Promise<number>
  history(id: string): Promise<VersionView[]>
  showVersion(id: string, n: number): Promise<ItemDetail>
  restoreVersion(id: string, n: number, gate?: GateInput): Promise<number>
  /** Attaches a file: its bytes go as the raw request body, never as JSON. */
  attach(id: string, name: string, mime: string, bytes: Uint8Array, gate?: GateInput): Promise<string>
  /** A file's bytes, for the person to save; the caller wipes them after. */
  attachment(id: string, attId: string, gate?: GateInput): Promise<Uint8Array>
  detach(id: string, attId: string, gate?: GateInput): Promise<number>
  /** Notes a use made outside the core (the site opened), for the ranking. */
  noteUse(id: string): Promise<void>
  expiryReminders(): Promise<ExpiryReminder[]>
  /** `addItem` into a chosen vault. */
  addItemIn(vaultId: string, input: { kind: string; title?: string; crownJewel?: boolean; fields: FieldChangeInput[] }): Promise<string>
  /** The security center's report from what this device knows now; no network. */
  securityReport(): Promise<SecurityReport>
  /** Checks again (breach ranges by five-character prefix, the account's alerts), then the report. */
  securityCheck(): Promise<SecurityReport>
  /** Dismisses (`until: null`) or snoozes a finding: a setting on the item, so it syncs. */
  securityDismiss(itemId: string, key: string, until: number | null): Promise<SecurityReport>
  /** Brings a dismissed or snoozed finding back. */
  securityRestore(itemId: string, key: string): Promise<SecurityReport>
  /** Dismisses an account alert for every device of the account. */
  securityDismissAlert(seq: number): Promise<SecurityReport>
  /** Opens the item's change-password page in the browser (the shell opens it) and says which page. */
  securityChangePassword(itemId: string): Promise<ChangeLink>
  /** What to do about each kind of finding. */
  securityPlaybooks(): Promise<Playbook[]>
}

/** Whether a rejection is the app's own error shape. */
export function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e
}

/** The words to show for any failure. */
export function describeError(e: unknown): string {
  if (isAppError(e)) return e.message
  if (e instanceof Error) return e.message
  return String(e)
}

function bytes(body: ArrayBuffer | Uint8Array | number[]): Uint8Array {
  if (body instanceof Uint8Array) return body
  if (body instanceof ArrayBuffer) return new Uint8Array(body)
  return Uint8Array.from(body)
}

const call = <T,>(cmd: string, args?: Record<string, unknown>) => invoke<T>(cmd, args)
const callBytes = async (cmd: string, args?: Record<string, unknown>) => bytes(await invoke<ArrayBuffer>(cmd, args))

/** The real client: every call is a Tauri command. */
export const tauriClient: Client = {
  probe: () => call("probe"),
  openVault: (create) => call("open_vault", { create }),
  unlock: () => call("unlock"),
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
  reveal: (id, fieldId, gate) => callBytes("reveal", { id, fieldId, gate }),
  copySecret: (id, fieldId, clearAfterSecs, gate) => call("copy_secret", { id, fieldId, clearAfterSecs, gate }),
  copyCode: (itemId, fieldId, clearAfterSecs, which, gate) => call("copy_code", { itemId, fieldId, clearAfterSecs, which, gate }),
  clipboardHolds: (copy) => call("clipboard_holds", { copy }),
  clipboardClear: (copy) => call("clipboard_clear", { copy }),
  addLogin: (input) => call("add_login", input),
  editLogin: (input) => call("edit_login", input),
  templates: () => call("templates"),
  passwordStrength: (candidate, context) => call("password_strength", { candidate, context }),
  addItem: (input) => call("add_item", input),
  editItem: (input, gate) => call("edit_item", { ...input, gate }),
  trash: (id) => call("trash", { id }),
  restore: (id) => call("restore", { id }),
  codes: () => call("codes"),
  advanceCode: (itemId, fieldId, gate) => call("advance_code", { itemId, fieldId, gate }),
  addCode: (input) => call("add_code", input),
  personas: () => call("personas"),
  status: () => call("status"),
  audit: () => call("audit"),
  accountRegister: async (input) => splitOncePage(await callBytes("account_register", input)),
  accountLogin: (input) => call("account_login", input),
  recover: (input) => call("recover", input),
  sync: () => call("sync"),
  kitCreate: async (input) => splitOncePage(await callBytes("kit_create", input)),
  kitVerify: (input) => call("kit_verify", input),
  deviceList: () => call("device_list"),
  deviceRevoke: (id, gate) => call("device_revoke", { id, gate }),
  importItems: (bytes, format) => call("import_items", { bytes, format }),
  openSite: (url) => call("open_site", { url }),
  openClockSettings: () => call("open_clock_settings"),
  importFormats: () => call("import_formats"),
  importDetect: (bytes, fileName) => call("import_detect", { bytes, fileName }),
  importPreview: (input) => call("import_preview", input),
  importCommit: (token) => call("import_commit", { token }),
  importCancel: () => call("import_cancel"),
  exportPreview: (format, vault) => call("export_preview", { format, vault: vault ?? null }),
  exportChoose: (format, sealed) => call("export_choose", { format, sealed }),
  exportSave: (input, gate) => call("export_save", { ...input, gate }),
  setExportOptIn: (id, on, gate) => call("set_export_opt_in", { id, on, gate }),
  backupStatus: () => call("backup_status"),
  backupPickFolder: () => call("backup_pick_folder"),
  backupSetup: (input, gate) => call("backup_setup", { ...input, gate }),
  backupNow: () => call("backup_now"),
  backupOff: (forget) => call("backup_off", { forget }),
  backupRestorePreview: (name, password) => call("backup_restore_preview", { name, password }),
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
  moveItem: (id, vaultId, gate) => call("move_item", { id, vaultId, gate }),
  setCrownJewel: (id, on, gate) => call("set_crown_jewel", { id, on, gate }),
  purge: (id, gate) => call("purge", { id, gate }),
  resolveConflict: (keep, other) => call("resolve_conflict", { keep, other }),
  duplicates: () => call("duplicates"),
  mergeDuplicates: (keep, others, gate) => call("merge_duplicates", { keep, others, gate }),
  history: (id) => call("history", { id }),
  showVersion: (id, n) => call("show_version", { id, n }),
  restoreVersion: (id, n, gate) => call("restore_version", { id, n, gate }),
  attach: (id, name, mime, bytes, gate) =>
    invoke<string>("attach", bytes, {
      headers: {
        "x-lockdown-item": id,
        "x-lockdown-name": encodeURIComponent(name),
        "x-lockdown-type": mime,
        ...(gate ? { "x-lockdown-gate": JSON.stringify(gate) } : {}),
      },
    }),
  attachment: (id, attId, gate) => callBytes("attachment", { id, attId, gate }),
  detach: (id, attId, gate) => call("detach", { id, attId, gate }),
  noteUse: (id) => call("note_use", { id }),
  expiryReminders: () => call("expiry_reminders"),
  addItemIn: (vaultId, input) => call("add_item", { ...input, vault: vaultId }),
  securityReport: () => call("security_report"),
  securityCheck: () => call("security_check"),
  securityDismiss: (itemId, key, until) => call("security_dismiss", { itemId, key, until }),
  securityRestore: (itemId, key) => call("security_restore", { itemId, key }),
  securityDismissAlert: (seq) => call("security_dismiss_alert", { seq }),
  securityChangePassword: (itemId) => call("security_change_password", { itemId }),
  securityPlaybooks: () => call("security_playbooks"),
}

const ClientContext = createContext<Client>(tauriClient)

/** The app's client; tests provide a fake one. */
export const ClientProvider = ClientContext.Provider

export function useClient(): Client {
  return useContext(ClientContext)
}
