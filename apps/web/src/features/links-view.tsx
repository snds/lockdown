import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from "react"

import {
  Button,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
  NativeSelect,
  NativeSelectOption,
  Spinner,
} from "@unicron/ui"
import { toast } from "sonner"

import { PaneHeader } from "@/components/master-detail"
import { describeError } from "@/core/client"
import { encodeSecret } from "@/core/secret-bytes"

import type { WebClipboard } from "../core/clipboard"
import type { Link } from "../core/link"

/** The lifetimes and open counts the desktop offers (D192). */
const DAYS = [1, 3, 7, 14, 30]
const OPENS: Array<[string, number | null]> = [
  ["Once", 1],
  ["3 times", 3],
  ["10 times", 10],
  ["Until it expires", null],
]

interface SendView {
  send_id: string
  created_at: number
  expires_at: number
  max_views: number | null
  views: number
  password: boolean
}

interface RequestView {
  request_id: string
  created_at: number
  expires_at: number
  answered_at: number | null
}

interface OpenedView {
  title: string
  values: Array<{ label: string; secret: boolean }>
}

const day = (secs: number) => new Date(secs * 1000).toLocaleDateString()

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold">{title}</h2>
      {children}
    </section>
  )
}

/** A link shown once, with Copy on the clipboard timer; the page keeps no copy after Done. */
function LinkOnce({ text, clipboard, onDone }: { text: string; clipboard: WebClipboard; onDone: () => void }) {
  const copy = async () => {
    try {
      const copied = await clipboard.copy(new TextEncoder().encode(text))
      toast(`Copied. The clipboard clears in ${copied.clears_in_secs} seconds.`)
    } catch (e) {
      toast.error(describeError(e))
    }
  }
  return (
    <div className="flex flex-col gap-2 rounded-md border p-3" role="group" aria-label="The new link">
      <code className="font-mono text-xs break-all" data-testid="new-link">
        {text}
      </code>
      <p className="text-xs text-muted-foreground">Shown once. Send it the way you'd send a password; the part after # is its key.</p>
      <div className="flex gap-2">
        <Button size="sm" onClick={() => void copy()}>
          Copy the link
        </Button>
        <Button size="sm" variant="outline" onClick={onDone}>
          Done
        </Button>
      </div>
    </div>
  )
}

interface LinksViewProps {
  link: Link
  clipboard: WebClipboard
}

/**
 * Share links and request links from the web vault (docs/plan/18-sharing.md
 * §7): share some text as a link, ask someone for a secret, and open what
 * they sent. Links are sealed in this tab's worker, the key in the
 * fragment; a link made here is shown once and copied like a password.
 */
export function LinksView({ link, clipboard }: LinksViewProps) {
  const [sends, setSends] = useState<SendView[] | null>(null)
  const [requests, setRequests] = useState<RequestView[] | null>(null)
  const [made, setMade] = useState<string | null>(null)
  const [asked, setAsked] = useState<string | null>(null)
  const [opened, setOpened] = useState<{ id: string; view: OpenedView; shown: Record<number, string> } | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const reload = useCallback(() => {
    link.call<SendView[]>("sends").then(setSends).catch((e) => setError(describeError(e)))
    link.call<RequestView[]>("requests").then(setRequests).catch((e) => setError(describeError(e)))
  }, [link])
  useEffect(reload, [reload])

  const run = async (f: () => Promise<void>) => {
    setBusy(true)
    setError(null)
    try {
      await f()
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const share = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const password = String(data.get("password") ?? "")
    const opens = String(data.get("opens") ?? "1")
    void run(async () => {
      const bytes = await link.callBytes("send_text", {
        title: String(data.get("title") ?? "").trim(),
        text: encodeSecret(String(data.get("text") ?? "")),
        password: password ? encodeSecret(password) : null,
        days: Number(data.get("days") ?? 7),
        maxViews: opens === "" ? null : Number(opens),
      })
      form.reset()
      setMade(new TextDecoder().decode(bytes))
      bytes.fill(0)
      reload()
    })
  }

  const ask = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    void run(async () => {
      const bytes = await link.callBytes("request_create", { label: String(data.get("label") ?? "").trim(), days: Number(data.get("days") ?? 7) })
      form.reset()
      setAsked(new TextDecoder().decode(bytes))
      bytes.fill(0)
      reload()
    })
  }

  const openAnswer = (id: string) =>
    void run(async () => {
      const view = await link.call<OpenedView>("request_open", { requestId: id })
      setOpened({ id, view, shown: {} })
    })
  const show = (index: number) =>
    void run(async () => {
      const bytes = await link.callBytes("opened_value", { index })
      const text = new TextDecoder().decode(bytes)
      bytes.fill(0)
      setOpened((o) => (o ? { ...o, shown: { ...o.shown, [index]: text } } : o))
    })

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-6 p-4 md:p-6">
      <PaneHeader title="Links" />
      {error && (
        <FieldError role="alert" className="text-sm">
          {error}
        </FieldError>
      )}

      <Section title="Share some text">
        {made ? (
          <LinkOnce text={made} clipboard={clipboard} onDone={() => setMade(null)} />
        ) : (
          <form onSubmit={share} className="flex flex-col gap-3" aria-label="Share some text">
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="send-title">What it is</FieldLabel>
                <Input id="send-title" name="title" autoComplete="off" placeholder="Optional" />
              </Field>
              <Field>
                <FieldLabel htmlFor="send-text">The text</FieldLabel>
                <Input id="send-text" name="text" type="password" required autoComplete="off" />
              </Field>
              <Field orientation="horizontal">
                <FieldLabel htmlFor="send-days">Stops opening after</FieldLabel>
                <NativeSelect id="send-days" name="days" defaultValue="7">
                  {DAYS.map((d) => (
                    <NativeSelectOption key={d} value={String(d)}>
                      {d === 1 ? "1 day" : `${d} days`}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </Field>
              <Field orientation="horizontal">
                <FieldLabel htmlFor="send-opens">Opens</FieldLabel>
                <NativeSelect id="send-opens" name="opens" defaultValue="1">
                  {OPENS.map(([label, n]) => (
                    <NativeSelectOption key={label} value={n === null ? "" : String(n)}>
                      {label}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </Field>
              <Field>
                <FieldLabel htmlFor="send-password">A password for it</FieldLabel>
                <Input id="send-password" name="password" type="password" autoComplete="off" placeholder="Optional" />
                <FieldDescription>Tell it to them another way than the link.</FieldDescription>
              </Field>
            </FieldGroup>
            <div>
              <Button type="submit" disabled={busy}>
                {busy ? <Spinner /> : null}
                Make the link
              </Button>
            </div>
          </form>
        )}
        {sends === null ? (
          <Spinner />
        ) : (
          <ul className="flex flex-col gap-2 text-sm" aria-label="Your share links">
            {sends.map((s) => (
              <li key={s.send_id} className="flex items-center justify-between gap-2 rounded-md border p-2">
                <span>
                  Made {day(s.created_at)}, until {day(s.expires_at)} · opened {s.views}
                  {s.max_views === null ? "" : ` of ${s.max_views}`}
                  {s.password ? " · password" : ""}
                </span>
                <Button size="sm" variant="ghost" onClick={() => void run(async () => (await link.call("send_delete", { sendId: s.send_id }), reload()))}>
                  Delete
                </Button>
              </li>
            ))}
          </ul>
        )}
      </Section>

      <Section title="Ask for a secret">
        {asked ? (
          <LinkOnce text={asked} clipboard={clipboard} onDone={() => setAsked(null)} />
        ) : (
          <form onSubmit={ask} className="flex flex-col gap-3" aria-label="Ask for a secret">
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="request-label">What you're asking for</FieldLabel>
                <Input id="request-label" name="label" required autoComplete="off" />
                <FieldDescription>They see this. Whatever they send is locked to your key; the link takes one answer.</FieldDescription>
              </Field>
              <Field orientation="horizontal">
                <FieldLabel htmlFor="request-days">Takes an answer for</FieldLabel>
                <NativeSelect id="request-days" name="days" defaultValue="7">
                  {DAYS.map((d) => (
                    <NativeSelectOption key={d} value={String(d)}>
                      {d === 1 ? "1 day" : `${d} days`}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </Field>
            </FieldGroup>
            <div>
              <Button type="submit" disabled={busy}>
                Make the request link
              </Button>
            </div>
          </form>
        )}
        {requests !== null && (
          <ul className="flex flex-col gap-2 text-sm" aria-label="Your requests">
            {requests.map((r) => (
              <li key={r.request_id} className="flex flex-col gap-2 rounded-md border p-2">
                <div className="flex items-center justify-between gap-2">
                  <span>
                    Asked {day(r.created_at)} · {r.answered_at ? `answered ${day(r.answered_at)}` : `open until ${day(r.expires_at)}`}
                  </span>
                  {r.answered_at && (
                    <Button size="sm" variant="outline" onClick={() => openAnswer(r.request_id)}>
                      Open the answer
                    </Button>
                  )}
                </div>
                {opened?.id === r.request_id && (
                  <div className="flex flex-col gap-2" aria-label="The answer" role="group">
                    <span className="font-medium">{opened.view.title || "The answer"}</span>
                    {opened.view.values.map((v, i) => (
                      <div key={`${i}:${v.label}`} className="flex items-center gap-2">
                        <span className="text-muted-foreground">{v.label}</span>
                        {opened.shown[i] !== undefined ? (
                          <code className="font-mono break-all">{opened.shown[i]}</code>
                        ) : (
                          <Button size="sm" variant="outline" onClick={() => show(i)} aria-label={`Show ${v.label}`}>
                            Show
                          </Button>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
      </Section>
    </div>
  )
}
