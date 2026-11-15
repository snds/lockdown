import { useCallback, useEffect, useState, type ReactNode } from "react"
import { listen } from "@tauri-apps/api/event"
import { ThemeProvider } from "next-themes"

import { AppFrameFocus, AppLockup, Button, Toaster, TooltipProvider, toast } from "@unicron/ui"

import { GateProvider } from "@/app/gate"
import { SettingsProvider } from "@/app/settings"
import { TemplatesProvider } from "@/app/templates"
import { tauriHygieneClient, tauriQuickClient } from "@/app/tauri-finish"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary } from "@/core/types"
import { HygieneClientProvider } from "@/features/hygiene/hygiene-client"
import { QuickPalette } from "@/features/palette/quick-palette"
import { QuickClientProvider, useQuickClient } from "@/features/quick/quick-client"

/** How often the open window asks whether the vault locked, as the main window's lock watch does. */
const LOCK_POLL_MS = 3000

/**
 * The quick-access window the system-wide shortcut opens
 * (docs/plan/26-desktop-finish.md §2): only the palette, its search in
 * focus each time it shows; Escape or an action hides it. Locked, it
 * draws the focus frame's mark, "Lockdown is locked.", UNLOCK and NOT NOW
 * (brief X4), and nothing of the vault. GO TO offers Lock alone here: the
 * window has a way to open an item in the main window, not a place. It reports nothing to the lock timers:
 * the main window's state is the app's.
 */
function QuickRoot() {
  const client = useClient()
  const quick = useQuickClient()
  const [locked, setLocked] = useState<boolean | null>(null)
  const [rows, setRows] = useState<ItemSummary[]>([])
  const [shown, setShown] = useState(0)

  const refresh = useCallback(() => {
    client
      .lockState()
      .then((s) => {
        setLocked(s.locked)
        if (!s.locked) client.list().then(setRows).catch(() => setRows([]))
      })
      .catch(() => setLocked(true))
  }, [client])

  useEffect(() => {
    refresh()
    const off = listen("lockdown://quick-shown", () => {
      setShown((n) => n + 1)
      refresh()
    })
    return () => {
      void off.then((f) => f())
    }
  }, [refresh])

  // A lock while the window is open closes it (brief X5, Lock): the palette never stays over a locked vault.
  useEffect(() => {
    if (locked !== false) return
    const t = window.setInterval(() => {
      client
        .lockState()
        .then((s) => {
          if (!s.locked) return
          setLocked(true)
          setRows([])
          void quick.hide()
        })
        .catch(() => {})
    }, LOCK_POLL_MS)
    return () => window.clearInterval(t)
  }, [locked, client, quick])

  const unlock = () =>
    client
      .unlock()
      .then(refresh)
      .catch((e) => toast.error(describeError(e)))

  if (locked === null) return <Window />
  if (locked) {
    // Locked (brief X4): the focus frame's mark and one sentence, UNLOCK and NOT NOW. Nothing of the vault.
    return (
      <div className="h-full bg-background">
      <AppFrameFocus lockup={<AppLockup name="Lockdown" showName={false} />}>
        <Heading />
        <div className="flex flex-col gap-6">
          <p className="type-title text-foreground">Lockdown is locked.</p>
          <div className="flex flex-wrap items-center gap-3">
            <Button autoFocus onClick={() => void unlock()}>
              Unlock
            </Button>
            <Button variant="ghost" onClick={() => void quick.hide()}>
              Not now
            </Button>
          </div>
        </div>
      </AppFrameFocus>
      </div>
    )
  }
  return (
    <Window>
      <QuickPalette key={shown} inline open onOpenChange={() => {}} rows={rows} onOpen={() => {}} onNotice={(t) => toast(t)} />
    </Window>
  )
}

/** The window's one heading: its own title (quick.rs), for a screen reader only (review R18). */
function Heading() {
  return <h1 className="sr-only">Lockdown quick access</h1>
}

/** The window's one landmark, while it is not locked (the focus frame is the landmark then). */
function Window({ children }: { children?: ReactNode }) {
  return (
    <main className="h-full bg-background">
      <Heading />
      {children}
    </main>
  )
}

export function QuickWindow() {
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem disableTransitionOnChange>
      <TooltipProvider>
        <SettingsProvider>
          <TemplatesProvider>
            <GateProvider>
              <QuickClientProvider value={tauriQuickClient}>
                <HygieneClientProvider value={tauriHygieneClient}>
                  <QuickRoot />
                </HygieneClientProvider>
              </QuickClientProvider>
            </GateProvider>
          </TemplatesProvider>
        </SettingsProvider>
        <Toaster position="bottom-center" />
      </TooltipProvider>
    </ThemeProvider>
  )
}
