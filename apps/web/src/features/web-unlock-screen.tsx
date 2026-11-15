import { useEffect, useId, useRef, useState, type FormEvent } from "react"

import { Banner, Button, ConfirmDialog, FloatButton, useAppFrame } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { describeError } from "@/core/client"
import type { LockReason, Status } from "@/core/types"
import { ClosedHeading, lockedLine } from "@/features/unlock/closed-frame"
import { useCapsLock } from "@/features/unlock/master-password"

import type { WebSession } from "../core/web-client"
import { Frame } from "./sign-in-screen"

/** What the lock screen says when another device ended this tab's session (brief W1). */
export const SESSION_ENDED_WORDS = "This tab's session was ended from another of your devices. Unlock with your password to sign in again."

interface WebUnlockScreenProps {
  session: WebSession
  login: string
  reason: LockReason | null
  onUnlocked: (status: Status) => void
  onSignedOut: () => void
  /** Another device ended this tab's session: the sentence says so (brief W1). Additive; the app passes it. */
  sessionEnded?: boolean
}

/**
 * The lock screen in the browser (brief W1, as F1). Locking dropped every
 * key and wiped this tab's device key file; the master password opens it
 * again through a fresh OPAQUE sign-in (no Secret Key: the tab is already
 * a device). One more way out: sign this tab out.
 */
export function WebUnlockScreen({ session, login, reason, onUnlocked, onSignedOut, sessionEnded = false }: WebUnlockScreenProps) {
  const { phone } = useAppFrame()
  const formId = useId()
  const field = useRef<HTMLInputElement & HTMLTextAreaElement>(null)
  const [password, setPassword] = useState("")
  const [busy, setBusy] = useState<"unlock" | "sign-out" | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [signOutError, setSignOutError] = useState<string | null>(null)
  const [caps, onKey] = useCapsLock()
  useEffect(() => {
    field.current?.focus()
  }, [])
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (busy) return
    if (password === "") {
      setError("Type your master password.")
      field.current?.focus()
      return
    }
    const typed = password
    setPassword("")
    setBusy("unlock")
    setError(null)
    try {
      onUnlocked(await session.unlock(typed))
    } catch (err) {
      // Offline, the core's words say the server can't be reached: the web can't unlock without it.
      setError(describeError(err))
      field.current?.focus()
    } finally {
      setBusy(null)
    }
  }
  const signOut = async () => {
    setBusy("sign-out")
    setSignOutError(null)
    try {
      await session.signOut()
      onSignedOut()
    } catch (err) {
      setSignOutError(describeError(err))
    } finally {
      setBusy(null)
    }
  }
  return (
    <Frame
      end={
        phone ? (
          <FloatButton kind="primary" size="l" icon="unlocked" label="Unlock" type="submit" form={formId} aria-busy={busy === "unlock" || undefined} disabled={busy !== null} />
        ) : undefined
      }
    >
      <ClosedHeading overline={lockedLine(reason, null, login)} title="Lockdown is locked.">
        {sessionEnded ? SESSION_ENDED_WORDS : "Your vault is encrypted in this tab. Your master password opens it."}
      </ClosedHeading>
      <form id={formId} onSubmit={submit} className="flex flex-col gap-6" aria-label="Unlock">
        <div className="flex flex-col gap-3" onKeyDown={onKey} onKeyUp={onKey}>
          <SecretField
            mode="edit"
            label="Master password"
            name="password"
            mono={false}
            value={password}
            onChange={setPassword}
            autoComplete="current-password"
            inputRef={field}
            error={error ?? undefined}
          />
          {caps ? <Banner tone="info" size="compact" title="Caps Lock is on." /> : null}
        </div>
        {!phone && (
          <div>
            <Button type="submit" loading={busy === "unlock"} disabled={busy === "sign-out"}>
              Unlock
            </Button>
          </div>
        )}
        <div className="flex flex-col gap-2">
          {/* A text button hangs its word on the column's edge. */}
          <div className="-ms-4">
            <Button type="button" variant="ghost" loading={busy === "sign-out"} disabled={busy === "unlock"} onClick={() => void signOut()}>
              Sign out of this tab
            </Button>
          </div>
          {signOutError && (
            <p role="alert" className="type-caption text-critical-text">
              {signOutError}
            </p>
          )}
        </div>
      </form>
    </Frame>
  )
}

/**
 * Signing out with changes this tab hasn't synced (brief W1): a Question,
 * focus on Cancel, the reason in the server's words, and `Sign out anyway`.
 * For `web-app.tsx`'s sign-out to open in place of today's toast.
 */
export function NotSyncedQuestion({ open, reason, onSignOut, onCancel }: { open: boolean; reason: string; onSignOut: () => void; onCancel: () => void }) {
  return (
    <ConfirmDialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onCancel()
      }}
      overline="Sign out"
      title="Not synced, so still signed in."
      description={reason}
      confirmLabel="Sign out anyway"
      tone="destructive"
      onConfirm={onSignOut}
      onCancel={onCancel}
    />
  )
}
