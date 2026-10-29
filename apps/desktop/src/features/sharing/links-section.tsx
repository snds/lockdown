import { useCallback, useEffect, useState, type FormEvent } from "react"

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Icon,
  Input,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
  NativeSelect,
  NativeSelectOption,
  Textarea,
} from "@unicron/ui"

import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError } from "@/core/client"
import { decodeAndWipe, encodeSecret } from "@/core/secret-bytes"
import { whenLocal } from "@/lib/format"

import { LinkFields } from "./item-share"
import { LinkOnce } from "./link-once"
import { OpenedPanel } from "./opened-panel"
import { useSharingClient, utf8, type OpenedView, type RequestView, type SendView } from "./sharing-client"

interface LinksProps {
  online: boolean
  onNotice: (text: string) => void
}

const viewsWords = (s: SendView) =>
  s.max_views === null ? `opened ${s.views} ${s.views === 1 ? "time" : "times"}` : `opened ${s.views} of ${s.max_views}`

/**
 * Share links (docs/plan/18-sharing.md §7): text (or an item, from its
 * detail) sealed on this device under a key that lives only in the link's
 * fragment, so the server keeps ciphertext it can't read, with an expiry,
 * a limit on opens and an optional password. Opening someone's link
 * happens here too, and what it holds shows in place.
 */
export function ShareLinksSection({ online, onNotice }: LinksProps) {
  const sharing = useSharingClient()
  const [sends, setSends] = useState<SendView[]>([])
  const [error, setError] = useState<string | null>(null)
  const [making, setMaking] = useState(false)
  const [link, setLink] = useState<string | null>(null)
  const [opened, setOpened] = useState<OpenedView | null>(null)

  const load = useCallback(() => {
    if (!online) return
    sharing
      .sends()
      .then(setSends)
      .catch((e) => setError(describeError(e)))
  }, [online, sharing])
  useEffect(load, [load])

  const make = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const title = String(data.get("title") ?? "")
    const text = String(data.get("text") ?? "")
    const days = Number(data.get("days") ?? 7)
    const views = String(data.get("views") ?? "")
    const password = String(data.get("password") ?? "")
    form.reset()
    if (!text) return
    setError(null)
    try {
      const bytes = await sharing.sendText(title, encodeSecret(text), days, views ? Number(views) : null, password ? utf8(password) : null)
      setLink(decodeAndWipe(bytes))
      load()
    } catch (err) {
      setError(describeError(err))
    }
  }
  const remove = async (s: SendView) => {
    try {
      await sharing.deleteSend(s.send_id)
      onNotice("The link is gone; it opens nothing now.")
      load()
    } catch (e) {
      setError(describeError(e))
    }
  }
  const open = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const typed = String(data.get("link") ?? "").trim()
    const password = String(data.get("password") ?? "")
    form.reset()
    if (!typed) return
    setError(null)
    setOpened(null)
    try {
      setOpened(await sharing.openSend(utf8(typed), password ? utf8(password) : null))
    } catch (err) {
      setError(describeError(err))
    }
  }

  return (
    <section aria-labelledby="sharing-links" className="flex flex-col gap-2">
      <h2 id="sharing-links" className="text-base font-semibold">
        Share links
      </h2>
      <div className="flex flex-wrap gap-2">
        <Button
          variant="outline"
          size="sm"
          disabled={!online}
          onClick={() => {
            setError(null)
            setMaking(true)
          }}
        >
          <Icon name="add_link" data-icon="inline-start" />
          Share some text…
        </Button>
      </div>
      {sends.length > 0 && (
        <ItemGroup role="list" aria-label="Your share links">
          {sends.map((s) => (
            <Item key={s.send_id} variant="outline" size="sm" role="listitem">
              <ItemContent>
                <ItemTitle>Made {whenLocal(s.created_at)}</ItemTitle>
                <ItemDescription>
                  Expires {whenLocal(s.expires_at)} · {viewsWords(s)}
                  {s.password ? " · needs a password" : ""}
                </ItemDescription>
              </ItemContent>
              <ItemActions>
                <Button variant="ghost-destructive" size="sm" onClick={() => void remove(s)}>
                  Delete
                </Button>
              </ItemActions>
            </Item>
          ))}
        </ItemGroup>
      )}
      <form className="flex flex-col gap-2" aria-label="Open a share link" onSubmit={(e) => void open(e)}>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="open-link">Open a share link someone sent you</FieldLabel>
            <Input id="open-link" name="link" autoComplete="off" spellCheck={false} placeholder="https://…/s/…#…" />
            <FieldDescription>It opens here, on this device; the server never sees what's in it.</FieldDescription>
          </Field>
          <Field>
            <FieldLabel htmlFor="open-link-password">Its password, if it has one</FieldLabel>
            <Input id="open-link-password" name="password" type="password" autoComplete="off" />
          </Field>
        </FieldGroup>
        <div>
          <Button type="submit" variant="outline" size="sm">
            <Icon name="lock_open" data-icon="inline-start" />
            Open
          </Button>
        </div>
      </form>
      {error && <FieldError role="alert">{error}</FieldError>}
      {opened && <OpenedPanel opened={opened} onNotice={onNotice} />}
      <Dialog
        open={making}
        onOpenChange={(o) => {
          if (!o) {
            setMaking(false)
            setLink(null)
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Share some text</DialogTitle>
            <DialogDescription>
              It is locked on this computer before it leaves; the key is the part of the link after <code>#</code>, which never reaches the
              server.
            </DialogDescription>
          </DialogHeader>
          {link ? (
            <LinkOnce link={link} />
          ) : (
            <form id="send-text" className="flex flex-col gap-3" onSubmit={(e) => void make(e)}>
              <Field>
                <FieldLabel htmlFor="send-title">What it is (they see this)</FieldLabel>
                <Input id="send-title" name="title" autoComplete="off" />
              </Field>
              <Field>
                <FieldLabel htmlFor="send-text-body">The text</FieldLabel>
                <Textarea id="send-text-body" name="text" rows={4} autoComplete="off" spellCheck={false} />
              </Field>
              <LinkFields />
            </form>
          )}
          {/* A refusal (a split view pauses links, D328 as changed by D375) shows where the person is looking. */}
          {error && !link && <FieldError role="alert">{error}</FieldError>}
          <LabelledActions>
            <LabelledAction
              kind="cancel"
              onClick={() => {
                setMaking(false)
                setLink(null)
              }}
            >
              {link ? "Done" : "Cancel"}
            </LabelledAction>
            {!link && (
              <LabelledAction kind="confirm" type="submit" form="send-text">
                Make the link
              </LabelledAction>
            )}
          </LabelledActions>
        </DialogContent>
      </Dialog>
    </section>
  )
}

/**
 * Request links (docs/plan/18-sharing.md §7): a link someone else uses
 * once to send you a secret, sealed on their device to your key, which
 * the link pins. Answering someone's request link happens here too.
 */
export function RequestLinksSection({ online, onNotice }: LinksProps) {
  const sharing = useSharingClient()
  const [requests, setRequests] = useState<RequestView[]>([])
  const [error, setError] = useState<string | null>(null)
  const [making, setMaking] = useState(false)
  const [link, setLink] = useState<string | null>(null)
  const [opened, setOpened] = useState<OpenedView | null>(null)
  const [answering, setAnswering] = useState<{ link: string; label: string } | null>(null)

  const load = useCallback(() => {
    if (!online) return
    sharing
      .requests()
      .then(setRequests)
      .catch((e) => setError(describeError(e)))
  }, [online, sharing])
  useEffect(load, [load])

  const make = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const data = new FormData(e.currentTarget)
    const label = String(data.get("label") ?? "").trim()
    const days = Number(data.get("days") ?? 7)
    if (!label) return
    setError(null)
    try {
      setLink(decodeAndWipe(await sharing.createRequest(label, days)))
      load()
    } catch (err) {
      setError(describeError(err))
    }
  }
  const openAnswer = async (r: RequestView) => {
    setError(null)
    try {
      setOpened(await sharing.openAnswer(r.request_id))
    } catch (e) {
      setError(describeError(e))
    }
  }
  const remove = async (r: RequestView) => {
    try {
      await sharing.deleteRequest(r.request_id)
      onNotice("The request is gone.")
      load()
    } catch (e) {
      setError(describeError(e))
    }
  }
  const read = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const typed = String(new FormData(form).get("link") ?? "").trim()
    if (!typed) return
    setError(null)
    try {
      const label = await sharing.requestLabel(utf8(typed))
      form.reset()
      setAnswering({ link: typed, label })
    } catch (err) {
      setError(describeError(err))
    }
  }

  return (
    <section aria-labelledby="sharing-requests" className="flex flex-col gap-2">
      <h2 id="sharing-requests" className="text-base font-semibold">
        Request links
      </h2>
      <div className="flex flex-wrap gap-2">
        <Button
          variant="outline"
          size="sm"
          disabled={!online}
          onClick={() => {
            setError(null)
            setMaking(true)
          }}
        >
          <Icon name="inbox" data-icon="inline-start" />
          Ask someone for a secret…
        </Button>
      </div>
      {requests.length > 0 && (
        <ItemGroup role="list" aria-label="Your request links">
          {requests.map((r) => (
            <Item key={r.request_id} variant="outline" size="sm" role="listitem">
              <ItemContent>
                <ItemTitle>{r.answered_at ? `Answered ${whenLocal(r.answered_at)}` : "Waiting for an answer"}</ItemTitle>
                <ItemDescription>
                  Made {whenLocal(r.created_at)} · expires {whenLocal(r.expires_at)}
                </ItemDescription>
              </ItemContent>
              <ItemActions>
                {r.answered_at && (
                  <Button variant="outline" size="sm" onClick={() => void openAnswer(r)}>
                    Open the answer
                  </Button>
                )}
                <Button variant="ghost-destructive" size="sm" onClick={() => void remove(r)}>
                  Delete
                </Button>
              </ItemActions>
            </Item>
          ))}
        </ItemGroup>
      )}
      <form className="flex flex-wrap items-end gap-2" aria-label="Answer a request link" onSubmit={(e) => void read(e)}>
        <Field>
          <FieldLabel htmlFor="answer-link">Answer someone's request link</FieldLabel>
          <Input id="answer-link" name="link" autoComplete="off" spellCheck={false} placeholder="https://…/r/…#…" />
        </Field>
        <Button type="submit" variant="outline" size="sm">
          Continue
        </Button>
      </form>
      {error && <FieldError role="alert">{error}</FieldError>}
      {opened && <OpenedPanel opened={opened} onNotice={onNotice} />}
      <Dialog
        open={making}
        onOpenChange={(o) => {
          if (!o) {
            setMaking(false)
            setLink(null)
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Ask someone for a secret</DialogTitle>
            <DialogDescription>
              They open the link and type the secret; it is locked on their device to your key and the link works once. The server can't read
              the answer.
            </DialogDescription>
          </DialogHeader>
          {link ? (
            <LinkOnce link={link} note="Send it to them. It works once, until it expires." />
          ) : (
            <form id="request-make" className="flex flex-col gap-3" onSubmit={(e) => void make(e)}>
              <Field>
                <FieldLabel htmlFor="request-label">What you're asking for (they see this)</FieldLabel>
                <Input id="request-label" name="label" autoComplete="off" placeholder="The Wi-Fi password" />
              </Field>
              <Field>
                <FieldLabel htmlFor="request-days">Lasts</FieldLabel>
                <NativeSelect id="request-days" name="days" defaultValue="7">
                  {[1, 3, 7, 14, 30].map((d) => (
                    <NativeSelectOption key={d} value={String(d)}>
                      {d === 1 ? "1 day" : `${d} days`}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </Field>
            </form>
          )}
          {error && !link && <FieldError role="alert">{error}</FieldError>}
          <LabelledActions>
            <LabelledAction
              kind="cancel"
              onClick={() => {
                setMaking(false)
                setLink(null)
              }}
            >
              {link ? "Done" : "Cancel"}
            </LabelledAction>
            {!link && (
              <LabelledAction kind="confirm" type="submit" form="request-make">
                Make the link
              </LabelledAction>
            )}
          </LabelledActions>
        </DialogContent>
      </Dialog>
      <AnswerDialog
        answering={answering}
        onClose={() => setAnswering(null)}
        onSent={() => {
          setAnswering(null)
          onNotice("Sent. Only the person who asked can open it.")
        }}
      />
    </section>
  )
}

function AnswerDialog({ answering, onClose, onSent }: { answering: { link: string; label: string } | null; onClose: () => void; onSent: () => void }) {
  const sharing = useSharingClient()
  const [error, setError] = useState<string | null>(null)
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!answering) return
    const form = e.currentTarget
    const data = new FormData(form)
    const title = String(data.get("title") ?? "")
    const secret = String(data.get("secret") ?? "")
    form.reset()
    if (!secret) return
    setError(null)
    const bytes = encodeSecret(secret)
    try {
      await sharing.answerRequest(utf8(answering.link), title, bytes)
      onSent()
    } catch (err) {
      setError(describeError(err))
    } finally {
      bytes.fill(0)
    }
  }
  return (
    <Dialog open={answering !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>They ask for: {answering?.label}</DialogTitle>
          <DialogDescription>
            What you type is locked on this computer to the key the link names, and only they can open it. Send it only if you know who sent
            you the link.
          </DialogDescription>
        </DialogHeader>
        <form id="request-answer" className="flex flex-col gap-3" onSubmit={(e) => void submit(e)}>
          <Field>
            <FieldLabel htmlFor="answer-title">A name for it (optional)</FieldLabel>
            <Input id="answer-title" name="title" autoComplete="off" />
          </Field>
          <Field>
            <FieldLabel htmlFor="answer-secret">The secret</FieldLabel>
            <Input id="answer-secret" name="secret" type="password" autoComplete="off" />
          </Field>
        </form>
        {error && <FieldError role="alert">{error}</FieldError>}
        <LabelledActions>
          <LabelledAction kind="cancel" onClick={onClose}>
            Cancel
          </LabelledAction>
          <LabelledAction kind="confirm" type="submit" form="request-answer">
            Send it
          </LabelledAction>
        </LabelledActions>
      </DialogContent>
    </Dialog>
  )
}
