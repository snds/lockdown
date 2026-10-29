import { useEffect, useRef, useState, type FormEvent } from "react"

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
  NativeSelect,
  NativeSelectOption,
  Spinner,
} from "@unicron/ui"

import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError, useClient } from "@/core/client"
import { encodeSecret } from "@/core/secret-bytes"
import type { ImportFormatView, ImportPreview, ImportReport } from "@/core/types"
import { useVaults } from "@/features/organization/vaults-context"

import { AUTO, confirmWords, doneLine, factLines, kindLines, passwordAsked, summaryLine } from "./import-model"

interface ImportWizardProps {
  open: boolean
  /** Restore one of the backup folder's files instead of picking one. */
  restore?: string | null
  onClose: () => void
  onImported: (report: ImportReport) => void
}

type Step = "choose" | "password" | "checking" | "preview" | "saving" | "done"

/** How many notes show before "Show all". */
const FIRST_NOTES = 8

/**
 * The import wizard (14-import-export.md): pick where the file is from and
 * the file, see what it holds and what can't come along, then bring it
 * in. The file is read in the window (the File API, no file permission)
 * and parsed by the core, which treats it as hostile; the preview is
 * counts and words, never a value. A file with a password of its own asks
 * for it; the password goes to the core once, as bytes. A restore starts
 * from a backup in the backup folder and opens it with the key kept on
 * this computer, or its password.
 */
export function ImportWizard({ open, restore = null, onClose, onImported }: ImportWizardProps) {
  const client = useClient()
  const [formats, setFormats] = useState<ImportFormatView[]>([])
  const [format, setFormat] = useState(AUTO)
  const { vaults } = useVaults()
  const [vault, setVault] = useState<string>("")
  const [fileName, setFileName] = useState("")
  const [step, setStep] = useState<Step>("choose")
  const [error, setError] = useState<string | null>(null)
  const [preview, setPreview] = useState<ImportPreview | null>(null)
  const [report, setReport] = useState<ImportReport | null>(null)
  const [allNotes, setAllNotes] = useState(false)
  const bytes = useRef<Uint8Array | null>(null)
  const fileRef = useRef<HTMLInputElement>(null)
  const passwordRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!open) return
    client.importFormats().then(setFormats).catch(() => setFormats([]))
  }, [client, open])

  const wipe = () => {
    bytes.current?.fill(0)
    bytes.current = null
    if (fileRef.current) fileRef.current.value = ""
    if (passwordRef.current) passwordRef.current.value = ""
  }

  const reset = () => {
    wipe()
    setStep("choose")
    setError(null)
    setPreview(null)
    setReport(null)
    setAllNotes(false)
    setFileName("")
  }

  const close = () => {
    if (preview && step !== "done") void client.importCancel().catch(() => {})
    reset()
    onClose()
  }

  /** Asks the core what the file holds, with the file's own password when one was typed. */
  const check = async (password?: number[], name = fileName) => {
    setStep("checking")
    setError(null)
    try {
      const p = restore
        ? await client.backupRestorePreview(restore, password)
        : await client.importPreview({ bytes: Array.from(bytes.current ?? []), format, fileName: name, password, vault: vault || null })
      setPreview(p)
      setStep("preview")
    } catch (e) {
      const asked = passwordAsked(e)
      if (asked) {
        setStep("password")
        setError(asked === "wrong" ? "That password didn't open the file. Try again." : null)
      } else {
        setStep(restore ? "password" : "choose")
        setError(describeError(e))
      }
    } finally {
      password?.fill(0)
      if (passwordRef.current) passwordRef.current.value = ""
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

  const choose = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const file = fileRef.current?.files?.[0]
    if (!file) {
      setError("Choose the file first.")
      return
    }
    bytes.current = new Uint8Array(await file.arrayBuffer())
    setFileName(file.name)
    await check(undefined, file.name)
  }

  const unlock = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const typed = passwordRef.current?.value ?? ""
    if (!typed) {
      setError("Type the file's password.")
      return
    }
    await check(encodeSecret(typed))
  }

  const commit = async () => {
    if (!preview) return
    setStep("saving")
    setError(null)
    try {
      const r = await client.importCommit(preview.token)
      wipe()
      setReport(r)
      setStep("done")
      onImported(r)
    } catch (e) {
      setStep("preview")
      setError(describeError(e))
    }
  }

  const chosen = formats.find((f) => f.id === format)
  const notes = preview ? [...preview.problems, ...preview.item_notes] : []
  const shownNotes = allNotes ? notes : notes.slice(0, FIRST_NOTES)

  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>{restore ? "Restore from a backup" : "Import from another app"}</DialogTitle>
          <DialogDescription>
            {restore
              ? `${restore}. Lockdown shows what's in it before anything changes.`
              : "Bring your passwords and notes over. Lockdown checks the file and shows what's in it before anything is kept."}
          </DialogDescription>
        </DialogHeader>

        {step === "choose" && (
          <form onSubmit={choose} className="flex flex-col gap-4" aria-label="Choose the file">
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="import-from">Where is it from?</FieldLabel>
                <NativeSelect id="import-from" value={format} onChange={(e) => setFormat(e.target.value)}>
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
                  <NativeSelect id="import-into" value={vault} onChange={(e) => setVault(e.target.value)}>
                    {vaults.map((v) => (
                      <NativeSelectOption key={v.id} value={v.primary ? "" : v.id}>
                        {v.name}
                      </NativeSelectOption>
                    ))}
                  </NativeSelect>
                  <FieldDescription>Folders in the file become folders in this vault.</FieldDescription>
                </Field>
              )}
              <Field>
                <FieldLabel htmlFor="import-file">The file</FieldLabel>
                <input id="import-file" ref={fileRef} type="file" className="text-sm" />
                <FieldDescription>Once it's in, delete the exported file: it holds your passwords in plain text.</FieldDescription>
              </Field>
            </FieldGroup>
            {error && <FieldError role="alert">{error}</FieldError>}
            <LabelledActions>
              <LabelledAction kind="cancel" type="button" onClick={close}>
                Cancel
              </LabelledAction>
              <LabelledAction kind="confirm" type="submit">
                Check the file
              </LabelledAction>
            </LabelledActions>
          </form>
        )}

        {step === "password" && (
          <form onSubmit={unlock} className="flex flex-col gap-4" aria-label="Unlock the file">
            <Field>
              <FieldLabel htmlFor="import-password">The file's password</FieldLabel>
              <Input id="import-password" ref={passwordRef} type="password" autoComplete="off" autoFocus />
              <FieldDescription>
                {restore
                  ? "The backup password you chose when you turned backups on, or the export's password."
                  : "The password you gave the file when you exported it (for KeePass, the database's own password)."}
              </FieldDescription>
            </Field>
            {error && <FieldError role="alert">{error}</FieldError>}
            <LabelledActions>
              <LabelledAction kind="cancel" type="button" onClick={close}>
                Cancel
              </LabelledAction>
              <LabelledAction kind="confirm" type="submit">
                Open the file
              </LabelledAction>
            </LabelledActions>
          </form>
        )}

        {(step === "checking" || step === "saving") && (
          <div role="status" className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner />
            {step === "checking" ? "Checking the file…" : "Bringing it in…"}
          </div>
        )}

        {step === "preview" && preview && (
          <div className="flex flex-col gap-4">
            <section aria-label="What the file holds" className="flex flex-col gap-2 text-sm">
              <p className="font-medium">{summaryLine(preview)}</p>
              <p className="text-muted-foreground">Read as: {preview.format_name}</p>
              <ul className="list-disc pl-5">
                {kindLines(preview).map((l) => (
                  <li key={l}>{l}</li>
                ))}
              </ul>
              {factLines(preview).map((l) => (
                <p key={l}>{l}</p>
              ))}
            </section>
            {notes.length > 0 && (
              <section aria-label="What can't come along" className="flex flex-col gap-1 text-sm">
                <p className="font-medium">Worth knowing ({notes.length})</p>
                <ul className="list-disc pl-5 text-muted-foreground">
                  {shownNotes.map((n, i) => (
                    <li key={`${i}-${n}`}>{n}</li>
                  ))}
                </ul>
                {notes.length > FIRST_NOTES && !allNotes && (
                  <div>
                    <Button type="button" size="sm" variant="ghost" onClick={() => setAllNotes(true)}>
                      Show all {notes.length}
                    </Button>
                  </div>
                )}
              </section>
            )}
            {error && <FieldError role="alert">{error}</FieldError>}
            <LabelledActions>
              <LabelledAction kind="cancel" type="button" onClick={close}>
                Cancel
              </LabelledAction>
              <LabelledAction kind="confirm" type="button" onClick={() => void commit()} disabled={preview.new + preview.updated === 0}>
                {confirmWords(preview)}
              </LabelledAction>
            </LabelledActions>
          </div>
        )}

        {step === "done" && report && (
          <div className="flex flex-col gap-4">
            <p role="status" className="text-sm">
              {doneLine(report)}
            </p>
            {!restore && <p className="text-sm text-muted-foreground">You can delete the file you imported now.</p>}
            <LabelledActions>
              <LabelledAction kind="confirm" type="button" onClick={close}>
                Done
              </LabelledAction>
            </LabelledActions>
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
