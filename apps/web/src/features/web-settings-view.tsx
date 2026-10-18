import { useState, type ReactNode } from "react"

import {
  Button,
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  NativeSelect,
  NativeSelectOption,
  Switch,
  ToggleGroup,
  ToggleGroupItem,
} from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { PaneHeader } from "@/components/master-detail"
import { describeError, useClient } from "@/core/client"
import type { Density, LockState, RevealGesture } from "@/core/types"
import { ServerChecks } from "@/features/server/server-checks"
import { toast } from "sonner"

const IDLE_CHOICES: Array<[string, number | null]> = [
  ["1 minute", 60],
  ["5 minutes", 300],
  ["10 minutes", 600],
  ["30 minutes", 1800],
  ["1 hour", 3600],
  ["4 hours", 14_400],
]

const CLIPBOARD_CHOICES = [10, 20, 30, 45, 60, 90]

interface WebSettingsViewProps {
  server: string
  login: string
  onLock: (state: LockState) => void
  onSignOut: () => void
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold">{title}</h2>
      {children}
    </section>
  )
}

/**
 * The web vault's settings: the lock timers (the core's, as on the
 * desktop), the clipboard timer, density and the reveal gesture, and this
 * tab's place: where it signed in and how to sign out. They hold for this
 * tab; a new sign-in starts from the defaults. Idle has no "never" here:
 * a browser tab is too easy to walk away from.
 */
export function WebSettingsView({ server, login, onLock, onSignOut }: WebSettingsViewProps) {
  const client = useClient()
  const { settings, update } = useSettings()
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const save = async (change: Parameters<typeof update>[0]) => {
    setBusy(true)
    setError(null)
    try {
      await update(change)
      onLock(await client.lockState())
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-6 p-4 md:p-6">
      <PaneHeader title="Settings" />
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}

      <Section title="Lock">
        <FieldGroup>
          <Field orientation="horizontal">
            <FieldLabel htmlFor="idle-timeout">Lock after idle</FieldLabel>
            <NativeSelect
              id="idle-timeout"
              value={String(settings.idle_lock_secs ?? 600)}
              onChange={(e) => void save({ idle_lock_secs: Number(e.target.value) })}
              disabled={busy}
            >
              {IDLE_CHOICES.map(([label, secs]) => (
                <NativeSelectOption key={label} value={String(secs)}>
                  {label}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </Field>
          <Field orientation="horizontal">
            <FieldLabel htmlFor="hidden-lock">Lock when this tab is out of view for 5 minutes</FieldLabel>
            <Switch
              id="hidden-lock"
              checked={settings.hidden_lock_secs !== null}
              onCheckedChange={(on) => void save({ hidden_lock_secs: on ? 300 : null })}
              disabled={busy}
            />
          </Field>
          <FieldDescription>Unlocking asks for the account password. Closing the tab ends everything at once.</FieldDescription>
        </FieldGroup>
      </Section>

      <Section title="Clipboard">
        <FieldGroup>
          <Field orientation="horizontal">
            <FieldLabel htmlFor="clipboard-timer">Clear a copied secret after</FieldLabel>
            <NativeSelect
              id="clipboard-timer"
              value={String(settings.clipboard_clear_secs)}
              onChange={(e) => void save({ clipboard_clear_secs: Number(e.target.value) })}
              disabled={busy}
            >
              {CLIPBOARD_CHOICES.map((secs) => (
                <NativeSelectOption key={secs} value={String(secs)}>
                  {secs} seconds
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </Field>
          <FieldDescription>
            In a browser the copied value passes through this page. It is cleared after the timer if the clipboard still holds it, or when the browser
            won't let the page look, unless you copied something from Lockdown since.
          </FieldDescription>
        </FieldGroup>
      </Section>

      <Section title="Appearance">
        <Field orientation="horizontal">
          <FieldLabel id="density-label">Density</FieldLabel>
          <ToggleGroup aria-labelledby="density-label" value={[settings.density]} onValueChange={(v) => v[0] && void save({ density: v[0] as Density })} disabled={busy}>
            <ToggleGroupItem value="comfortable">Comfortable</ToggleGroupItem>
            <ToggleGroupItem value="compact">Compact</ToggleGroupItem>
          </ToggleGroup>
        </Field>
        <Field orientation="horizontal">
          <FieldLabel id="reveal-label">Showing a hidden value</FieldLabel>
          <ToggleGroup aria-labelledby="reveal-label" value={[settings.reveal]} onValueChange={(v) => v[0] && void save({ reveal: v[0] as RevealGesture })} disabled={busy}>
            <ToggleGroupItem value="hold">Hold to reveal</ToggleGroupItem>
            <ToggleGroupItem value="toggle">Tap to reveal</ToggleGroupItem>
          </ToggleGroup>
        </Field>
      </Section>

      <ServerChecks onNotice={(text) => toast(text)} framed={false} />

      <Section title="This web vault">
        <p className="text-sm text-muted-foreground">
          Signed in as <span className="font-medium text-foreground">{login}</span> at <span className="font-mono">{new URL(server).host}</span>. The page's code
          comes from that server too, so it can only be as trustworthy as the server that sends it; the desktop app's code comes signed from Lockdown. For
          crown jewels and recovery, prefer the desktop app.
        </p>
        <p className="text-sm text-muted-foreground">
          This tab is one of your devices. Signing out forgets its key; another device can then remove it from the list.
        </p>
        <div>
          <Button variant="outline" onClick={onSignOut} disabled={busy}>
            Sign out of this tab
          </Button>
        </div>
      </Section>
    </div>
  )
}
