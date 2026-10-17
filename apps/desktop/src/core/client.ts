import { createContext, useContext } from "react"
import { invoke } from "@tauri-apps/api/core"

import type {
  AppError,
  DeviceInfo,
  ItemDetail,
  ItemSummary,
  LockState,
  OncePage,
  PolicyInput,
  Probe,
  Status,
  SyncOutcome,
} from "./types"
import { splitOncePage } from "./secret-bytes"

/**
 * The command surface of lockdown-app as the UI sees it. One method per
 * Tauri command (apps/desktop/src-tauri/src/commands.rs). Secrets go out
 * as byte arrays and come back as `Uint8Array`, never as strings in JSON.
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
  list(): Promise<ItemSummary[]>
  show(id: string): Promise<ItemDetail>
  reveal(id: string, fieldId: string): Promise<Uint8Array>
  /** Copies on the Rust side; resolves to the seconds until the clipboard is cleared. */
  copySecret(id: string, fieldId: string, clearAfterSecs?: number): Promise<number>
  addLogin(input: { site: string; username: string; password: number[]; title?: string }): Promise<string>
  editLogin(input: { id: string; title?: string; username?: string; password?: number[] }): Promise<number>
  trash(id: string): Promise<void>
  status(): Promise<Status>
  accountRegister(input: { server: string; login: string; password: string; secretKey?: string }): Promise<OncePage>
  accountLogin(input: { server: string; login: string; password: string; secretKey?: string }): Promise<string[]>
  recover(input: { server: string; login: string; password: string; secretKey: string; recoveryCode: string }): Promise<string[]>
  sync(): Promise<SyncOutcome>
  kitCreate(input: { replace: boolean; password?: string }): Promise<OncePage>
  kitVerify(input: { secretKey: string; recoveryCode: string; password?: string; drill: boolean }): Promise<string[]>
  deviceList(): Promise<DeviceInfo[]>
  deviceRevoke(id: string): Promise<string[]>
  openSite(url: string): Promise<void>
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
  list: () => call("list"),
  show: (id) => call("show", { id }),
  reveal: (id, fieldId) => callBytes("reveal", { id, fieldId }),
  copySecret: (id, fieldId, clearAfterSecs) => call("copy_secret", { id, fieldId, clearAfterSecs }),
  addLogin: (input) => call("add_login", input),
  editLogin: (input) => call("edit_login", input),
  trash: (id) => call("trash", { id }),
  status: () => call("status"),
  accountRegister: async (input) => splitOncePage(await callBytes("account_register", input)),
  accountLogin: (input) => call("account_login", input),
  recover: (input) => call("recover", input),
  sync: () => call("sync"),
  kitCreate: async (input) => splitOncePage(await callBytes("kit_create", input)),
  kitVerify: (input) => call("kit_verify", input),
  deviceList: () => call("device_list"),
  deviceRevoke: (id) => call("device_revoke", { id }),
  openSite: (url) => call("open_site", { url }),
}

const ClientContext = createContext<Client>(tauriClient)

/** The app's client; tests provide a fake one. */
export const ClientProvider = ClientContext.Provider

export function useClient(): Client {
  return useContext(ClientContext)
}
