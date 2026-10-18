import { useCallback, useEffect, useMemo, useState } from "react"
import { ThemeProvider } from "next-themes"

import { Toaster, TooltipProvider } from "@unicron/ui"
import { toast } from "sonner"

import { GateProvider } from "@/app/gate"
import { useLockWatch } from "@/app/lock-watch"
import { LockedProvider } from "@/app/locked"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider, useTemplates } from "@/app/templates"
import { AppShell, type Nav } from "@/app/shell"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary, LockState, PersonaInfo, Status, VaultView as VaultEntry } from "@/core/types"
import { useExpiryReminder } from "@/features/organization/expiry"
import { useAwayReminder } from "@/features/vault/use-away-reminder"
import { OrganizationNav } from "@/features/organization/organization-nav"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { CodesView } from "@/features/codes/codes-view"
import { DevicesView } from "@/features/devices/devices-view"
import { QuickPalette } from "@/features/palette/quick-palette"
import { PairingDialog } from "@/features/browsers/pairing-dialog"
import { PasskeyApprovalDialog } from "@/features/passkeys/passkey-approval-dialog"
import { RecoveryView } from "@/features/recovery/recovery-view"
import { SecurityView } from "@/features/security/security-view"
import { SettingsView } from "@/features/settings/settings-view"
import { SharingView } from "@/features/sharing/sharing-view"
import { StatusView } from "@/features/status/status-view"
import { LockedWash } from "@/features/unlock/locked-wash"
import { UnlockScreen } from "@/features/unlock/unlock-screen"
import { countViews } from "@/features/vault/list-model"
import { VaultView } from "@/features/vault/vault-view"
import { useLive } from "@/features/sessions/use-live"
import { LiveClientProvider } from "@/features/sessions/live-client"
import { tauriLiveClient } from "@/app/tauri-live"
import { tauriDepthClient } from "@/app/tauri-depth"
import { DepthClientProvider } from "@/features/depth/depth-client"
import { RemindersPrompt } from "@/features/depth/reminders"
import { BridgesProvider } from "@/features/bridges/bridges-client"
import { BackupIndicator } from "@/features/bridges/backup-indicator"
import { tauriBridgesClient } from "@/app/tauri-bridges"
import { tauriHygieneClient, tauriQuickClient, tauriScanClient } from "@/app/tauri-finish"
import { listen } from "@tauri-apps/api/event"
import { HygieneClientProvider } from "@/features/hygiene/hygiene-client"
import { useSecretWipe } from "@/features/hygiene/secret-wipe"
import { QuickClientProvider } from "@/features/quick/quick-client"
import { ScanClientProvider } from "@/features/scan/scan-client"

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
  const [personas, setPersonas] = useState<PersonaInfo[]>([])
  const [selected, setSelected] = useState<string | null>(null)
  const [palette, setPalette] = useState(false)
  const [vaults, setVaults] = useState<VaultEntry[]>([])

  const onLockState = useCallback((state: LockState) => {
    setLock(state)
    if (state.locked) {
      setStatus(null)
      setRows(null)
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
    client.personas().then(setPersonas).catch(() => setPersonas([]))
    client.vaults().then(setVaults).catch(() => setVaults([]))
  }, [client])

  useEffect(() => {
    if (status) reload()
  }, [status, reload])

  const refreshStatus = () => {
    client.status().then(setStatus).catch((e) => toast.error(describeError(e)))
    reload()
  }
  const lockNow = () => client.lock().then(() => setStatus(null)).catch((e) => toast.error(describeError(e)))
  const notice = useCallback((text: string) => {
    toast(text)
  }, [])
  const counts = useMemo(() => countViews(rows ?? []), [rows])
  const vaultsValue = useMemo(() => ({ vaults, reload }), [vaults, reload])
  const showExpiring = useCallback(() => setNav({ area: "vault", view: "expiring" }), [])
  const remind = useCallback((text: string, show: () => void) => {
    toast(text, { action: { label: "Show", onClick: show } })
  }, [])
  useExpiryReminder(client, status !== null, remind, showExpiring)
  // A sign-in on someone else's screen that is due to be signed out (D237).
  const showAttention = useCallback(() => setNav({ area: "vault", view: "attention" }), [])
  useAwayReminder(client, status !== null, remind, showAttention)
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
    if (n.kind === "synced") client.status().then(setStatus).catch(() => {})
    else if (n.kind === "session_ended") toast.error("This device's session was ended from another of your devices. Log in again on Status to keep syncing.")
    else if (n.kind === "sync_failed") toast.error(`A change from another device didn't sync: ${n.message}`)
  })

  if (!status) {
    // Locked: the shell's shape stays, washed, with nothing readable in it;
    // the unlock screen sits over it (brief §4).
    return (
      <div className="relative h-full">
        <LockedWash />
        <div className="absolute inset-0 overflow-y-auto">
          <UnlockScreen reason={lock?.reason ?? null} onOpened={setStatus} />
        </div>
      </div>
    )
  }
  return (
    <LockedProvider value={lock?.locked ?? false}>
      <VaultsProvider value={vaultsValue}>
      <AppShell
        nav={nav}
        personas={personas}
        counts={counts}
        onNavigate={setNav}
        onLock={lockNow}
        organization={<OrganizationNav rows={rows ?? []} nav={nav} onNavigate={setNav} />}
        footer={<BackupIndicator onOpen={() => setNav({ area: "settings", view: nav.view })} />}
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
        {nav.area === "codes" && <CodesView rows={rows ?? []} onChanged={reload} />}
        {nav.area === "devices" && <DevicesView status={status} onChanged={refreshStatus} onNotice={notice} />}
        {nav.area === "recovery" && <RecoveryView status={status} onChanged={refreshStatus} onNotice={notice} />}
        {nav.area === "status" && <StatusView status={status} onStatus={setStatus} onNotice={notice} />}
        {nav.area === "settings" && <SettingsView status={status} lock={lock} onLock={setLock} onChanged={refreshStatus} onNotice={notice} />}
        {nav.area === "sharing" && (
          <SharingView
            rows={rows ?? []}
            onOpenItem={(id) => {
              setNav({ area: "vault", view: "all" })
              setSelected(id)
            }}
            onChanged={reload}
            onNotice={notice}
          />
        )}
        {nav.area === "security" && (
          <SecurityView
            onOpenItem={(id) => {
              setNav({ area: "vault", view: "all" })
              setSelected(id)
            }}
            onDevices={() => setNav({ area: "devices", view: nav.view })}
            onRecovery={() => setNav({ area: "recovery", view: nav.view })}
            onChanged={reload}
            onNotice={notice}
          />
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
      </VaultsProvider>
    </LockedProvider>
  )
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
                          <Root />
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
