import type { GateInput } from "@/core/types"
import type { HygieneClient } from "@/features/hygiene/hygiene-client"
import type { QuickClient } from "@/features/quick/quick-client"
import type { Frame, ScanClient } from "@/features/scan/scan-client"
import type { SharingClient } from "@/features/sharing/sharing-client"

import type { Link } from "./link"

const g = (gate?: GateInput) => (gate ? { gate } : {})

function notHere(what: string): Promise<never> {
  return Promise.reject({ code: "refused", message: `${what} isn't in the web vault; use the desktop app for it` })
}

/**
 * Quick access over the worker (crates/web/src/commands/finish.rs): the
 * core's one index, the code shown in place. The web vault can't fill
 * another tab, so "Fill in the browser" isn't offered here.
 */
export function webQuickClient(link: Link): QuickClient {
  return {
    search: (query, limit) => link.call("quick_search", { query, limit }),
    quickCode: (itemId, fieldId, gate) => link.callBytes("quick_code", { itemId, fieldId, ...g(gate) }),
    quickFill: () => notHere("Filling another tab"),
    canFill: false,
    hide: () => Promise.resolve(),
    openMain: () => Promise.resolve(),
    hotkeyStatus: () => Promise.resolve(null),
    setHotkey: () => Promise.resolve(null),
  }
}

const frameArgs = (f: Frame) => ({ width: f.width, height: f.height, luma: Array.from(f.luma) })

/**
 * The scan flow over the worker: the tab's camera frames go to the core,
 * which reads the code and runs the proximity gate and the signed list.
 * An approved page opens in a new tab (https only), where the person signs
 * in; the web vault has no extension to fill it.
 */
export function webScanClient(link: Link, open: (url: string) => void): ScanClient {
  return {
    frame: (f, source) => link.call("scan_frame", { ...frameArgs(f), source }),
    text: (text) => link.call("scan_text", { text }),
    approve: async (activationId, itemId, gate) => {
      const started = await link.call<{ url: string; host: string; service: string; code: string; code_in_url: boolean }>("activation_approve", { activationId, itemId, ...g(gate) })
      if (started.url.startsWith("https://")) open(started.url)
      return started
    },
    cancel: (activationId) => link.call("activation_cancel", { activationId }),
    contactFrame: (f) => link.call("qr_contact_frame", frameArgs(f)),
  }
}

/** What the core keeps out of swap; a tab can't keep itself out of screen captures. */
export function webHygieneClient(link: Link): HygieneClient {
  return {
    protect: () => Promise.resolve(false),
    memory: () => link.call("memory_report"),
  }
}

/**
 * The contacts screen's part of the desktop's sharing client: look a person
 * up in the key-transparency log, compare (typed, pasted or scanned), and
 * forget. Sharing vaults and items stays the desktop's.
 */
export function webSharingClient(link: Link): SharingClient {
  const no = () => notHere("Sharing a vault or an item")
  return {
    overview: () => link.call("sharing_overview"),
    lookup: (login) => link.call("contact_lookup", { login }),
    verify: (account, fingerprint) => link.call("contact_verify", { account, fingerprint }),
    forget: (account) => link.call("contact_forget", { account }),
    vault: no,
    shareVault: no,
    setRight: no,
    removeMember: no,
    createHousehold: no,
    addToHousehold: no,
    shareItem: no,
    unshareItem: no,
    sendItem: no,
    sendText: no,
    sends: no,
    deleteSend: no,
    openSend: no,
    openedValue: no,
    openedFile: no,
    saveOpened: no,
    createRequest: no,
    requests: no,
    openAnswer: no,
    deleteRequest: no,
    requestLabel: no,
    answerRequest: no,
    addEmergency: no,
    emergency: no,
    emergencyStep: no,
    emergencyOpen: no,
    copyOpened: no,
    copyLink: no,
  }
}
