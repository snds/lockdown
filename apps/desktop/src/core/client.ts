import { createContext, useContext } from "react"
import { invoke } from "@tauri-apps/api/core"

import type {
  AppError,
  ApprovalKind,
  ApprovalRoute,
  CodeView,
  DeviceInfo,
  GateInput,
  ImportReport,
  ItemDetail,
  ItemSummary,
  LockState,
  OncePage,
  PersonaInfo,
  PolicyInput,
  Probe,
  Settings,
  Status,
  SyncOutcome,
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
  /** Copies on the Rust side; resolves to the seconds until the clipboard is cleared. */
  copySecret(id: string, fieldId: string, clearAfterSecs?: number, gate?: GateInput): Promise<number>
  /**
   * Copies the code valid now (or, with `which: "next"`, the one after it)
   * on the Rust side, with the same clipboard timer. A code inside a crown
   * jewel goes through the `Use` gate (D101).
   */
  copyCode(itemId: string, fieldId: string, clearAfterSecs?: number, which?: "code" | "next", gate?: GateInput): Promise<number>
  /**
   * Whether the clipboard still holds what the shell last copied. `hash` is
   * the component's name for the copy (hex SHA-256 of the bytes it handed
   * over), or `null` when the shell copied by reference; it is compared
   * with the shell's own record only.
   */
  clipboardHolds(hash: string | null): Promise<boolean>
  /** Clears the clipboard now, only if it still holds what the shell wrote. */
  clipboardClear(): Promise<void>
  addLogin(input: { site: string; username: string; password: number[]; title?: string }): Promise<string>
  editLogin(input: { id: string; title?: string; username?: string; password?: number[] }): Promise<number>
  trash(id: string): Promise<void>
  codes(): Promise<CodeView[]>
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
  exportItems(format: string, gate?: GateInput): Promise<Uint8Array>
  openSite(url: string): Promise<void>
  /** Opens the OS date and time settings; `false` where this platform has no way in (Linux). */
  openClockSettings(): Promise<boolean>
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
  clipboardHolds: (hash) => call("clipboard_holds", { hash }),
  clipboardClear: () => call("clipboard_clear"),
  addLogin: (input) => call("add_login", input),
  editLogin: (input) => call("edit_login", input),
  trash: (id) => call("trash", { id }),
  codes: () => call("codes"),
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
  exportItems: (format, gate) => callBytes("export_items", { format, gate }),
  openSite: (url) => call("open_site", { url }),
  openClockSettings: () => call("open_clock_settings"),
}

const ClientContext = createContext<Client>(tauriClient)

/** The app's client; tests provide a fake one. */
export const ClientProvider = ClientContext.Provider

export function useClient(): Client {
  return useContext(ClientContext)
}
