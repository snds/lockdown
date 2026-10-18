import { useCallback, useEffect, useRef, useState } from "react"

import {
  Button,
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  Icon,
  Item,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemTitle,
  Separator,
  Spinner,
} from "@unicron/ui"

import { PaneHeader } from "@/components/master-detail"
import { describeError, useClient } from "@/core/client"
import type { Finding, SecurityReport } from "@/core/types"
import { whenLocal } from "@/lib/format"

import { FindingRow } from "./finding-row"
import { PlaybookDialog } from "./playbook-dialog"
import { SEVERITIES, SEVERITY_TEXT, groupBySeverity, summaryWords } from "./security-model"
import { SecuritySettings } from "./security-settings"
import { usePlaybooks } from "./use-playbooks"

/** A breach check older than this runs again when the screen opens. */
const RECHECK_AFTER_SECS = 24 * 3600

interface SecurityViewProps {
  /** Opens an item in the vault. */
  onOpenItem: (id: string) => void
  /** Goes to the Devices screen (a new device's sign-in). */
  onDevices: () => void
  /** A dismissal wrote a new version of an item: reload the list. */
  onChanged: () => void
  onNotice: (text: string) => void
}

/**
 * The Security screen: what to fix, grouped by how pressing it is (Fix
 * now, Fix soon, Worth doing) with counts, each finding with its playbook
 * ("What to do"), its fix, its item, and Dismiss or snooze, which is kept
 * on the item so every device agrees. The report comes from the core in
 * words; the screen never sees a password. Opening the screen shows what
 * the core already knows at once, then checks again in the background
 * when the last breach check is a day old or the account's alerts haven't
 * been fetched this session.
 */
export function SecurityView({ onOpenItem, onDevices, onChanged, onNotice }: SecurityViewProps) {
  const client = useClient()
  const playbooks = usePlaybooks()
  const [report, setReport] = useState<SecurityReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [checking, setChecking] = useState(false)
  const [busy, setBusy] = useState(false)
  const [open, setOpen] = useState<Finding | null>(null)
  const [showDismissed, setShowDismissed] = useState(false)
  const autoChecked = useRef(false)

  const check = useCallback(async () => {
    setChecking(true)
    try {
      setReport(await client.securityCheck())
      setError(null)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setChecking(false)
    }
  }, [client])

  useEffect(() => {
    let live = true
    client
      .securityReport()
      .then((r) => {
        if (!live) return
        setReport(r)
        const now = Math.floor(Date.now() / 1000)
        const stale =
          (r.breach.enabled && (r.breach.unchecked > 0 || !r.breach.checked_at || now - r.breach.checked_at > RECHECK_AFTER_SECS)) ||
          (r.alerts.account && r.alerts.checked_at === null)
        if (stale && !autoChecked.current) {
          autoChecked.current = true
          void check()
        }
      })
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client, check])

  const act = async (run: () => Promise<SecurityReport>, notice: string) => {
    setBusy(true)
    try {
      setReport(await run())
      onNotice(notice)
      onChanged()
    } catch (e) {
      onNotice(describeError(e))
    } finally {
      setBusy(false)
    }
  }
  const dismiss = (f: Finding, until: number | null) => {
    if (f.alert !== null) {
      const seq = f.alert
      void act(() => client.securityDismissAlert(seq), "Dismissed on all your devices.")
    } else if (f.item_id) {
      const id = f.item_id
      void act(() => client.securityDismiss(id, f.key, until), until ? `Snoozed until ${whenLocal(until)}.` : "Dismissed on all your devices.")
    }
  }
  const restore = (f: Finding) => {
    if (f.item_id) {
      const id = f.item_id
      void act(() => client.securityRestore(id, f.key), "It's back on the list.")
    }
  }

  const groups = groupBySeverity(report?.findings ?? [])
  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-4 p-4 md:p-6">
      <PaneHeader title="Security">
        <Button size="sm" variant="outline" onClick={() => void check()} disabled={checking}>
          {checking ? <Spinner data-icon="inline-start" /> : <Icon name="refresh" data-icon="inline-start" />}
          {checking ? "Checking…" : "Check now"}
        </Button>
      </PaneHeader>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      {!report && !error && <Spinner />}
      {report && (
        <>
          <p className="text-base" aria-live="polite">
            {summaryWords(report)}
          </p>
          <div role="group" aria-label="Summary" className="grid grid-cols-1 gap-2 sm:grid-cols-3">
            {SEVERITIES.map(({ severity, heading, hint }) => (
              <Item key={severity} variant="outline" size="sm">
                <ItemContent>
                  <ItemTitle>
                    <span className={`text-2xl font-semibold tabular-nums ${SEVERITY_TEXT[severity]}`}>{report.counts[severity]}</span>
                    <span>{heading}</span>
                  </ItemTitle>
                  <ItemDescription>{hint}</ItemDescription>
                </ItemContent>
              </Item>
            ))}
          </div>
          <CheckFacts report={report} />
          {report.findings.length === 0 && (
            <Empty>
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <Icon name="verified_user" />
                </EmptyMedia>
                <EmptyTitle>All clear</EmptyTitle>
                <EmptyDescription>
                  None of your {report.passwords} saved passwords is weak, reused or known from a breach, and nothing else needs your attention.
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          )}
          {SEVERITIES.filter(({ severity }) => groups[severity].length > 0).map(({ severity, heading }) => (
            <section key={severity} aria-labelledby={`sev-${severity}`} className="flex flex-col gap-2">
              <h2 id={`sev-${severity}`} className={`text-base font-semibold ${SEVERITY_TEXT[severity]}`}>
                {heading} ({groups[severity].length})
              </h2>
              <ItemGroup role="list" aria-label={heading}>
                {groups[severity].map((f) => (
                  <FindingRow
                    key={f.id}
                    finding={f}
                    busy={busy}
                    onWhatToDo={setOpen}
                    onOpenItem={onOpenItem}
                    onDismiss={dismiss}
                    onRestore={restore}
                    onNotice={onNotice}
                    onDevices={onDevices}
                  />
                ))}
              </ItemGroup>
            </section>
          ))}
          {report.dismissed.length > 0 && (
            <section aria-label="Dismissed" className="flex flex-col gap-2">
              <div>
                <Button size="sm" variant="ghost" onClick={() => setShowDismissed((s) => !s)} aria-expanded={showDismissed}>
                  <Icon name={showDismissed ? "keyboard_arrow_up" : "keyboard_arrow_down"} data-icon="inline-start" />
                  {showDismissed ? "Hide" : "Show"} dismissed and snoozed ({report.dismissed.length})
                </Button>
              </div>
              {showDismissed && (
                <ItemGroup role="list" aria-label="Dismissed and snoozed">
                  {report.dismissed.map((f) => (
                    <FindingRow
                      key={f.id}
                      finding={f}
                      busy={busy}
                      onWhatToDo={setOpen}
                      onOpenItem={onOpenItem}
                      onDismiss={dismiss}
                      onRestore={restore}
                      onNotice={onNotice}
                    />
                  ))}
                </ItemGroup>
              )}
            </section>
          )}
          <Separator />
          <SecuritySettings onSaved={() => void client.securityReport().then(setReport).catch(() => {})} />
          <p className="text-xs text-muted-foreground">
            Two-step sign-in information for {report.directory.sites.toLocaleString()} sites comes from 2fa.directory ({report.directory.licence}{" "}
            licence), as of {report.directory.date}. It is built into Lockdown, so looking a site up tells no one which sites you use.
          </p>
        </>
      )}
      <PlaybookDialog
        finding={open}
        playbook={open ? (playbooks?.[open.playbook] ?? null) : null}
        onClose={() => setOpen(null)}
        onNotice={onNotice}
        onOpenItem={onOpenItem}
        onDevices={onDevices}
      />
    </div>
  )
}

/** When each check last ran, and anything that stopped one, in words. */
function CheckFacts({ report }: { report: SecurityReport }) {
  const b = report.breach
  const breach = !b.enabled
    ? "The breach check is off."
    : b.checked_at
      ? `Checked ${b.checked} of ${report.passwords} passwords against known breaches (${b.source}), last on ${whenLocal(b.checked_at)}.`
      : "Your passwords haven't been checked against known breaches yet."
  const alerts = !report.alerts.account
    ? "Sign-in alerts need an account; this device has none."
    : report.alerts.checked_at
      ? `Sign-ins to your account last checked ${whenLocal(report.alerts.checked_at)}.`
      : "Sign-ins to your account haven't been checked yet."
  return (
    <div className="flex flex-col gap-1 text-sm text-muted-foreground">
      <p>{breach}</p>
      {b.enabled && b.message && <p className="text-destructive">The last breach check didn't finish: {b.message}</p>}
      <p>{alerts}</p>
      {report.alerts.message && <p className="text-destructive">{report.alerts.message}</p>}
    </div>
  )
}
