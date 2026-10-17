import { useMemo, useState, type FormEvent } from "react"

import {
  Button,
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
  Input,
  NativeSelect,
  NativeSelectOption,
  Spinner,
  Switch,
} from "@unicron/ui"
import { CrownJewelMark } from "@unicron/ui/security/crown-jewel"

import { useGate } from "@/app/gate"
import { useTemplates } from "@/app/templates"
import { describeError, useClient } from "@/core/client"
import type { ItemDetail, TemplateView } from "@/core/types"

import { DERIVED_TITLE, buildChanges, customRow, initialRows, wipeRows, type EditorRow } from "./editor-model"
import { FieldControl } from "./field-control"

export interface EditorTarget {
  /** The kind to make, or the item's kind. */
  kind: string
  /** The item to change, or `null` to make one. */
  item: ItemDetail | null
}

interface ItemEditorProps {
  /** `null` when closed. */
  target: EditorTarget | null
  onClose: () => void
  /** Saved: the item's id, and whether the core generated a value the person should copy from the item. */
  onSaved: (id: string, generated: boolean) => void
}

/**
 * The one editor for every kind (D110), drawn from the core's template:
 * the fields in the template's order and sections, then anything else the
 * item holds, then fields the person adds by hand. Saving sends each
 * field once: a secret as bytes when retyped, `keep` when not (its bytes
 * never cross back), `generate` for the core to make. The dialog is
 * unicron's `DialogContent` at `size="lg"`.
 */
export function ItemEditor({ target, onClose, onSaved }: ItemEditorProps) {
  const { forKind, error: templatesError, templates } = useTemplates()
  const template = target ? forKind(target.kind) : null
  return (
    <Dialog open={target !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent size="lg">
        {target && template ? (
          <EditorForm key={`${target.kind}:${target.item?.id ?? "new"}:${target.item?.version ?? 0}`} template={template} item={target.item} onClose={onClose} onSaved={onSaved} />
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>{target?.item ? "Edit" : "New item"}</DialogTitle>
              <DialogDescription>{templatesError ?? (templates ? "This kind of item can't be edited here." : "Loading the item kinds…")}</DialogDescription>
            </DialogHeader>
            {!templatesError && !templates && <Spinner />}
            {templatesError && <FieldError role="alert">{templatesError}</FieldError>}
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}

function plainTitle(item: ItemDetail | null): string {
  return item?.fields.find((f) => f.field_id === "title")?.value ?? ""
}

function EditorForm({ template, item, onClose, onSaved }: { template: TemplateView; item: ItemDetail | null; onClose: () => void; onSaved: ItemEditorProps["onSaved"] }) {
  const client = useClient()
  const gate = useGate()
  const { templates } = useTemplates()
  const editing = item !== null
  const [title, setTitle] = useState(plainTitle(item))
  const [rows, setRows] = useState<EditorRow[]>(() => initialRows(template, item))
  const [crownJewel, setCrownJewel] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [adding, setAdding] = useState<{ label: string; kind: string }>({ label: "", kind: "text" })
  const [added, setAdded] = useState(0)
  const customKinds = templates?.custom_kinds ?? []

  const patch = (key: string, p: Partial<EditorRow>) => setRows((rs) => rs.map((r) => (r.key === key ? { ...r, ...p } : r)))
  const sections = useMemo(() => {
    const out: { name: string; rows: EditorRow[] }[] = []
    for (const r of rows) {
      const name = r.template?.section ?? (r.fieldId === null || r.fieldId.startsWith("custom.") ? "Your fields" : "Other fields")
      const s = out.find((x) => x.name === name)
      if (s) s.rows.push(r)
      else out.push({ name, rows: [r] })
    }
    return out
  }, [rows])

  const addField = () => {
    const label = adding.label.trim()
    if (!label) return
    setRows((rs) => [...rs, customRow(added, label, adding.kind)])
    setAdded((n) => n + 1)
    setAdding({ label: "", kind: adding.kind })
  }

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    const changes = buildChanges(rows)
    const generated = changes.some((c) => c.action === "generate")
    try {
      let id: string
      if (item) {
        const t = title.trim()
        await gate.run((g) => client.editItem({ id: item.id, title: t && t !== plainTitle(item) ? t : undefined, changes }, g))
        id = item.id
      } else {
        id = await client.addItem({ kind: template.kind, title: title.trim() || undefined, crownJewel: crownJewel || undefined, fields: changes })
      }
      setRows(wipeRows)
      onSaved(id, generated)
    } catch (err) {
      setError(describeError(err))
    } finally {
      for (const c of changes) c.secret?.fill(0)
      setBusy(false)
    }
  }

  const close = () => {
    setRows(wipeRows)
    onClose()
  }

  const titleHint = editing ? undefined : DERIVED_TITLE[template.kind]
  return (
    <form onSubmit={submit} className="grid gap-4" aria-busy={busy}>
      <DialogHeader>
        <DialogTitle>{editing ? `Edit ${plainTitle(item) || template.name.toLowerCase()}` : `New ${template.name.toLowerCase()}`}</DialogTitle>
        <DialogDescription>{editing ? "Saves a new version; the old one stays in the history." : template.summary}</DialogDescription>
      </DialogHeader>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="editor-title">Title</FieldLabel>
          <Input id="editor-title" value={title} onChange={(e) => setTitle(e.target.value)} required={!titleHint} autoComplete="off" autoFocus />
          {titleHint && <FieldDescription>{titleHint}</FieldDescription>}
        </Field>
        {sections.map((s) => (
          <FieldSet key={s.name}>
            <FieldLegend variant="label">{s.name}</FieldLegend>
            <FieldGroup>
              {s.rows.map((r) => (
                <FieldControl
                  key={r.key}
                  row={r}
                  onChange={(p) => patch(r.key, p)}
                  onDiscard={r.fieldId === null ? () => setRows((rs) => rs.filter((x) => x.key !== r.key)) : undefined}
                />
              ))}
            </FieldGroup>
          </FieldSet>
        ))}
        <FieldSet>
          <FieldLegend variant="label">Add a field of your own</FieldLegend>
          <Field orientation="horizontal">
            <Input aria-label="New field's label" placeholder="Label" value={adding.label} onChange={(e) => setAdding({ ...adding, label: e.target.value })} autoComplete="off" />
            <NativeSelect aria-label="New field's kind" value={adding.kind} onChange={(e) => setAdding({ ...adding, kind: e.target.value })}>
              {customKinds.map((k) => (
                <NativeSelectOption key={k.kind} value={k.kind}>
                  {k.label}
                </NativeSelectOption>
              ))}
            </NativeSelect>
            <Button type="button" variant="outline" size="sm" onClick={addField} disabled={!adding.label.trim()}>
              Add field
            </Button>
          </Field>
        </FieldSet>
        {!editing &&
          (template.crown_jewel ? (
            <p className="flex items-center gap-2 text-sm text-muted-foreground">
              <CrownJewelMark size="sm" />
              Always kept as a crown jewel: showing or using it asks it's you first, and it is never filled into a page.
            </p>
          ) : (
            <Field orientation="horizontal">
              <Switch id="editor-crown-jewel" checked={crownJewel} onCheckedChange={setCrownJewel} />
              <FieldContent>
                <FieldLabel htmlFor="editor-crown-jewel">Keep as a crown jewel</FieldLabel>
                <FieldDescription>Showing or using it will ask it's you first, and it is never filled into a page. This can't be undone later.</FieldDescription>
              </FieldContent>
            </Field>
          ))}
      </FieldGroup>
      {error && <FieldError role="alert">{error}</FieldError>}
      <DialogActions>
        <DialogAction kind="cancel" type="button" onClick={close} disabled={busy}>
          Cancel
        </DialogAction>
        <DialogAction kind="confirm" type="submit" disabled={busy}>
          {busy ? "Saving…" : editing ? "Save" : "Add"}
        </DialogAction>
      </DialogActions>
    </form>
  )
}
