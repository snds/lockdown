import { useCallback, useEffect, useState, type FormEvent } from "react"

import { Button, ConfirmDialog, EmptyState, FloatButton, ListRow, SectionHeader, TextField } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { describeError } from "@/core/client"
import { decodeAndWipe, encodeSecret } from "@/core/secret-bytes"
import { fullDate } from "@/lib/format"

import { monthDay } from "./emergency-model"
import { LastsField, LinkFields, LinkLayer, linkOptions } from "./link-once"
import { OpenedPanel } from "./opened-panel"
import { OFFLINE_REASON } from "./people-section"
import { ShareLayer } from "./share-layer"
import { useSharingClient, utf8, type OpenedView, type RequestView, type SendView } from "./sharing-client"
import { SharingPanes } from "./sharing-panes"

interface LinksTabProps {
  /** The server can be reached. */
  online: boolean
  onNotice: (text: string) => void
  /** A share link someone sent opens here (the desktop). Left out, it opens on its own page (the web vault's /s/ page). */
  onShareLink?: (link: string) => void
  /** KEEP IN MY VAULT on what a link held (not in the web vault, whose worker has no call for it). */
  canKeep?: boolean
  /** Tells the page how many links and requests there are, for the tab's count. */
  onCount?: (n: number) => void
  /** Tells the page whether a detail is open, so a pushed one can take the page (narrower than expanded). */
  onDetail?: (open: boolean) => void
}

/** The row of a share link (brief S4): `expires NOV 20 // opened 1 of 3 // password`. */
export function sendLine(s: SendView, nowSecs: number = Date.now() / 1000): string {
  const opens = s.max_views === null ? `opened ${s.views}` : `opened ${s.views} of ${s.max_views}`
  return [`${s.expires_at < nowSecs ? "expired" : "expires"} ${monthDay(s.expires_at)}`, opens, ...(s.password ? ["password"] : [])].join(" // ")
}

/** The row of a request link: `waiting // expires NOV 08`, or `answered NOV 03`. */
export function requestLine(r: RequestView): string {
  return r.answered_at ? `answered ${monthDay(r.answered_at)}` : `waiting // expires ${monthDay(r.expires_at)}`
}

/** A request link's path is `/r/`; anything else pasted is taken for a share link. */
export function isRequestLink(text: string): boolean {
  try {
    return new URL(text).pathname.includes("/r/")
  } catch {
    return /\/r\//.test(text)
  }
}

type Deleting = { kind: "send"; send: SendView } | { kind: "request"; request: RequestView } | null

/**
 * Sharing → LINKS (brief S4): send a secret to someone with no account,
 * ask someone for one, and open what was sent to you. The list holds
 * SHARE TEXT and ASK FOR SECRET (secondary), one field that opens a share
 * link or answers a request link, then the links and the requests made
 * here; what a link or an answer held opens in the detail.
 *
 * Share links are sealed on this device under a key that lives only in
 * the link's fragment (docs/plan/18-sharing.md §7), so the server keeps
 * ciphertext it can't read. A link made here is shown once. Deleting one
 * asks first. The lists do not say what a link was called: the list calls
 * return no title (wish W8).
 */
export function LinksTab({ online, onNotice, onShareLink, canKeep = true, onCount, onDetail }: LinksTabProps) {
  const sharing = useSharingClient()
  const [sends, setSends] = useState<SendView[] | null>(null)
  const [requests, setRequests] = useState<RequestView[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [making, setMaking] = useState<"text" | "ask" | null>(null)
  const [opened, setOpened] = useState<{ key: string; view: OpenedView } | null>(null)
  const [answering, setAnswering] = useState<{ link: string; label: string } | null>(null)
  const [deleting, setDeleting] = useState<Deleting>(null)
  const reason = online ? undefined : OFFLINE_REASON

  const load = useCallback(() => {
    if (!online) return
    Promise.all([sharing.sends(), sharing.requests()])
      .then(([s, r]) => {
        setSends(s)
        setRequests(r)
        onCount?.(s.length + r.length)
      })
      .catch((e) => setError(describeError(e)))
  }, [online, sharing, onCount])
  useEffect(load, [load])
  const isOpen = opened !== null
  useEffect(() => onDetail?.(isOpen), [isOpen, onDetail])

  const openAnswer = async (r: RequestView) => {
    setError(null)
    try {
      setOpened({ key: `answer:${r.request_id}`, view: await sharing.openAnswer(r.request_id) })
    } catch (e) {
      setError(describeError(e))
    }
  }
  const remove = async () => {
    const d = deleting
    if (!d) return
    try {
      if (d.kind === "send") await sharing.deleteSend(d.send.send_id)
      else await sharing.deleteRequest(d.request.request_id)
      onNotice(d.kind === "send" ? "Link deleted" : "Request deleted")
      load()
    } catch (e) {
      setError(describeError(e))
      throw e
    }
  }

  const list = (
    <>
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="secondary" disabled={!online} disabledReason={reason} onClick={() => setMaking("text")}>
          Share text
        </Button>
        <Button type="button" variant="secondary" disabled={!online} disabledReason={reason} onClick={() => setMaking("ask")}>
          Ask for secret
        </Button>
      </div>
      <OpenField
        online={online}
        onOpened={(view, link) => setOpened({ key: `link:${link.length}:${Date.now()}`, view })}
        onAnswer={setAnswering}
        onShareLink={onShareLink}
      />
      {error && (
        <p role="alert" className="type-caption text-critical-text">
          {error}
        </p>
      )}
      {sends !== null && requests !== null && sends.length + requests.length === 0 && (
        <EmptyState size="sm" overline="Links // 00" title="No links yet." body="Share some text as a link, or ask someone for a secret." />
      )}
      {sends !== null && sends.length > 0 && (
        <section aria-label="Share links" className="flex flex-col">
          <SectionHeader title="Share links" count={sends.length} />
          <div role="list" aria-label="Share links">
            {sends.map((s) => (
              <ListRow
                key={s.send_id}
                role="listitem"
                size="compact"
                title={`Created ${fullDate(s.created_at)}`}
                meta={sendLine(s)}
                action={
                  <Button type="button" size="sm" variant="ghost" disabled={!online} disabledReason={reason} onClick={() => setDeleting({ kind: "send", send: s })}>
                    Delete
                  </Button>
                }
              />
            ))}
          </div>
        </section>
      )}
      {requests !== null && requests.length > 0 && (
        <section aria-label="Requests" className="flex flex-col">
          <SectionHeader title="Requests" count={requests.length} />
          <div role="list" aria-label="Requests">
            {requests.map((r) => (
              <ListRow
                key={r.request_id}
                role="listitem"
                size="compact"
                selected={opened?.key === `answer:${r.request_id}`}
                title={`Asked ${monthDay(r.created_at)}`}
                meta={requestLine(r)}
                action={
                  <span className="flex items-center gap-1">
                    {r.answered_at && (
                      <Button type="button" size="sm" variant="ghost" disabled={!online} disabledReason={reason} onClick={() => void openAnswer(r)}>
                        Open answer
                      </Button>
                    )}
                    <Button type="button" size="sm" variant="ghost" disabled={!online} disabledReason={reason} onClick={() => setDeleting({ kind: "request", request: r })}>
                      Delete
                    </Button>
                  </span>
                }
              />
            ))}
          </div>
        </section>
      )}
    </>
  )

  return (
    <>
      <SharingPanes
        name="Links"
        backLabel="Links"
        onBack={() => setOpened(null)}
        emptyBody="Open a link or an answer to see what it holds here."
        list={list}
        detail={opened ? <OpenedPanel key={opened.key} opened={opened.view} onNotice={onNotice} canKeep={canKeep} /> : null}
      />
      <ShareTextLayer
        open={making === "text"}
        onClose={() => {
          setMaking(null)
          load()
        }}
      />
      <AskLayer
        open={making === "ask"}
        onClose={() => {
          setMaking(null)
          load()
        }}
      />
      <AnswerLayer
        answering={answering}
        onClose={() => setAnswering(null)}
        onSent={() => {
          setAnswering(null)
          onNotice("Sent. Only the person who asked can open it")
        }}
      />
      <ConfirmDialog
        open={deleting !== null}
        onOpenChange={(o) => !o && setDeleting(null)}
        overline={deleting?.kind === "send" ? `Share link // ${sendLine(deleting.send)}` : deleting ? `Request // ${requestLine(deleting.request)}` : "Links"}
        title={deleting?.kind === "request" ? "Delete this request?" : "Delete this link?"}
        description={
          deleting?.kind === "request"
            ? "The link stops taking an answer, and an answer it took is gone."
            : "It opens nothing from now on. What someone already opened, they may have kept."
        }
        confirmLabel="Delete"
        tone="destructive"
        onConfirm={remove}
      />
    </>
  )
}

/**
 * SOMEONE SENT YOU A LINK (brief S4): one field for a share link or a
 * request link, its password if it has one, and OPEN. A share link opens
 * in the detail; a request link opens the answer Form. A link that will
 * not open says why under the field, in the core's words.
 */
function OpenField({ online, onOpened, onAnswer, onShareLink }: {
  online: boolean
  onOpened: (view: OpenedView, link: string) => void
  onAnswer: (a: { link: string; label: string }) => void
  onShareLink?: (link: string) => void
}) {
  const sharing = useSharingClient()
  const [password, setPassword] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const typed = String(new FormData(form).get("link") ?? "").trim()
    if (!typed) return
    setError(null)
    setBusy(true)
    const pw = password
    try {
      if (isRequestLink(typed)) {
        const label = await sharing.requestLabel(utf8(typed))
        onAnswer({ link: typed, label })
      } else if (onShareLink) {
        onShareLink(typed)
      } else {
        onOpened(await sharing.openSend(utf8(typed), pw ? utf8(pw) : null), typed)
      }
      form.reset()
      setPassword("")
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <section aria-label="Someone sent you a link" className="flex flex-col gap-3">
      <SectionHeader title="Someone sent you a link" />
      <form aria-label="Open a link" className="flex flex-col gap-3" onSubmit={(e) => void submit(e)}>
        <TextField
          label="Paste a share link or a request link"
          name="link"
          autoComplete="off"
          spellCheck={false}
          placeholder="https://"
          error={error ?? undefined}
          disabled={!online}
          disabledReason={online ? undefined : OFFLINE_REASON}
        />
        {!onShareLink && <SecretField mode="edit" label="Its password, if it has one" value={password} onChange={setPassword} autoComplete="off" disabled={!online} />}
        <div>
          <Button type="submit" variant="secondary" disabled={!online} disabledReason={online ? undefined : OFFLINE_REASON} loading={busy}>
            Open
          </Button>
        </div>
      </form>
    </section>
  )
}

/** Share text (brief S4): what it is, the text, how long, how many opens, a password; then the link once. */
function ShareTextLayer({ open, onClose }: { open: boolean; onClose: () => void }) {
  const sharing = useSharingClient()
  const [password, setPassword] = useState("")
  return (
    <LinkLayer
      open={open}
      onClose={onClose}
      overline="Links // share text"
      title="Share text"
      formId="send-text"
      onReset={() => setPassword("")}
      onMake={async (form) => {
        const data = new FormData(form)
        const title = String(data.get("title") ?? "")
        const text = String(data.get("text") ?? "")
        const { days, views } = linkOptions(form)
        if (!text) throw new Error("Type the text to share.")
        const bytes = encodeSecret(text)
        try {
          return decodeAndWipe(await sharing.sendText(title, bytes, days, views, password ? utf8(password) : null))
        } finally {
          bytes.fill(0)
        }
      }}
    >
      <TextField label="What it is (they see this)" name="title" autoComplete="off" />
      <TextField label="The text" name="text" multiline minRows={6} autoComplete="off" spellCheck={false} />
      <LinkFields idPrefix="send" password={password} onPassword={setPassword} />
    </LinkLayer>
  )
}

/** Ask for a secret (brief S4): what you're asking for, how long; then the link once. */
function AskLayer({ open, onClose }: { open: boolean; onClose: () => void }) {
  const sharing = useSharingClient()
  return (
    <LinkLayer
      open={open}
      onClose={onClose}
      overline="Links // ask for a secret"
      title="Ask for a secret"
      formId="request-make"
      note="Send it to them. It works once, until it expires."
      onMake={async (form) => {
        const data = new FormData(form)
        const label = String(data.get("label") ?? "").trim()
        if (!label) throw new Error("Say what you're asking for.")
        return decodeAndWipe(await sharing.createRequest(label, Number(data.get("days") ?? 7)))
      }}
    >
      <TextField label="What you're asking for (they see this)" name="label" autoComplete="off" placeholder="The Wi-Fi password" />
      <LastsField id="request-days" />
    </LinkLayer>
  )
}

/** Answer a request (brief S4): what they asked for, a name, the secret; SEND IT. */
function AnswerLayer({ answering, onClose, onSent }: { answering: { link: string; label: string } | null; onClose: () => void; onSent: () => void }) {
  const sharing = useSharingClient()
  const [secret, setSecret] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const close = () => {
    setSecret("")
    setError(null)
    onClose()
  }
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!answering || !secret) return
    const title = String(new FormData(e.currentTarget).get("title") ?? "")
    setError(null)
    setBusy(true)
    const bytes = encodeSecret(secret)
    try {
      await sharing.answerRequest(utf8(answering.link), title, bytes)
      setSecret("")
      onSent()
    } catch (err) {
      setError(describeError(err))
    } finally {
      bytes.fill(0)
      setBusy(false)
    }
  }
  return (
    <ShareLayer
      open={answering !== null}
      onClose={close}
      overline="Links // answer a request"
      title="Answer a request"
      footer={
        <>
          <Button type="button" variant="ghost" onClick={close}>
            Cancel
          </Button>
          <Button type="submit" form="request-answer" loading={busy}>
            Send it
          </Button>
        </>
      }
      phoneAction={<FloatButton label="Send it" icon="send" kind="primary" size="l" extended type="submit" form="request-answer" disabled={busy} />}
    >
      <form id="request-answer" aria-label="Answer a request" className="flex flex-col gap-4" onSubmit={(e) => void submit(e)}>
        <p className="type-body text-text-body">They asked for: {answering?.label}</p>
        <TextField label="A name for it (optional)" name="title" autoComplete="off" />
        <SecretField mode="edit" label="The secret" value={secret} onChange={setSecret} autoComplete="off" required />
        <p className="type-caption text-text-secondary">Send it only if you know who sent you the link.</p>
        {error && (
          <p role="alert" className="type-caption text-critical-text">
            {error}
          </p>
        )}
      </form>
    </ShareLayer>
  )
}

