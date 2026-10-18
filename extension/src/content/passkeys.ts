// The passkey bridge: Lockdown's isolated-world script in a tab's top frame,
// from document_start (docs/plan/19-passkeys.md §3). It takes the port the
// main-world wrapper offers, carries each request to the background, shows
// the passkey sheet (an extension-origin frame in a closed shadow root in
// the top layer) when the background asks for it, and hands the answer
// back. It carries nothing that approves: the page can talk to this port
// as well as the wrapper can, and gets no more for it (the wrapper's
// header says why).

import type { ToContent } from "../shared/messages"
import { extensionOrigin } from "../shared/messages"
import { makeHost } from "./host"
import { hostVisible } from "./visibility"

const ORIGIN = extensionOrigin()
const SHEET_W = 360

interface PageMsg {
  t: "req" | "abort" | "status" | "signal"
  id?: number
  op?: "create" | "get"
  mediation?: string
  request?: Record<string, unknown>
  signal?: Record<string, unknown>
}

let port: MessagePort | null = null
/** The page's request id for each open background request, and back. */
const pageId = new Map<string, number>()
const rid = new Map<number, string>()
let sheet: { rid: string; host: HTMLElement; frame: HTMLIFrameElement } | null = null

function reply(msg: Record<string, unknown>) {
  port?.postMessage({ t: "res", ...msg })
}

function closeSheet(forRid?: string) {
  if (!sheet || (forRid && sheet.rid !== forRid)) return
  sheet.host.remove()
  sheet = null
}

function showSheet(r: string) {
  closeSheet()
  const { host, frame } = makeHost(chrome.runtime.getURL(`frame.html#passkey:${r}`), "Lockdown passkeys", SHEET_W, 150)
  host.style.top = "12px"
  host.style.left = `${Math.max(4, Math.round((window.innerWidth - SHEET_W) / 2))}px`
  sheet = { rid: r, host, frame }
}

async function onPage(e: MessageEvent) {
  const msg = e.data as PageMsg
  if (!msg || typeof msg !== "object") return
  try {
    if (msg.t === "status" && typeof msg.id === "number") {
      const a = (await chrome.runtime.sendMessage({ t: "pk_status" })) as { active?: boolean }
      reply({ id: msg.id, active: a?.active === true })
      return
    }
    if (msg.t === "signal" && msg.signal) {
      await chrome.runtime.sendMessage({ t: "pk_signal", signal: msg.signal })
      return
    }
    if (msg.t === "abort" && typeof msg.id === "number") {
      const r = rid.get(msg.id)
      if (r) {
        rid.delete(msg.id)
        pageId.delete(r)
        closeSheet(r)
        await chrome.runtime.sendMessage({ t: "pk_abort", rid: r })
      }
      return
    }
    if (msg.t === "req" && typeof msg.id === "number" && (msg.op === "create" || msg.op === "get")) {
      const id = msg.id
      const a = (await chrome.runtime.sendMessage({ t: "pk_begin", op: msg.op, mediation: String(msg.mediation ?? "optional"), request: msg.request ?? {} })) as {
        handoff?: string
        pending?: string
        sheet?: boolean
        credential?: unknown
        error?: { name: string; message: string }
      } | null
      if (a?.pending) {
        rid.set(id, a.pending)
        pageId.set(a.pending, id)
        if (a.sheet) showSheet(a.pending)
      } else if (a?.credential) reply({ id, credential: a.credential })
      else if (a?.error) reply({ id, error: a.error })
      else reply({ id, handoff: "same" })
    }
  } catch {
    // The extension went away (an update, a reload): the browser's own.
    if (msg.t === "req" && typeof msg.id === "number") reply({ id: msg.id, handoff: "same" })
    if (msg.t === "status" && typeof msg.id === "number") reply({ id: msg.id, active: false })
  }
}

// The wrapper's hello: the first port offered is the one kept. Both
// scripts run at document_start, before any page script.
window.addEventListener("message", (e) => {
  if (port || e.source !== window || (e.data as { lockdown_passkeys?: string })?.lockdown_passkeys !== "hello" || !e.ports[0]) return
  port = e.ports[0]
  port.onmessage = (m) => void onPage(m)
  port.postMessage({ t: "ack" })
})

// The sheet says how tall it wants to be; only Lockdown's own frame.
window.addEventListener("message", (e) => {
  if (!sheet || e.source !== sheet.frame.contentWindow || e.origin !== ORIGIN) return
  const data = e.data as { lockdown?: string; px?: number }
  if (data?.lockdown === "height" && typeof data.px === "number") sheet.host.style.height = `${Math.max(40, Math.min(420, Math.round(data.px)))}px`
})

chrome.runtime.onMessage.addListener((msg: ToContent, sender, sendResponse) => {
  if (sender.id !== chrome.runtime.id) return false
  if (msg.t === "pk_done") {
    const id = pageId.get(msg.rid)
    closeSheet(msg.rid)
    if (id !== undefined) {
      pageId.delete(msg.rid)
      rid.delete(id)
      reply({ id, credential: msg.credential, error: msg.error, handoff: msg.handoff })
    }
    sendResponse({ ok: true })
    return false
  }
  if (msg.t === "pk_measure") {
    sendResponse({ visible: !!sheet && sheet.rid === msg.rid && hostVisible(sheet.host) })
    return false
  }
  return false
})
