import { useCallback, useEffect, useMemo, useState } from "react"

import {
  AdaptiveDialog,
  Banner,
  Button,
  Checkbox,
  ConfirmDialog,
  EmptyState,
  Field,
  FieldLabel,
  FieldRow,
  FieldRowAction,
  FieldRowValue,
  FloatButton,
  ListRow,
  SectionHeader,
  Skeleton,
  ToggleRow,
  useAppFrame,
} from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { useDetailBottomRow } from "@/app/detail-bottom-row"
import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import { ClockTime } from "@/components/clock-time"
import { encodeSecret } from "@/core/secret-bytes"
import type { BackupFile, BackupStatus, Chosen, ImportReport } from "@/core/types"
import { MIN_PASSWORD, passwordProblem } from "@/features/export/export-model"
import { madeAtWords } from "@/features/import/import-model"
import { ImportWizard } from "@/features/import/import-wizard"
import { ChangeSetting, SaveError, useRowSize } from "@/features/settings/setting-rows"
import { count } from "@/lib/format"

import {
  INTERVAL_CHOICES,
  KEEP_CHOICES,
  crownLine,
  crownSwitchHelp,
  crownSwitchNotice,
  includesCrownJewels,
  lastBackupLine,
  nextLine,
  sizeWords,
  waitingLine,
} from "./backups-model"

interface BackupsSectionProps {
  onChanged: () => void
  onNotice: (text: string) => void
}

/** The rows that save themselves once backups are on; each has its own critical line. */
type Row = "backup" | "folder" | "interval" | "keep" | "crown" | "password" | "off"

/**
 * Settings → Backups (brief T8, doc 83; matrix `auto-backups`). Backups are
 * locked files the core writes into a folder the person picks in the
 * computer's own dialog, on a schedule, while Lockdown is unlocked; never
 * plain text on disk. Each can be restored from here, or opened on any
 * computer with the backup password alone.
 *
 * - **Off:** a short form (folder, how often, keep, the password twice) and
 *   the section's one primary, TURN ON BACKUPS (gate). On a phone it ends the
 *   detail bottom row as an extended FloatButton.
 * - **On:** rows that save by themselves: BACKUPS with BACK UP NOW, LAST
 *   BACKUP, "Include crown jewels", FOLDER, HOW OFTEN, KEEP, BACKUP
 *   PASSWORD, and the files IN THE FOLDER with RESTORE…; at the foot TURN OFF
 *   BACKUPS, which asks, with "Also forget the backup password" as its child.
 * - **The last try failed:** the shell's critical Banner (brief X2 table 4,
 *   D754) at the top, here with BACK UP NOW.
 *
 * "Include crown jewels" (D138 as decided, doc 48; still Sean's open
 * question) is on by default, as today: locked backups and a locked
 * Lockdown export carry every crown jewel, under the backup's password. It
 * changes at once, behind the `Reveal` gate both ways; off brings back the
 * per-item opt-in and the count.
 */
export function BackupsSection({ onChanged, onNotice }: BackupsSectionProps) {
  const client = useClient()
  const gate = useGate()
  const size = useRowSize()
  const { phone } = useAppFrame()
  const [status, setStatus] = useState<BackupStatus | null>(null)
  const [readError, setReadError] = useState<string | null>(null)
  const [busy, setBusy] = useState<Row | null>(null)
  const [errors, setErrors] = useState<Partial<Record<Row, string>>>({})
  const [restore, setRestore] = useState<BackupFile | null>(null)
  // The form while backups are off.
  const [folder, setFolder] = useState<Chosen | null>(null)
  const [interval, setInterval_] = useState(24)
  const [keep, setKeep] = useState(10)
  // The two password boxes (the off form, and CHANGE… once on). Controlled (unicron gap: SecretField's edit mode); cleared after every try.
  const [first, setFirst] = useState("")
  const [second, setSecond] = useState("")
  const [passwordError, setPasswordError] = useState<{ field: "first" | "second"; text: string } | null>(null)
  const [changingPassword, setChangingPassword] = useState(false)
  const [turningOff, setTurningOff] = useState(false)
  const [forget, setForget] = useState(false)

  const take = useCallback((s: BackupStatus) => {
    setStatus(s)
    setInterval_(s.interval_hours)
    setKeep(s.keep_last)
  }, [])

  const read = useCallback(() => {
    setReadError(null)
    client
      .backupStatus()
      .then(take)
      .catch((e) => setReadError(describeError(e)))
  }, [client, take])
  useEffect(read, [read])

  const clearPasswords = () => {
    setFirst("")
    setSecond("")
    setPasswordError(null)
  }

  /** Runs one change; a failure is one critical line by its row, in the core's words. */
  const run = async (row: Row, what: () => Promise<BackupStatus>, notice: string | null): Promise<boolean> => {
    setBusy(row)
    setErrors(({ [row]: _gone, ...rest }) => rest)
    try {
      take(await what())
      if (notice) onNotice(notice)
      return true
    } catch (e) {
      setErrors((all) => ({ ...all, [row]: describeError(e) }))
      return false
    } finally {
      setBusy(null)
    }
  }

  /** The typed password as bytes, or `undefined` with the problem said under its box. */
  const typedPassword = (): number[] | undefined => {
    const problem = passwordProblem(first, second)
    if (problem) {
      setPasswordError({ field: first.length < MIN_PASSWORD ? "first" : "second", text: problem })
      return undefined
    }
    setPasswordError(null)
    return encodeSecret(first)
  }

  /** Saves one setting while backups are on (silent on success). */
  const saveSetting = (row: Row, change: { folder?: number | null; intervalHours?: number; keepLast?: number; password?: number[] }) =>
    run(
      row,
      async () => {
        try {
          return await gate.run((g) =>
            client.backupSetup(
              { folder: change.folder ?? null, password: change.password, intervalHours: change.intervalHours ?? interval, keepLast: change.keepLast ?? keep },
              g,
            ),
          )
        } finally {
          change.password?.fill(0)
        }
      },
      null,
    )

  const pick = async (row: Row): Promise<Chosen | null> => {
    setErrors(({ [row]: _gone, ...rest }) => rest)
    try {
      return await client.backupPickFolder()
    } catch (e) {
      setErrors((all) => ({ ...all, [row]: describeError(e) }))
      return null
    }
  }

  const turnOn = async () => {
    if (!folder) return
    // A password kept from before may stay; a new one is checked like any other.
    const keepOld = status?.key_set === true && !first && !second
    const password = keepOld ? undefined : typedPassword()
    if (!keepOld && !password) return
    const ok = await run(
      "backup",
      async () => {
        try {
          return await gate.run((g) => client.backupSetup({ folder: folder.token, password, intervalHours: interval, keepLast: keep }, g))
        } finally {
          password?.fill(0)
        }
      },
      "Backups turned on",
    )
    clearPasswords()
    if (ok) setFolder(null)
  }

  const off = status === null || !status.enabled
  const turning = busy === "backup"
  const missing = folder ? null : "Choose a folder first."
  // On a phone the section's one primary ends the detail bottom row (brief T8, #2).
  const rowEnd = useMemo(
    () =>
      status !== null && off && phone ? (
        <FloatButton label="Turn on backups" icon="data-backup" kind="primary" size="l" extended disabled={turning || missing !== null} onClick={() => void turnOn()} />
      ) : undefined,
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `turnOn` reads the same state these name.
    [status, off, phone, turning, missing, folder, first, second, interval, keep],
  )
  const inRow = useDetailBottomRow({ end: rowEnd })

  const restored = (r: ImportReport) => {
    onChanged()
    onNotice(`Restored: ${r.imported} brought in, ${r.updated} updated`)
  }

  if (!status) {
    if (readError) return <EmptyState tone="critical" overline={`Backups // ${count(0)}`} title="Couldn't read your backups" body={readError} actionLabel="Try again" onAction={read} />
    return (
      <div role="status" aria-label="Checking backups" className="flex flex-col gap-1">
        <Skeleton variant="row" />
        <Skeleton variant="row" />
        <Skeleton variant="row" />
      </div>
    )
  }

  const crown = crownLine(status)
  const waiting = waitingLine(status)
  const includeCrown = includesCrownJewels(status)
  const next = nextLine(status)
  const backUpNow = (variant: "secondary" | "ghost") => (
    <Button size="sm" variant={variant} loading={busy === "backup"} disabled={busy !== null && busy !== "backup"} onClick={() => void run("backup", () => client.backupNow(), "Backed up")}>
      Back up now
    </Button>
  )

  const restoreWizard = (
    <ImportWizard
      open={restore !== null}
      restore={restore?.name ?? null}
      restoreMadeAt={restore?.made_at ?? null}
      onClose={() => setRestore(null)}
      onImported={restored}
    />
  )

  const passwordBoxes = (
    <div className="flex flex-col gap-6">
      <SecretField
        mode="edit"
        id="backup-password"
        label="Backup password"
        name="backup-password"
        value={first}
        onChange={setFirst}
        disabled={busy !== null}
        error={passwordError?.field === "first" ? passwordError.text : undefined}
        description="You'll need it to restore on another computer, so keep it somewhere safe. Every backup is locked with it."
      />
      <SecretField
        mode="edit"
        id="backup-password-again"
        label="The same password again"
        name="backup-password-again"
        value={second}
        onChange={setSecond}
        disabled={busy !== null}
        error={passwordError?.field === "second" ? passwordError.text : undefined}
      />
    </div>
  )

  if (off) {
    return (
      <div className="flex flex-col gap-8">
        <p className="type-body text-text-secondary">
          Backups are off. Choose a folder, how often and a password, and Lockdown keeps locked copies of your vault there.
        </p>
        <div className="flex flex-col">
          <FieldRow
            size={size}
            label="Folder"
            description="On this computer, an external drive, or a folder your cloud drive syncs."
            action={
              <FieldRowAction
                disabled={turning}
                onClick={() =>
                  void pick("folder").then((chosen) => {
                    if (chosen) setFolder(chosen)
                  })
                }
              >
                Choose…
              </FieldRowAction>
            }
          >
            <FieldRowValue truncate="middle">{folder?.path ?? "None chosen yet"}</FieldRowValue>
          </FieldRow>
          <SaveError text={errors.folder ?? null} />
          <ChangeSetting label="How often" choices={INTERVAL_CHOICES} current={String(interval)} onPick={(k) => setInterval_(Number(k))} busy={turning} />
          <ChangeSetting label="Keep" choices={KEEP_CHOICES} current={String(keep)} onPick={(k) => setKeep(Number(k))} busy={turning} />
        </div>
        {status.key_set ? (
          <p className="type-caption text-text-secondary">Your backup password from before is kept on this computer.</p>
        ) : (
          passwordBoxes
        )}
        {errors.backup && <Banner tone="critical" size="compact" title={errors.backup} />}
        {/* The bottom row's yes has no room for its reason: it is said here (#37). */}
        {inRow && missing && <p className="type-caption text-text-secondary">{missing}</p>}
        {!inRow && (
          <div>
            <Button onClick={() => void turnOn()} loading={turning} disabled={missing !== null} disabledReason={turning ? undefined : (missing ?? undefined)}>
              Turn on backups
            </Button>
          </div>
        )}
        {restoreWizard}
      </div>
    )
  }

  const changePassword = async () => {
    const password = typedPassword()
    if (!password) return
    const ok = await saveSetting("password", { password })
    clearPasswords()
    if (ok) setChangingPassword(false)
  }

  return (
    <div className="flex flex-col gap-8">
      {status.last_error && <Banner tone="critical" title="The last backup didn't work" body={status.last_error} action={backUpNow("secondary")} />}

      <div className="flex flex-col">
        <FieldRow size={size} label="Backups" action={status.last_error ? undefined : backUpNow("secondary")}>
          On
        </FieldRow>
        <SaveError text={errors.backup ?? null} />
        <FieldRow size={size} label="Last backup" description={next ?? undefined}>
          <FieldRowValue truncate="wrap">{status.last_success_at ? <ClockTime secs={status.last_success_at} prefix="Backed up" /> : lastBackupLine(status)}</FieldRowValue>
        </FieldRow>
        <ToggleRow
          id="backup-crown-jewels"
          label="Include crown jewels"
          description={crownSwitchHelp(includeCrown)}
          checked={includeCrown}
          disabled={busy !== null}
          onCheckedChange={(v) => void run("crown", () => gate.run((g) => client.backupCrownJewels(v, g)), crownSwitchNotice(v))}
        />
        {waiting && (
          <p role="status" className="type-caption text-text-secondary pb-3">
            {waiting}
          </p>
        )}
        {crown && <p className="type-caption text-text-secondary pb-3">{crown}</p>}
        <SaveError text={errors.crown ?? null} />
        <FieldRow
          size={size}
          label="Folder"
          action={
            <FieldRowAction
              disabled={busy !== null}
              onClick={() =>
                void pick("folder").then((chosen) => {
                  if (chosen) void saveSetting("folder", { folder: chosen.token })
                })
              }
            >
              Choose…
            </FieldRowAction>
          }
        >
          <FieldRowValue truncate="middle">{status.folder ?? "None chosen yet"}</FieldRowValue>
        </FieldRow>
        <SaveError text={errors.folder ?? null} />
        <ChangeSetting
          label="How often"
          choices={INTERVAL_CHOICES}
          current={String(status.interval_hours)}
          onPick={(k) => void saveSetting("interval", { intervalHours: Number(k) })}
          busy={busy !== null}
          error={errors.interval ?? null}
        />
        <ChangeSetting
          label="Keep"
          choices={KEEP_CHOICES}
          current={String(status.keep_last)}
          onPick={(k) => void saveSetting("keep", { keepLast: Number(k) })}
          busy={busy !== null}
          error={errors.keep ?? null}
        />
        <FieldRow
          size={size}
          label="Backup password"
          rule="both"
          action={
            <FieldRowAction disabled={busy !== null} onClick={() => setChangingPassword(true)}>
              Change…
            </FieldRowAction>
          }
        >
          {status.key_set ? "Set" : "Not set"}
        </FieldRow>
      </div>

      {status.backups.length > 0 && (
        <section aria-label="Your backups" className="flex flex-col">
          <SectionHeader level={3} title="In the folder" count={count(status.backups.length)} />
          <div role="list">
            {status.backups.map((b) => (
              <ListRow
                key={b.name}
                role="listitem"
                size="compact"
                title={madeAtWords(b.made_at)}
                meta={sizeWords(b.bytes)}
                metaStyle="plain"
                action={
                  <Button size="sm" variant="ghost" onClick={() => setRestore(b)}>
                    Restore…
                  </Button>
                }
              />
            ))}
          </div>
        </section>
      )}

      <div className="flex flex-col gap-1">
        <div>
          <Button size="sm" variant="ghost-destructive" disabled={busy !== null} onClick={() => setTurningOff(true)}>
            Turn off backups
          </Button>
        </div>
      </div>

      <AdaptiveDialog
        open={changingPassword}
        onOpenChange={(o) => {
          if (!o && busy !== "password") {
            clearPasswords()
            setChangingPassword(false)
          }
        }}
        size="form"
        closeOnOutsideClick={false}
        overline="Backups"
        title="Change backup password"
        description="New backups are locked with it. Backups you already have keep the password they were made with."
        footer={
          <>
            <Button
              type="button"
              variant="secondary"
              disabled={busy === "password"}
              onClick={() => {
                clearPasswords()
                setChangingPassword(false)
              }}
            >
              Cancel
            </Button>
            <Button type="button" loading={busy === "password"} disabled={!first} disabledReason={first || busy === "password" ? undefined : "Type the new password first."} onClick={() => void changePassword()}>
              Change password
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-6">
          {passwordBoxes}
          {errors.password && <Banner tone="critical" size="compact" title={errors.password} />}
        </div>
      </AdaptiveDialog>

      {/* Turning off asks, with its one child (C11). Focus starts on CANCEL. */}
      <ConfirmDialog
        open={turningOff}
        onOpenChange={(o) => {
          if (!o) {
            setTurningOff(false)
            setForget(false)
          }
        }}
        overline="Backups"
        title="Turn off backups?"
        description="No new backups are made. The files in the folder stay where they are."
        confirmLabel="Turn off"
        tone="destructive"
        error={errors.off ?? null}
        onConfirm={async () => {
          const ok = await run("off", () => client.backupOff(forget), "Backups turned off")
          if (!ok) throw new Error("not turned off")
          setForget(false)
        }}
      >
        <Field orientation="horizontal">
          <Checkbox id="backup-forget" checked={forget} onCheckedChange={(v) => setForget(v === true)} />
          <FieldLabel htmlFor="backup-forget">Also forget the backup password on this computer</FieldLabel>
        </Field>
      </ConfirmDialog>

      {restoreWizard}
    </div>
  )
}
