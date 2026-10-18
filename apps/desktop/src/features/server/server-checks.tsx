import { useCallback, useEffect, useState, type FormEvent } from "react"

import { Button, Field, FieldDescription, FieldLabel, Icon, Textarea } from "@unicron/ui"

import { describeError } from "@/core/client"

import { useServerClient, type MirrorCheck, type MirrorsView, type RestoreDrillView, type ServerClient } from "./server-client"
import { mirrorLine, splits } from "./server-words"

/**
 * The warning a split view earns (docs/plan/27-web-server-finish.md §4):
 * a mirror holds a head this server signed that its log can't be connected
 * to, so the server shows different people different histories. Plain
 * words and what to do; nothing in the vault was changed by it.
 */
export function SplitWarning({ checks }: { checks: readonly MirrorCheck[] | undefined | null }) {
  const found = splits(checks)
  if (found.length === 0) return null
  return (
    <div role="alert" className="flex flex-col gap-1 rounded-md border border-destructive p-3 text-sm text-destructive">
      <p className="flex items-center gap-2 font-medium">
        <Icon name="warning" />
        This server's public log doesn't match {found.length === 1 ? "a mirror" : `${found.length} mirrors`}.
      </p>
      <p>
        The server signed one history for you and another for the world. Your vault is unchanged, but don't share keys with anyone through this
        server until it's explained: tell whoever runs it, and check your devices and the people you share with.
      </p>
      <ul className="list-disc pl-5">
        {found.map((c) => (
          <li key={c.url}>
            <span className="font-mono">{c.url}</span>: {mirrorLine(c)}
          </li>
        ))}
      </ul>
    </div>
  )
}

interface ServerChecksProps {
  onNotice: (text: string) => void
  /** Drawn as a card (the desktop's Settings) or plain (the web vault's). */
  framed?: boolean
}

/**
 * Settings → This server: the last successful restore drill, read from the
 * server (never made up here), and the key-transparency log mirrors each
 * sync compares the server's log against, with the ones you add.
 */
export function ServerChecks({ onNotice, framed = true }: ServerChecksProps) {
  const server = useServerClient()
  return server ? <Checks server={server} onNotice={onNotice} framed={framed} /> : null
}

function Checks({ server, onNotice, framed }: ServerChecksProps & { server: ServerClient }) {
  const [drill, setDrill] = useState<RestoreDrillView | null>(null)
  const [drillError, setDrillError] = useState<string | null>(null)
  const [mirrors, setMirrors] = useState<MirrorsView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(() => {
    server
      .restoreDrill()
      .then((d) => {
        setDrill(d)
        setDrillError(null)
      })
      .catch((e) => setDrillError(describeError(e)))
    server
      .ktMirrors()
      .then(setMirrors)
      .catch((e) => setError(describeError(e)))
  }, [server])
  useEffect(load, [load])

  const save = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const text = String(new FormData(e.currentTarget).get("mirrors") ?? "")
    const urls = text
      .split(/\s+/)
      .map((u) => u.trim())
      .filter(Boolean)
    setBusy(true)
    setError(null)
    try {
      setMirrors(await server.setKtMirrors(urls))
      onNotice(urls.length ? "Mirrors saved. The next sync compares against them." : "Your mirrors were removed.")
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section aria-labelledby="server-checks-title" className={framed ? "flex flex-col gap-3 rounded-xl border bg-card p-4" : "flex flex-col gap-3"}>
      <h2 id="server-checks-title" className="font-medium">
        This server
      </h2>
      <div className="flex flex-col gap-1 text-sm">
        <p className="font-medium">Restore drills</p>
        {drill ? (
          <p role="status" className={drill.last_passed === false ? "text-destructive" : drill.last_success_at ? "" : "text-muted-foreground"}>
            {drill.words}
          </p>
        ) : drillError ? (
          <p className="text-muted-foreground">{drillError}</p>
        ) : (
          <p className="text-muted-foreground">Asking the server…</p>
        )}
        <p className="text-muted-foreground">As the server reports it. A restore drill proves its backups would bring it back; your vault never depends on it.</p>
      </div>

      <div className="flex flex-col gap-2 text-sm">
        <p className="font-medium">Public log mirrors</p>
        <p className="text-muted-foreground">
          Each sync compares this server's public log with copies kept elsewhere. A server that showed you a different history from everyone else is
          caught this way.
        </p>
        <SplitWarning checks={mirrors?.checks} />
        {mirrors && mirrors.checks.length > 0 ? (
          <ul className="flex flex-col gap-1">
            {mirrors.checks.map((c) => (
              <li key={c.url} className={c.state === "split" ? "text-destructive" : c.state === "agrees" ? "" : "text-attention-text"}>
                <span className="font-mono">{c.url}</span>
                <span className="text-muted-foreground"> · {c.named_by === "server" ? "named by the server" : "added by you"}</span>
                <br />
                {mirrorLine(c)}
              </li>
            ))}
          </ul>
        ) : mirrors ? (
          <p className="text-muted-foreground">No mirror has been compared yet. The server names none, and you've added none.</p>
        ) : null}
        <form onSubmit={(e) => void save(e)} className="flex flex-col gap-2">
          <Field>
            <FieldLabel htmlFor="kt-mirrors">Your own mirrors</FieldLabel>
            <Textarea
              id="kt-mirrors"
              name="mirrors"
              rows={2}
              placeholder="https://mirror.example"
              defaultValue={(mirrors?.configured ?? []).join("\n")}
              key={(mirrors?.configured ?? []).join("|")}
              autoComplete="off"
              spellCheck={false}
            />
            <FieldDescription>One address per line, up to eight. Mirrors are public: nothing of yours goes to them.</FieldDescription>
          </Field>
          {error && (
            <p role="alert" className="text-destructive">
              {error}
            </p>
          )}
          <div>
            <Button type="submit" size="sm" variant="outline" disabled={busy || !mirrors}>
              Save mirrors
            </Button>
          </div>
        </form>
      </div>
    </section>
  )
}
