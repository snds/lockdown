import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react"

import { Spinner, Tabs, TabsList, TabsTrigger, Toaster, TooltipProvider, toast } from "@unicron/ui"

import { MessageBoardProvider, criticalBanner, firstCritical, useNews, type CriticalSources } from "@/app/banners"
import { GateProvider, useGate } from "@/app/gate"
import { useIdleCountdown, useLockWatch } from "@/app/lock-watch"
import { notify, savedWords } from "@/app/notify"
import { StatusProvider, clockWords, useLiveWork, type LiveStatus, type StatusFact } from "@/app/status"
import { LockedProvider } from "@/app/locked"
import { WaitingHost, type NeedsYouPlace } from "@/app/needs-you"
import { SettingsProvider } from "@/app/settings"
import { navFor, sharingTabs, type SharingTab } from "@/app/places"
import { AppShell, type Nav } from "@/app/shell"
import { PageColumn } from "@/components/master-detail"
import { TemplatesProvider, useTemplates } from "@/app/templates"
import { describeError, isSignInNeeded, useClient } from "@/core/client"
import type { ItemSummary, LockReason, LockState, Status, VaultView as VaultEntry } from "@/core/types"
import { CodesView } from "@/features/codes/codes-view"
import { useExpiryReminder } from "@/features/organization/expiry"
import { useAwayReminder } from "@/features/vault/use-away-reminder"
import { VaultsProvider } from "@/features/organization/vaults-context"
import { QuickPalette } from "@/features/palette/quick-palette"
import { toFixCount } from "@/features/security/security-model"
import { SearchView } from "@/features/vault/search-view"
import { VaultView } from "@/features/vault/vault-view"
import { DepthClientProvider } from "@/features/depth/depth-client"

import type { WebClipboard } from "../core/clipboard"
import type { Link } from "../core/link"
import type { WebSession, WebState } from "../core/web-client"
import { webDepthClient } from "../core/web-depth-client"
import { webHygieneClient, webQuickClient, webScanClient } from "../core/web-finish-client"
import { PeopleView } from "../features/people-view"
import { HygieneClientProvider } from "@/features/hygiene/hygiene-client"
import { useSecretWipe, wipeWords } from "@/features/hygiene/secret-wipe"
import { QuickClientProvider } from "@/features/quick/quick-client"
import { ScanClientProvider } from "@/features/scan/scan-client"
import { LiveClientProvider } from "@/features/sessions/live-client"
import { useLive } from "@/features/sessions/use-live"
import { webLiveClient } from "../core/web-live-client"
import { webServerClient } from "../core/web-server-client"
import { ServerClientProvider } from "@/features/server/server-client"
import { LinksView } from "../features/links-view"
import { Frame, SignInScreen, WebCantStart, WebStarting } from "../features/sign-in-screen"
import { WebSettingsView } from "../features/web-settings-view"
import { NotSyncedQuestion, WebUnlockScreen } from "../features/web-unlock-screen"
import { WebSecurityView } from "../features/web-security-view"
import { webPqStatusClient } from "../core/web-pq-status-client"
import { PqStatusClientProvider } from "@/features/security/pq-status-client"
import { PaperClientProvider } from "@/features/paper/paper-client"
import { webPaperClient } from "../core/web-paper-client"

/** The line of a sync that says the server's log is split from its mirrors (D328), or null: a critical Banner until a sync without it. */
function splitLine(lines: readonly string[]): string | null {
  return lines.find((l) => l.includes("Split view")) ?? null
}

/** The web vault's Sharing tabs (brief W1): People and Links. */
const TAB_LABELS: Record<SharingTab, string> = { people: "People", vaults: "Vaults", links: "Links", emergency: "Emergency" }
const WEB_TABS = sharingTabs("web")

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
    typeof window !== "undefined" && window.location.hash === "#/devices" ? { area: "settings", view: "all", section: "devices" } : { area: "vault", view: "all" },
  )
  const [rows, setRows] = useState<ItemSummary[] | null>(null)
  const [rowsError, setRowsError] = useState<string | null>(null)
  const [vaults, setVaults] = useState<VaultEntry[]>([])
  const [selected, setSelected] = useState<string | null>(null)
  const [palette, setPalette] = useState(false)
  const syncing = useRef(false)
  // What is broken and fixable (brief X2 table 4): a split view, a sync that failed.
  const [split, setSplit] = useState<string | null>(null)
  const [syncError, setSyncError] = useState<string | null>(null)
  // When this tab last synced, for the status line's SYNCED 14:31 (brief X2 row 3, web vault).
  const [syncedAt, setSyncedAt] = useState<number | null>(null)
  // The live channel said another device ended this tab's session: the unlock screen says so (brief W1).
  const [sessionEnded, setSessionEnded] = useState(false)
  // Signing out with changes not synced: the Question's reason, in the server's words (brief W1).
  const [notSynced, setNotSynced] = useState<string | null>(null)
  const news = useNews()
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

  const { dismiss: dismissGate } = useGate()
  // Lock (brief X5): the content goes, and every layer with it, the gate and the palette included.
  const onLockState = useCallback(
    (state: LockState) => {
      setLock(state)
      if (state.locked && status) {
        dismissGate()
        setReason(state.reason)
        setStatus(null)
        setRows(null)
        setPalette(false)
        refreshWeb()
      }
    },
    [status, refreshWeb, dismissGate],
  )
  useLockWatch(onLockState)
  const idle = useIdleCountdown(status ? lock : null)
  // Typed secrets go when the tab hides or stays out of focus (docs/plan/26-desktop-finish.md §7).
  useSecretWipe(useCallback((n: number) => void notify(wipeWords(n, "tab")), []))

  const list = useCallback(() => {
    client
      .list()
      .then((r) => {
        setRows(r)
        setRowsError(null)
      })
      .catch((e) => setRowsError(describeError(e)))
    client.vaults().then(setVaults).catch(() => setVaults([]))
  }, [client])

  // Opening the vault and a sync show in the status line and the strip while they run (brief X2 row 2).
  const { work, track } = useLiveWork(status !== null && rows === null)

  // A change made here goes to the server at once, then the list shows
  // what the sync left (another device's changes too). What goes wrong is a
  // critical Banner with TRY AGAIN, not a toast (brief X2).
  const syncThenList = useCallback((): Promise<void> => {
    list()
    if (syncing.current) return Promise.resolve()
    syncing.current = true
    return track("syncing", client.sync())
      .then((outcome) => {
        setSyncError(null)
        setSplit(splitLine(outcome.lines))
        setSyncedAt(Date.now())
        list()
      })
      .catch((e) => {
        // The server wouldn't renew this tab's session: the words say why,
        // and the tab locks so unlocking signs in again with the password.
        // The tab is locked at once, so this one is a toast: no Banner would show.
        if (isSignInNeeded(e)) {
          toast.error(e.message, { id: "sync" })
          void client.lock().catch(() => undefined)
        } else setSyncError(describeError(e))
      })
      .finally(() => {
        syncing.current = false
      })
  }, [client, list, track])

  // No vault could open yet (the only one is shared and this tab hasn't
  // checked who changed it): one sync now opens it, rather than waiting for
  // the next change (docs/plan/49-shared-vault-quiet.md §6).
  const waiting = status?.waiting === true
  useEffect(() => {
    if (waiting) syncThenList()
  }, [waiting, syncThenList])

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
      setSessionEnded(false)
      setStatus(s)
      setSyncedAt(s.account?.last_sync_at ? s.account.last_sync_at * 1000 : null)
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
      .catch((e) => setNotSynced(describeError(e)))
  }, [client, forget])
  const lockNow = () =>
    client
      .lock()
      .then(() => {
        dismissGate()
        setPalette(false)
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
      setSyncError(null)
      setSplit(splitLine(n.lines))
      setSyncedAt(Date.now())
    } else if (n.kind === "sync_failed") setSyncError(n.message)
    else if (n.kind === "session_ended") {
      liveUp.current = false
      // The tab locks at once; the unlock screen says why (brief W1).
      setSessionEnded(true)
      lockNow()
    }
  })

  const notice = useCallback((text: string) => {
    notify(text)
  }, [])
  // Health counts only what the web can know (brief X1): the Needs attention list.
  const toFix = useMemo(() => toFixCount(null, rows ?? []), [rows])
  const go = useCallback((id: string) => {
    const to = navFor(id)
    if (to) setNav(to)
  }, [])
  const sync = useCallback(() => void syncThenList(), [syncThenList])
  const vaultsValue = useMemo(() => ({ vaults, reload: sync }), [vaults, sync])
  // Where the person is, for what waits on one item or on Sharing only (brief X3). The web vault has no source yet.
  const itemOpen = nav.area === "vault" || nav.area === "search" ? selected : null
  const needsYouPlace = useMemo<NeedsYouPlace>(() => ({ sharing: nav.area === "sharing", item: itemOpen }), [nav.area, itemOpen])
  const openItem = useCallback((id: string) => {
    setNav({ area: "vault", view: "all" })
    setSelected(id)
  }, [])
  // News that can wait (brief X2 table 5): one dismissable info Banner on All items, not a toast (D753).
  const { add: addNews, clear: clearNews } = news
  useEffect(() => {
    if (status === null) clearNews()
  }, [status, clearNews])
  const showExpiring = useCallback(() => go("expiring"), [go])
  const expiryNews = useCallback((n: { title: string; body?: string }, show: () => void) => addNews({ id: "expiry", ...n, action: { label: "Show", onClick: show } }), [addNews])
  useExpiryReminder(client, status !== null, expiryNews, showExpiring)
  const showAway = useCallback((itemId: string | null) => (itemId ? openItem(itemId) : go("attention")), [openItem, go])
  const awayNews = useCallback((n: { title: string; body?: string }, show: () => void) => addNews({ id: "away", ...n, action: { label: "Show", onClick: show } }), [addNews])
  useAwayReminder(client, status !== null, awayNews, showAway)
  // The web vault's critical Banners: a split view (SEE STATUS opens This server, where the mirrors are) and a failed sync.
  // A session the server won't renew locks the tab instead, and the web has no backups.
  const critical = useMemo(() => {
    const sources: CriticalSources = { signIn: null, split, sync: syncError, backup: null }
    const kind = firstCritical(sources)
    return kind ? criticalBanner(kind, sources, { signIn: () => {}, seeStatus: () => go("settings-server"), tryAgain: sync, openBackups: () => {} }) : null
  }, [split, syncError, go, sync])
  // The web vault's own line: when this tab last synced (brief X2 row 3).
  const syncedLine = useMemo<StatusFact | null>(() => (syncedAt ? { text: `Synced ${clockWords(syncedAt)}` } : null), [syncedAt])
  const live = useMemo<LiveStatus>(() => ({ idleLeft: idle.left, onStay: idle.stay, work }), [idle.left, idle.stay, work])
  const { refreshUserTemplates } = useTemplates()
  const open = status !== null
  useEffect(() => {
    if (open) refreshUserTemplates()
  }, [open, refreshUserTemplates])

  if (webError) {
    return (
      <>
        <WebCantStart server={server} reason={webError} onTryAgain={() => location.reload()} />
        <Toaster position="bottom-center" />
      </>
    )
  }
  if (!web) {
    return (
      <>
        <WebStarting />
        <Toaster position="bottom-center" />
      </>
    )
  }
  if (!status) {
    return (
      // Locked or signed out: that screen alone, nothing of the shell (Q15; changes D107's wash).
      <div className="h-full overflow-y-auto">
          {web.signed_in && web.locked && web.login ? (
            <WebUnlockScreen session={session} login={web.login} reason={reason} sessionEnded={sessionEnded} onUnlocked={opened} onSignedOut={refreshWeb} />
          ) : web.signed_in ? (
            <Frame>
              <Spinner />
            </Frame>
          ) : (
            <SignInScreen session={session} server={server} onSignedIn={opened} credentials={navigator.credentials} />
          )}
        {/* The open shell holds its own, inside the frame. */}
        <Toaster position="bottom-center" />
      </div>
    )
  }
  return (
    <LockedProvider value={lock?.locked ?? false}>
      <WaitingHost place={needsYouPlace}>
      <VaultsProvider value={vaultsValue}>
        <StatusProvider live={live} fallback={syncedLine}>
        <MessageBoardProvider critical={critical} news={news.news} onDismissNews={news.dismiss}>
        <AppShell
          surface="web"
          nav={nav}
          onNavigate={setNav}
          rows={rows}
          onLock={lockNow}
          toFix={toFix}
          onNotice={notice}
          onRefresh={sync}
          onSync={syncThenList}
          onSaved={(id, generated) => {
            setSelected(id)
            if (nav.area !== "vault" && nav.area !== "search") setNav({ area: "vault", view: "all" })
            const title = rows?.find((r) => r.id === id)?.title ?? null
            void syncThenList()
            // The toast names what was saved (brief X2); a new item's title arrives with the list, so it says "Saved".
            notify(savedWords(title, generated))
          }}
        >
          {nav.area === "vault" && (
            <VaultView
              view={nav.view}
              rows={rows}
              error={rowsError}
              onChanged={sync}
              onNotice={notice}
              selected={selected}
              onSelect={setSelected}
              onPalette={() => setPalette(true)}
              scope={nav.scope}
            />
          )}
          {nav.area === "search" && <SearchView rows={rows} selected={selected} onSelect={setSelected} onChanged={sync} onNotice={notice} />}
          {nav.area === "codes" && (
            <PageColumn>
              <CodesView rows={rows ?? []} onChanged={sync} onOpenItem={openItem} />
            </PageColumn>
          )}
          {nav.area === "sharing" && (
            <PageColumn>
              <Tabs value={nav.tab === "links" ? "links" : "people"} onValueChange={(tab) => go(`sharing-${tab}`)}>
                <TabsList variant="line" aria-label="Sharing">
                  {WEB_TABS.map((tab) => (
                    <TabsTrigger key={tab} value={tab}>
                      {TAB_LABELS[tab]}
                    </TabsTrigger>
                  ))}
                </TabsList>
              </Tabs>
              {nav.tab === "links" ? <LinksView link={link} clipboard={clipboard} /> : <PeopleView link={link} onNotice={notice} />}
            </PageColumn>
          )}
          {nav.area === "security" && (
            <PageColumn>
              <WebSecurityView onDevice={(device) => setNav({ area: "settings", view: "all", section: "devices", device })} />
            </PageColumn>
          )}
          {nav.area === "settings" && <WebSettingsView status={status} onChanged={sync} onNotice={notice} server={server} login={web.login ?? ""} onLock={setLock} onSignOut={signOut} />}
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
          onGo={go}
          onLock={lockNow}
          surface="web"
        />
        <NotSyncedQuestion
          open={notSynced !== null}
          reason={notSynced ?? ""}
          onSignOut={() => {
            setNotSynced(null)
            forget()
          }}
          onCancel={() => setNotSynced(null)}
        />
      </VaultsProvider>
      </WaitingHost>
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
  const paper = useMemo(() => webPaperClient(link), [link])
  return (
    <LiveClientProvider value={live}>
    <DepthClientProvider value={depth}>
      <QuickClientProvider value={quick}>
        <ScanClientProvider value={scan}>
          <HygieneClientProvider value={hygiene}>
            <ServerClientProvider value={serverChecks}>
              <PqStatusClientProvider value={pqStatus}>
                <PaperClientProvider value={paper}>{children}</PaperClientProvider>
              </PqStatusClientProvider>
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
