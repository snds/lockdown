import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react"

import { Spinner, Toaster, TooltipProvider } from "@unicron/ui"
import { toast } from "sonner"

import { GateProvider } from "@/app/gate"
import { useLockWatch } from "@/app/lock-watch"
import { LockedProvider } from "@/app/locked"
import { SettingsProvider } from "@/app/settings"
import { AppShell, type Nav, type ShellScreen } from "@/app/shell"
import { TemplatesProvider, useTemplates } from "@/app/templates"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary, LockReason, LockState, PersonaInfo, Status, VaultView as VaultEntry } from "@/core/types"
import { CodesView } from "@/features/codes/codes-view"
import { DevicesView } from "@/features/devices/devices-view"
import { useExpiryReminder } from "@/features/organization/expiry"
import { useAwayReminder } from "@/features/vault/use-away-reminder"
import { OrganizationNav } from "@/features/organization/organization-nav"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { QuickPalette } from "@/features/palette/quick-palette"
import { LockedWash } from "@/features/unlock/locked-wash"
import { countViews } from "@/features/vault/list-model"
import { VaultView } from "@/features/vault/vault-view"
import { DepthClientProvider } from "@/features/depth/depth-client"

import type { WebClipboard } from "../core/clipboard"
import type { Link } from "../core/link"
import type { WebSession, WebState } from "../core/web-client"
import { webDepthClient } from "../core/web-depth-client"
import { webHygieneClient, webQuickClient, webScanClient } from "../core/web-finish-client"
import { PeopleView } from "../features/people-view"
import { HygieneClientProvider } from "@/features/hygiene/hygiene-client"
import { useSecretWipe } from "@/features/hygiene/secret-wipe"
import { QuickClientProvider } from "@/features/quick/quick-client"
import { ScanClientProvider } from "@/features/scan/scan-client"
import { LiveClientProvider } from "@/features/sessions/live-client"
import { useLive } from "@/features/sessions/use-live"
import { webLiveClient } from "../core/web-live-client"
import { webServerClient } from "../core/web-server-client"
import { ServerClientProvider } from "@/features/server/server-client"
import { LinksView } from "../features/links-view"
import { Frame, SignInScreen } from "../features/sign-in-screen"
import { WebSettingsView } from "../features/web-settings-view"
import { WebUnlockScreen } from "../features/web-unlock-screen"
import { WebSecurityView } from "../features/web-security-view"
import { webPqStatusClient } from "../core/web-pq-status-client"
import { PqStatusClientProvider } from "@/features/security/pq-status-client"

/** A sync whose mirrors caught a split view says so until dismissed (docs/plan/27-web-server-finish.md §4). */
function warnOnSplit(lines: readonly string[]) {
  if (lines.some((l) => l.includes("Split view")))
    toast.error("This server's public log doesn't match a mirror: it may be showing you a different history from everyone else. See Settings.", {
      id: "kt-split",
      duration: Infinity,
    })
}

/** The screens the web vault has beside the vault's views. */
const WEB_SCREENS: ReadonlyArray<ShellScreen> = [
  ["sharing", "link", "Links"],
  ["people", "group", "People"],
  ["devices", "devices", "Devices"],
  ["security", "security", "Security"],
  ["settings", "settings", "Settings"],
]

/** How often an open vault syncs on its own while the tab is in view and
 *  the live channel is down (with it up, changes arrive by themselves). */
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
  // `#/devices` (the link in the server's alert emails) opens on Devices,
  // where every signed-in session can be ended.
  const [nav, setNav] = useState<Nav>(() =>
    typeof window !== "undefined" && window.location.hash === "#/devices" ? { area: "devices", view: "all" } : { area: "vault", view: "all" },
  )
  const [rows, setRows] = useState<ItemSummary[] | null>(null)
  const [rowsError, setRowsError] = useState<string | null>(null)
  const [personas, setPersonas] = useState<PersonaInfo[]>([])
  const [vaults, setVaults] = useState<VaultEntry[]>([])
  const [selected, setSelected] = useState<string | null>(null)
  const [palette, setPalette] = useState(false)
  const syncing = useRef(false)
  // The worker's live channel (docs/plan/27-web-server-finish.md §2) is open.
  const liveUp = useRef(false)

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
  // Typed secrets go when the tab hides or stays out of focus (docs/plan/26-desktop-finish.md §7).
  useSecretWipe(useCallback((n: number) => toast(n === 1 ? "Lockdown cleared a password you were typing while the tab was away." : `Lockdown cleared ${n} passwords you were typing while the tab was away.`), []))

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
      .then((outcome) => {
        warnOnSplit(outcome.lines)
        list()
      })
      .catch((e) => toast.error(`Not synced: ${describeError(e)}`, { id: "sync" }))
      .finally(() => {
        syncing.current = false
      })
  }, [client, list])

  useEffect(() => {
    if (!status) return
    list()
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible" && !liveUp.current) syncThenList()
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

  // The live channel: another device's change was synced in the worker
  // (every check included); redraw. A session ended elsewhere locks the tab.
  useLive((n) => {
    if (n.kind === "connected") liveUp.current = true
    else if (n.kind === "disconnected") liveUp.current = false
    if (!status) return
    if (n.kind === "synced") {
      list()
      warnOnSplit(n.lines)
    }
    else if (n.kind === "sync_failed") toast.error(`A change from another device didn't sync: ${n.message}`, { id: "sync" })
    else if (n.kind === "session_ended") {
      liveUp.current = false
      toast.error("This tab's session was ended from another of your devices. Unlock with your password to sign in again.")
      lockNow()
    }
  })

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
  const showAttention = useCallback(() => setNav({ area: "vault", view: "attention" }), [])
  useAwayReminder(client, status !== null, remind, showAttention)
  const { refreshUserTemplates } = useTemplates()
  const open = status !== null
  useEffect(() => {
    if (open) refreshUserTemplates()
  }, [open, refreshUserTemplates])

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
        {/* Every screen has one page heading (design review R18); this one only for a screen reader. */}
        <h1 className="sr-only">Lockdown web vault</h1>
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
            <SignInScreen session={session} server={server} onSignedIn={opened} credentials={navigator.credentials} />
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
          {nav.area === "people" && <PeopleView link={link} onNotice={notice} />}
          {nav.area === "devices" && <DevicesView status={status} onChanged={syncThenList} onNotice={notice} focus={nav.device} />}
          {nav.area === "security" && <WebSecurityView onDevice={(device) => setNav({ area: "devices", view: "all", device })} />}
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
            <WebDepth link={props.link}>
              <WebRoot {...props} />
            </WebDepth>
          </GateProvider>
        </TemplatesProvider>
      </SettingsProvider>
      <Toaster position="bottom-center" />
    </TooltipProvider>
  )
}

/** Opens the server's own page in a new tab: this tab's vault lives in its memory. */
function openTab(url: string) {
  if (new URL(url, window.location.origin).origin === window.location.origin) window.open(url, "_blank", "noopener")
}

/**
 * Security depth over the worker (docs/plan/22-security-depth.md). The
 * recovery reminders stay on the desktop: every unlock here is the
 * password, and the kit is tested where it was made.
 */
function WebDepth({ link, children }: { link: Link; children: ReactNode }) {
  const depth = useMemo(() => webDepthClient(link, openTab), [link])
  const quick = useMemo(() => webQuickClient(link), [link])
  const scan = useMemo(() => webScanClient(link, openActivation), [link])
  const hygiene = useMemo(() => webHygieneClient(link), [link])
  const live = useMemo(() => webLiveClient(link), [link])
  const serverChecks = useMemo(() => webServerClient(link), [link])
  const pqStatus = useMemo(() => webPqStatusClient(link), [link])
  return (
    <LiveClientProvider value={live}>
    <DepthClientProvider value={depth}>
      <QuickClientProvider value={quick}>
        <ScanClientProvider value={scan}>
          <HygieneClientProvider value={hygiene}>
            <ServerClientProvider value={serverChecks}>
              <PqStatusClientProvider value={pqStatus}>{children}</PqStatusClientProvider>
            </ServerClientProvider>
          </HygieneClientProvider>
        </ScanClientProvider>
      </QuickClientProvider>
    </DepthClientProvider>
    </LiveClientProvider>
  )
}

/** An approved activation page (https, on the signed list) in a new tab. */
function openActivation(url: string) {
  if (url.startsWith("https://")) window.open(url, "_blank", "noopener")
}
