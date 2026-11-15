import { useState, type FormEvent } from "react"

import {
  AdaptiveDialog,
  Banner,
  Button,
  Checkbox,
  ConfirmDialog,
  EmptyState,
  Icon,
  IconButton,
  ListRow,
  NativeSelect,
  NativeSelectOption,
  SectionHeader,
  TextField,
} from "@unicron/ui"

import { useTemplates } from "@/app/templates"
import { describeError, useClient } from "@/core/client"
import type { UserTemplateView } from "@/core/types"
import { count } from "@/lib/format"

import { draftToFields, type DraftField } from "./depth-model"

const DESIGNER_FORM = "template-designer-form"

interface TemplateDesignerProps {
  /** `null` to make a new one. */
  template: UserTemplateView | null
  onDone: (notice: string) => void
  onCancel: () => void
  /** The form's busy state and its yes, for the layer's footer. */
  onBusy?: (busy: boolean) => void
}

/** How many fields, in the row's caps: `03 FIELDS`. */
const fieldsWord = (n: number) => `${count(n)} ${n === 1 ? "field" : "fields"}`

/**
 * A kind of item of the person's own (D232; brief E4): a name, what it's
 * for, and an ordered list of fields, each with a label, one of the kinds a
 * field of your own takes and whether it is needed. Saved as an item of its
 * own, so it syncs and keeps a history; changing it later leaves items
 * already made as they are. The fields only: the layer around it draws the
 * header and the footer (BACK and SAVE TEMPLATE).
 */
export function TemplateDesigner({ template, onDone, onBusy }: TemplateDesignerProps) {
  const client = useClient()
  const { templates } = useTemplates()
  const kinds = templates?.custom_kinds ?? []
  const [name, setName] = useState(template?.name ?? "")
  const [summary, setSummary] = useState(template?.summary ?? "")
  const [fields, setFields] = useState<DraftField[]>(
    template?.fields.map((f) => ({ key: f.key, label: f.label, kind: f.kind, required: f.required ?? false })) ?? [{ key: null, label: "", kind: "text", required: false }],
  )
  const [error, setError] = useState<string | null>(null)

  const patch = (i: number, p: Partial<DraftField>) => setFields((fs) => fs.map((f, j) => (j === i ? { ...f, ...p } : f)))
  const move = (i: number, by: number) => {
    const j = i + by
    if (j < 0 || j >= fields.length) return
    setFields((fs) => {
      const out = [...fs]
      const [f] = out.splice(i, 1)
      out.splice(j, 0, f as DraftField)
      return out
    })
    // Focus stays on the field that moved: its own button at its new place.
    requestAnimationFrame(() => document.getElementById(`template-field-${j}-${by < 0 ? "up" : "down"}`)?.focus())
  }

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    onBusy?.(true)
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
      onBusy?.(false)
    }
  }

  return (
    <form id={DESIGNER_FORM} onSubmit={(e) => void submit(e)} className="flex flex-col gap-6">
      <TextField id="template-name" label="Name" value={name} onChange={(e) => setName(e.target.value)} required maxLength={60} autoComplete="off" />
      <TextField id="template-summary" label="What it's for" value={summary} onChange={(e) => setSummary(e.target.value)} maxLength={120} autoComplete="off" />
      <section aria-label="Fields" className="flex flex-col gap-4">
        <SectionHeader title="Fields" count={fields.length} level={3} />
        <ol className="flex flex-col gap-4">
          {fields.map((f, i) => (
            <li key={i} className="flex flex-wrap items-center gap-2">
              <div className="min-w-0 flex-1 basis-48">
                <TextField aria-label={`Field ${i + 1}'s label`} placeholder="Label" value={f.label} onChange={(e) => patch(i, { label: e.target.value })} maxLength={60} autoComplete="off" />
              </div>
              <NativeSelect aria-label={`Field ${i + 1}'s kind`} value={f.kind} onChange={(e) => patch(i, { kind: e.target.value })}>
                {kinds.map((k) => (
                  <NativeSelectOption key={k.kind} value={k.kind}>
                    {k.label}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
              <label className="type-label text-text-secondary flex items-center gap-2">
                <Checkbox aria-label={`Field ${i + 1} is needed`} checked={f.required} onCheckedChange={(required) => patch(i, { required })} />
                Needed
              </label>
              <IconButton id={`template-field-${i}-up`} label={`Move field ${i + 1} up`} icon="arrow--up" size="icon-sm" onClick={() => move(i, -1)} />
              <IconButton id={`template-field-${i}-down`} label={`Move field ${i + 1} down`} icon="arrow--down" size="icon-sm" onClick={() => move(i, 1)} />
              <IconButton label={`Drop field ${i + 1}`} icon="close" size="icon-sm" onClick={() => setFields((fs) => fs.filter((_, j) => j !== i))} />
            </li>
          ))}
        </ol>
        <div>
          <Button type="button" variant="ghost" size="sm" onClick={() => setFields((fs) => [...fs, { key: null, label: "", kind: "text", required: false }])}>
            <Icon name="add" data-icon="inline-start" />
            Add a field
          </Button>
        </div>
      </section>
      {error && <Banner tone="critical" size="compact" title="Not saved" body={error} />}
    </form>
  )
}

interface TemplatesManagerProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onNotice: (text: string) => void
}

/**
 * Your templates (D232; brief E4): one layer, two steps. The list (each
 * template's name, what it's for and how many fields; CHANGE and DELETE;
 * NEW TEMPLATE), then the designer, whose BACK returns to the list. A
 * dialog, form 640, on a desktop; a full-height sheet on a phone. Delete
 * asks first, over the layer.
 */
export function TemplatesManager({ open, onOpenChange, onNotice }: TemplatesManagerProps) {
  const client = useClient()
  const { userTemplates, refreshUserTemplates } = useTemplates()
  const [editing, setEditing] = useState<UserTemplateView | "new" | null>(null)
  const [deleting, setDeleting] = useState<UserTemplateView | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const done = (notice: string) => {
    setEditing(null)
    refreshUserTemplates()
    onNotice(notice)
  }

  const remove = async () => {
    const t = deleting
    if (!t) return
    setError(null)
    try {
      await client.deleteUserTemplate(t.id)
      setDeleting(null)
      refreshUserTemplates()
      onNotice(`"${t.name}" is in Recently deleted; items made from it keep their fields.`)
    } catch (e) {
      setError(describeError(e))
      throw e
    }
  }

  const designing = editing !== null
  const list = userTemplates.length
  return (
    <>
      <AdaptiveDialog
        open={open}
        onOpenChange={(o) => {
          if (!o) setEditing(null)
          onOpenChange(o)
        }}
        size="form"
        height="full"
        overline={designing ? (editing === "new" ? "Your templates // new" : "Your templates // change") : `Your templates ${count(list)}`}
        title={designing ? (editing === "new" ? "New template" : editing.name) : "Your templates"}
        footer={
          designing ? (
            <>
              <Button type="button" variant="secondary" onClick={() => setEditing(null)} disabled={busy}>
                Back
              </Button>
              <Button type="submit" form={DESIGNER_FORM} loading={busy}>
                Save template
              </Button>
            </>
          ) : (
            <>
              <Button type="button" variant="secondary" onClick={() => onOpenChange(false)}>
                Close
              </Button>
              {list > 0 && (
                <Button type="button" onClick={() => setEditing("new")}>
                  <Icon name="add" data-icon="inline-start" />
                  New template
                </Button>
              )}
            </>
          )
        }
      >
        {designing ? (
          <TemplateDesigner key={editing === "new" ? "new" : editing.id} template={editing === "new" ? null : editing} onDone={done} onCancel={() => setEditing(null)} onBusy={setBusy} />
        ) : list === 0 ? (
          <EmptyState
            overline="Your templates // 00"
            title="No templates yet."
            body="A template is a form of your own: a gym locker, a pet's microchip, a club's door codes."
            actionLabel="New template"
            onAction={() => setEditing("new")}
          />
        ) : (
          <div className="flex flex-col gap-4">
            <p className="type-caption text-text-secondary">Kinds of item you made. Changing one leaves items already made from it as they are.</p>
            <ul aria-label="Your templates" className="flex flex-col">
              {userTemplates.map((t) => (
                <li key={t.id}>
                  <ListRow
                    size="compact"
                    leading={<Icon name="categories" />}
                    title={t.name}
                    detail={t.summary || undefined}
                    meta={fieldsWord(t.fields.length)}
                    action={
                      <span className="flex items-center gap-1">
                        <Button type="button" variant="ghost" size="sm" aria-label={`Change ${t.name}`} onClick={() => setEditing(t)}>
                          Change
                        </Button>
                        <Button type="button" variant="ghost" size="sm" aria-label={`Delete ${t.name}`} onClick={() => setDeleting(t)}>
                          Delete
                        </Button>
                      </span>
                    }
                  />
                </li>
              ))}
            </ul>
          </div>
        )}
      </AdaptiveDialog>
      <ConfirmDialog
        open={deleting !== null}
        onOpenChange={(o) => {
          if (!o) {
            setDeleting(null)
            setError(null)
          }
        }}
        overline="Your templates"
        title={`Delete ${deleting?.name ?? "this template"}?`}
        description="It goes to Recently deleted like an item. Items you made from it stay as they are."
        confirmLabel="Delete"
        tone="destructive"
        error={error}
        onConfirm={remove}
      />
    </>
  )
}
