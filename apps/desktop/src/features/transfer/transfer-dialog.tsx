import { useRef, useState, type FormEvent } from "react"

import {
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  NativeSelect,
  NativeSelectOption,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import type { ImportReport } from "@/core/types"

interface TransferDialogProps {
  kind: "import" | "export" | null
  onClose: () => void
  onImported: (report: ImportReport) => void
}

/** The format a file name implies; the person can still change it. */
export function formatOf(name: string): "cxf" | "csv" {
  return name.toLowerCase().endsWith(".csv") ? "csv" : "cxf"
}

/**
 * Import and export (brief §3.2, D103). Import reads the file in the
 * webview with the File API (no `fs` capability) and hands the bytes to
 * the core, which treats them as hostile and says why when it refuses.
 * Export goes through the Reveal gate (the password) and comes back as
 * bytes the webview saves as a download, then forgets.
 */
export function TransferDialog({ kind, onClose, onImported }: TransferDialogProps) {
  const client = useClient()
  const gate = useGate()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [notes, setNotes] = useState<string[]>([])
  const fileRef = useRef<HTMLInputElement>(null)
  const [format, setFormat] = useState<"cxf" | "csv">("cxf")

  const reset = () => {
    setError(null)
    setNotes([])
    setBusy(false)
  }

  const runImport = async () => {
    const file = fileRef.current?.files?.[0]
    if (!file) {
      setError("Choose a file first.")
      return
    }
    setBusy(true)
    setError(null)
    try {
      const bytes = new Uint8Array(await file.arrayBuffer())
      const report = await client.importItems(Array.from(bytes), format)
      bytes.fill(0)
      setNotes(report.notes)
      if (report.notes.length === 0) onImported(report)
      else setError(null)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
      if (fileRef.current) fileRef.current.value = ""
    }
  }

  const runExport = async () => {
    setBusy(true)
    setError(null)
    try {
      const bytes = await gate.run((g) => client.exportItems(format, g))
      const blob = new Blob([bytes as BlobPart], { type: format === "csv" ? "text/csv" : "application/json" })
      bytes.fill(0)
      const url = URL.createObjectURL(blob)
      const a = document.createElement("a")
      a.href = url
      a.download = `lockdown-export.${format === "csv" ? "csv" : "json"}`
      document.body.appendChild(a)
      a.click()
      a.remove()
      window.setTimeout(() => URL.revokeObjectURL(url), 10_000)
      onClose()
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const submit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    void (kind === "import" ? runImport() : runExport())
  }

  return (
    <Dialog
      open={kind !== null}
      onOpenChange={(o) => {
        if (!o) {
          reset()
          onClose()
        }
      }}
    >
      <DialogContent>
        <form onSubmit={submit} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>{kind === "import" ? "Import" : "Export"}</DialogTitle>
            <DialogDescription>
              {kind === "import"
                ? "A CXF file (the FIDO exchange format most managers write) or a CSV with the usual columns. The file is checked before anything is kept."
                : "One file of everything in this vault, in plain text. You will be asked to confirm it is you."}
            </DialogDescription>
          </DialogHeader>
          <FieldGroup>
            {kind === "import" && (
              <Field>
                <FieldLabel htmlFor="import-file">File</FieldLabel>
                <input
                  id="import-file"
                  ref={fileRef}
                  type="file"
                  accept=".json,.cxf,.csv,application/json,text/csv"
                  className="text-sm"
                  onChange={(e) => {
                    const f = e.target.files?.[0]
                    if (f) setFormat(formatOf(f.name))
                  }}
                />
              </Field>
            )}
            <Field>
              <FieldLabel htmlFor="transfer-format">Format</FieldLabel>
              <NativeSelect id="transfer-format" value={format} onChange={(e) => setFormat(e.target.value as "cxf" | "csv")}>
                <NativeSelectOption value="cxf">CXF (JSON)</NativeSelectOption>
                <NativeSelectOption value="csv">CSV (logins only)</NativeSelectOption>
              </NativeSelect>
              {kind === "export" && <FieldDescription>CXF carries every kind; CSV carries logins and their codes only.</FieldDescription>}
            </Field>
          </FieldGroup>
          {error && <FieldError role="alert">{error}</FieldError>}
          {notes.length > 0 && (
            <div role="status" className="flex flex-col gap-1 text-sm">
              <p>Imported, with some parts left out:</p>
              <ul className="list-disc pl-5 text-muted-foreground">
                {notes.map((n) => (
                  <li key={n}>{n}</li>
                ))}
              </ul>
            </div>
          )}
          <DialogActions>
            <DialogAction
              kind="cancel"
              type="button"
              onClick={() => {
                reset()
                onClose()
              }}
              disabled={busy}
            >
              {notes.length > 0 ? "Done" : "Cancel"}
            </DialogAction>
            {notes.length === 0 && (
              <DialogAction kind="confirm" type="submit" disabled={busy}>
                {kind === "import" ? "Import" : "Export"}
              </DialogAction>
            )}
          </DialogActions>
        </form>
      </DialogContent>
    </Dialog>
  )
}
