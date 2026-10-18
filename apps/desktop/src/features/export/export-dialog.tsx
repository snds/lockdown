import { useEffect, useRef, useState } from "react"

import {
  Checkbox,
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
  Input,
  NativeSelect,
  NativeSelectOption,
  Spinner,
  Textarea,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import { encodeSecret } from "@/core/secret-bytes"
import type { ExportDone, ExportPreview } from "@/core/types"
import { AGE_CHOICE, choicesWithAge } from "@/features/bridges/age-export"
import { useBridges } from "@/features/bridges/bridges-client"
import { useVaults } from "@/features/organization/vaults-context"

import { EXPORT_CHOICES, carriesLine, leftOutLine, passwordProblem } from "./export-model"

interface ExportDialogProps {
  open: boolean
  onClose: () => void
  onNotice: (text: string) => void
}

/**
 * Export (14-import-export.md, D103, D121): pick a kind of file, see what
 * it will hold and what stays out (crown jewels not chosen for export,
 * and what the format itself can't carry), give a locked file its
 * password or say yes to an unprotected one, then choose where in the
 * computer's own save dialog. The core writes the file there after the
 * `Reveal` gate; the file's bytes never come to this window.
 */
export function ExportDialog({ open, onClose, onNotice }: ExportDialogProps) {
  const client = useClient()
  const gate = useGate()
  const bridges = useBridges()
  const choices = choicesWithAge(EXPORT_CHOICES, bridges !== null)
  const [recipients, setRecipients] = useState("")
  const [choiceId, setChoiceId] = useState(EXPORT_CHOICES[0].id)
  const { vaults } = useVaults()
  const [vault, setVault] = useState("")
  const [counted, setCounted] = useState<ExportPreview | null>(null)
  const [plainOk, setPlainOk] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [done, setDone] = useState<ExportDone | null>(null)
  const first = useRef<HTMLInputElement>(null)
  const second = useRef<HTMLInputElement>(null)
  const choice = choices.find((c) => c.id === choiceId) ?? EXPORT_CHOICES[0]
  const toKey = choice.id === AGE_CHOICE.id
  const [countedFor, setCountedFor] = useState("")
  const preview = counted?.format === choice.format && countedFor === vault ? counted : null

  useEffect(() => {
    if (!open) return
    let live = true
    client
      .exportPreview(choice.format, vault || null)
      .then((p) => {
        if (!live) return
        setCounted(p)
        setCountedFor(vault)
      })
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client, open, choice.format, vault])

  const clearPasswords = () => {
    if (first.current) first.current.value = ""
    if (second.current) second.current.value = ""
  }

  const close = () => {
    clearPasswords()
    setPlainOk(false)
    setError(null)
    setDone(null)
    setBusy(false)
    onClose()
  }

  const save = async () => {
    setError(null)
    if (toKey && bridges) {
      const keys = recipients
        .split(/[\s,]+/)
        .map((r) => r.trim())
        .filter(Boolean)
      if (keys.length === 0) {
        setError("Paste the age public key (age1…) the file should open with.")
        return
      }
      setBusy(true)
      try {
        const target = await client.exportChoose("age", true)
        if (!target) return
        const result = await gate.run((g) => bridges.exportSaveAge({ target: target.token, recipients: keys, vault: vault || null }, g))
        setDone(result)
        onNotice(`Saved to ${result.path}.`)
      } catch (e) {
        setError(describeError(e))
      } finally {
        setBusy(false)
      }
      return
    }
    let password: number[] | undefined
    if (choice.sealed) {
      const problem = passwordProblem(first.current?.value ?? "", second.current?.value ?? "")
      if (problem) {
        setError(problem)
        return
      }
      password = encodeSecret(first.current?.value ?? "")
    } else if (!plainOk) {
      setError("Tick the box to say you understand the file won't be protected.")
      return
    }
    setBusy(true)
    try {
      const target = await client.exportChoose(choice.format, choice.sealed)
      if (!target) return
      const result = await gate.run((g) =>
        client.exportSave({ target: target.token, format: choice.format, password, plainConfirmed: !choice.sealed && plainOk, vault: vault || null }, g),
      )
      clearPasswords()
      setDone(result)
      onNotice(`Saved to ${result.path}.`)
    } catch (e) {
      setError(describeError(e))
    } finally {
      password?.fill(0)
      setBusy(false)
    }
  }

  const leftOut = preview ? leftOutLine(preview) : null

  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>Export</DialogTitle>
          <DialogDescription>
            One file of your vault, saved where you choose. Lockdown asks it's you first. Export is always free.
          </DialogDescription>
        </DialogHeader>

        {done ? (
          <div className="flex flex-col gap-3 text-sm">
            <p role="status">Saved to {done.path}.</p>
            <p>{carriesLine(done.preview)}</p>
            {leftOutLine(done.preview) && <p>{leftOutLine(done.preview)}</p>}
            {!done.preview.encrypted && (
              <p className="text-attention-text">This file isn't protected. Move it somewhere safe, or delete it when you're done with it.</p>
            )}
            <DialogActions>
              <DialogAction kind="confirm" type="button" onClick={close}>
                Done
              </DialogAction>
            </DialogActions>
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            <FieldGroup>
              {vaults.length > 1 && (
                <Field>
                  <FieldLabel htmlFor="export-vault">What to export</FieldLabel>
                  <NativeSelect id="export-vault" value={vault} onChange={(e) => setVault(e.target.value)}>
                    <NativeSelectOption value="">Every vault</NativeSelectOption>
                    {vaults.map((v) => (
                      <NativeSelectOption key={v.id} value={v.id}>
                        {v.name}
                      </NativeSelectOption>
                    ))}
                  </NativeSelect>
                </Field>
              )}
              <Field>
                <FieldLabel htmlFor="export-kind">Kind of file</FieldLabel>
                <NativeSelect
                  id="export-kind"
                  value={choiceId}
                  onChange={(e) => {
                    setChoiceId(e.target.value)
                    setPlainOk(false)
                    setError(null)
                  }}
                >
                  {choices.map((c) => (
                    <NativeSelectOption key={c.id} value={c.id}>
                      {c.label}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
                <FieldDescription>{choice.detail}</FieldDescription>
              </Field>
            </FieldGroup>

            <section aria-label="What the file will hold" className="flex flex-col gap-1 text-sm">
              {preview ? (
                <>
                  <p>{carriesLine(preview)}</p>
                  {leftOut && <p>{leftOut}</p>}
                  {preview.dropped.length > 0 && (
                    <>
                      <p className="text-muted-foreground">Not in this kind of file:</p>
                      <ul className="list-disc pl-5 text-muted-foreground">
                        {preview.dropped.map((d) => (
                          <li key={d}>{d}</li>
                        ))}
                      </ul>
                    </>
                  )}
                </>
              ) : (
                <span className="flex items-center gap-2 text-muted-foreground">
                  <Spinner />
                  Counting…
                </span>
              )}
            </section>

            {toKey ? (
              <Field>
                <FieldLabel htmlFor="export-recipients">The public keys it opens with</FieldLabel>
                <Textarea
                  id="export-recipients"
                  value={recipients}
                  onChange={(e) => setRecipients(e.target.value)}
                  placeholder="age1…"
                  autoComplete="off"
                  spellCheck={false}
                />
                <FieldDescription>
                  One or more age public keys, one per line. The file opens only with a matching private key (AGE-SECRET-KEY-1…), in Lockdown or any
                  age tool. Lockdown doesn't keep the private key; make sure you have it.
                </FieldDescription>
              </Field>
            ) : choice.sealed ? (
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor="export-password">A password for the file</FieldLabel>
                  <Input id="export-password" ref={first} type="password" autoComplete="new-password" />
                  <FieldDescription>
                    You'll need it to open the file. It isn't kept anywhere, so write it down or keep it in a safe place.
                  </FieldDescription>
                </Field>
                <Field>
                  <FieldLabel htmlFor="export-password-again">The same password again</FieldLabel>
                  <Input id="export-password-again" ref={second} type="password" autoComplete="new-password" />
                </Field>
              </FieldGroup>
            ) : (
              <Field orientation="horizontal">
                <Checkbox id="export-plain-ok" checked={plainOk} onCheckedChange={(v) => setPlainOk(v === true)} />
                <FieldLabel htmlFor="export-plain-ok">
                  I understand this file won't be protected: anyone who gets it can read every password in it.
                </FieldLabel>
              </Field>
            )}

            {error && <FieldError role="alert">{error}</FieldError>}
            <DialogActions>
              <DialogAction kind="cancel" type="button" onClick={close} disabled={busy}>
                Cancel
              </DialogAction>
              <DialogAction kind="confirm" type="button" onClick={() => void save()} disabled={busy || !preview}>
                Choose where to save…
              </DialogAction>
            </DialogActions>
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
