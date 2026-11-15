import { useMemo } from "react"

import { notify } from "@/app/notify"
import { LinksTab } from "@/features/sharing/links-section"
import { SharingClientProvider, type SharingClient } from "@/features/sharing/sharing-client"

import type { WebClipboard } from "../core/clipboard"
import type { Link } from "../core/link"

interface LinksViewProps {
  link: Link
  clipboard: WebClipboard
}

/** What the worker has no call for here: said plainly, never drawn (the W1 table). */
const notHere = (what: string) => () => Promise.reject({ code: "refused", message: `${what} isn't in the web vault; use the desktop app for it` })

/** Text from the bytes a desktop call takes (the worker takes the link as text). */
const text = (bytes: number[]) => new TextDecoder().decode(new Uint8Array(bytes))

/**
 * The calls the Links tab makes, over this tab's worker (docs/plan/17-web-vault.md).
 * Text links, requests and answers only: the web vault shares no item, and opens a
 * share link on its own /s/ page. A copy goes through the page's clipboard with its
 * clear timer, as every copy in the web vault does.
 */
function webLinksClient(link: Link, clipboard: WebClipboard): SharingClient {
  const no = notHere("Sharing a vault, an item or emergency access")
  return {
    overview: no,
    lookup: no,
    verify: no,
    forget: no,
    vault: no,
    shareVault: no,
    setRight: no,
    removeMember: no,
    createHousehold: no,
    addToHousehold: no,
    shareItem: no,
    unshareItem: no,
    sendItem: no,
    sendText: (title, body, days, maxViews, password) => link.callBytes("send_text", { title: title.trim(), text: body, password, days, maxViews }),
    sends: () => link.call("sends"),
    deleteSend: (sendId) => link.call("send_delete", { sendId }),
    openSend: notHere("Opening a share link here"),
    openedValue: (index) => link.callBytes("opened_value", { index }),
    openedFile: notHere("A file from a link"),
    saveOpened: notHere("Keeping what a link held"),
    createRequest: (label, days) => link.callBytes("request_create", { label, days }),
    requests: () => link.call("requests"),
    openAnswer: (requestId) => link.call("request_open", { requestId }),
    deleteRequest: (requestId) => link.call("request_delete", { requestId }),
    requestLabel: (l) => link.call("request_label", { link: text(l) }),
    answerRequest: (l, title, secret) => link.call("request_answer", { link: text(l), title, secret }),
    addEmergency: no,
    emergency: no,
    emergencyStep: no,
    emergencyOpen: no,
    copyOpened: async (index, secs) => clipboard.copy(await link.callBytes("opened_value", { index }), secs),
    copyLink: (bytes, secs) => clipboard.copy(new Uint8Array(bytes), secs),
    letBackIn: no,
  }
}

/** A share link pasted in the web vault opens on its own page, which keeps the key in the fragment. */
function openSharePage(pasted: string) {
  const url = new URL(pasted)
  if (url.origin !== window.location.origin) throw new Error("This link is for another server. Open it in a new tab.")
  window.location.assign(url.href)
}

/**
 * The web vault's Links tab (brief S4's web row, S1): the desktop's own
 * Links list and detail over the worker. Share some text, ask for a
 * secret, answer a request, open an answer; a link made here is shown
 * once and copied like a password.
 */
export function LinksView({ link, clipboard }: LinksViewProps) {
  const sharing = useMemo(() => webLinksClient(link, clipboard), [link, clipboard])
  return (
    <SharingClientProvider value={sharing}>
      <div className="flex w-full flex-col gap-6 pt-6">
        <LinksTab online onNotice={notify} onShareLink={openSharePage} canKeep={false} />
      </div>
    </SharingClientProvider>
  )
}
