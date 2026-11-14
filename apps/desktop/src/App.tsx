import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from "react"
import { ThemeProvider } from "next-themes"

import { Toaster, TooltipProvider, toast } from "@unicron/ui"

import { GateProvider } from "@/app/gate"
import { useLockWatch } from "@/app/lock-watch"
import { LockedProvider } from "@/app/locked"
import { SettingsProvider, useSettings } from "@/app/settings"
import { TemplatesProvider, useTemplates } from "@/app/templates"
import { navFor } from "@/app/places"
import { AppShell, type Nav } from "@/app/shell"
import { PageColumn } from "@/components/master-detail"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary, LockState, SecurityReport, Status, VaultView as VaultEntry } from "@/core/types"
import { useExpiryReminder } from "@/features/organization/expiry"
import { useAwayReminder } from "@/features/vault/use-away-reminder"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { CodesView } from "@/features/codes/codes-view"
import { QuickPalette } from "@/features/palette/quick-palette"
import { PairingDialog } from "@/features/browsers/pairing-dialog"
import { PasskeyApprovalDialog } from "@/features/passkeys/passkey-approval-dialog"
import type { PasskeyUpgrade } from "@/features/passkeys/passkey-client"
import { upgradeToast, usePasskeyUpgrades } from "@/features/passkeys/use-passkey-upgrades"
import { toFixCount } from "@/features/security/security-model"
import { SecurityView } from "@/features/security/security-view"
import { SettingsView } from "@/features/settings/settings-view"
import { useSharingClient } from "@/features/sharing/sharing-client"
import { SharingLocalOnly, SharingView } from "@/features/sharing/sharing-view"
import { LockedWash } from "@/features/unlock/locked-wash"
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
import { useSecretWipe } from "@/features/hygiene/secret-wipe"
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

  const onLockState = useCallback((state: LockState) => {
    setLock(state)
    if (state.locked) {
      setStatus(null)
      setRows(null)
      setReport(null)
      setSharingMark(0)
      setPalette(false)
    }
  }, [])
  useLockWatch(onLockState)
  // Typed secrets go when the window hides or stays out of focus (docs/plan/26-desktop-finish.md §7).
  useSecretWipe(
    useCallback((n: number) => {
      toast(n === 1 ? "Lockdown cleared a password you were typing while the window was away." : `Lockdown cleared ${n} passwords you were typing while the window was away.`)
    }, []),
  )
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

  const reload = useCallback(() => {
    client
      .list()
      .then((r) => {
        setRows(r)
        setRowsError(null)
      })
      .catch((e) => setRowsError(describeError(e)))
    client.vaults().then(setVaults).catch(() => setVaults([]))
    // The cached report only (brief 4.0 Data): the network checks run when Health opens, as before.
    client.securityReport().then(setReport).catch(() => setReport(null))
  }, [client])

  useEffect(() => {
    if (status) reload()
  }, [status, reload])

  // No vault could open yet (the only one is shared and this device hasn't
  // checked who changed it): one sync now opens it, rather than waiting for
  // the next change (docs/plan/49-shared-vault-quiet.md §6).
  const waiting = status?.waiting === true
  useEffect(() => {
    if (!waiting) return
    client
      .sync()
      .then(() => client.status().then(setStatus))
      .catch(() => {})
  }, [waiting, client])

  const refreshStatus = () => {
    client.status().then(setStatus).catch((e) => toast.error(describeError(e)))
    reload()
  }
  const lockNow = () => client.lock().then(() => setStatus(null)).catch((e) => toast.error(describeError(e)))
  const notice = useCallback((text: string) => {
    toast(text)
  }, [])
  // A check someone asked for (the macOS menu, docs/plan/60-auto-update.md §6), told where Settings isn't open.
  useUpdateNotices(nav.area !== "settings" || status === null, notice)
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
  const go = useCallback((id: string) => {
    const to = navFor(id)
    if (to) setNav(to)
  }, [])
  const openItem = useCallback((id: string) => {
    setNav({ area: "vault", view: "all" })
    setSelected(id)
  }, [])
  const vaultsValue = useMemo(() => ({ vaults, reload }), [vaults, reload])
  const showExpiring = useCallback(() => go("expiring"), [go])
  const remind = useCallback((text: string, show: () => void) => {
    toast(text, { action: { label: "Show", onClick: show } })
  }, [])
  useExpiryReminder(client, status !== null, remind, showExpiring)
  // A sign-in on someone else's screen that is due to be signed out (D237).
  const showAttention = useCallback(() => go("attention"), [go])
  useAwayReminder(client, status !== null, remind, showAttention)
  // A site's automatic passkey upgrade, quiet in the browser, told here (D210, D312).
  const upgraded = useCallback((u: PasskeyUpgrade) => {
    toast(upgradeToast(u), {
      description: u.title,
      action: {
        label: "Show",
        onClick: () => {
          setNav({ area: "vault", view: "all" })
          setSelected(u.item_id)
        },
      },
    })
  }, [])
  usePasskeyUpgrades(status !== null, upgraded)
  // The person's own templates live in the vault: load them once it is open (D232).
  const { refreshUserTemplates } = useTemplates()
  const open = status !== null
  useEffect(() => {
    if (open) refreshUserTemplates()
  }, [open, refreshUserTemplates])
  // The server's live channel (docs/plan/20-self-host-server.md §5): a
  // change from another device was synced in the core; redraw.
  useLive((n) => {
    if (!status) return
    if (n.kind === "synced") {
      client.status().then(setStatus).catch(() => {})
      warnOnSplit(n.lines)
    }
    else if (n.kind === "session_ended")
      // A device revoked elsewhere hears that, in the core's words (its sessions end with it).
      client
        .status()
        .then((s) => (s.account?.sign_in?.reason === "revoked" ? s.account.sign_in.message : null))
        .catch(() => null)
        .then((revoked) => toast.error(revoked ?? "This device's session was ended from another of your devices. Log in again on Status to keep syncing."))
    else if (n.kind === "sync_failed") toast.error(`A change from another device didn't sync: ${n.message}`)
  })

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
    // Locked: the shell's shape stays, washed, with nothing readable in it;
    // the unlock screen sits over it (brief §4).
    return (
      <div className="relative h-full">
        <LockedWash />
        <div className="absolute inset-0 overflow-y-auto">
          <UnlockScreen reason={lock?.reason ?? null} onOpened={setStatus} />
        </div>
        {devtools}
      </div>
    )
  }
  return (
    <LockedProvider value={lock?.locked ?? false}>
      <VaultsProvider value={vaultsValue}>
      <AppShell
        nav={nav}
        onNavigate={setNav}
        rows={rows}
        onLock={lockNow}
        toFix={toFix}
        sharingMark={shares ? sharingMark : 0}
        onNotice={notice}
        onRefresh={reload}
        onSaved={(id, generated) => {
          setSelected(id)
          if (nav.area !== "vault" && nav.area !== "search") setNav({ area: "vault", view: "all" })
          reload()
          notice(generated ? "Saved. The new password is in the item; copy it from there." : "Saved.")
        }}
      >
        {nav.area === "vault" && (
          <VaultView
            view={nav.view}
            rows={rows}
            error={rowsError}
            onChanged={reload}
            onNotice={notice}
            selected={selected}
            onSelect={setSelected}
            onPalette={() => setPalette(true)}
            scope={nav.scope}
          />
        )}
        {nav.area === "search" && <SearchView rows={rows} selected={selected} onSelect={setSelected} onChanged={reload} onNotice={notice} />}
        {nav.area === "codes" && (
          <PageColumn>
            <CodesView rows={rows ?? []} onChanged={reload} />
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
            <SharingView rows={rows ?? []} section={nav.tab ?? "people"} vault={nav.members} onSection={(tab) => go(`sharing-${tab}`)} onOpenItem={openItem} onChanged={reload} onNotice={notice} />
          </PageColumn>
        )}
        {nav.area === "security" && (
          <PageColumn>
            <SecurityView
              rows={rows ?? []}
              onOpenItem={openItem}
              onDevices={(device) => setNav({ area: "settings", view: "all", section: "devices", device })}
              onRecovery={() => go("settings-recovery")}
              onChanged={reload}
              onNotice={notice}
            />
          </PageColumn>
        )}
      </AppShell>
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
      <PairingDialog onNotice={notice} />
      <PasskeyApprovalDialog onNotice={notice} />
      <RemindersPrompt onNotice={notice} onChanged={refreshStatus} />
      {devtools}
      </VaultsProvider>
    </LockedProvider>
  )
}

/** A sync whose mirrors caught a split view says so until dismissed (docs/plan/27-web-server-finish.md §4). */
function warnOnSplit(lines: readonly string[]) {
  const split = lines.find((l) => l.includes("Split view"))
  if (split)
    toast.error("This server's public log doesn't match a mirror: it may be showing you a different history from everyone else. New sharing is paused until it clears. See Status.", {
      id: "kt-split",
      duration: Infinity,
    })
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
        <Toaster position="bottom-center" />
      </TooltipProvider>
    </ThemeProvider>
  )
}
