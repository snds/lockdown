import { useEffect, useState, type ReactNode } from "react"

import {
  AdaptiveDialog,
  Banner,
  Button,
  Checkbox,
  Field,
  FieldLabel,
  NativeSelect,
  NativeSelectOption,
  RadioGroup,
  RadioRow,
  SectionHeader,
  TextField,
} from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { useGate } from "@/app/gate"
import { describeError, useClient } from "@/core/client"
import { encodeSecret } from "@/core/secret-bytes"
import type { ExportDone, ExportPreview } from "@/core/types"
import { AGE_CHOICE, choicesWithAge } from "@/features/bridges/age-export"
import { useBridges } from "@/features/bridges/bridges-client"
import { useVaults } from "@/features/organization/vaults-context"

import { EXPORT_CHOICES, MIN_PASSWORD, carriesLine, folderOf, leftOutLine, passwordProblem } from "./export-model"

interface ExportDialogProps {
  open: boolean
  onClose: () => void
  onNotice: (text: string) => void
}

/**
 * Export (brief T7, doc 83; 14-import-export.md, D103, D121, D142): one
 * Form, a 640 dialog on a desktop and full screen on a phone. What to
 * export; the kind of file as Radio Rows, each with its sentence, the
 * recommended one first; what the file will hold and what stays out (crown
 * jewels not chosen for export, by name, and what the kind itself can't
 * carry); then what that kind needs: a password twice, public keys, or the
 * person's yes to an unprotected file. CHOOSE WHERE TO SAVE… opens the
 * computer's own save dialog, and the core writes the file there after the
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
  // The failed save (a compact critical Banner above the footer) and the field that needs fixing (under the field).
  const [error, setError] = useState<string | null>(null)
  const [fieldError, setFieldError] = useState<{ field: "first" | "second" | "keys"; text: string } | null>(null)
  const [done, setDone] = useState<ExportDone | null>(null)
  // Controlled fields (unicron gap: SecretField's edit mode has no uncontrolled form); cleared after each try and on close.
  const [first, setFirst] = useState("")
  const [second, setSecond] = useState("")
  const choice = choices.find((c) => c.id === choiceId) ?? EXPORT_CHOICES[0]
  const toKey = choice.id === AGE_CHOICE.id
  const [countedFor, setCountedFor] = useState("")
  // A locked Lockdown backup and a readable one differ in the crown jewels they carry (doc 48).
  const countKey = `${vault}|${choice.sealed}`
  const preview = counted?.format === choice.format && countedFor === countKey ? counted : null

  useEffect(() => {
    if (!open) return
    let live = true
    client
      .exportPreview(choice.format, vault || null, choice.sealed)
      .then((p) => {
        if (!live) return
        setCounted(p)
        setCountedFor(`${vault}|${choice.sealed}`)
      })
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client, open, choice.format, choice.sealed, vault])

  const clearPasswords = () => {
    setFirst("")
    setSecond("")
  }

  const close = () => {
    clearPasswords()
    setPlainOk(false)
    setError(null)
    setFieldError(null)
    setDone(null)
    setBusy(false)
    onClose()
  }

  const saved = (result: ExportDone) => {
    setDone(result)
    onNotice(`Saved to ${folderOf(result.path)}`)
  }

  const save = async () => {
    setError(null)
    setFieldError(null)
    if (toKey && bridges) {
      const keys = recipients
        .split(/[\s,]+/)
        .map((r) => r.trim())
        .filter(Boolean)
      if (keys.length === 0) {
        setFieldError({ field: "keys", text: "Paste the age public key (age1…) the file should open with." })
        return
      }
      setBusy(true)
      try {
        const target = await client.exportChoose("age", true)
        if (!target) return
        saved(await gate.run((g) => bridges.exportSaveAge({ target: target.token, recipients: keys, vault: vault || null }, g)))
      } catch (e) {
        setError(describeError(e))
      } finally {
        setBusy(false)
      }
      return
    }
    let password: number[] | undefined
    if (choice.sealed) {
      const problem = passwordProblem(first, second)
      if (problem) {
        setFieldError({ field: first.length < MIN_PASSWORD ? "first" : "second", text: problem })
        return
      }
      password = encodeSecret(first)
    } else if (!plainOk) {
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
      saved(result)
    } catch (e) {
      setError(describeError(e))
    } finally {
      password?.fill(0)
      setBusy(false)
    }
  }

  const leftOut = preview ? leftOutLine(preview) : null
  const needsYes = !toKey && !choice.sealed && !plainOk

  if (done) {
    const doneLeftOut = leftOutLine(done.preview)
    return (
      <AdaptiveDialog
        open={open}
        onOpenChange={(o) => !o && close()}
        size="form"
        height="full"
        overline="Export"
        title={<span role="status">Saved to {folderOf(done.path)}</span>}
        description={done.path}
        footer={<Button onClick={close}>Done</Button>}
      >
        <div className="flex flex-col gap-3">
          <p className="type-body">{carriesLine(done.preview)}</p>
          {doneLeftOut && <p className="type-body text-text-secondary">{doneLeftOut}</p>}
          {!done.preview.encrypted && (
            <Banner tone="warning" size="compact" title="This file isn't protected." body="Move it somewhere safe, or delete it when you're done with it." />
          )}
        </div>
      </AdaptiveDialog>
    )
  }

  let needs: ReactNode
  if (toKey) {
    needs = (
      <TextField
        multiline
        id="export-recipients"
        label="The public keys it opens with"
        value={recipients}
        onChange={(e) => setRecipients(e.target.value)}
        placeholder="age1…"
        autoComplete="off"
        spellCheck={false}
        error={fieldError?.field === "keys" ? fieldError.text : undefined}
        helper="One or more age public keys, one per line. The file opens only with a matching private key (AGE-SECRET-KEY-1…), in Lockdown or any age tool. Lockdown doesn't keep the private key; make sure you have it."
      />
    )
  } else if (choice.sealed) {
    needs = (
      <div className="flex flex-col gap-6">
        <SecretField
          mode="edit"
          id="export-password"
          label="A password for the file"
          name="export-password"
          value={first}
          onChange={setFirst}
          disabled={busy}
          error={fieldError?.field === "first" ? fieldError.text : undefined}
          description={`Use at least ${MIN_PASSWORD} characters. You'll need it to open the file, and it isn't kept anywhere, so keep it somewhere safe.`}
        />
        <SecretField
          mode="edit"
          id="export-password-again"
          label="The same password again"
          name="export-password-again"
          value={second}
          onChange={setSecond}
          disabled={busy}
          error={fieldError?.field === "second" ? fieldError.text : undefined}
        />
      </div>
    )
  } else {
    needs = (
      <div className="flex flex-col gap-4">
        <Banner tone="warning" size="compact" title="This kind of file isn't protected." body="Keep it somewhere safe, and delete it when you're done with it." />
        <Field orientation="horizontal">
          <Checkbox id="export-plain-ok" checked={plainOk} onCheckedChange={(v) => setPlainOk(v === true)} />
          <FieldLabel htmlFor="export-plain-ok">I understand this file won't be protected: anyone who gets it can read every password in it.</FieldLabel>
        </Field>
      </div>
    )
  }

  return (
    <AdaptiveDialog
      open={open}
      onOpenChange={(o) => !o && !busy && close()}
      size="form"
      height="full"
      closeOnOutsideClick={false}
      overline="Export"
      title="Export your vault"
      description="One file, saved where you choose. Lockdown asks it's you first. Export is always free."
      footer={
        <>
          <Button type="button" variant="secondary" onClick={close} disabled={busy}>
            Cancel
          </Button>
          <Button
            type="button"
            onClick={() => void save()}
            loading={busy}
            disabled={!preview || needsYes}
            disabledReason={needsYes ? "Tick the box to say you understand the file won't be protected." : undefined}
          >
            Choose where to save…
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-8">
        {vaults.length > 1 && (
          <Field>
            <FieldLabel htmlFor="export-vault">What to export</FieldLabel>
            <NativeSelect id="export-vault" value={vault} onChange={(e) => setVault(e.target.value)} disabled={busy}>
              <NativeSelectOption value="">Every vault</NativeSelectOption>
              {vaults.map((v) => (
                <NativeSelectOption key={v.id} value={v.id}>
                  {v.name}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </Field>
        )}

        <div className="flex flex-col">
          <SectionHeader level={3} title="Kind of file" />
          <RadioGroup
            aria-label="Kind of file"
            value={choiceId}
            onValueChange={(v) => {
              setChoiceId(String(v))
              setPlainOk(false)
              setError(null)
              setFieldError(null)
            }}
          >
            {/* The locked kinds first; the unprotected ones fold under More kinds, open when one of them is
                picked (Lockdown 38): the safe export is the easy one. */}
            {choices
              .filter((c) => c.sealed)
              .map((c) => (
                <RadioRow key={c.id} value={c.id} label={c.label} description={c.detail} disabled={busy} />
              ))}
            <SectionHeader level={3} title="More kinds" summary="Not protected by a password" collapsible defaultOpen={!choice.sealed}>
              {choices
                .filter((c) => !c.sealed)
                .map((c) => (
                  <RadioRow key={c.id} value={c.id} label={c.label} description={c.detail} disabled={busy} />
                ))}
            </SectionHeader>
          </RadioGroup>
        </div>

        <section aria-label="What the file will hold" className="flex flex-col gap-2">
          <SectionHeader level={3} title="What the file will hold" />
          {preview ? (
            <>
              <p className="type-body">{carriesLine(preview)}</p>
              {leftOut && <p className="type-body text-text-secondary">{leftOut}</p>}
              {preview.dropped.length > 0 && (
                <div className="flex flex-col gap-1 pt-2">
                  <p className="type-overline text-text-label">Not in this kind of file</p>
                  <ul role="list" className="flex flex-col gap-1">
                    {preview.dropped.map((d) => (
                      <li key={d} className="type-body text-text-secondary">
                        {d}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
            </>
          ) : (
            <p role="status" className="type-caption text-text-secondary">
              Counting…
            </p>
          )}
        </section>

        {needs}
        {error && <Banner tone="critical" size="compact" title={error} />}
      </div>
    </AdaptiveDialog>
  )
}
