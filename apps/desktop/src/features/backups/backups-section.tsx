import { useCallback, useEffect, useRef, useState } from "react"

import {
  Button,
  Checkbox,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldTitle,
  Icon,
  Input,
  NativeSelect,
  NativeSelectOption,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import { encodeSecret } from "@/core/secret-bytes"
import type { BackupStatus, Chosen, ImportReport } from "@/core/types"
import { whenLocal } from "@/lib/format"
import { passwordProblem } from "@/features/export/export-model"
import { ImportWizard } from "@/features/import/import-wizard"

import { INTERVALS, KEEP, crownLine, sizeWords, statusLine, whenLine } from "./backups-model"

interface BackupsSectionProps {
  onChanged: () => void
  onNotice: (text: string) => void
}

/**
 * Backups in Settings (matrix `auto-backups`): a folder the person picks
 * in the computer's own dialog, how often, how many to keep, and a backup
 * password set once. Backups are locked files written by the core while
 * Lockdown is unlocked; never plain text on disk. Each one can be restored
 * from here, or opened on any computer with the password alone.
 */
export function BackupsSection({ onChanged, onNotice }: BackupsSectionProps) {
  const client = useClient()
  const gate = useGate()
  const [status, setStatus] = useState<BackupStatus | null>(null)
  const [folder, setFolder] = useState<Chosen | null>(null)
  const [interval, setInterval_] = useState(24)
  const [keep, setKeep] = useState(10)
  const [forget, setForget] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [restore, setRestore] = useState<string | null>(null)
  const first = useRef<HTMLInputElement>(null)
  const second = useRef<HTMLInputElement>(null)

  const take = useCallback((s: BackupStatus) => {
    setStatus(s)
    setInterval_(s.interval_hours)
    setKeep(s.keep_last)
  }, [])

  useEffect(() => {
    client
      .backupStatus()
      .then(take)
      .catch((e) => setError(describeError(e)))
  }, [client, take])

  const run = async (what: () => Promise<BackupStatus>, notice: string) => {
    setBusy(true)
    setError(null)
    try {
      take(await what())
      onNotice(notice)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const pick = async () => {
    setError(null)
    try {
      const chosen = await client.backupPickFolder()
      if (chosen) setFolder(chosen)
    } catch (e) {
      setError(describeError(e))
    }
  }

  const save = async () => {
    const typed = first.current?.value ?? ""
    const needsPassword = !status?.key_set
    let password: number[] | undefined
    if (needsPassword || typed) {
      const problem = passwordProblem(typed, second.current?.value ?? "")
      if (problem) {
        setError(problem)
        return
      }
      password = encodeSecret(typed)
    }
    if (!folder && !status?.folder) {
      setError("Choose a folder first.")
      return
    }
    await run(async () => {
      try {
        return await gate.run((g) => client.backupSetup({ folder: folder?.token ?? null, password, intervalHours: interval, keepLast: keep }, g))
      } finally {
        password?.fill(0)
        if (first.current) first.current.value = ""
        if (second.current) second.current.value = ""
        setFolder(null)
      }
    }, "Backups are on; the first one is written.")
  }

  const restored = (r: ImportReport) => {
    onChanged()
    onNotice(`Restored: ${r.imported} brought in, ${r.updated} updated.`)
  }

  if (!status) {
    return error ? <p role="alert" className="text-sm text-destructive">{error}</p> : <p className="text-sm text-muted-foreground">Checking backups…</p>
  }
  const crown = crownLine(status)
  const shownFolder = folder?.path ?? status.folder

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-1 text-sm">
        <p>{statusLine(status)}</p>
        <p className="text-muted-foreground">{whenLine(status)}</p>
        {status.last_error && <p className="text-attention-text">The last try didn't work: {status.last_error}</p>}
        {crown && <p className="text-muted-foreground">{crown}</p>}
      </div>

      <FieldGroup>
        <Field>
          <FieldTitle>Folder</FieldTitle>
          <div className="flex flex-wrap items-center gap-2">
            <Button size="sm" variant="outline" onClick={() => void pick()} disabled={busy}>
              <Icon name="folder_open" data-icon="inline-start" />
              Choose folder…
            </Button>
            <span className="text-sm text-muted-foreground">{shownFolder ?? "None chosen yet"}</span>
          </div>
          <FieldDescription>A folder on this computer, an external drive, or a folder your cloud drive syncs.</FieldDescription>
        </Field>
        <Field orientation="horizontal">
          <FieldLabel htmlFor="backup-interval">How often</FieldLabel>
          <NativeSelect id="backup-interval" value={String(interval)} onChange={(e) => setInterval_(Number(e.target.value))} disabled={busy}>
            {INTERVALS.map(([label, hours]) => (
              <NativeSelectOption key={hours} value={String(hours)}>
                {label}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </Field>
        <Field orientation="horizontal">
          <FieldLabel htmlFor="backup-keep">Backups to keep</FieldLabel>
          <NativeSelect id="backup-keep" value={String(keep)} onChange={(e) => setKeep(Number(e.target.value))} disabled={busy}>
            {KEEP.map((n) => (
              <NativeSelectOption key={n} value={String(n)}>
                The newest {n}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </Field>
        <Field>
          <FieldLabel htmlFor="backup-password">{status.key_set ? "A new backup password (leave empty to keep the one you have)" : "Backup password"}</FieldLabel>
          <Input id="backup-password" ref={first} type="password" autoComplete="new-password" />
          <FieldDescription>
            Every backup is locked with it. You'll need it to restore on another computer, so write it down somewhere safe. Changing it affects new backups
            only.
          </FieldDescription>
        </Field>
        <Field>
          <FieldLabel htmlFor="backup-password-again">The same password again</FieldLabel>
          <Input id="backup-password-again" ref={second} type="password" autoComplete="new-password" />
        </Field>
      </FieldGroup>
      {error && <FieldError role="alert">{error}</FieldError>}
      <div className="flex flex-wrap gap-2">
        <Button size="sm" onClick={() => void save()} disabled={busy}>
          {status.enabled ? "Save changes" : "Turn on backups"}
        </Button>
        {status.enabled && (
          <Button size="sm" variant="outline" onClick={() => void run(() => client.backupNow(), "Backup written.")} disabled={busy}>
            Back up now
          </Button>
        )}
      </div>

      {status.enabled && (
        <div className="flex flex-wrap items-center gap-3">
          <Field orientation="horizontal">
            <Checkbox id="backup-forget" checked={forget} onCheckedChange={(v) => setForget(v === true)} />
            <FieldLabel htmlFor="backup-forget">Also forget the backup password on this computer</FieldLabel>
          </Field>
          <Button size="sm" variant="ghost-destructive" onClick={() => void run(() => client.backupOff(forget), "Backups are off. The files you have stay where they are.")} disabled={busy}>
            Turn off backups
          </Button>
        </div>
      )}

      {status.backups.length > 0 && (
        <section aria-label="Your backups" className="flex flex-col gap-2">
          <h3 className="text-sm font-medium">In the folder</h3>
          <ul className="flex flex-col gap-1">
            {status.backups.map((b) => (
              <li key={b.name} className="flex flex-wrap items-center justify-between gap-2 text-sm">
                <span>
                  {whenLocal(b.made_at)} · {sizeWords(b.bytes)}
                </span>
                <Button size="sm" variant="outline" onClick={() => setRestore(b.name)}>
                  Restore…
                </Button>
              </li>
            ))}
          </ul>
        </section>
      )}

      <ImportWizard open={restore !== null} restore={restore} onClose={() => setRestore(null)} onImported={restored} />
    </div>
  )
}
