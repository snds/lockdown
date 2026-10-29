import { useState, type ReactNode } from "react"

import { Badge, Button, Icon, Spinner } from "@unicron/ui"

import { PaneHeader } from "@/components/master-detail"
import { describeError, useClient } from "@/core/client"
import { decodeAndWipe } from "@/core/secret-bytes"
import type { OncePage as OncePageData, Status, TransparencyView } from "@/core/types"
import { AccountDialog } from "@/features/account/account-dialog"
import { SplitWarning } from "@/features/server/server-checks"
import { splits } from "@/features/server/server-words"
import { shortId, whenLocal } from "@/lib/format"

import { OncePage, type ShownPage } from "./once-page"

interface StatusViewProps {
  status: Status
  onStatus: (status: Status) => void
  onNotice: (text: string) => void
}

/**
 * The Status screen (brief §3.2; D95): the proofs of "locked, even from
 * us", one line each, with **Check now** that re-runs the audit of the
 * server's public log and shows its own words; the account and sync
 * health; this device's facts. The proofs are not repeated anywhere else
 * in the app after account creation.
 */
export function StatusView({ status, onStatus, onNotice }: StatusViewProps) {
  const client = useClient()
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [dialog, setDialog] = useState<"register" | "login" | null>(null)
  const [once, setOnce] = useState<ShownPage | null>(null)
  /** The core's words when the last sync stopped at the server's log; cleared by a sync that passed. */
  const [syncRefusal, setSyncRefusal] = useState<string | null>(null)
  /** What the last "Check now" said. */
  const [audit, setAudit] = useState<{ ok: boolean; lines: string[] } | null>(null)
  const showOnce = (page: OncePageData) => setOnce({ title: "Your recovery kit", lines: page.lines, text: page.page.length ? decodeAndWipe(page.page) : null })

  const run = async (name: string, action: () => Promise<string[] | void>) => {
    setBusy(name)
    setError(null)
    try {
      const lines = await action()
      onStatus(await client.status())
      if (lines && lines.length) onNotice(lines.join(" "))
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(null)
    }
  }

  const syncNow = () =>
    run("sync", async () => {
      try {
        const outcome = await client.sync()
        setSyncRefusal(null)
        return outcome.lines
      } catch (e) {
        setSyncRefusal(describeError(e))
        return
      }
    })

  const checkNow = () =>
    run("audit", async () => {
      try {
        setAudit({ ok: true, lines: await client.audit() })
      } catch (e) {
        setAudit({ ok: false, lines: [describeError(e)] })
      }
    })

  const account = status.account
  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-6 p-4 md:p-6">
      <PaneHeader title="Status" />
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}

      <Section
        title="Locked, even from us"
        actions={
          <Button size="sm" onClick={checkNow} disabled={busy !== null || !account}>
            {busy === "audit" ? <Spinner /> : <Icon name="verified_user" data-icon="inline-start" />}
            Check now
          </Button>
        }
      >
        <Proof ok={account !== null} text={account ? "Your password never leaves this device: the server holds a record it cannot turn back into it (OPAQUE)." : "No account yet, so no password exists anywhere."} />
        <Proof ok={status.recovery !== "none"} text={status.recovery !== "none" ? "The Secret Key on your kit was made here; the server has never seen it." : "No recovery kit yet; make one on the Recovery screen."} />
        <Proof ok text={`Your vault history is signed by your devices: head ${status.head_seq}, ${status.head_hash.slice(0, 16)}…`} />
        <Proof ok={!!account?.transparency} text={account ? `The server's public log: ${transparencyLine(account.transparency)}` : "The server's public log is checked once an account exists."} />
        {account?.transparency && <MirrorProof checks={account.transparency.mirrors} />}
        <SplitWarning checks={account?.transparency?.mirrors} />
        {audit && (
          <div role="status" className={`flex flex-col gap-1 rounded-md border p-3 text-sm ${audit.ok ? "" : "text-attention-text"}`}>
            <p className="font-medium">{audit.ok ? "Checked just now" : "The check failed"}</p>
            {audit.lines.map((l) => (
              <p key={l} className="font-mono text-xs break-all">
                {l}
              </p>
            ))}
            {!audit.ok && <p>Nothing was changed. Try again when the server is reachable; if the words say the log was rewritten, stop syncing and keep this device's copy.</p>}
          </div>
        )}
      </Section>

      <Section
        title="Account and sync"
        actions={
          account ? (
            <Button size="sm" variant="outline" onClick={syncNow} disabled={busy !== null}>
              {busy === "sync" ? <Spinner /> : <Icon name="sync" data-icon="inline-start" />}
              Sync now
            </Button>
          ) : (
            <>
              <Button size="sm" variant="outline" onClick={() => setDialog("login")}>
                Log in
              </Button>
              <Button size="sm" onClick={() => setDialog("register")}>
                Register
              </Button>
            </>
          )
        }
      >
        {account ? (
          <>
            <Fact label="Server" value={account.server} mono />
            <Fact label="Login" value={account.login} />
            <Fact label="Synced" value={`seq ${account.synced_seq} · item revision ${account.synced_rev}`} />
            <Fact label="Last sync" value={whenLocal(account.last_sync_at)} />
            <Fact label="Session until" value={whenLocal(account.session_expires_at)} />
            <Fact label="Identity records" value={`manifest seq ${status.identity_seq}`} />
            <Fact label="Transparency log" value={transparencyLine(account.transparency)} tone={account.transparency ? "secure" : "attention"} />
            {syncRefusal && (
              <p role="alert" className="text-sm text-attention-text">
                Sync refused: {syncRefusal}
              </p>
            )}
            <div className="pt-1">
              <Button size="sm" variant="ghost-muted" onClick={() => setDialog("login")}>
                Log in again
              </Button>
            </div>
          </>
        ) : (
          <p className="text-sm text-muted-foreground">No account on this device. Register one on a server (yours or ours) to sync, or log in to one you have.</p>
        )}
      </Section>

      <Section title="This device">
        <Fact label="Identity root" value={status.root} mono />
        <Fact label="Data" value={status.data} mono />
        <Fact label="Profile" value={`${shortId(status.profile_id)} · managed by ${status.manager ?? "no app"}`} />
        <Fact label="Device" value={`${status.device_name} (${shortId(status.device_id)})`} />
        <Fact label="Persona" value={status.persona_label} />
        <Fact label="Vault" value={`${shortId(status.vault_id)} · ${status.live_items} live items`} />
        <Fact label="Devices" value={`${status.devices}${status.devices_revoked ? `, ${status.devices_revoked} revoked` : ""}`} />
      </Section>

      <AccountDialog
        kind={dialog}
        onClose={() => setDialog(null)}
        onDone={(lines, page) => {
          setDialog(null)
          if (page && page.page.length) showOnce(page)
          void run("account", async () => lines)
        }}
      />
      <OncePage page={once} onClose={() => setOnce(null)} />
    </div>
  )
}

/** "N entries, checked <time>": the server's public log as this device last verified it (D78, D79). */
/** The mirrors' word on the server's log, when any were compared. */
function MirrorProof({ checks }: { checks: TransparencyView["mirrors"] }) {
  const all = checks ?? []
  if (all.length === 0) return null
  const agree = all.filter((c) => c.state === "agrees").length
  if (splits(all).length > 0) return null
  return (
    <Proof
      ok={agree > 0}
      text={
        agree === all.length
          ? `${agree === 1 ? "A mirror" : `${agree} mirrors`} of the public log ${agree === 1 ? "agrees" : "agree"} it's the one everyone sees.`
          : `${agree} of ${all.length} log mirrors agree; the rest couldn't be checked (Settings → This server).`
      }
    />
  )
}

function transparencyLine(view: TransparencyView | null): string {
  if (!view) return "not checked yet; the next sync verifies it"
  return `${view.log_size} entries, checked ${whenLocal(view.checked_at)}`
}

function Proof({ ok, text }: { ok: boolean; text: string }) {
  return (
    <p className="flex items-start gap-2 text-sm">
      <Badge variant={ok ? "secondary" : "outline"}>{ok ? "verified" : "not yet"}</Badge>
      <span>{text}</span>
    </p>
  )
}

function Section({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3 rounded-xl border bg-card p-4">
      <div className="flex items-center justify-between gap-2">
        <h2 className="font-medium">{title}</h2>
        <div className="flex items-center gap-1">{actions}</div>
      </div>
      {children}
    </section>
  )
}

function Fact({ label, value, mono, tone }: { label: string; value: string; mono?: boolean; tone?: "secure" | "attention" }) {
  return (
    <div className="flex flex-col gap-0.5 text-sm sm:flex-row sm:gap-3">
      <span className="w-36 shrink-0 text-muted-foreground">{label}</span>
      <span className={[mono ? "font-mono break-all" : "", tone === "secure" ? "text-secure-text" : "", tone === "attention" ? "text-attention-text" : ""].join(" ")}>{value}</span>
    </div>
  )
}
