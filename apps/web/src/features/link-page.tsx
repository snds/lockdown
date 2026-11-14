import { useEffect, useMemo, useRef, useState, type FormEvent } from "react"

import { Banner, Button, EmptyState, FieldRow, FieldRowValue, FileChip, Icon, Skeleton, TextField } from "@unicron/ui"
import { SecretField, type SecretFieldReferenceHost } from "@unicron/ui/security/secret-field"

import { describeError } from "@/core/client"
import { encodeSecret } from "@/core/secret-bytes"
import { ClosedHeading } from "@/features/unlock/closed-frame"
import { count, fullDate } from "@/lib/format"

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
 * (docs/plan/18-sharing.md §7, D198; brief W2). Everything happens in this
 * tab's worker; the server sees the link's id, never its key. The focus
 * frame, with no way into the vault.
 */
export function LinkPage({ page, link, clipboard }: LinkPageProps) {
  return page.kind === "share" ? <SharePage link={page.link} rpc={link} clipboard={clipboard} /> : <RequestPage link={page.link} rpc={link} />
}

/** A link that is gone, used up or expired, or a lookup that failed: the server's words and Try again (cross-app decisions §9). */
function GoneLink({ overline, words, onTryAgain }: { overline: string; words: string; onTryAgain: () => void }) {
  return (
    <Frame>
      <EmptyState tone="critical" role="alert" overline={overline} title={<h1>This link won't open.</h1>} body={words} actionLabel="Try again" onAction={onTryAgain} />
    </Frame>
  )
}

function SharePage({ link, rpc, clipboard }: { link: string; rpc: Link; clipboard: WebClipboard }) {
  const [meta, setMeta] = useState<LinkMeta | null>(null)
  const [lookup, setLookup] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  const [opened, setOpened] = useState<OpenedView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [password, setPassword] = useState("")
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let live = true
    rpc
      .call<LinkMeta>("link_meta", { link })
      .then((m) => live && setMeta(m))
      .catch((e) => live && setLookup(describeError(e)))
    return () => {
      live = false
    }
  }, [rpc, link, attempt])

  const open = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const typed = password
    setPassword("")
    setBusy(true)
    setError(null)
    try {
      setOpened(await rpc.call<OpenedView>("link_open", { link, password: typed ? encodeSecret(typed) : null }))
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  if (opened) return <OpenedLink view={opened} rpc={rpc} clipboard={clipboard} />
  if (lookup)
    return (
      <GoneLink
        overline="Shared link"
        words={lookup}
        onTryAgain={() => {
          setLookup(null)
          setAttempt((n) => n + 1)
        }}
      />
    )
  return (
    <Frame>
      <ClosedHeading overline="Shared link" title="Someone shared something with you.">
        It was locked on their device. It opens here, in this tab.
      </ClosedHeading>
      {meta === null ? (
        <Skeleton variant="row" aria-hidden />
      ) : (
        <form onSubmit={open} className="flex flex-col gap-6" aria-label="Open the link">
          {/* The link's facts, as mono lines: the views it has left, and the last day it opens. */}
          <div className="flex flex-col gap-1 type-overline text-text-label">
            {meta.views_left !== null && <p>{`${count(meta.views_left)} ${meta.views_left === 1 ? "view" : "views"} left`}</p>}
            <p>{`Opens until ${fullDate(meta.expires_at)}`}</p>
          </div>
          {meta.password && (
            <SecretField
              mode="edit"
              label="The link's password"
              name="password"
              mono={false}
              required
              value={password}
              onChange={setPassword}
              autoComplete="off"
              description="The sender gave it to you separately."
              error={error ?? undefined}
            />
          )}
          {error && !meta.password && (
            <p role="alert" className="type-caption text-critical-text">
              {error}
            </p>
          )}
          <div>
            <Button type="submit" loading={busy}>
              Open link
            </Button>
          </div>
        </form>
      )}
    </Frame>
  )
}

/** One value of an opened link: shown and copied only through the SecretField, its bytes fetched when asked for. */
function LinkSecret({ index, label, rpc, clipboard }: { index: number; label: string; rpc: Link; clipboard: WebClipboard }) {
  const lastCopy = useRef<number | null>(null)
  const host = useMemo<SecretFieldReferenceHost>(
    () => ({
      copyRef: async () => {
        const { copy } = await clipboard.copy(await rpc.callBytes("link_value", { index }))
        lastCopy.current = copy
        return { hash: String(copy) }
      },
      clear: async () => {
        if (lastCopy.current !== null) await clipboard.clear(lastCopy.current)
      },
      clipboardHolds: (hash) => Promise.resolve(clipboard.holds(Number(hash))),
    }),
    [rpc, clipboard, index],
  )
  return <SecretField label={label} getSecret={() => rpc.callBytes("link_value", { index })} host={host} revealMode="toggle" mono errorText={(e) => describeError(e)} />
}

/** A value the sender didn't mark secret: read once and shown as it is. */
function LinkText({ index, label, rpc }: { index: number; label: string; rpc: Link }) {
  const [text, setText] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let live = true
    rpc
      .callBytes("link_value", { index })
      .then((bytes) => {
        const t = new TextDecoder().decode(bytes)
        bytes.fill(0)
        if (live) setText(t)
      })
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [rpc, index])
  return (
    <FieldRow label={label} rule="none">
      {error ? (
        <span role="alert" className="type-caption text-critical-text">
          {error}
        </span>
      ) : text === null ? (
        <Skeleton variant="row" aria-hidden />
      ) : (
        <FieldRowValue truncate="wrap">{text}</FieldRowValue>
      )}
    </FieldRow>
  )
}

/** A file's size in words. */
const sizeWords = (bytes: number) => (bytes < 1024 * 1024 ? `${Math.max(1, Math.round(bytes / 1024))} KB` : `${(bytes / (1024 * 1024)).toFixed(1)} MB`)

function OpenedLink({ view, rpc, clipboard }: { view: OpenedView; rpc: Link; clipboard: WebClipboard }) {
  const [saving, setSaving] = useState(false)
  const [fileError, setFileError] = useState<string | null>(null)
  const save = async () => {
    if (!view.file) return
    setSaving(true)
    setFileError(null)
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
      setFileError(describeError(e))
    } finally {
      setSaving(false)
    }
  }
  return (
    <Frame>
      <ClosedHeading overline="Shared link // opened" title={view.title || "Shared with you"} />
      <Banner tone="info" size="compact" title="Keep what you need now. The link may not open again." body="Nothing here is saved in this browser." />
      <ul className="flex flex-col gap-4" aria-label="What the link holds">
        {view.values.map((v, i) => (
          <li key={`${i}:${v.label}`} data-testid={`value-${i}`}>
            {v.secret ? <LinkSecret index={i} label={v.label} rpc={rpc} clipboard={clipboard} /> : <LinkText index={i} label={v.label} rpc={rpc} />}
          </li>
        ))}
        {view.file && (
          <li>
            <FileChip
              name={view.file.name || "shared-file"}
              size={sizeWords(view.file.size)}
              busy={saving}
              critical={fileError ?? undefined}
              action={
                <Button variant="ghost" size="icon-sm" aria-label={`Save ${view.file.name || "the file"}`} onClick={() => void save()} disabled={saving}>
                  <Icon name="download" />
                </Button>
              }
            />
          </li>
        )}
      </ul>
    </Frame>
  )
}

function RequestPage({ link, rpc }: { link: string; rpc: Link }) {
  const [label, setLabel] = useState<string | null>(null)
  const [lookup, setLookup] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)
  const [error, setError] = useState<string | null>(null)
  const [secret, setSecret] = useState("")
  const [busy, setBusy] = useState(false)
  const [sent, setSent] = useState(false)

  useEffect(() => {
    let live = true
    rpc
      .call<string>("request_label", { link })
      .then((l) => live && setLabel(l))
      .catch((e) => live && setLookup(describeError(e)))
    return () => {
      live = false
    }
  }, [rpc, link, attempt])

  const answer = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const title = String(new FormData(form).get("title") ?? "").trim()
    const typed = secret
    setBusy(true)
    setError(null)
    try {
      await rpc.call("request_answer", { link, title, secret: encodeSecret(typed) })
      setSecret("")
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
        <ClosedHeading overline="Request link // sent" title="Sent." focusTitle>
          Only the person who asked can open it. The link takes no more answers.
        </ClosedHeading>
      </Frame>
    )
  }
  if (lookup)
    return (
      <GoneLink
        overline="Request link"
        words={lookup}
        onTryAgain={() => {
          setLookup(null)
          setAttempt((n) => n + 1)
        }}
      />
    )
  return (
    <Frame>
      <ClosedHeading overline="Request link" title="Someone asked you for a secret.">
        {label !== null ? (
          <>
            They asked for: <span className="text-text-body">{label}</span>
          </>
        ) : null}
      </ClosedHeading>
      {label === null ? (
        <Skeleton variant="row" aria-hidden />
      ) : (
        <form onSubmit={answer} className="flex flex-col gap-6" aria-label="Answer the request">
          <div className="flex flex-col gap-4">
            <TextField label="What it is (optional)" name="title" autoComplete="off" />
            <SecretField
              mode="edit"
              label="The secret"
              name="secret"
              mono={false}
              required
              value={secret}
              onChange={setSecret}
              autoComplete="off"
              description="What you send is locked to their key here."
            />
          </div>
          {error && <Banner tone="critical" size="compact" title={error} />}
          <div>
            <Button type="submit" loading={busy}>
              Send
            </Button>
          </div>
        </form>
      )}
    </Frame>
  )
}
