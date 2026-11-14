import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react"
import { ThemeProvider } from "next-themes"

import { Toaster, TooltipProvider, toast } from "@unicron/ui"

import { MessageBoardProvider, criticalBanner, firstCritical, useNews, type CriticalSources } from "@/app/banners"
import { GateProvider, useGate } from "@/app/gate"
import { useIdleCountdown, useLockWatch } from "@/app/lock-watch"
import { notify, savedWords } from "@/app/notify"
import { StatusProvider, useLiveWork, type LiveStatus, type StatusFact } from "@/app/status"
import { LockedProvider } from "@/app/locked"
import { WaitingHost, type NeedsYouPlace } from "@/app/needs-you"
import { SettingsProvider, useSettings } from "@/app/settings"
import { TemplatesProvider, useTemplates } from "@/app/templates"
import { navFor } from "@/app/places"
import { AppShell, type Nav } from "@/app/shell"
import { PageColumn } from "@/components/master-detail"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary, LockState, SecurityReport, Status, VaultView as VaultEntry } from "@/core/types"
import { decodeAndWipe } from "@/core/secret-bytes"
import { AccountDialog } from "@/features/account/account-dialog"
import { useBackupTrouble } from "@/features/bridges/backup-indicator"
import { OncePage, type ShownPage } from "@/features/status/once-page"
import { useUpdateMark } from "@/features/updates/restart-row"
import { useExpiryReminder } from "@/features/organization/expiry"
import { useAwayReminder } from "@/features/vault/use-away-reminder"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { CodesView } from "@/features/codes/codes-view"
import { QuickPalette } from "@/features/palette/quick-palette"
import { PairingDialog } from "@/features/browsers/pairing-dialog"
import { PasskeyApprovalDialog } from "@/features/passkeys/passkey-approval-dialog"
import type { PasskeyUpgrade } from "@/features/passkeys/passkey-client"
import { upgradeNews, usePasskeyUpgrades } from "@/features/passkeys/use-passkey-upgrades"
import { toFixCount } from "@/features/security/security-model"
import { SecurityView } from "@/features/security/security-view"
import { SettingsView } from "@/features/settings/settings-view"
import { useSharingClient } from "@/features/sharing/sharing-client"
import { SharingLocalOnly, SharingView } from "@/features/sharing/sharing-view"
import { UnlockScreen } from "@/features/unlock/unlock-screen"
import { SearchView } from "@/features/vault/search-view"
import { VaultView } from "@/features/vault/vault-view"
import { useLive } from "@/features/sessions/use-live"
import { LiveClientProvider } from "@/features/sessions/live-client"
import { tauriLiveClient } from "@/app/tauri-live"
import { tauriServerClient } from "@/app/tauri-server"
import { ServerClientProvider } from "@/features/server/server-client"
import { tauriPqStatusClient } from "@/app/tauri-pq-status"
import { PqStatusClientProvider } from "@/features/security/pq-status-client"
import { tauriDepthClient } from "@/app/tauri-depth"
import { DepthClientProvider } from "@/features/depth/depth-client"
import { RemindersPrompt } from "@/features/depth/reminders"
import { BridgesProvider } from "@/features/bridges/bridges-client"
import { tauriBridgesClient } from "@/app/tauri-bridges"
import { tauriHygieneClient, tauriQuickClient, tauriScanClient } from "@/app/tauri-finish"
import { listen } from "@tauri-apps/api/event"
import { HygieneClientProvider } from "@/features/hygiene/hygiene-client"
import { useSecretWipe, wipeWords } from "@/features/hygiene/secret-wipe"
import { count as countWords } from "@/lib/format"
import { QuickClientProvider } from "@/features/quick/quick-client"
import { ScanClientProvider } from "@/features/scan/scan-client"
import { PaperClientProvider } from "@/features/paper/paper-client"
import { FirstRunClientProvider } from "@/features/unlock/first-run-client"
import { tauriFirstRunClient } from "@/app/tauri-first-run"
import { tauriPaperClient } from "@/app/tauri-paper"
import { tauriUpdateClient } from "@/app/tauri-updates"
import { UpdateClientProvider } from "@/features/updates/update-client"
import { useUpdateNotices } from "@/features/updates/use-update-notices"

// The developer reset and the demo (docs/plan/40-devtools.md): one lazy
// mount each, gone from a release bundle with the define.
const ResetMount = __LOCKDOWN_DEVTOOLS__ ? lazy(() => import("@/devtools/reset-mount")) : null
const DemoMount = __LOCKDOWN_DEVTOOLS__ ? lazy(() => import("@/devtools/demo-mount")) : null

/**
 * Locked is the default state. `status` is set when the core opened the
 * vault and cleared the moment the core says it is locked, whichever
 * timer did it; nothing secret is ever kept here. The item rows (masked
 * summaries) are loaded once per change and shared by the sidebar's
 * counts and the vault's list.
 */
function Root() {
  const client = useClient()
  const [status, setStatus] = useState<Status | null>(null)
  const [lock, setLock] = useState<LockState | null>(null)
  const [nav, setNav] = useState<Nav>({ area: "vault", view: "all" })
  const [rows, setRows] = useState<ItemSummary[] | null>(null)
  const [rowsError, setRowsError] = useState<string | null>(null)
  const [report, setReport] = useState<SecurityReport | null>(null)
  const [sharingMark, setSharingMark] = useState(0)
  const sharing = useSharingClient()
  const [selected, setSelected] = useState<string | null>(null)
  const [palette, setPalette] = useState(false)
  const [vaults, setVaults] = useState<VaultEntry[]>([])
  // What is broken and fixable (brief X2 table 4), besides what Status and the backups say themselves.
  const [sessionEnded, setSessionEnded] = useState<string | null>(null)
  const [split, setSplit] = useState<string | null>(null)
  const [syncError, setSyncError] = useState<string | null>(null)
  const [signingIn, setSigningIn] = useState(false)
  const [once, setOnce] = useState<ShownPage | null>(null)
  // Health is read again after a pull runs its check.
  const [healthRead, setHealthRead] = useState(0)
  const news = useNews()
  const { dismiss: dismissGate } = useGate()

  // Lock (brief X5): no question, no animation. The content goes first (the locked branch below draws the
  // unlock screen alone), then every layer closes with it: the gate, the palette, every dialog and sheet.
  const onLockState = useCallback((state: LockState) => {
    setLock(state)
    if (state.locked) {
      dismissGate()
      setStatus(null)
      setRows(null)
      setReport(null)
      setSharingMark(0)
      setPalette(false)
      setSigningIn(false)
    }
  }, [dismissGate])
  useLockWatch(onLockState)
  const idle = useIdleCountdown(status ? lock : null)
  // Typed secrets go when the window hides or stays out of focus (docs/plan/26-desktop-finish.md §7).
  useSecretWipe(useCallback((n: number) => void notify(wipeWords(n, "window")), []))
  // An item opened from the quick-access window.
  useEffect(() => {
    const off = listen<string | null>("lockdown://open-item", (e) => {
      if (!e.payload) return
      setNav({ area: "vault", view: "all" })
      setSelected(e.payload)
    })
    return () => {
      void off.then((f) => f())
    }
  }, [])

  const reload = useCallback((): Promise<ItemSummary[] | null> => {
    client.vaults().then(setVaults).catch(() => setVaults([]))
    // The cached report only (brief 4.0 Data): the network checks run when Health opens, as before.
    client.securityReport().then(setReport).catch(() => setReport(null))
    return client
      .list()
      .then((r) => {
        setRows(r)
        setRowsError(null)
        return r
      })
      .catch((e) => {
        setRowsError(describeError(e))
        return null
      })
  }, [client])

  useEffect(() => {
    if (status) void reload()
  }, [status, reload])

  // Opening the vault, a sync and a check the person waits on show in the status line and the strip (brief X2 row 2).
  const { work, track } = useLiveWork(status !== null && rows === null)

  // A sync the person asked for (a pull, "Check again", TRY AGAIN) or the app needed. What it finds wrong is a
  // critical Banner, not a toast: a failure until the next sync works, a split view until a sync without one.
  const sync = useCallback(
    () =>
      track("syncing", client.sync())
        .then((out) => {
          setSyncError(null)
          setSplit(splitLine(out.lines))
        })
        .catch((e) => setSyncError(describeError(e)))
        .then(() => client.status().then(setStatus).catch(() => {}))
        .then(() => reload()),
    [client, track, reload],
  )

  // No vault could open yet (the only one is shared and this device hasn't
  // checked who changed it): one sync now opens it, rather than waiting for
  // the next change (docs/plan/49-shared-vault-quiet.md §6).
  const waiting = status?.waiting === true
  useEffect(() => {
    if (!waiting) return
    void sync()
  }, [waiting, sync])

  // A pull on Health runs its CHECK NOW, then Health reads the fresh report.
  const check = useCallback(
    () =>
      track("checking", client.securityCheck())
        .then(setReport)
        .catch(() => {})
        .finally(() => setHealthRead((n) => n + 1)),
    [client, track],
  )

  const refreshStatus = () => {
    client.status().then(setStatus).catch((e) => toast.error(describeError(e)))
    void reload()
  }
  const lockNow = () =>
    client
      .lock()
      .then(() => {
        dismissGate()
        setPalette(false)
        setStatus(null)
      })
      .catch((e) => toast.error(describeError(e)))
  const notice = useCallback((text: string) => {
    notify(text)
  }, [])
  const go = useCallback((id: string) => {
    const to = navFor(id)
    if (to) setNav(to)
  }, [])
  // A check someone asked for from the macOS menu: About opens, where the update rows say how it went (D755).
  const showAbout = useCallback(() => go("settings-about"), [go])
  useUpdateNotices(nav.area !== "settings" || nav.section !== "about", showAbout)
  const updateMark = useUpdateMark()
  const toFix = useMemo(() => toFixCount(report, rows ?? []), [report, rows])
  // What waits on the person in Sharing (brief X3): read at unlock and every minute while unlocked, and
  // not at all with no account or a vault kept on this computer only.
  const shares = status !== null && status.account != null && !status.account.local
  useEffect(() => {
    if (!shares) return
    let live = true
    const read = () =>
      sharing
        .overview()
        .then((o) => live && setSharingMark((o.resets?.length ?? 0) + o.waiting_on_you))
        .catch(() => {})
    void read()
    const timer = window.setInterval(() => void read(), 60_000)
    return () => {
      live = false
      window.clearInterval(timer)
    }
  }, [shares, sharing])
  const openItem = useCallback((id: string) => {
    setNav({ area: "vault", view: "all" })
    setSelected(id)
  }, [])
  const vaultsValue = useMemo(() => ({ vaults, reload: () => void reload() }), [vaults, reload])
  // Where the person is, for what waits on one item or on Sharing only (brief X3).
  const itemOpen = nav.area === "vault" || nav.area === "search" ? selected : null
  const needsYouPlace = useMemo<NeedsYouPlace>(() => ({ sharing: nav.area === "sharing", item: itemOpen }), [nav.area, itemOpen])

  // News that can wait (brief X2 table 5): one dismissable info Banner on All items, not a toast (D753).
  const { add: addNews, clear: clearNews } = news
  useEffect(() => {
    if (status === null) clearNews()
  }, [status, clearNews])
  const showExpiring = useCallback(() => go("expiring"), [go])
  const expiryNews = useCallback((n: { title: string; body?: string }, show: () => void) => addNews({ id: "expiry", ...n, action: { label: "Show", onClick: show } }), [addNews])
  useExpiryReminder(client, status !== null, expiryNews, showExpiring)
  // A sign-in on someone else's screen that is due to be signed out (D237): Show opens the item, or the list of them.
  const showAway = useCallback((itemId: string | null) => (itemId ? openItem(itemId) : go("attention")), [openItem, go])
  const awayNews = useCallback((n: { title: string; body?: string }, show: () => void) => addNews({ id: "away", ...n, action: { label: "Show", onClick: show } }), [addNews])
  useAwayReminder(client, status !== null, awayNews, showAway)
  // A site's automatic passkey upgrade, quiet in the browser, told here (D210, D312).
  const upgraded = useCallback(
    (u: PasskeyUpgrade) => addNews({ id: `upgrade:${u.item_id}`, ...upgradeNews(u), action: { label: "Show", onClick: () => openItem(u.item_id) } }),
    [addNews, openItem],
  )
  usePasskeyUpgrades(status !== null, upgraded)
  // The person's own templates live in the vault: load them once it is open (D232).
  const { refreshUserTemplates } = useTemplates()
  const open = status !== null
  useEffect(() => {
    if (open) refreshUserTemplates()
  }, [open, refreshUserTemplates])
  // The server's live channel (docs/plan/20-self-host-server.md §5): a
  // change from another device was synced in the core; redraw. What goes
  // wrong on it is a critical Banner (brief X2), never a toast.
  useLive((n) => {
    if (!status) return
    if (n.kind === "synced") {
      client.status().then(setStatus).catch(() => {})
      setSyncError(null)
      setSplit(splitLine(n.lines))
    } else if (n.kind === "session_ended")
      // A device revoked elsewhere hears that, in the core's words (its sessions end with it).
      client
        .status()
        .then((s) => {
          setStatus(s)
          return s.account?.sign_in?.message ?? null
        })
        .catch(() => null)
        .then((words) => setSessionEnded(words ?? "This device's session was ended from another of your devices."))
    else if (n.kind === "sync_failed") setSyncError(n.message)
  })

  const backup = useBackupTrouble(status !== null)
  const sources: CriticalSources = { signIn: status?.account?.sign_in?.message ?? sessionEnded, split, sync: syncError, backup }
  const kind = firstCritical(sources)
  const critical = kind
    ? criticalBanner(kind, sources, {
        signIn: () => setSigningIn(true),
        seeStatus: () => go("status"),
        tryAgain: () => void sync(),
        openBackups: () => go("settings-backups"),
      })
    : null

  // The status line's own line where a screen says none: on Items and Sharing, what Health has to fix (brief X2 row 3).
  const healthLine = useMemo<StatusFact | null>(
    () =>
      report && (nav.area === "vault" || nav.area === "sharing")
        ? { text: toFix ? `Health // ${countWords(toFix)} to fix` : "Health // all clear", onClick: () => go("health"), label: "Open Health" }
        : null,
    [report, nav.area, toFix, go],
  )
  const live = useMemo<LiveStatus>(() => ({ idleLeft: idle.left, onStay: idle.stay, work }), [idle.left, idle.stay, work])

  // The demo moved the app to its own home and back (docs/plan/40-devtools.md).
  const { reload: reloadSettings } = useSettings()
  const devtools = (
    <Suspense fallback={null}>
      {ResetMount && <ResetMount />}
      {DemoMount && (
        <DemoMount
          onOpened={(s) => {
            setNav({ area: "vault", view: "all" })
            setSelected(null)
            setStatus(s)
            reloadSettings()
            refreshUserTemplates()
          }}
          onClosed={() => {
            setNav({ area: "vault", view: "all" })
            setSelected(null)
            setRows(null)
            setStatus(null)
            reloadSettings()
          }}
        />
      )}
    </Suspense>
  )

  if (!status) {
    // Locked: the unlock screen alone. Nothing of the shell is drawn, not even its shape (Q15, changes D107's
    // wash; brief 4.0 "Locked means nothing readable is drawn").
    return (
      <div className="h-full overflow-y-auto">
        <UnlockScreen reason={lock?.reason ?? null} onOpened={setStatus} />
        {devtools}
        {/* The open shell holds its own, inside the frame. */}
        <Toaster position="bottom-center" />
      </div>
    )
  }
  return (
    <LockedProvider value={lock?.locked ?? false}>
      <WaitingHost place={needsYouPlace}>
      <VaultsProvider value={vaultsValue}>
      <StatusProvider live={live} fallback={healthLine}>
      <MessageBoardProvider critical={critical} news={news.news} onDismissNews={news.dismiss}>
      <AppShell
        nav={nav}
        onNavigate={setNav}
        rows={rows}
        onLock={lockNow}
        toFix={toFix}
        sharingMark={shares ? sharingMark : 0}
        settingsMark={updateMark}
        onNotice={notice}
        onRefresh={() => void reload()}
        onSync={sync}
        onCheck={check}
        onSaved={(id, generated) => {
          setSelected(id)
          if (nav.area !== "vault" && nav.area !== "search") setNav({ area: "vault", view: "all" })
          // The toast names what was saved (brief X2): its title, once the list has it.
          void reload().then((r) => notify(savedWords(r?.find((x) => x.id === id)?.title ?? null, generated)))
        }}
      >
        {nav.area === "vault" && (
          <VaultView
            view={nav.view}
            rows={rows}
            error={rowsError}
            onChanged={() => void reload()}
            onNotice={notice}
            selected={selected}
            onSelect={setSelected}
            onPalette={() => setPalette(true)}
            scope={nav.scope}
          />
        )}
        {nav.area === "search" && <SearchView rows={rows} selected={selected} onSelect={setSelected} onChanged={() => void reload()} onNotice={notice} />}
        {nav.area === "codes" && (
          <PageColumn>
            <CodesView rows={rows ?? []} onChanged={() => void reload()} />
          </PageColumn>
        )}
        {nav.area === "settings" && <SettingsView status={status} onStatus={setStatus} lock={lock} onLock={setLock} onChanged={refreshStatus} onNotice={notice} />}
        {nav.area === "sharing" && status.account?.local && (
          <PageColumn>
            <SharingLocalOnly />
          </PageColumn>
        )}
        {nav.area === "sharing" && !status.account?.local && (
          <PageColumn>
            <SharingView rows={rows ?? []} section={nav.tab ?? "people"} vault={nav.members} onSection={(tab) => go(`sharing-${tab}`)} onOpenItem={openItem} onChanged={() => void reload()} onNotice={notice} />
          </PageColumn>
        )}
        {nav.area === "security" && (
          <PageColumn>
            <SecurityView
              key={healthRead}
              rows={rows ?? []}
              onOpenItem={openItem}
              onDevices={(device) => setNav({ area: "settings", view: "all", section: "devices", device })}
              onRecovery={() => go("settings-recovery")}
              onChanged={() => void reload()}
              onNotice={notice}
            />
          </PageColumn>
        )}
      </AppShell>
      </MessageBoardProvider>
      </StatusProvider>
      <QuickPalette
        open={palette}
        onOpenChange={setPalette}
        rows={rows ?? []}
        onOpen={(id) => {
          setNav({ area: "vault", view: "all" })
          setSelected(id)
        }}
        onNotice={notice}
      />
      {/* SIGN IN on the Banner: today's account dialog, as Status opens it (brief X2 table 4). */}
      <AccountDialog
        kind={signingIn ? "login" : null}
        reason={status.account?.sign_in?.message ?? sessionEnded}
        onClose={() => setSigningIn(false)}
        onDone={(_lines, page) => {
          setSigningIn(false)
          setSessionEnded(null)
          // A page shown once (the recovery kit), as Status shows it.
          if (page && page.page.length) setOnce({ title: "Your recovery kit", lines: page.lines, text: decodeAndWipe(page.page) })
          refreshStatus()
        }}
      />
      <OncePage page={once} onClose={() => setOnce(null)} />
      <PairingDialog onNotice={notice} />
      <PasskeyApprovalDialog onNotice={notice} />
      {/* The reminders are needs-you rows 7 to 9 (brief X3, Q4), not a dialog after unlock. */}
      <RemindersPrompt onNotice={notice} onChanged={refreshStatus} />
      {devtools}
      </VaultsProvider>
      </WaitingHost>
    </LockedProvider>
  )
}

/** The line of a sync that says the server's log is split from its mirrors (D328), or null. */
function splitLine(lines: readonly string[]): string | null {
  return lines.find((l) => l.includes("Split view")) ?? null
}

export default function App() {
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem disableTransitionOnChange>
      <TooltipProvider>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <LiveClientProvider value={tauriLiveClient}>
                <DepthClientProvider value={tauriDepthClient}>
                  <BridgesProvider value={tauriBridgesClient}>
                    <QuickClientProvider value={tauriQuickClient}>
                      <ScanClientProvider value={tauriScanClient}>
                        <HygieneClientProvider value={tauriHygieneClient}>
                          <ServerClientProvider value={tauriServerClient}>
                            <PqStatusClientProvider value={tauriPqStatusClient}>
                              <PaperClientProvider value={tauriPaperClient}>
                                <FirstRunClientProvider value={tauriFirstRunClient}>
                                  <UpdateClientProvider value={tauriUpdateClient}>
                                    <Root />
                                  </UpdateClientProvider>
                                </FirstRunClientProvider>
                              </PaperClientProvider>
                            </PqStatusClientProvider>
                          </ServerClientProvider>
                        </HygieneClientProvider>
                      </ScanClientProvider>
                    </QuickClientProvider>
                  </BridgesProvider>
                </DepthClientProvider>
              </LiveClientProvider>
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
      </TooltipProvider>
    </ThemeProvider>
  )
}
