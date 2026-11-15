import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react"

import {
  Banner,
  Button,
  EmptyState,
  FieldRow,
  FieldRowAction,
  Icon,
  ListRow,
  Metric,
  NavItem,
  SectionHeader,
  Skeleton,
  useAppFrame,
} from "@unicron/ui"

import { useCrumb } from "@/app/crumbs"
import { NeedsYouSlot, useNeedsYouBlock, useNeedsYouUnderHead } from "@/app/needs-you"
import { notify } from "@/app/notify"
import { usePlaces } from "@/app/places"
import { useShell } from "@/app/shell"
import { clockWords, useStatusItem } from "@/app/status"
import { describeError, useClient } from "@/core/client"
import type { BackupStatus, Finding, ItemSummary, SecurityReport, Status } from "@/core/types"
import { count, countName, fullDate, rowDate } from "@/lib/format"

import { AccountDetail, AccountsList, useFederation } from "@/features/bridges/federation"
import { IncidentDetail, IncidentsList, flowsUnderWay, useIncidents } from "@/features/bridges/incidents"
import { GuideAttribution } from "@/features/bridges/setup-guide"
import { useVaults } from "@/features/organization/vaults-context"
import { DuplicateDetail, DuplicatesList, duplicatesWords, groupKey, useDuplicates } from "@/features/organization/duplicates-review"
import { expiryStatus, soonestFirst } from "@/features/organization/expiry"
import { DetailPane } from "@/features/vault/detail-pane"
import { kindWord } from "@/features/vault/detail-model"
import { rowsIn } from "@/features/vault/list-model"

import { FindingDetail } from "./finding-detail"
import { FindingRow } from "./finding-row"
import { PqStatusLine } from "./pq-status-line"
import { SEVERITIES, attentionWord, groupBySeverity, hostOf, kindCounts, nextAfter, placeOf, toFixCount } from "./security-model"
import { usePlaybooks } from "./use-playbooks"

/** A breach check older than this runs again when the screen opens. */
const RECHECK_AFTER_SECS = 24 * 3600

/** Worth doing shows this many rows until SHOW ALL. */
const WORTH_DOING_SHOWN = 5

interface SecurityViewProps {
  /** The item rows, for the lists Health holds (Expiring, Needs attention, Duplicates). Left out, Health reads them itself (the web vault). */
  rows?: ItemSummary[]
  /** Kept for hosts that pass it: Health opens items in its own detail (brief H2). */
  onOpenItem?: (id: string) => void
  /** Goes to Settings → Devices, to one device when named. */
  onDevices: (deviceId?: string) => void
  /** Goes to Settings → Recovery (a guided flow's new kit). */
  onRecovery?: () => void
  /** A dismissal, a merge or a delete wrote a new version of an item: reload the list. */
  onChanged?: () => void
  onNotice?: (text: string) => void
  /** The web vault draws no password checks (brief H1, W1): no hero, no metrics, no Fix groups, no MORE HELP. */
  surface?: "desktop" | "web"
}

/** What Health has open beside its lists, or pushed on a phone (brief H2, H3, H4, V3). */
type Open =
  | { place: "fix"; finding: string }
  | { place: "item"; item: string }
  | { place: "duplicates"; group?: string }
  | { place: "accounts"; provider?: string }
  | { place: "incidents"; flow?: string }

/**
 * A pushed screen inside Health's one column (a phone, medium width): the
 * frame's top bar takes its way back, and its bottom row the screen's
 * primary, as for a pushed detail. Health is a page the shell mounts in one
 * column, so it tells the frame itself (unicron gap: a pushed screen inside
 * an `AppFrameColumn`). Beside the sidebar at the expanded width nothing is
 * pushed.
 */
function usePushedInPage(pushed: boolean, onBack: () => void) {
  const { setDetailOpen, setBack } = useAppFrame()
  const latest = useRef(onBack)
  useLayoutEffect(() => {
    latest.current = onBack
  })
  useLayoutEffect(() => {
    if (!pushed) return
    setDetailOpen(true)
    setBack({ label: "Back", onBack: () => latest.current() })
    return () => {
      setDetailOpen(false)
      setBack(null)
    }
  }, [pushed, setDetailOpen, setBack])
}

/**
 * Health (briefs H1 to H4, V3; Mobile / 09 and 10, Desktop / 07 and 08).
 *
 * The page: the count to fix as the hero (no score: the core keeps none),
 * Metrics for the kinds the core reports, the findings in Fix now, Fix soon
 * and Worth doing (D125), the Needs attention and Expiring lists, the
 * dismissed findings, and PROTECTIONS: what already keeps the person safe,
 * each row saying its state once. MORE HELP holds Accounts you sign in with
 * and Something happened?. CHECK NOW runs the checks; so does a pull on a
 * phone. The report is the core's cached one; the network checks run when
 * it is a day old, or on CHECK NOW.
 *
 * A finding, an item, Duplicates, an account or a guided flow opens beside
 * the lists at the expanded width (the crumb follows: TOOLS // HEALTH //
 * …), and as a pushed screen narrower. Health stays the current place.
 */
export function SecurityView({ rows: given, onDevices, onRecovery, onChanged, onNotice, surface = "desktop" }: SecurityViewProps) {
  const client = useClient()
  const frame = useAppFrame()
  const web = surface === "web"
  const { go } = usePlaces()
  const { edit } = useShell()
  const { vaults } = useVaults()
  const playbooks = usePlaybooks()
  const say = useCallback((text: string) => (onNotice ? onNotice(text) : notify(text)), [onNotice])
  const changed = useCallback(() => onChanged?.(), [onChanged])

  // --- what Health reads ------------------------------------------------------------------------
  const [report, setReport] = useState<SecurityReport | null>(null)
  const [readError, setReadError] = useState<string | null>(null)
  const [checkError, setCheckError] = useState<string | null>(null)
  const [checking, setChecking] = useState(false)
  const [busy, setBusy] = useState(false)
  const [checks, setChecks] = useState(0)
  const [reads, setReads] = useState(0)
  const autoChecked = useRef(false)
  const [own, setOwn] = useState<ItemSummary[] | null>(null)
  const [status, setStatus] = useState<Status | null>(null)
  const [backup, setBackup] = useState<BackupStatus | null>(null)
  const rows = given ?? own

  const check = useCallback(async () => {
    setChecking(true)
    try {
      setReport(await client.securityCheck())
      setCheckError(null)
      setReadError(null)
    } catch (e) {
      setCheckError(describeError(e))
    } finally {
      setChecking(false)
      setChecks((n) => n + 1)
    }
  }, [client])

  useEffect(() => {
    if (web) return
    let live = true
    client
      .securityReport()
      .then((r) => {
        if (!live) return
        setReport(r)
        setReadError(null)
        const now = Math.floor(Date.now() / 1000)
        const stale =
          (r.breach.enabled && (r.breach.unchecked > 0 || !r.breach.checked_at || now - r.breach.checked_at > RECHECK_AFTER_SECS)) ||
          (r.alerts.account && r.alerts.checked_at === null)
        if (stale && !autoChecked.current) {
          autoChecked.current = true
          void check()
        }
      })
      .catch((e) => live && setReadError(describeError(e)))
    return () => {
      live = false
    }
  }, [client, check, web, reads])

  const [rowsRound, setRowsRound] = useState(0)
  useEffect(() => {
    if (given) return
    let live = true
    client
      .list()
      .then((r) => live && setOwn(r))
      .catch(() => live && setOwn([]))
    return () => {
      live = false
    }
  }, [client, given, rowsRound])
  const reloadRows = useCallback(() => {
    changed()
    setRowsRound((n) => n + 1)
  }, [changed])

  useEffect(() => {
    let live = true
    client
      .status()
      .then((s) => live && setStatus(s))
      .catch(() => {})
    if (!web)
      client
        .backupStatus()
        .then((b) => live && setBackup(b))
        .catch(() => {})
    return () => {
      live = false
    }
  }, [client, web])

  // --- what is open -----------------------------------------------------------------------------
  const [open, setOpen] = useState<Open | null>(null)
  const [showAll, setShowAll] = useState(false)
  const dupes = useDuplicates(open?.place === "duplicates")
  const federation = useFederation(reloadRows, say)
  const incidents = useIncidents()
  const bridges = !web && federation.available

  const findings = report?.findings ?? []
  const dismissed = report?.dismissed ?? []
  const finding = open?.place === "fix" ? ([...findings, ...dismissed].find((f) => f.id === open.finding) ?? null) : null
  const attention = useMemo(() => (rows ? rowsIn(rows, "attention") : []), [rows])
  const expiring = useMemo(() => (rows ? soonestFirst(rowsIn(rows, "expiring")) : []), [rows])
  const itemTitle = open?.place === "item" ? (rows?.find((r) => r.id === open.item)?.title ?? "Item") : null
  const group = open?.place === "duplicates" && open.group ? (dupes.groups?.find((g) => groupKey(g) === open.group) ?? null) : null
  const provider = open?.place === "accounts" && open.provider ? (federation.map?.providers.find((p) => p.id === open.provider) ?? null) : null
  const flow = open?.place === "incidents" && open.flow ? (incidents.list?.find((i) => i.id === open.flow) ?? null) : null

  // The crumb follows what is open: TOOLS // HEALTH // {item}, // DUPLICATES, // ACCOUNTS, // SOMETHING HAPPENED.
  const crumb: string[] | null = !open
    ? null
    : open.place === "fix"
      ? [finding?.item_title ?? finding?.title ?? "Finding"]
      : open.place === "item"
        ? [itemTitle ?? "Item"]
        : open.place === "duplicates"
          ? ["Duplicates"]
          : open.place === "accounts"
            ? ["Accounts", ...(provider ? [provider.name] : [])]
            : ["Something happened", ...(flow ? [flow.title] : [])]
  useCrumb(crumb)

  // The status line's end: CHECKING, FIX 1 OF 3, or CHECKED 09:40.
  const where = finding && !finding.dismissed ? placeOf(findings, finding.id) : null
  const checkedAt = report ? (report.breach.checked_at ?? report.generated_at) : null
  useStatusItem(
    checking
      ? { text: "Checking" }
      : where
        ? { text: `Fix ${where.at} of ${where.of}` }
        : !open && checkedAt
          ? { text: `Checked ${clockWords(checkedAt * 1000)}` }
          : null,
  )

  // Narrower than the expanded width, what is open is a pushed screen; back goes one level up.
  const deeper = open !== null && ((open.place === "duplicates" && open.group) || (open.place === "accounts" && open.provider) || (open.place === "incidents" && open.flow))
  const back = useCallback(() => {
    setOpen((o) => {
      if (!o) return null
      if (o.place === "duplicates" && o.group) return { place: "duplicates" }
      if (o.place === "accounts" && o.provider) return { place: "accounts" }
      if (o.place === "incidents" && o.flow) return { place: "incidents" }
      return null
    })
  }, [])
  usePushedInPage(!frame.split && open !== null, back)
  // A pushed screen starts at its top; the page comes back where it was.
  const root = useRef<HTMLDivElement>(null)
  const kept = useRef(0)
  const screenKey = open ? JSON.stringify(open) : ""
  useLayoutEffect(() => {
    const column = root.current?.closest<HTMLElement>("[data-column]")
    if (!column) return
    if (screenKey) column.scrollTop = 0
    else column.scrollTop = kept.current
  }, [screenKey])
  const openThis = (next: Open) => {
    const column = root.current?.closest<HTMLElement>("[data-column]")
    if (!open && column) kept.current = column.scrollTop
    setOpen(next)
  }

  // --- acting on findings -----------------------------------------------------------------------
  const act = async (run: () => Promise<SecurityReport>): Promise<SecurityReport | null> => {
    setBusy(true)
    try {
      const r = await run()
      setReport(r)
      changed()
      return r
    } catch (e) {
      say(describeError(e))
      return null
    } finally {
      setBusy(false)
    }
  }
  const restore = (f: Finding) => {
    if (!f.item_id) return
    const id = f.item_id
    void act(() => client.securityRestore(id, f.key)).then((r) => r && say("It's back on the list"))
  }
  const dismiss = (f: Finding, until: number | null) => {
    // The next finding opens once this one leaves the list (brief H2).
    const next = nextAfter(findings, f.id)
    const after = () => setOpen((o) => (o?.place === "fix" && o.finding === f.id ? (next ? { place: "fix", finding: next.id } : null) : o))
    if (f.alert !== null) {
      const seq = f.alert
      void act(() => client.securityDismissAlert(seq)).then((r) => {
        if (!r) return
        notify("Dismissed on all your devices")
        after()
      })
    } else if (f.item_id) {
      const id = f.item_id
      void act(() => client.securityDismiss(id, f.key, until)).then((r) => {
        if (!r) return
        // D132's words, with UNDO: the core brings it back.
        notify(until ? `Snoozed until ${fullDate(until)}` : "Dismissed on all your devices", {
          action: { label: "Undo", onClick: () => void act(() => client.securityRestore(id, f.key)) },
        })
        after()
      })
    }
  }

  const stepAction = (a: string) => {
    if (a === "check") void check()
    else if (a === "recovery_kit") (onRecovery ?? (() => go("settings-recovery")))()
    else onDevices()
  }

  // --- the lists --------------------------------------------------------------------------------
  const groups = groupBySeverity(findings)
  const selectedFinding = open?.place === "fix" ? open.finding : null
  const selectedItem = open?.place === "item" ? open.item : null
  const openFinding = (f: Finding) => openThis({ place: "fix", finding: f.id })
  const openItem = (id: string) => openThis({ place: "item", item: id })

  const fixLists = !web && report && (
    <>
      {findings.length === 0 && !open && (
        <EmptyState
          overline="Health // 00"
          title="All clear."
          body={
            report.passwords === 1
              ? "Your saved password isn't weak, reused or known from a breach."
              : `None of your ${countName(report.passwords, "saved password")} is weak, reused or known from a breach.`
          }
        />
      )}
      {SEVERITIES.filter(({ severity }) => groups[severity].length > 0).map(({ severity, heading, hint }) => {
        const list = groups[severity]
        const cut = severity === "suggested" && !showAll && list.length > WORTH_DOING_SHOWN
        const shown = cut ? list.slice(0, WORTH_DOING_SHOWN) : list
        return (
          <section key={severity} aria-labelledby={`sev-${severity}`} className="flex flex-col">
            <SectionHeader
              id={`sev-${severity}`}
              title={heading}
              count={list.length}
              action={
                cut ? (
                  <Button size="sm" variant="ghost" onClick={() => setShowAll(true)}>
                    {`Show all ${count(list.length)}`}
                  </Button>
                ) : undefined
              }
            />
            {!open && <p className="type-caption pt-2 text-muted-foreground">{hint}</p>}
            <ul aria-label={heading} className="flex flex-col">
              {shown.map((f) => (
                <FindingRow key={f.id} finding={f} selected={selectedFinding === f.id} onOpen={openFinding} />
              ))}
            </ul>
          </section>
        )
      })}
    </>
  )

  // Each list as today's view (sort, group, its own search), under WHOLE LISTS: nothing that was reachable
  // from Health before becomes unreachable (D724's entry rows).
  const fullList = (place: "attention" | "expiring", title: string, n: number) => (
    // The row's button is the way in; a press anywhere on the row (the harness's go(), a stray tap) goes too.
    <ListRow
      render={<li onClick={(e) => e.target === e.currentTarget && go(place)} />}
      size="compact"
      metaStyle="plain"
      title={title}
      meta="Sort and group the whole list"
      status={count(n)}
      onClick={() => go(place)}
    />
  )

  const itemLists = (
    <>
      {attention.length > 0 && (
        <section aria-labelledby="needs-attention" className="flex flex-col">
          <SectionHeader id="needs-attention" title="Needs attention" count={attention.length} />
          <ul aria-label="Needs attention" className="flex flex-col">
            {attention.map((r) => (
              <ListRow
                key={r.id}
                render={<li />}
                metaStyle="plain"
                selected={selectedItem === r.id}
                title={r.title}
                meta={(r.site ? hostOf(r.site) : r.subtitle) || kindWord(r.kind)}
                status={attentionWord(r)}
                onClick={() => openItem(r.id)}
              />
            ))}
          </ul>
        </section>
      )}
      {expiring.length > 0 && (
        <section aria-labelledby="expiring" className="flex flex-col">
          <SectionHeader id="expiring" title="Expiring" count={expiring.length} />
          <ul aria-label="Expiring" className="flex flex-col">
            {expiring.map((r) => (
              <ListRow
                key={r.id}
                render={<li />}
                metaStyle="plain"
                selected={selectedItem === r.id}
                title={r.title}
                meta={[kindWord(r.kind), r.expires_at?.replaceAll("-", ".")].filter(Boolean).join(" // ")}
                status={expiryStatus(r.expires_in_days)}
                onClick={() => openItem(r.id)}
              />
            ))}
          </ul>
        </section>
      )}
    </>
  )

  const dismissedList = !web && dismissed.length > 0 && (
    <SectionHeader collapsible defaultOpen={false} title="Dismissed" count={dismissed.length} summary={`${countName(dismissed.length, "finding")} dismissed or snoozed`}>
      <ul aria-label="Dismissed and snoozed" className="flex flex-col">
        {dismissed.map((f) => (
          <FindingRow key={f.id} finding={f} selected={selectedFinding === f.id} onOpen={openFinding} onRestore={restore} busy={busy} />
        ))}
      </ul>
    </SectionHeader>
  )

  // --- protections (brief H1, right column) -----------------------------------------------------
  const breached = findings.filter((f) => f.kind === "breached").length
  const passkeys = rows ? rows.filter((r) => r.passkeys > 0).length : null
  const rowSize = frame.phone ? "stacked" : "default"
  const protections = (
    <section aria-labelledby="protections" className="flex flex-col">
      <SectionHeader id="protections" title="Protections" />
      <div className="flex flex-col">
        {!web && report && (
          <FieldRow
            size={rowSize}
            label="Breach check"
            description={
              report.breach.enabled
                ? report.breach.message
                  ? `The last check didn't finish: ${report.breach.message}`
                  : `${report.breach.checked} of ${countName(report.passwords, "password")} checked against ${report.breach.source}`
                : "Your passwords aren't checked against known breaches."
            }
            action={report.breach.enabled ? undefined : <FieldRowAction onClick={() => go("settings-security-checks")}>Turn on</FieldRowAction>}
          >
            {!report.breach.enabled
              ? "Off"
              : !report.breach.checked_at
                ? "Not checked yet"
                : `${breached ? `${count(breached)} breached` : "No breaches"} // checked ${clockWords(report.breach.checked_at * 1000)}`}
          </FieldRow>
        )}
        {!web && report && (
          <FieldRow size={rowSize} label="Sign-in alerts" description={report.alerts.message ?? undefined}>
            {!report.alerts.account ? "Needs an account" : report.alerts.checked_at ? `Checked ${rowDate(report.alerts.checked_at)}` : "Not checked yet"}
          </FieldRow>
        )}
        {passkeys !== null && passkeys > 0 && (
          <FieldRow size={rowSize} label="Passkeys">
            {`${countName(passkeys, "login")} ${passkeys === 1 ? "has" : "have"} a passkey`}
          </FieldRow>
        )}
        {backup && (
          <FieldRow size={rowSize} label="Backups" action={<FieldRowAction onClick={() => go("settings-backups")}>Open</FieldRowAction>}>
            {!backup.enabled ? "Off" : backup.last_error ? "The last backup didn't work" : backup.last_success_at ? `Backed up ${rowDate(backup.last_success_at)}` : "No backup yet"}
          </FieldRow>
        )}
        {status && (
          <FieldRow size={rowSize} label="Devices" action={<FieldRowAction onClick={() => onDevices()}>Open</FieldRowAction>}>
            {status.devices > 1 ? countName(status.devices, "device") : "Only this device"}
          </FieldRow>
        )}
        <PqStatusLine onDevice={(id) => onDevices(id)} refresh={checks} />
        <FieldRow
          size={rowSize}
          label="Duplicates"
          action={
            <FieldRowAction onClick={() => openThis({ place: "duplicates" })} aria-label="Find duplicates">
              Find
            </FieldRowAction>
          }
        >
          {duplicatesWords(dupes.groups)}
        </FieldRow>
      </div>
    </section>
  )

  const moreHelp = bridges && (
    <nav aria-labelledby="more-help" className="flex flex-col gap-2">
      <SectionHeader id="more-help" title="More help" />
      <NavItem label="Accounts you sign in with" icon="user--multiple" onClick={() => openThis({ place: "accounts" })} />
      <NavItem label="Something happened?" icon="help" description={flowsUnderWay(incidents.list)} onClick={() => openThis({ place: "incidents" })} />
    </nav>
  )

  const wholeLists = (
    <section aria-labelledby="whole-lists" className="flex flex-col">
      <SectionHeader id="whole-lists" title="Whole lists" />
      <ul aria-label="Whole lists" className="flex flex-col">
        {fullList("attention", "Needs attention, in full", attention.length)}
        {fullList("expiring", "Expiring, in full", expiring.length)}
      </ul>
    </section>
  )

  const credits = !web && report && (
    <div className="flex flex-col gap-1">
      <p className="type-caption text-muted-foreground">
        Two-step sign-in information for {report.directory.sites.toLocaleString("en-US")} sites comes from 2fa.directory ({report.directory.licence} licence), as of{" "}
        {report.directory.date}. It is built into Lockdown, so looking a site up tells no one which sites you use.
      </p>
      <GuideAttribution />
    </div>
  )

  // --- what opens ------------------------------------------------------------------------------
  const detail: ReactNode = !open ? null : open.place === "fix" ? (
    finding ? (
      <FindingDetail
        key={finding.id}
        finding={finding}
        playbook={playbooks ? (playbooks[finding.playbook] ?? null) : null}
        place={where}
        busy={busy}
        onDismiss={dismiss}
        onRestore={restore}
        onOpenItem={openItem}
        onDevices={() => onDevices()}
        onNotice={say}
      />
    ) : (
      <EmptyState overline="Health // fixed" title="Nothing open." body="That finding is no longer on the list." />
    )
  ) : open.place === "item" ? (
    <DetailPane
      key={open.item}
      id={open.item}
      rows={rows ?? []}
      onEdit={(item) => edit({ kind: item.template ?? item.kind, item })}
      onTrashed={() => {
        setOpen(null)
        reloadRows()
      }}
      onChanged={(text) => {
        reloadRows()
        say(text)
      }}
      onReplaced={(id, text) => {
        setOpen({ place: "item", item: id })
        reloadRows()
        say(text)
      }}
      onNotice={say}
    />
  ) : open.place === "duplicates" ? (
    group ? (
      <DuplicateDetail
        key={open.group}
        group={group}
        rows={rows ?? []}
        onChanged={() => {
          setOpen({ place: "duplicates" })
          dupes.reload()
          reloadRows()
        }}
        onOpen={openItem}
      />
    ) : null
  ) : open.place === "accounts" ? (
    provider ? (
      <AccountDetail
        key={provider.id}
        provider={provider}
        busy={federation.busy}
        onOpenItem={openItem}
        onPick={(item) => void federation.run((b) => b.federationSetAccount(provider.id, item), `That's your ${provider.name} account now`)}
      />
    ) : null
  ) : flow ? (
    <IncidentDetail key={flow.id} id={flow.id} onOpenItem={openItem} onAction={stepAction} onNotice={say} onProgress={incidents.reload} />
  ) : null

  const subList: ReactNode = !open ? null : open.place === "duplicates" ? (
    <DuplicatesList groups={dupes.groups} failed={dupes.failed} chosen={open.group ?? null} onChoose={(key) => openThis({ place: "duplicates", group: key })} onTryAgain={dupes.reload} />
  ) : open.place === "accounts" ? (
    <AccountsList
      map={federation.map}
      failed={federation.failed}
      busy={federation.busy}
      chosen={open.provider ?? null}
      onChoose={(id) => openThis({ place: "accounts", provider: id })}
      onRecord={(p, item) => void federation.run((b) => b.federationAccept(p.id, item), `Noted: you sign in to ${p.host} with ${p.provider_name}`)}
      onForget={(p) => void federation.run((b) => b.federationDismiss(p.id), "Forgotten")}
      onTryAgain={federation.reload}
    />
  ) : open.place === "incidents" ? (
    <IncidentsList list={incidents.list} failed={incidents.failed} chosen={open.flow ?? null} onChoose={(id) => openThis({ place: "incidents", flow: id })} onTryAgain={incidents.reload} />
  ) : (
    <div className="flex flex-col gap-8">
      {fixLists}
      {itemLists}
    </div>
  )
  const subTitle = open?.place === "duplicates" ? "Duplicates" : open?.place === "accounts" ? "Accounts you sign in with" : open?.place === "incidents" ? "Something happened?" : null

  // What waits on the person: under the page's head, or at the top of the detail when one is open (brief X3).
  const underHead = useNeedsYouUnderHead()
  const needsYou = useNeedsYouBlock()

  // --- one place open --------------------------------------------------------------------------
  if (open) {
    const nothing = (
      <EmptyState
        overline={subTitle ?? "Health"}
        title="Nothing open."
        body={open.place === "duplicates" ? "Pick a group to see it here." : open.place === "accounts" ? "Pick an account to see what hangs on it." : "Pick a guide to start it."}
      />
    )
    if (frame.split)
      return (
        <div ref={root} className="flex min-h-full items-stretch">
          <h1 className="sr-only">Health</h1>
          <div className={`flex shrink-0 flex-col gap-6 pe-3 ${frame.width === "wide" || frame.width === "ultrawide" ? "w-(--c-app-frame-list)" : "w-(--c-app-frame-list-compact)"}`}>
            {subTitle && <h2 className="type-title">{subTitle}</h2>}
            {subList}
          </div>
          <div className="flex min-w-0 flex-1 flex-col gap-6 ps-6 rule-l">
            {needsYou && <NeedsYouSlot />}
            {detail ?? nothing}
          </div>
        </div>
      )
    // Pushed (a phone, medium width): the detail when one is chosen, else the place's own list.
    return (
      <div ref={root} className="flex flex-col gap-6">
        <h1 className="sr-only">Health</h1>
        {deeper || open.place === "fix" || open.place === "item" ? (
          detail
        ) : (
          <>
            {subTitle && <h2 className={frame.phone ? "type-display-compact" : "type-title"}>{subTitle}</h2>}
            {subList}
          </>
        )}
      </div>
    )
  }

  // --- the page -------------------------------------------------------------------------------
  const toFix = report ? toFixCount(report, rows ?? []) : 0
  // The hero's overline: on a phone when it was checked (Mobile / 09); beside the sidebar, whose vaults (Desktop / 07),
  // as the status line already says when.
  const heroLabel = checking
    ? "Health // checking"
    : !frame.phone && vaults.length > 0
      ? `Health // ${vaults.map((v) => v.name).join(" + ")}`
      : checkedAt
        ? `Health // checked ${clockWords(checkedAt * 1000)}`
        : "Health"
  const kinds = report ? kindCounts(report) : null
  // The report could not be read: nothing to say about passwords, never an all-clear (cross-app §9). The lists
  // and Protections that do not hang on it still show.
  const unread = !web && readError && !report && (
    <EmptyState
      tone="critical"
      overline="Health // not read"
      title="Couldn't read your health report."
      body={readError}
      actionLabel="Try again"
      onAction={() => {
        setReadError(null)
        setReads((n) => n + 1)
      }}
    />
  )
  const checkNow = !web && (
    <Button variant="ghost" size="sm" loading={checking} onClick={() => void check()}>
      <Icon name="renew" data-icon="inline-start" />
      Check now
    </Button>
  )
  const lists = (
    <div className="flex flex-col gap-8">
      {!web && !report && !readError && (
        <div aria-busy="true" aria-label="Reading your health report" className="flex flex-col">
          <Skeleton variant="row" />
          <Skeleton variant="row" />
          <Skeleton variant="row" />
        </div>
      )}
      {fixLists}
      {itemLists}
      {web && attention.length === 0 && expiring.length === 0 && rows && <EmptyState overline="Health // 00" title="Nothing needs you." body="No item needs attention or expires soon." />}
      {dismissedList}
    </div>
  )
  const side = (
    <div className="flex flex-col gap-8">
      {protections}
      {moreHelp}
      {wholeLists}
      {credits}
    </div>
  )
  return (
    <div ref={root} className="flex flex-col gap-8 pb-12">
      <h1 className="sr-only">Health</h1>
      {web ? (
        <Banner tone="info" title="Checks for weak, reused and breached passwords are in the desktop app." />
      ) : unread ? (
        unread
      ) : (
        <div className="flex flex-col gap-6">
          {/* The hero takes the width (its unit stays beside the number on a phone); CHECK NOW sits at the end of its overline's line. */}
          <div className="relative">
            <Metric
              size="hero"
              valueTone="primary"
              label={heroLabel}
              value={report ? count(toFix) : "—"}
              unit="TO FIX"
              aria-label={report ? `${countName(toFix, "thing", "things")} to fix` : undefined}
            />
            <div className="absolute end-0 -top-2">{checkNow}</div>
          </div>
          {checkError && <Banner tone="critical" title="The check didn't finish." body={checkError} action={<Button variant="secondary" size="sm" onClick={() => void check()}>Try again</Button>} />}
          {kinds && (
            <div role="group" aria-label="Findings by kind" className={`grid gap-x-6 gap-y-6 ${frame.phone ? "grid-cols-2" : "grid-cols-4"}`}>
              <Metric label="Weak" value={count(kinds.weak)} delta={`of ${countName(report?.passwords ?? 0, "password")}`} />
              <Metric label="Reused" value={count(kinds.reused)} delta="passwords" />
              <Metric label="Exposed" value={kinds.exposed === null ? "—" : count(kinds.exposed)} delta={kinds.exposed === null ? "Check is off" : "in breaches"} />
              <Metric label="No second step" value={count(kinds.noSecondStep)} delta="sites" />
            </div>
          )}
        </div>
      )}
      {underHead}
      {frame.split ? (
        <div className="grid grid-cols-2 gap-x-12">
          {lists}
          {side}
        </div>
      ) : (
        <>
          {lists}
          {side}
        </>
      )}
    </div>
  )
}
