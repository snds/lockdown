import { useEffect, useState, type FormEvent } from "react"

import {
  Button,
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  Icon,
  Input,
  NativeSelect,
  NativeSelectOption,
  Switch,
  ToggleGroup,
  ToggleGroupItem,
} from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { PaneHeader } from "@/components/master-detail"
import { describeError, useClient } from "@/core/client"
import type { ApprovalRoute, Density, LockState, RevealGesture, Status } from "@/core/types"
import { BackupsSection } from "@/features/backups/backups-section"
import { AliasSettings } from "@/features/bridges/alias-settings"
import { ClockSetting } from "@/features/bridges/clock"
import { ExportDialog } from "@/features/export/export-dialog"
import { ImportWizard } from "@/features/import/import-wizard"
import { BrowsersSection } from "@/features/browsers/browsers-section"
import { QuickSettings } from "@/features/quick/quick-settings"

interface SettingsViewProps {
  status: Status
  lock: LockState | null
  onLock: (state: LockState) => void
  onChanged: () => void
  onNotice: (text: string) => void
}

const IDLE_CHOICES: Array<[string, number | null]> = [
  ["1 minute", 60],
  ["5 minutes", 300],
  ["10 minutes", 600],
  ["30 minutes", 1800],
  ["1 hour", 3600],
  ["4 hours", 14_400],
  ["Never", null],
]

const CLIPBOARD_CHOICES = [10, 20, 30, 45, 60, 90]

/**
 * Settings (brief §3.2): lock timing, clipboard timer, density, reveal
 * gesture, server, approvals, and the import/export entry. Everything
 * here is written to `settings.cbor` through the core (D100), which also
 * applies the lock timers; this screen only edits them.
 */
export function SettingsView({ status, lock, onLock, onChanged, onNotice }: SettingsViewProps) {
  const client = useClient()
  const { settings, update } = useSettings()
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [transfer, setTransfer] = useState<"import" | "export" | null>(null)
  const [approvals, setApprovals] = useState<ApprovalRoute | null>(null)

  useEffect(() => {
    client.approvalRoute("use").then(setApprovals).catch(() => setApprovals(null))
  }, [client, status.account])

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

  const saveServer = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const value = String(new FormData(e.currentTarget).get("server") ?? "").trim()
    void save({ server: value || null }).then(() => onNotice(value ? "Server saved." : "Server cleared."))
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
              value={settings.idle_lock_secs === null ? "" : String(settings.idle_lock_secs)}
              onChange={(e) => void save({ idle_lock_secs: e.target.value === "" ? null : Number(e.target.value) })}
              disabled={busy}
            >
              {IDLE_CHOICES.map(([label, secs]) => (
                <NativeSelectOption key={label} value={secs === null ? "" : String(secs)}>
                  {label}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </Field>
          <Field orientation="horizontal">
            <FieldLabel htmlFor="hidden-lock">Lock when the window is out of view for 5 minutes</FieldLabel>
            <Switch id="hidden-lock" checked={settings.hidden_lock_secs !== null} onCheckedChange={(on) => void save({ hidden_lock_secs: on ? 300 : null })} disabled={busy} />
          </Field>
          <FieldDescription>
            The vault also locks when this computer wakes from sleep. The core enforces every timer; this screen only sets them.
            {lock?.idle_timeout_secs !== undefined && lock?.idle_timeout_secs !== null ? ` In force now: ${Math.round(lock.idle_timeout_secs / 60)} minutes idle.` : ""}
          </FieldDescription>
        </FieldGroup>
      </Section>

      <Section title="Clipboard">
        <FieldGroup>
          <Field orientation="horizontal">
            <FieldLabel htmlFor="clipboard-timer">Clear a copied secret after</FieldLabel>
            <NativeSelect id="clipboard-timer" value={String(settings.clipboard_clear_secs)} onChange={(e) => void save({ clipboard_clear_secs: Number(e.target.value) })} disabled={busy}>
              {CLIPBOARD_CHOICES.map((secs) => (
                <NativeSelectOption key={secs} value={String(secs)}>
                  {secs} seconds
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </Field>
          <FieldDescription>Cleared only if the clipboard still holds what Lockdown copied, so your own later copy is never destroyed.</FieldDescription>
        </FieldGroup>
      </Section>

      <Section title="Appearance">
        <FieldGroup>
          <Field orientation="horizontal">
            <FieldLabel id="density-label">Density</FieldLabel>
            <ToggleGroup aria-labelledby="density-label" value={[settings.density]} onValueChange={(v) => v[0] && void save({ density: v[0] as Density })} disabled={busy}>
              <ToggleGroupItem value="comfortable">Comfortable</ToggleGroupItem>
              <ToggleGroupItem value="compact">Compact</ToggleGroupItem>
            </ToggleGroup>
          </Field>
          <FieldDescription>Comfortable rows are 40 px; compact rows are 32 px.</FieldDescription>
        </FieldGroup>
      </Section>

      <Section title="Reveal">
        <FieldGroup>
          <Field orientation="horizontal">
            <FieldLabel id="reveal-label">Showing a hidden value</FieldLabel>
            <ToggleGroup aria-labelledby="reveal-label" value={[settings.reveal]} onValueChange={(v) => v[0] && void save({ reveal: v[0] as RevealGesture })} disabled={busy}>
              <ToggleGroupItem value="hold">Hold to reveal</ToggleGroupItem>
              <ToggleGroupItem value="toggle">Tap to reveal</ToggleGroupItem>
            </ToggleGroup>
          </Field>
          <FieldDescription>
            Hold shows a value only while you press; release hides it. Tap shows it until you tap again or 30 seconds pass, for anyone who cannot hold a
            pointer still.
          </FieldDescription>
        </FieldGroup>
      </Section>

      <Section title="Approvals">
        <p className="text-sm text-muted-foreground">
          {approvals === null
            ? "Checking what this computer offers…"
            : approvals.route === "platform"
              ? `Using or approving protected things is confirmed with ${approvals.method}. Revealing one asks for your account password.`
              : approvals.route === "password"
                ? "This computer has no built-in check, so protected things ask for your account password."
                : "No account yet and no built-in check on this computer: protected things ask you to confirm in this window. Register an account to protect them with a password."}
        </p>
      </Section>

      <Section title="Server">
        <form onSubmit={saveServer} className="flex flex-col gap-3">
          <Field>
            <FieldLabel htmlFor="default-server">Server to suggest</FieldLabel>
            <Input id="default-server" name="server" type="url" placeholder="https://" defaultValue={settings.server ?? ""} autoComplete="off" />
            <FieldDescription>Filled in on the join, register and log-in forms. Your own server is a first-class choice.</FieldDescription>
          </Field>
          <div>
            <Button type="submit" size="sm" variant="outline" disabled={busy}>
              Save server
            </Button>
          </div>
        </form>
      </Section>

      <Section title="Import and export">
        <p className="text-sm text-muted-foreground">
          Bring your data in from another app, or take all of it out as one file. Lockdown shows what a file holds, and what it leaves out, before
          anything happens.
        </p>
        <div className="flex gap-2">
          <Button size="sm" variant="outline" onClick={() => setTransfer("import")}>
            <Icon name="upload" data-icon="inline-start" />
            Import…
          </Button>
          <Button size="sm" variant="outline" onClick={() => setTransfer("export")}>
            <Icon name="download" data-icon="inline-start" />
            Export…
          </Button>
        </div>
      </Section>

      <Section title="Backups">
        <BackupsSection onChanged={onChanged} onNotice={onNotice} />
      </Section>

      <BrowsersSection onNotice={onNotice} />

      <QuickSettings onNotice={onNotice} />

      <AliasSettings onNotice={onNotice} onChanged={onChanged} />

      <ClockSetting />

      <ImportWizard
        open={transfer === "import"}
        onClose={() => setTransfer(null)}
        onImported={(report) => {
          onNotice(`${report.imported} ${report.imported === 1 ? "item" : "items"} brought in.`)
          onChanged()
        }}
      />
      <ExportDialog open={transfer === "export"} onClose={() => setTransfer(null)} onNotice={onNotice} />
    </div>
  )
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-3 rounded-xl border bg-card p-4">
      <h2 className="font-medium">{title}</h2>
      {children}
    </section>
  )
}
