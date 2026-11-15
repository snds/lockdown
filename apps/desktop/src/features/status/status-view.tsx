import { useState, type ReactNode } from "react"

import { Badge, Banner, Button, FieldRow, FieldRowAction, FieldRowValue, MetaItem, MetaList, SectionHeader } from "@unicron/ui"

import { BannerSlot, useBanner } from "@/app/banners"
import { usePlaces } from "@/app/places"
import { describeError, isSignInNeeded, useClient } from "@/core/client"
import { decodeAndWipe } from "@/core/secret-bytes"
import type { OncePage as OncePageData, Status, TransparencyView } from "@/core/types"
import { AccountDialog } from "@/features/account/account-dialog"
import { SplitWarning } from "@/features/server/server-checks"
import { mirrorLine, splits } from "@/features/server/server-words"
import { SaveError, useGroupLevel, useRowSize } from "@/features/settings/setting-rows"
import { hostOf } from "@/features/unlock/first-run-client"
import { count, fullDate, rowDate } from "@/lib/format"

import { OncePage, type ShownPage } from "./once-page"

interface StatusViewProps {
  status: Status
  onStatus: (status: Status) => void
  onNotice: (text: string) => void
}

/** "until 2026.11.14 02:00": when this device's session runs out. */
export function sessionUntil(secs: number): string {
  const d = new Date(secs * 1000)
  const two = (n: number) => String(n).padStart(2, "0")
  return `Until ${fullDate(secs)} ${two(d.getHours())}:${two(d.getMinutes())}`
}

/** PUBLIC LOG's value: when this device last verified the server's log (D78, D79). */
export function logValue(view: TransparencyView | null): string {
  if (!view) return "Not checked yet. The next sync checks it."
  return view.checked_at ? `Checked ${rowDate(view.checked_at)}` : "Not checked yet. The next sync checks it."
}

/**
 * Settings → Status (brief T4, doc 82): proves "locked, even from us"
 * (D95) on demand, says whether sync is healthy, and keeps support's facts
 * one step away. Top to bottom:
 *
 * - The one critical Banner Status shows: this device must sign in again
 *   (SIGN IN, the shell's Banner), was revoked (no sign-in), or the split
 *   view in D328's whole sentence.
 * - LOCKED, EVEN FROM US: one Field Row per proof with a VERIFIED or NOT
 *   YET Badge, and CHECK NOW (secondary) in its header. A pass updates the
 *   rows without a message (its lines are under Details); a failure is a
 *   critical Banner under the header in the core's words.
 * - ACCOUNT AND SYNC: the host, the login, the last sync and the session,
 *   SYNC NOW as a text action. No account: one sentence, REGISTER and SIGN
 *   IN. A local account: no server words, one line and SET UP SYNC.
 * - THIS DEVICE: its name, the persona (a fact, moved from the sidebar),
 *   the devices and the items.
 * - DETAILS, shut: the ids, folders, hashes and log numbers (R14), and the
 *   records held back until they verify (D574).
 *
 * No solid primary in the content (R19).
 */
export function StatusView({ status, onStatus, onNotice }: StatusViewProps) {
  const client = useClient()
  const { go } = usePlaces()
  const { critical } = useBanner()
  const size = useRowSize()
  const level = useGroupLevel()
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [dialog, setDialog] = useState<"register" | "login" | null>(null)
  /** Why the server wouldn't renew this device's session, when a sync just said so. */
  const [signIn, setSignIn] = useState<string | null>(null)
  const [once, setOnce] = useState<ShownPage | null>(null)
  /** The core's words when the last sync stopped at the server's log; cleared by a sync that passed. */
  const [syncRefusal, setSyncRefusal] = useState<string | null>(null)
  /** What the last CHECK NOW said: a pass's lines go under Details, a failure's words in a Banner. */
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
        if (isSignInNeeded(e) && e.detail !== "revoked") {
          // Only the password helps: ask for it, saying why.
          setSignIn(e.message)
          setDialog("login")
        } else if (!isSignInNeeded(e)) setSyncRefusal(describeError(e))
        // A revoked device: the password doesn't help either; the Banner says so (docs/plan/35-revoke-split.md §1).
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

  const signInNow = (why: string | null) => {
    setSignIn(why)
    setDialog("login")
  }

  // The local account the app keeps itself has no server words on screen
  // (docs/plan/39-first-run.md §1.8): its proofs are this computer's.
  const local = status.account?.local === true
  const account = local ? null : status.account
  const view = account?.transparency ?? null
  const mirrors = view?.mirrors ?? []
  const split = splits(mirrors).length > 0
  const revoked = account?.sign_in?.reason === "revoked"
  const agree = mirrors.filter((c) => c.state === "agrees").length

  // One critical Banner (cross-app §1, channel 4). A revoked device: the
  // core's sentence and no sign-in. A split the mirrors caught: D328's
  // whole sentence here, in place of the shell's short one. Otherwise the
  // shell's Banner for Status, or, where no shell holds one (a test, an
  // embed), this screen's own sign-in Banner.
  let top: ReactNode = null
  if (revoked) top = <Banner tone="critical" title="This device can't sync" body={account?.sign_in?.message} />
  else if (split && (critical === null || critical.kind === "split")) top = <SplitWarning checks={mirrors} />
  else if (critical !== null) top = <BannerSlot place="status" />
  else if (account?.sign_in)
    top = (
      <Banner
        tone="critical"
        title="Lockdown isn't syncing"
        body={account.sign_in.message}
        action={
          <Button variant="secondary" size="sm" onClick={() => signInNow(account.sign_in?.message ?? null)}>
            Sign in
          </Button>
        }
      />
    )

  const kit = status.recovery !== "none"
  return (
    <div className="flex flex-col gap-8">
      {top}
      <SaveError text={error} />

      <section aria-label="Locked, even from us" className="flex flex-col">
        <SectionHeader
          level={level}
          title="Locked, even from us"
          action={
            account ? (
              <Button variant="secondary" size="sm" onClick={() => void checkNow()} loading={busy === "audit"} disabled={busy !== null && busy !== "audit"}>
                Check now
              </Button>
            ) : undefined
          }
        />
        {audit && !audit.ok && (
          <Banner
            tone="critical"
            size="compact"
            title="The check failed"
            body={`${audit.lines.join(" ")} Nothing was changed. Try again when the server is reachable. If the words say the log was rewritten, stop syncing and keep this device's copy.`}
          />
        )}
        <Proof
          label="Master password"
          ok={account !== null || local}
          value={account !== null || local ? "Never leaves this computer." : "No account yet, so it is nowhere else."}
        />
        <Proof label="Secret Key" ok={kit} value={!kit ? "No recovery kit yet." : local ? "Made on this computer." : "Made here. The server has never seen it."} />
        <Proof label="Vault history" ok value="Signed by your devices" />
        {account && <Proof label="Public log" ok={!!view?.checked_at} value={logValue(view)} />}
        {account && mirrors.length > 0 && !split && (
          <Proof label="Log mirrors" ok={agree === mirrors.length} value={agree === mirrors.length ? "Other servers agree" : `${count(agree)} of ${count(mirrors.length)} agree`} />
        )}
      </section>

      {local ? (
        <section aria-label="Sync" className="flex flex-col">
          <SectionHeader level={level} title="Sync" />
          <FieldRow
            size={size}
            label="Your vault"
            action={<FieldRowAction onClick={() => go("settings-server")}>Set up sync</FieldRowAction>}
          >
            <FieldRowValue truncate="wrap">Your vault is kept on this computer only.</FieldRowValue>
          </FieldRow>
        </section>
      ) : (
        <section aria-label="Account and sync" className="flex flex-col">
          <SectionHeader
            level={level}
            title="Account and sync"
            action={
              account ? (
                <Button variant="ghost" size="sm" onClick={() => void syncNow()} loading={busy === "sync"} disabled={busy !== null && busy !== "sync"}>
                  Sync now
                </Button>
              ) : undefined
            }
          />
          {account ? (
            <>
              <FieldRow size={size} label="Server">
                {hostOf(account.server)}
              </FieldRow>
              <FieldRow size={size} label="Login">
                {account.login}
              </FieldRow>
              <FieldRow size={size} label="Last sync">
                {account.last_sync_at ? rowDate(account.last_sync_at) : "Not yet"}
              </FieldRow>
              <FieldRow
                size={size}
                label="Session"
                action={!revoked && !account.sign_in ? <FieldRowAction onClick={() => signInNow(null)}>Sign in</FieldRowAction> : undefined}
              >
                {account.sign_in ? "Ended" : sessionUntil(account.session_expires_at)}
              </FieldRow>
              <SaveError text={syncRefusal ? `Sync refused: ${syncRefusal}` : null} />
            </>
          ) : (
            <div className="flex flex-col gap-4 pt-3">
              <p className="type-body text-text-secondary">No account on this device. Register one on a server, yours or ours, to sync, or sign in to one you have.</p>
              <div className="flex flex-wrap gap-3">
                <Button variant="secondary" size="sm" onClick={() => setDialog("register")}>
                  Register
                </Button>
                <Button variant="secondary" size="sm" onClick={() => signInNow(null)}>
                  Sign in
                </Button>
              </div>
            </div>
          )}
        </section>
      )}

      <section aria-label="This device" className="flex flex-col">
        <SectionHeader level={level} title="This device" />
        <FieldRow size={size} label="Name">
          {status.device_name}
        </FieldRow>
        <FieldRow size={size} label="Persona">
          {status.persona_label}
        </FieldRow>
        <FieldRow size={size} label="Devices">
          {status.devices_revoked ? `${count(status.devices)} // ${count(status.devices_revoked)} revoked` : count(status.devices)}
        </FieldRow>
        <FieldRow size={size} label="Items">
          {count(status.live_items)}
        </FieldRow>
      </section>

      <Details status={status} local={local} audit={audit?.ok ? audit.lines : null} level={level} />

      <AccountDialog
        kind={dialog}
        reason={dialog === "login" ? signIn : null}
        hasKit={kit}
        onClose={() => {
          setDialog(null)
          setSignIn(null)
        }}
        onDone={(lines, page) => {
          setDialog(null)
          setSignIn(null)
          if (page && page.page.length) showOnce(page)
          void run("account", async () => lines)
        }}
      />
      <OncePage page={once} onClose={() => setOnce(null)} />
    </div>
  )
}

/** One proof: what it is, its fact in a sentence, and VERIFIED or NOT YET (the word carries it). */
function Proof({ label, ok, value }: { label: string; ok: boolean; value: string }) {
  const size = useRowSize()
  return (
    <FieldRow size={size} label={label} badge={<Badge tone={ok ? "success" : "neutral"}>{ok ? "Verified" : "Not yet"}</Badge>}>
      <FieldRowValue truncate="wrap">{value}</FieldRowValue>
    </FieldRow>
  )
}

/**
 * DETAILS (brief T4, R14): what support asks for, shut until opened. The
 * ids, folders and hashes; the log and the mirrors' words (not for a local
 * account: no server words); the last CHECK NOW's lines; then the records
 * held back until they verify, in the check's own words (D574).
 */
function Details({ status, local, audit, level }: { status: Status; local: boolean; audit: string[] | null; level: 2 | 3 }) {
  const account = local ? null : status.account
  const view = account?.transparency ?? null
  const held = status.held_back ?? []
  return (
    <SectionHeader level={level} title="Details" summary={local ? "Ids and folders, for support" : "Ids, folders and the log, for support"} collapsible defaultOpen={false}>
      <div className="flex flex-col gap-6 pt-3">
        <MetaList>
          <MetaItem label="Identity folder">
            <span className="break-all">{status.root}</span>
          </MetaItem>
          <MetaItem label="Data folder">
            <span className="break-all">{status.data}</span>
          </MetaItem>
          <MetaItem label="Profile">
            <span className="break-all">{`${status.profile_id} // managed by ${status.manager ?? "no app"}`}</span>
          </MetaItem>
          <MetaItem label="Device id">
            <span className="break-all">{status.device_id}</span>
          </MetaItem>
          <MetaItem label="Vault id">
            <span className="break-all">{status.vault_id}</span>
          </MetaItem>
          <MetaItem label="Vault head">
            <span className="break-all">{`${status.head_seq} // ${status.head_hash}`}</span>
          </MetaItem>
          <MetaItem label="Identity records">{`Manifest ${status.identity_seq}`}</MetaItem>
          {account && <MetaItem label="Synced">{`Seq ${account.synced_seq} // revision ${account.synced_rev}`}</MetaItem>}
          {account && (
            <MetaItem label="Public log">
              <span className="break-all">{view ? `${view.log_size.toLocaleString("en-US")} entries // root ${view.root_prefix} // ${view.audited.toLocaleString("en-US")} of yours verified` : "Not checked yet"}</span>
            </MetaItem>
          )}
        </MetaList>
        {account && (view?.mirrors ?? []).length > 0 && (
          <ul role="list" aria-label="Log mirrors" className="flex flex-col gap-2">
            {(view?.mirrors ?? []).map((c) => (
              <li key={c.url} className="type-caption text-text-secondary break-words">
                {`${c.url}: ${mirrorLine(c)}`}
              </li>
            ))}
          </ul>
        )}
        {audit && (
          <ul role="list" aria-label="Last check" className="flex flex-col gap-2">
            {audit.map((l) => (
              <li key={l} className="type-caption text-text-secondary break-words">
                {l}
              </li>
            ))}
          </ul>
        )}
        {held.length > 0 && (
          // For support: which record was held back and which check it failed. The person sees one calm line on the vault (docs/plan/49-shared-vault-quiet.md).
          <div className="flex flex-col">
            <SectionHeader level={level === 2 ? 3 : 4} title="Held back until they verify" count={held.length} />
            <ul role="list" aria-label="Held back until they verify" className="flex flex-col gap-2 pt-3">
              {held.map((l) => (
                <li key={l} className="type-caption text-text-secondary break-words">
                  {l}
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </SectionHeader>
  )
}
