import { useCallback, useEffect, useState } from "react"
import { listen } from "@tauri-apps/api/event"
import { ThemeProvider } from "next-themes"

import { Button, Icon, Toaster, TooltipProvider, toast } from "@unicron/ui"

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
 * offers Unlock and nothing else. It reports nothing to the lock timers:
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

  if (locked === null) return null
  if (locked) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        <Icon name="locked" />
        <p className="text-sm">Lockdown is locked.</p>
        <Button autoFocus onClick={() => void unlock()}>
          Unlock
        </Button>
        <Button variant="ghost" size="sm" onClick={() => void quick.hide()}>
          Not now
        </Button>
      </div>
    )
  }
  return <QuickPalette key={shown} inline open onOpenChange={() => {}} rows={rows} onOpen={() => {}} onNotice={(t) => toast(t)} />
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
                  {/* The window's one landmark and heading (review R18); the heading is the
                      window's own title (quick.rs), for a screen reader only. */}
                  <main className="h-full bg-background">
                    <h1 className="sr-only">Lockdown quick access</h1>
                    <QuickRoot />
                  </main>
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
