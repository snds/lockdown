import { useEffect, useState, type FormEvent } from "react"

import { Button, Field, FieldDescription, FieldError, FieldGroup, FieldLabel, Input, Spinner, Toaster } from "@unicron/ui"
import { toast } from "sonner"

import { describeError } from "@/core/client"
import { encodeSecret } from "@/core/secret-bytes"

import type { WebClipboard } from "../core/clipboard"
import type { Link } from "../core/link"
import type { PageLink } from "../core/page-link"
import { Frame } from "./sign-in-screen"

interface OpenedValue {
  label: string
  secret: boolean
}

interface OpenedView {
  kind: "text" | "file" | "item"
  title: string
  values: OpenedValue[]
  file: { name: string; mime: string; size: number } | null
}

interface LinkMeta {
  password: boolean
  expires_at: number
  views_left: number | null
}

interface LinkPageProps {
  page: PageLink
  link: Link
  clipboard: WebClipboard
}

/**
 * Opening a share link, or answering a request, with no account
 * (docs/plan/18-sharing.md §7, D198). Everything happens in this tab's
 * worker; the server sees the link's id, never its key.
 */
export function LinkPage({ page, link, clipboard }: LinkPageProps) {
  return (
    <>
      {page.kind === "share" ? <SharePage link={page.link} rpc={link} clipboard={clipboard} /> : <RequestPage link={page.link} rpc={link} />}
      <Toaster position="bottom-center" />
    </>
  )
}

function SharePage({ link, rpc, clipboard }: { link: string; rpc: Link; clipboard: WebClipboard }) {
  const [meta, setMeta] = useState<LinkMeta | null>(null)
  const [opened, setOpened] = useState<OpenedView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    rpc
      .call<LinkMeta>("link_meta", { link })
      .then(setMeta)
      .catch((e) => setError(describeError(e)))
  }, [rpc, link])

  const open = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const typed = String(new FormData(form).get("password") ?? "")
    setBusy(true)
    setError(null)
    try {
      const view = await rpc.call<OpenedView>("link_open", { link, password: typed ? encodeSecret(typed) : null })
      form.reset()
      setOpened(view)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  if (opened) return <OpenedLink view={opened} rpc={rpc} clipboard={clipboard} />
  return (
    <Frame>
      <h1 className="mb-2 text-lg font-semibold tracking-tight">Someone shared something with you</h1>
      <p className="mb-4 text-sm text-muted-foreground">
        It was locked on their device, and the key is in the part of the link after the <code>#</code>, which never reaches the server. It opens here, in this
        tab.
      </p>
      {meta === null && !error && <Spinner />}
      {error && (
        <FieldError role="alert" className="mb-4">
          {error}
        </FieldError>
      )}
      {meta && (
        <form onSubmit={open} className="flex flex-col gap-4" aria-label="Open the link">
          <p className="text-sm text-muted-foreground">
            {meta.views_left === null
              ? "Opening it counts as a view."
              : `Opening it counts as one of its views: ${meta.views_left} left.`}{" "}
            It stops opening on {new Date(meta.expires_at * 1000).toLocaleDateString()}.
          </p>
          {meta.password && (
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="link-password">The link's password</FieldLabel>
                <Input id="link-password" name="password" type="password" required autoComplete="off" />
                <FieldDescription>The sender gave it to you separately.</FieldDescription>
              </Field>
            </FieldGroup>
          )}
          <Button type="submit" disabled={busy}>
            {busy ? <Spinner /> : null}
            Open
          </Button>
        </form>
      )}
    </Frame>
  )
}

function OpenedLink({ view, rpc, clipboard }: { view: OpenedView; rpc: Link; clipboard: WebClipboard }) {
  const [shown, setShown] = useState<Record<number, string>>({})
  const reveal = async (index: number) => {
    const bytes = await rpc.callBytes("link_value", { index })
    const text = new TextDecoder().decode(bytes)
    bytes.fill(0)
    setShown((s) => ({ ...s, [index]: text }))
  }
  const hide = (index: number) =>
    setShown((s) => {
      const next = { ...s }
      delete next[index]
      return next
    })
  const copy = async (index: number) => {
    try {
      const copied = await clipboard.copy(await rpc.callBytes("link_value", { index }))
      toast(`Copied. The clipboard clears in ${copied.clears_in_secs} seconds.`)
    } catch (e) {
      toast.error(describeError(e))
    }
  }
  const save = async () => {
    if (!view.file) return
    try {
      const bytes = await rpc.callBytes("link_file")
      const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: view.file.mime || "application/octet-stream" }))
      const a = document.createElement("a")
      a.href = url
      a.download = view.file.name || "shared-file"
      a.click()
      setTimeout(() => {
        URL.revokeObjectURL(url)
        bytes.fill(0)
      }, 1000)
    } catch (e) {
      toast.error(describeError(e))
    }
  }
  return (
    <Frame>
      <h1 className="mb-1 text-lg font-semibold tracking-tight">{view.title || "Shared with you"}</h1>
      <p className="mb-4 text-sm text-muted-foreground">Keep what you need now: the link may not open again. Nothing here is saved in this browser.</p>
      <ul className="flex flex-col gap-3" aria-label="What the link holds">
        {view.values.map((v, i) => (
          <li key={`${i}:${v.label}`} className="flex flex-col gap-1 rounded-md border p-3">
            <span className="text-sm font-medium">{v.label}</span>
            {shown[i] !== undefined ? (
              <code className="font-mono text-sm break-all whitespace-pre-wrap" data-testid={`value-${i}`}>
                {shown[i]}
              </code>
            ) : (
              <span className="text-sm text-muted-foreground" aria-hidden>
                ••••••••
              </span>
            )}
            <div className="flex gap-2">
              {shown[i] !== undefined ? (
                <Button size="sm" variant="outline" onClick={() => hide(i)}>
                  Hide
                </Button>
              ) : (
                <Button size="sm" variant="outline" onClick={() => void reveal(i)} aria-label={`Show ${v.label}`}>
                  Show
                </Button>
              )}
              <Button size="sm" variant="outline" onClick={() => void copy(i)} aria-label={`Copy ${v.label}`}>
                Copy
              </Button>
            </div>
          </li>
        ))}
      </ul>
      {view.file && (
        <div className="mt-4 flex items-center justify-between gap-2 rounded-md border p-3 text-sm">
          <span className="truncate">
            {view.file.name} · {Math.max(1, Math.round(view.file.size / 1024))} KB
          </span>
          <Button size="sm" onClick={() => void save()}>
            Save the file
          </Button>
        </div>
      )}
    </Frame>
  )
}

function RequestPage({ link, rpc }: { link: string; rpc: Link }) {
  const [label, setLabel] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [sent, setSent] = useState(false)

  useEffect(() => {
    rpc
      .call<string>("request_label", { link })
      .then(setLabel)
      .catch((e) => setError(describeError(e)))
  }, [rpc, link])

  const answer = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const data = new FormData(form)
    const title = String(data.get("title") ?? "").trim()
    const secret = String(data.get("secret") ?? "")
    setBusy(true)
    setError(null)
    try {
      await rpc.call("request_answer", { link, title, secret: encodeSecret(secret) })
      form.reset()
      setSent(true)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  if (sent) {
    return (
      <Frame>
        <h1 className="mb-2 text-lg font-semibold tracking-tight">Sent</h1>
        <p className="text-sm text-muted-foreground">It was locked in this tab to the key of the person who asked. Only they can open it, and the link takes no more answers.</p>
      </Frame>
    )
  }
  return (
    <Frame>
      <h1 className="mb-2 text-lg font-semibold tracking-tight">Someone asked you for a secret</h1>
      {label !== null && (
        <p className="mb-4 text-sm text-muted-foreground">
          They asked for: <span className="font-medium text-foreground">{label}</span>
        </p>
      )}
      {label === null && !error && <Spinner />}
      {error && (
        <FieldError role="alert" className="mb-4">
          {error}
        </FieldError>
      )}
      {label !== null && (
        <form onSubmit={answer} className="flex flex-col gap-4" aria-label="Answer the request">
          <p className="text-sm text-muted-foreground">What you send is locked to their key here, so the server can't read it.</p>
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="answer-title">What it is</FieldLabel>
              <Input id="answer-title" name="title" autoComplete="off" placeholder="Optional" />
            </Field>
            <Field>
              <FieldLabel htmlFor="answer-secret">The secret</FieldLabel>
              <Input id="answer-secret" name="secret" type="password" required autoComplete="off" />
            </Field>
          </FieldGroup>
          <Button type="submit" disabled={busy}>
            {busy ? <Spinner /> : null}
            Send
          </Button>
        </form>
      )}
    </Frame>
  )
}
