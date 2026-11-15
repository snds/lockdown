import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react"

import {
  AdaptiveDialog,
  Banner,
  Button,
  Field,
  FieldDescription,
  FieldLabel,
  FileChip,
  ListRow,
  NativeSelect,
  NativeSelectOption,
  SectionHeader,
} from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { describeError, useClient } from "@/core/client"
import { encodeSecret } from "@/core/secret-bytes"
import type { ImportFormatView, ImportPreview, ImportReport } from "@/core/types"
import { sizeWords } from "@/features/backups/backups-model"
import { useVaults } from "@/features/organization/vaults-context"
import { count, countName } from "@/lib/format"

import {
  AUTO,
  confirmWords,
  doneLine,
  factLines,
  nothingReason,
  passwordAsked,
  restoreOverline,
  stepOverline,
  summaryLine,
} from "./import-model"

interface ImportWizardProps {
  open: boolean
  /** Restore one of the backup folder's files instead of picking one. */
  restore?: string | null
  /** When that backup was made (seconds), for the overline `Restore // 2026.11.13 09:00`. */
  restoreMadeAt?: number | null
  onClose: () => void
  onImported: (report: ImportReport) => void
  /** The format to start with (the first run's list picks one); "auto" otherwise. */
  initialFormat?: string
  /**
   * The last step's FIND DUPLICATES (brief T7, V3): where the host can open
   * Health. Left out (the first run, which has no shell yet), the step has no
   * such action.
   */
  onFindDuplicates?: () => void
}

type Step = "choose" | "password" | "preview" | "done"

/** How many notes show before "Show all". */
const FIRST_NOTES = 8

/**
 * The import wizard (brief T7, doc 83; 14-import-export.md): a Form in
 * steps, a 640 dialog on a desktop and full screen on a phone. Choose where
 * the file is from and the file; open it with its own password when it has
 * one; see what it holds and what can't come along; bring it in. The file is
 * read in the window (the File API, no file permission) and parsed by the
 * core, which treats it as hostile; the preview is counts and words, never a
 * value. The file's password goes to the core once, as bytes. A restore
 * starts from a backup in the backup folder, at the password or the preview,
 * and opens it with the key kept on this computer, or its password.
 */
export function ImportWizard({ open, restore = null, restoreMadeAt = null, onClose, onImported, initialFormat, onFindDuplicates }: ImportWizardProps) {
  const client = useClient()
  const [formats, setFormats] = useState<ImportFormatView[]>([])
  const [format, setFormat] = useState(initialFormat ?? AUTO)
  const { vaults } = useVaults()
  const [vault, setVault] = useState<string>("")
  const [file, setFile] = useState<{ name: string; size: number } | null>(null)
  const [step, setStep] = useState<Step>("choose")
  const [busy, setBusy] = useState<"checking" | "saving" | null>(null)
  // A refusal of the whole file or the commit (a compact critical Banner), and a wrong password (under its field).
  const [error, setError] = useState<string | null>(null)
  const [passwordError, setPasswordError] = useState<string | null>(null)
  // The field is controlled (unicron gap: SecretField's edit mode has no uncontrolled form); it is cleared after every try and on close.
  const [password, setPassword] = useState("")
  const [preview, setPreview] = useState<ImportPreview | null>(null)
  const [report, setReport] = useState<ImportReport | null>(null)
  const [allNotes, setAllNotes] = useState(false)
  const bytes = useRef<Uint8Array | null>(null)
  const fileRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!open) return
    client.importFormats().then(setFormats).catch(() => setFormats([]))
  }, [client, open])

  const wipe = () => {
    bytes.current?.fill(0)
    bytes.current = null
    if (fileRef.current) fileRef.current.value = ""
    setPassword("")
  }

  const reset = () => {
    wipe()
    setStep("choose")
    setBusy(null)
    setError(null)
    setPasswordError(null)
    setPreview(null)
    setReport(null)
    setAllNotes(false)
    setFile(null)
  }

  const close = () => {
    if (preview && step !== "done") void client.importCancel().catch(() => {})
    reset()
    onClose()
  }

  /** Asks the core what the file holds, with the file's own password when one was typed. */
  const check = async (typed?: number[], name = file?.name ?? "") => {
    setBusy("checking")
    setError(null)
    setPasswordError(null)
    try {
      const p = restore
        ? await client.backupRestorePreview(restore, typed)
        : await client.importPreview({ bytes: Array.from(bytes.current ?? []), format, fileName: name, password: typed, vault: vault || null })
      setPreview(p)
      setStep("preview")
    } catch (e) {
      const asked = passwordAsked(e)
      if (asked) {
        setStep("password")
        setPasswordError(asked === "wrong" ? "That password didn't open the file. Try again." : null)
      } else {
        setStep(restore ? "password" : "choose")
        setError(describeError(e))
      }
    } finally {
      typed?.fill(0)
      setPassword("")
      setBusy(null)
    }
  }

  // A restore starts on its own once, when the dialog opens on a backup.
  const started = useRef<string | null>(null)
  useEffect(() => {
    if (!open) {
      started.current = null
      return
    }
    if (restore && started.current !== restore) {
      started.current = restore
      void check()
    }
  })

  const picked = () => {
    const chosen = fileRef.current?.files?.[0]
    setError(null)
    setFile(chosen ? { name: chosen.name, size: chosen.size } : null)
  }

  const choose = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const chosen = fileRef.current?.files?.[0]
    if (!chosen) return
    bytes.current = new Uint8Array(await chosen.arrayBuffer())
    await check(undefined, chosen.name)
  }

  const unlock = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!password) return
    await check(encodeSecret(password))
  }

  const commit = async () => {
    if (!preview) return
    setBusy("saving")
    setError(null)
    try {
      const r = await client.importCommit(preview.token)
      wipe()
      setReport(r)
      setStep("done")
      onImported(r)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(null)
    }
  }

  const chosen = formats.find((f) => f.id === format)
  const notes = preview ? [...preview.problems, ...preview.item_notes] : []
  const shownNotes = allNotes ? notes : notes.slice(0, FIRST_NOTES)
  const checking = busy === "checking"
  const live = busy === "checking" ? "Checking the file…" : busy === "saving" ? "Bringing it in…" : null
  // While the core works, one live line says what it does; the primary keeps its label with its spinner.
  const liveLine = live && (
    <p role="status" className="type-caption text-text-secondary">
      {live}
    </p>
  )
  const refused = error && <Banner tone="critical" size="compact" title={error} />

  const overline = restore ? restoreOverline(restoreMadeAt) : stepOverline(step === "preview" ? 2 : step === "done" ? 3 : 1)
  const cancel = (
    <Button type="button" variant="secondary" onClick={close} disabled={busy === "saving"}>
      Cancel
    </Button>
  )

  let title: ReactNode
  let description: ReactNode
  let footer: ReactNode
  let body: ReactNode

  if (step === "choose" && !restore) {
    title = "Import from another app"
    description = "Lockdown checks the file and shows what's in it before anything is kept."
    footer = (
      <>
        {cancel}
        <Button type="submit" form="import-choose" loading={checking} disabled={!file} disabledReason={file ? undefined : "Choose the file first."}>
          Check file
        </Button>
      </>
    )
    body = (
      <form id="import-choose" onSubmit={(e) => void choose(e)} className="flex flex-col gap-6" aria-label="Choose the file">
        <Field>
          <FieldLabel htmlFor="import-from">Where is it from?</FieldLabel>
          <NativeSelect id="import-from" value={format} onChange={(e) => setFormat(e.target.value)} disabled={checking}>
            <NativeSelectOption value={AUTO}>Let Lockdown tell from the file</NativeSelectOption>
            {formats.map((f) => (
              <NativeSelectOption key={f.id} value={f.id}>
                {f.name}
              </NativeSelectOption>
            ))}
          </NativeSelect>
          <FieldDescription>
            {chosen ? chosen.how : "1Password, Bitwarden, Proton Pass, KeePass, Dashlane, Keeper, LastPass, Enpass, a CXF or CSV file, or a Lockdown backup."}
          </FieldDescription>
        </Field>
        {vaults.length > 1 && (
          <Field>
            <FieldLabel htmlFor="import-into">Into which vault</FieldLabel>
            <NativeSelect id="import-into" value={vault} onChange={(e) => setVault(e.target.value)} disabled={checking}>
              {vaults.map((v) => (
                <NativeSelectOption key={v.id} value={v.primary ? "" : v.id}>
                  {v.name}
                </NativeSelectOption>
              ))}
            </NativeSelect>
            <FieldDescription>Folders in the file become folders in this vault.</FieldDescription>
          </Field>
        )}
        <div className="flex flex-col gap-3">
          {/* The window's own file input, opened by CHOOSE FILE: it stays out of the tab order, the button is the way in. */}
          <input id="import-file" ref={fileRef} type="file" aria-label="The file" tabIndex={-1} className="sr-only" onChange={picked} />
          {file && <FileChip name={file.name} size={sizeWords(file.size)} />}
          <div>
            <Button type="button" variant="secondary" size="sm" onClick={() => fileRef.current?.click()} disabled={checking}>
              {file ? "Choose another file…" : "Choose file…"}
            </Button>
          </div>
          <p className="type-caption text-text-secondary">Once it's in, delete the exported file. It holds your passwords in plain text.</p>
        </div>
        {liveLine}
        {refused}
      </form>
    )
  } else if (step === "password") {
    title = restore ? "Restore from a backup" : "This file has its own password"
    description = restore ? "Lockdown shows what's in it before anything changes." : file?.name
    footer = (
      <>
        {cancel}
        <Button type="submit" form="import-password" loading={checking} disabled={!password} disabledReason={password || checking ? undefined : "Type the file's password first."}>
          Open file
        </Button>
      </>
    )
    body = (
      <form id="import-password" onSubmit={(e) => void unlock(e)} className="flex flex-col gap-6" aria-label="Unlock the file">
        <SecretField
          mode="edit"
          id="import-password-field"
          label="The file's password"
          name="password"
          value={password}
          onChange={setPassword}
          disabled={checking}
          autoComplete="off"
          error={passwordError ?? undefined}
          description={
            restore
              ? "The backup password you chose when you turned backups on, or the export's password."
              : "The password you gave the file when you exported it (for KeePass, the database's own password)."
          }
        />
        {liveLine}
        {refused}
      </form>
    )
  } else if (step === "preview" && preview) {
    const reason = nothingReason(preview)
    title = summaryLine(preview)
    description = `Read as: ${preview.format_name}`
    footer = (
      <>
        {cancel}
        <Button type="button" onClick={() => void commit()} loading={busy === "saving"} disabled={reason !== null} disabledReason={reason ?? undefined}>
          {confirmWords(preview)}
        </Button>
      </>
    )
    body = (
      <div className="flex flex-col gap-8">
        <section aria-label="What the file holds" className="flex flex-col">
          <SectionHeader level={3} title="What the file holds" />
          <div role="list" aria-label="Kinds">
            {[...preview.by_kind]
              .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name))
              .map((k) => (
                <ListRow key={k.kind} role="listitem" size="compact" title={k.name} status={<span aria-label={countName(k.count, "item")}>{count(k.count)}</span>} />
              ))}
          </div>
          {factLines(preview).map((l) => (
            <p key={l} className="type-body text-text-secondary pt-3">
              {l}
            </p>
          ))}
        </section>
        {notes.length > 0 && (
          <section aria-label="What can't come along" className="flex flex-col">
            <SectionHeader level={3} title="What can't come along" count={count(notes.length)} />
            <ul role="list" className="flex flex-col gap-2 pt-3">
              {shownNotes.map((n, i) => (
                <li key={`${i}-${n}`} className="type-body">
                  {n}
                </li>
              ))}
            </ul>
            {notes.length > FIRST_NOTES && !allNotes && (
              <div className="pt-2">
                <Button type="button" size="sm" variant="ghost" onClick={() => setAllNotes(true)}>
                  Show all {count(notes.length)}
                </Button>
              </div>
            )}
          </section>
        )}
        {liveLine}
        {refused}
      </div>
    )
  } else if (step === "done" && report) {
    // The report is said once, when the step arrives.
    title = <span role="status">{doneLine(report)}</span>
    description = restore ? undefined : "You can delete the file you imported now."
    footer = <Button onClick={close}>Done</Button>
    body = onFindDuplicates ? (
      <div className="flex flex-col gap-2">
        <p className="type-body text-text-secondary">Items from another app can say the same thing twice. Health finds them.</p>
        <div>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => {
              close()
              onFindDuplicates()
            }}
          >
            Find duplicates
          </Button>
        </div>
      </div>
    ) : null
  } else {
    // A restore before its first answer: the live line alone.
    title = "Restore from a backup"
    description = "Lockdown shows what's in it before anything changes."
    footer = cancel
    body = liveLine ?? refused
  }

  return (
    <AdaptiveDialog
      open={open}
      onOpenChange={(o) => !o && close()}
      size="form"
      height="full"
      closeOnOutsideClick={false}
      overline={overline}
      title={title}
      description={description}
      footer={footer}
    >
      {body}
    </AdaptiveDialog>
  )
}
