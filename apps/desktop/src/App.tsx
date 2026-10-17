import { useCallback, useState } from "react"
import { ThemeProvider } from "next-themes"

import { Toaster, TooltipProvider } from "@unicron/ui"
import { toast } from "sonner"

import { AppShell, type Area } from "@/app/shell"
import { useLockWatch } from "@/app/lock-watch"
import { describeError, useClient } from "@/core/client"
import type { LockState, Status } from "@/core/types"
import { StatusView } from "@/features/status/status-view"
import { UnlockScreen } from "@/features/unlock/unlock-screen"
import { VaultView } from "@/features/vault/vault-view"

/**
 * Locked is the default state. `status` is set when the core opened the
 * vault and cleared the moment the core says it is locked, whichever
 * timer did it; nothing secret is ever kept here.
 */
function Root() {
  const client = useClient()
  const [status, setStatus] = useState<Status | null>(null)
  const [lock, setLock] = useState<LockState | null>(null)
  const [area, setArea] = useState<Area>("vault")

  const onLockState = useCallback((state: LockState) => {
    setLock(state)
    if (state.locked) setStatus(null)
  }, [])
  useLockWatch(onLockState)

  const lockNow = () => client.lock().then(() => setStatus(null)).catch((e) => toast.error(describeError(e)))
  const notice = (text: string) => toast(text)

  if (!status) {
    return <UnlockScreen reason={lock?.reason ?? null} onOpened={setStatus} />
  }
  return (
    <AppShell active={area} onNavigate={setArea} onLock={lockNow}>
      {area === "vault" ? (
        <VaultView onNotice={notice} />
      ) : (
        <StatusView status={status} lock={lock} onStatus={setStatus} onLock={setLock} onLockNow={lockNow} onNotice={notice} />
      )}
    </AppShell>
  )
}

export default function App() {
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem disableTransitionOnChange>
      <TooltipProvider>
        <Root />
        <Toaster position="bottom-center" />
      </TooltipProvider>
    </ThemeProvider>
  )
}
