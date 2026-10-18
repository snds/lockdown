import { useCallback, useEffect, useMemo, useState } from "react"
import { ThemeProvider } from "next-themes"

import { Toaster, TooltipProvider } from "@unicron/ui"
import { toast } from "sonner"

import { GateProvider } from "@/app/gate"
import { useLockWatch } from "@/app/lock-watch"
import { LockedProvider } from "@/app/locked"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { AppShell, type Nav } from "@/app/shell"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary, LockState, PersonaInfo, Status, VaultView as VaultEntry } from "@/core/types"
import { useExpiryReminder } from "@/features/organization/expiry"
import { OrganizationNav } from "@/features/organization/organization-nav"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { CodesView } from "@/features/codes/codes-view"
import { DevicesView } from "@/features/devices/devices-view"
import { QuickPalette } from "@/features/palette/quick-palette"
import { RecoveryView } from "@/features/recovery/recovery-view"
import { SettingsView } from "@/features/settings/settings-view"
import { StatusView } from "@/features/status/status-view"
import { LockedWash } from "@/features/unlock/locked-wash"
import { UnlockScreen } from "@/features/unlock/unlock-screen"
import { countViews } from "@/features/vault/list-model"
import { VaultView } from "@/features/vault/vault-view"

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
              <Root />
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
        <Toaster position="bottom-center" />
      </TooltipProvider>
    </ThemeProvider>
  )
}
