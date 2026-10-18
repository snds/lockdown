import { useCallback, useEffect, useMemo, useRef, useState } from "react"

import { Spinner, Toaster, TooltipProvider } from "@unicron/ui"
import { toast } from "sonner"

import { GateProvider } from "@/app/gate"
import { useLockWatch } from "@/app/lock-watch"
import { LockedProvider } from "@/app/locked"
import { SettingsProvider } from "@/app/settings"
import { AppShell, type Nav, type ShellScreen } from "@/app/shell"
import { TemplatesProvider } from "@/app/templates"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary, LockReason, LockState, PersonaInfo, Status, VaultView as VaultEntry } from "@/core/types"
import { CodesView } from "@/features/codes/codes-view"
import { DevicesView } from "@/features/devices/devices-view"
import { useExpiryReminder } from "@/features/organization/expiry"
import { OrganizationNav } from "@/features/organization/organization-nav"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { QuickPalette } from "@/features/palette/quick-palette"
import { LockedWash } from "@/features/unlock/locked-wash"
import { countViews } from "@/features/vault/list-model"
import { VaultView } from "@/features/vault/vault-view"

import type { WebClipboard } from "../core/clipboard"
import type { Link } from "../core/link"
import type { WebSession, WebState } from "../core/web-client"
import { LinksView } from "../features/links-view"
import { Frame, SignInScreen } from "../features/sign-in-screen"
import { WebSettingsView } from "../features/web-settings-view"
import { WebUnlockScreen } from "../features/web-unlock-screen"

/** The screens the web vault has beside the vault's views. */
const WEB_SCREENS: ReadonlyArray<ShellScreen> = [
  ["sharing", "link", "Links"],
  ["devices", "devices", "Devices"],
  ["settings", "settings", "Settings"],
]

/** How often an open vault syncs on its own while the tab is in view. */
const SYNC_MS = 60_000

interface WebRootProps {
  session: WebSession
  /** The server the page came from (its origin). */
  server: string
  /** The worker, for the screens the desktop's client has no calls for (links). */
  link: Link
  clipboard: WebClipboard
}

/**
 * The web vault: signed out → the sign-in screen; signed in and locked →
 * the password; open → the desktop's own shell and views over the same
 * core. Nothing secret is kept in this component: `status` is cleared the
 * moment the core says it locked, whichever timer did it.
 */
function WebRoot({ session, server, link, clipboard }: WebRootProps) {
  const client = useClient()
  const [web, setWeb] = useState<WebState | null>(null)
  const [webError, setWebError] = useState<string | null>(null)
  const [status, setStatus] = useState<Status | null>(null)
  const [lock, setLock] = useState<LockState | null>(null)
  const [reason, setReason] = useState<LockReason | null>(null)
  const [nav, setNav] = useState<Nav>({ area: "vault", view: "all" })
  const [rows, setRows] = useState<ItemSummary[] | null>(null)
  const [rowsError, setRowsError] = useState<string | null>(null)
  const [personas, setPersonas] = useState<PersonaInfo[]>([])
  const [vaults, setVaults] = useState<VaultEntry[]>([])
  const [selected, setSelected] = useState<string | null>(null)
  const [palette, setPalette] = useState(false)
  const syncing = useRef(false)

  const refreshWeb = useCallback(() => {
    session
      .state()
      .then((s) => {
        setWeb(s)
        setWebError(null)
      })
      .catch((e) => setWebError(describeError(e)))
  }, [session])
  useEffect(refreshWeb, [refreshWeb])

  const onLockState = useCallback(
    (state: LockState) => {
      setLock(state)
      if (state.locked && status) {
        setReason(state.reason)
        setStatus(null)
        setRows(null)
        setPalette(false)
        refreshWeb()
      }
    },
    [status, refreshWeb],
  )
  useLockWatch(onLockState)

  const list = useCallback(() => {
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

  // A change made here goes to the server at once, then the list shows
  // what the sync left (another device's changes too).
  const syncThenList = useCallback(() => {
    list()
    if (syncing.current) return
    syncing.current = true
    client
      .sync()
      .then(list)
      .catch((e) => toast.error(`Not synced: ${describeError(e)}`, { id: "sync" }))
      .finally(() => {
        syncing.current = false
      })
  }, [client, list])

  useEffect(() => {
    if (!status) return
    list()
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") syncThenList()
    }, SYNC_MS)
    return () => window.clearInterval(timer)
  }, [status, list, syncThenList])

  const opened = useCallback(
    (s: Status) => {
      refreshWeb()
      setReason(null)
      setStatus(s)
    },
    [refreshWeb],
  )
  const forget = useCallback(() => {
    session
      .signOut()
      .then(() => {
        setStatus(null)
        setRows(null)
        refreshWeb()
      })
      .catch((e) => toast.error(describeError(e)))
  }, [session, refreshWeb])
  // What this tab changed goes to the server before the tab forgets it;
  // when it can't, the person decides.
  const signOut = useCallback(() => {
    client
      .sync()
      .then(forget)
      .catch((e) =>
        toast.error(`Not synced, so still signed in: ${describeError(e)}`, {
          action: { label: "Sign out anyway", onClick: forget },
        }),
      )
  }, [client, forget])
  const lockNow = () =>
    client
      .lock()
      .then(() => {
        setReason("requested")
        setStatus(null)
        setRows(null)
        refreshWeb()
      })
      .catch((e) => toast.error(describeError(e)))

  const notice = useCallback((text: string) => {
    toast(text)
  }, [])
  const counts = useMemo(() => countViews(rows ?? []), [rows])
  const vaultsValue = useMemo(() => ({ vaults, reload: syncThenList }), [vaults, syncThenList])
  const showExpiring = useCallback(() => setNav({ area: "vault", view: "expiring" }), [])
  const remind = useCallback((text: string, show: () => void) => {
    toast(text, { action: { label: "Show", onClick: show } })
  }, [])
  useExpiryReminder(client, status !== null, remind, showExpiring)

  if (webError) {
    return (
      <Frame>
        <h1 className="mb-4 text-lg font-semibold tracking-tight">The web vault can't start</h1>
        <p role="alert" className="text-sm text-destructive">
          {webError}
        </p>
      </Frame>
    )
  }
  if (!web) {
    return (
      <Frame>
        <Spinner />
        <p className="mt-2 text-sm text-muted-foreground">Starting the vault…</p>
      </Frame>
    )
  }
  if (!status) {
    return (
      <div className="relative h-full">
        <LockedWash />
        <div className="absolute inset-0 overflow-y-auto">
          {web.signed_in && web.locked && web.login ? (
            <WebUnlockScreen session={session} login={web.login} reason={reason} onUnlocked={opened} onSignedOut={refreshWeb} />
          ) : web.signed_in ? (
            <Frame>
              <Spinner />
            </Frame>
          ) : (
            <SignInScreen session={session} server={server} onSignedIn={opened} />
          )}
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
          screens={WEB_SCREENS}
          organization={<OrganizationNav rows={rows ?? []} nav={nav} onNavigate={setNav} />}
        >
          {nav.area === "vault" && (
            <VaultView
              view={nav.view}
              rows={rows}
              error={rowsError}
              onChanged={syncThenList}
              onNotice={notice}
              selected={selected}
              onSelect={setSelected}
              onPalette={() => setPalette(true)}
              scope={nav.scope}
            />
          )}
          {nav.area === "codes" && <CodesView rows={rows ?? []} onChanged={syncThenList} />}
          {nav.area === "sharing" && <LinksView link={link} clipboard={clipboard} />}
          {nav.area === "devices" && <DevicesView status={status} onChanged={syncThenList} onNotice={notice} />}
          {nav.area === "settings" && <WebSettingsView server={server} login={web.login ?? ""} onLock={setLock} onSignOut={signOut} />}
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

export function WebApp(props: WebRootProps) {
  return (
    <TooltipProvider>
      <SettingsProvider>
        <TemplatesProvider>
          <GateProvider>
            <WebRoot {...props} />
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
      <Toaster position="bottom-center" />
    </TooltipProvider>
  )
}
