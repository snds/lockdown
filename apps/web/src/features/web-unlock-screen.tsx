import { useState, type FormEvent } from "react"

import { Button, Field, FieldError, FieldGroup, FieldLabel, Input, Spinner } from "@unicron/ui"

import { describeError } from "@/core/client"
import type { LockReason, Status } from "@/core/types"

import type { WebSession } from "../core/web-client"
import { Frame } from "./sign-in-screen"

const REASONS: Record<LockReason, string> = {
  requested: "Locked.",
  idle: "Locked after a while without you.",
  hidden: "Locked while this tab was out of view.",
  sleep: "Locked while this computer slept.",
}

interface WebUnlockScreenProps {
  session: WebSession
  login: string
  reason: LockReason | null
  onUnlocked: (status: Status) => void
  onSignedOut: () => void
}

/**
 * The lock screen in the browser. Locking dropped every key and wiped this
 * tab's device key file; the master password opens it again through a
 * fresh OPAQUE sign-in (no Secret Key: the tab is already a device).
 */
export function WebUnlockScreen({ session, login, reason, onUnlocked, onSignedOut }: WebUnlockScreenProps) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const form = e.currentTarget
    const password = String(new FormData(form).get("password") ?? "")
    setBusy(true)
    setError(null)
    try {
      const status = await session.unlock(password)
      form.reset()
      onUnlocked(status)
    } catch (err) {
      form.reset()
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  const signOut = async () => {
    setBusy(true)
    try {
      await session.signOut()
      onSignedOut()
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Frame>
      <h1 className="mb-4 text-lg font-semibold tracking-tight">Lockdown</h1>
      <form onSubmit={submit} className="flex flex-col gap-4" aria-label="Unlock">
        <p className="text-sm text-muted-foreground">
          {reason ? REASONS[reason] : "The vault is locked."} Signed in as <span className="font-medium text-foreground">{login}</span>.
        </p>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="unlock-password">Master password</FieldLabel>
            <Input id="unlock-password" name="password" type="password" required autoFocus autoComplete="current-password" />
          </Field>
        </FieldGroup>
        {error && <FieldError role="alert">{error}</FieldError>}
        <Button type="submit" disabled={busy}>
          {busy ? <Spinner /> : null}
          Unlock
        </Button>
        <Button type="button" variant="ghost" disabled={busy} onClick={() => void signOut()}>
          Sign out of this tab
        </Button>
      </form>
    </Frame>
  )
}
