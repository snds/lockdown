import { useState, type FormEvent } from "react"

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
  Icon,
  IconButton,
  Input,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
  NativeSelect,
  NativeSelectOption,
  Switch,
} from "@unicron/ui"

import { useTemplates } from "@/app/templates"
import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError, useClient } from "@/core/client"
import type { UserTemplateView } from "@/core/types"

import { draftToFields, type DraftField } from "./depth-model"

interface TemplateDesignerProps {
  /** `null` to make a new one. */
  template: UserTemplateView | null
  onDone: (notice: string) => void
  onCancel: () => void
}

/**
 * A kind of item of the person's own (D232): a name and an ordered list
 * of fields, each with a label, one of the kinds a field of your own takes
 * and whether it is needed. Saved as an item of its own, so it syncs and
 * keeps a history; changing it later leaves items already made as they are.
 */
export function TemplateDesigner({ template, onDone, onCancel }: TemplateDesignerProps) {
  const client = useClient()
  const { templates } = useTemplates()
  const kinds = templates?.custom_kinds ?? []
  const [name, setName] = useState(template?.name ?? "")
  const [summary, setSummary] = useState(template?.summary ?? "")
  const [fields, setFields] = useState<DraftField[]>(
    template?.fields.map((f) => ({ key: f.key, label: f.label, kind: f.kind, required: f.required ?? false })) ?? [{ key: null, label: "", kind: "text", required: false }],
  )
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const patch = (i: number, p: Partial<DraftField>) => setFields((fs) => fs.map((f, j) => (j === i ? { ...f, ...p } : f)))
  const move = (i: number, by: number) =>
    setFields((fs) => {
      const j = i + by
      if (j < 0 || j >= fs.length) return fs
      const out = [...fs]
      const [f] = out.splice(i, 1)
      out.splice(j, 0, f as DraftField)
      return out
    })

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      await client.saveUserTemplate({
        id: template?.id,
        vault: template?.vault_id,
        template: { name: name.trim(), summary: summary.trim(), fields: draftToFields(fields.filter((f) => f.label.trim())) },
      })
      onDone(template ? `Saved "${name.trim()}"; items made from it keep their fields.` : `Made the template "${name.trim()}".`)
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <form onSubmit={submit} className="grid gap-4" aria-busy={busy}>
      <DialogHeader>
        <DialogTitle>{template ? `Change "${template.name}"` : "New template"}</DialogTitle>
        <DialogDescription>A kind of item of your own: its fields, in order. It syncs like an item, and New item lists it.</DialogDescription>
      </DialogHeader>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="template-name">Name</FieldLabel>
          <Input id="template-name" value={name} onChange={(e) => setName(e.target.value)} required maxLength={60} autoComplete="off" autoFocus />
        </Field>
        <Field>
          <FieldLabel htmlFor="template-summary">What it's for</FieldLabel>
          <Input id="template-summary" value={summary} onChange={(e) => setSummary(e.target.value)} maxLength={120} autoComplete="off" />
        </Field>
        <FieldSet>
          <FieldLegend variant="label">Fields</FieldLegend>
          <FieldGroup>
            {fields.map((f, i) => (
              <Field key={i} orientation="horizontal">
                <Input aria-label={`Field ${i + 1}'s label`} placeholder="Label" value={f.label} onChange={(e) => patch(i, { label: e.target.value })} maxLength={60} autoComplete="off" />
                <NativeSelect aria-label={`Field ${i + 1}'s kind`} value={f.kind} onChange={(e) => patch(i, { kind: e.target.value })}>
                  {kinds.map((k) => (
                    <NativeSelectOption key={k.kind} value={k.kind}>
                      {k.label}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
                <Switch aria-label={`Field ${i + 1} is needed`} checked={f.required} onCheckedChange={(required) => patch(i, { required })} />
                <IconButton label={`Move field ${i + 1} up`} icon="arrow_upward" size="icon-sm" onClick={() => move(i, -1)} />
                <IconButton label={`Move field ${i + 1} down`} icon="arrow_downward" size="icon-sm" onClick={() => move(i, 1)} />
                <IconButton label={`Drop field ${i + 1}`} icon="close" size="icon-sm" onClick={() => setFields((fs) => fs.filter((_, j) => j !== i))} />
              </Field>
            ))}
            <Button type="button" variant="outline" size="sm" onClick={() => setFields((fs) => [...fs, { key: null, label: "", kind: "text", required: false }])}>
              <Icon name="add" data-icon="inline-start" />
              Add a field
            </Button>
          </FieldGroup>
        </FieldSet>
      </FieldGroup>
      {error && <FieldError role="alert">{error}</FieldError>}
      <LabelledActions>
        <LabelledAction kind="cancel" type="button" onClick={onCancel} disabled={busy}>
          Cancel
        </LabelledAction>
        <LabelledAction kind="confirm" type="submit" disabled={busy}>
          {busy ? "Saving…" : "Save template"}
        </LabelledAction>
      </LabelledActions>
    </form>
  )
}

interface TemplatesManagerProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onNotice: (text: string) => void
}

/** The person's templates: each to change or delete, and a new one (D232). */
export function TemplatesManager({ open, onOpenChange, onNotice }: TemplatesManagerProps) {
  const client = useClient()
  const { userTemplates, refreshUserTemplates } = useTemplates()
  const [editing, setEditing] = useState<UserTemplateView | "new" | null>(null)
  const [deleting, setDeleting] = useState<UserTemplateView | null>(null)
  const [error, setError] = useState<string | null>(null)

  const done = (notice: string) => {
    setEditing(null)
    refreshUserTemplates()
    onNotice(notice)
  }

  const remove = async () => {
    const t = deleting
    setDeleting(null)
    if (!t) return
    try {
      await client.deleteUserTemplate(t.id)
      refreshUserTemplates()
      onNotice(`"${t.name}" is in the trash; items made from it keep their fields.`)
    } catch (e) {
      setError(describeError(e))
    }
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (!o) setEditing(null)
        onOpenChange(o)
      }}
    >
      <DialogContent size="lg">
        {editing ? (
          <TemplateDesigner template={editing === "new" ? null : editing} onDone={done} onCancel={() => setEditing(null)} />
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>Your templates</DialogTitle>
              <DialogDescription>Kinds of item you made. Changing one leaves items already made from it as they are.</DialogDescription>
            </DialogHeader>
            {userTemplates.length === 0 ? (
              <p className="text-sm text-muted-foreground">None yet. A template is a form of your own: a gym locker, a pet's microchip, a club's door codes.</p>
            ) : (
              <ItemGroup>
                {userTemplates.map((t) => (
                  <Item key={t.id} size="sm">
                    <ItemMedia variant="icon">
                      <Icon name="category" />
                    </ItemMedia>
                    <ItemContent>
                      <ItemTitle>{t.name}</ItemTitle>
                      <ItemDescription>{t.fields.map((f) => f.label).join(" · ")}</ItemDescription>
                    </ItemContent>
                    <ItemActions>
                      <IconButton label={`Change ${t.name}`} icon="edit" size="icon-sm" onClick={() => setEditing(t)} />
                      <IconButton label={`Delete ${t.name}`} icon="delete" size="icon-sm" variant="ghost-destructive" onClick={() => setDeleting(t)} />
                    </ItemActions>
                  </Item>
                ))}
              </ItemGroup>
            )}
            {error && <FieldError role="alert">{error}</FieldError>}
            <LabelledActions>
              <LabelledAction kind="cancel" onClick={() => onOpenChange(false)}>
                Close
              </LabelledAction>
              <LabelledAction kind="confirm" onClick={() => setEditing("new")}>
                New template
              </LabelledAction>
            </LabelledActions>
          </>
        )}
      </DialogContent>
      <Dialog open={deleting !== null} onOpenChange={(o) => !o && setDeleting(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete "{deleting?.name}"?</DialogTitle>
            <DialogDescription>It goes to the trash like an item. Items you made from it stay as they are.</DialogDescription>
          </DialogHeader>
          <LabelledActions>
            <LabelledAction kind="cancel" onClick={() => setDeleting(null)}>
              Cancel
            </LabelledAction>
            <LabelledAction kind="destructive" onClick={() => void remove()}>
              Delete
            </LabelledAction>
          </LabelledActions>
        </DialogContent>
      </Dialog>
    </Dialog>
  )
}
