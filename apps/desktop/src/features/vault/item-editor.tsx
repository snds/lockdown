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

import { folderTree } from "@/features/organization/org-model"
import { TagsEditor } from "@/features/organization/tags-editor"
import { useVaults } from "@/features/organization/vaults-context"

import { DERIVED_TITLE, buildChanges, customRow, initialRows, isSecretControl, repeatRow, wipeRows, type EditorRow } from "./editor-model"
import { FieldControl } from "./field-control"

export interface EditorTarget {
  /**
   * The template to make from, or the item's (D231, D232): a kind
   * (`login`), a variant (`identity/passport`) or one of the person's own
   * (`user:<id>`).
   */
  kind: string
  /** The item to change, or `null` to make one. */
  item: ItemDetail | null
  /** A new item's vault, folder and tags (the list's scope when New item was pressed). */
  vaultId?: string
  folder?: string
  tags?: string[]
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
  const { forTemplate, error: templatesError, templates } = useTemplates()
  const template = target ? forTemplate(target.kind, target.item?.kind) : null
  return (
    <Dialog open={target !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent size="lg">
        {target && template ? (
          <EditorForm key={`${target.kind}:${target.item?.id ?? "new"}:${target.item?.version ?? 0}`} template={template} item={target.item} target={target} onClose={onClose} onSaved={onSaved} />
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

/** A login's first website and its own password rules, from the rows being edited (D236). */
function siteOf(rows: EditorRow[]): { url?: string; rule?: string } {
  return { url: rows.find((r) => r.fieldId === "url.0")?.text || undefined, rule: rows.find((r) => r.fieldId === "pw.rule")?.text || undefined }
}

function plainTitle(item: ItemDetail | null): string {
  return item?.fields.find((f) => f.field_id === "title")?.value ?? ""
}

function EditorForm({
  template,
  item,
  target,
  onClose,
  onSaved,
}: {
  template: TemplateView
  item: ItemDetail | null
  target: EditorTarget
  onClose: () => void
  onSaved: ItemEditorProps["onSaved"]
}) {
  const client = useClient()
  const { vaults } = useVaults()
  const [vaultId, setVaultId] = useState(item?.vault_id ?? target.vaultId ?? vaults[0]?.id ?? "")
  const startOrg = item?.organization ?? { folder: target.folder ?? null, tags: target.tags ?? [], favourite: false }
  const [folder, setFolder] = useState(startOrg.folder ?? "")
  const [tags, setTags] = useState(startOrg.tags)
  const [favourite, setFavourite] = useState(startOrg.favourite)
  const vault = vaults.find((v) => v.id === vaultId)
  const repeating = template.fields.filter((t) => t.repeats)
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

  // Plain words from this form that a password shouldn't lean on (the strength estimate's context).
  const context = useMemo(
    () => [title, ...rows.filter((r) => !isSecretControl(r.control) && (r.control === "text" || r.control === "email" || r.control === "url")).map((r) => r.text)].map((w) => w.trim()).filter(Boolean),
    [title, rows],
  )

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
    // Where it is filed rides in the same save, as the core's organization
    // fields, sent only when it differs from what the item had (nothing, for a new one).
    const was = item?.organization ?? { folder: null, tags: [], favourite: false }
    const filed = vault?.folders.some((f) => f.id === folder) ? folder : ""
    if (filed !== (was.folder ?? "")) changes.push({ field_id: "org.folder", action: "set", text: filed })
    if (tags.join("\n") !== was.tags.join("\n")) changes.push({ field_id: "org.tags", action: "set", text: tags.join("\n") })
    if (favourite !== was.favourite) changes.push({ field_id: "org.favourite", action: "set", flag: favourite })
    const generated = changes.some((c) => c.action === "generate")
    try {
      let id: string
      if (item) {
        const t = title.trim()
        await gate.run((g) => client.editItem({ id: item.id, title: t && t !== plainTitle(item) ? t : undefined, changes }, g))
        id = item.id
      } else {
        const input = { kind: template.id ?? template.kind, title: title.trim() || undefined, crownJewel: crownJewel || undefined, fields: changes }
        id = vaultId && vaults.length > 1 ? await client.addItemIn(vaultId, input) : await client.addItem(input)
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
              {s.rows.filter((r) => !r.quiet).map((r) => (
                <FieldControl
                  key={r.key}
                  row={r}
                  onChange={(p) => patch(r.key, p)}
                  context={context}
                  linkScope={{ vaultId, itemId: item?.id }}
                  site={r.fieldId === "pw" && template.kind === "login" ? siteOf(rows) : undefined}
                  onDiscard={r.fieldId === null || (!r.exists && r.template?.repeats && r.fieldId !== r.template.field_id) ? () => setRows((rs) => rs.filter((x) => x.key !== r.key)) : undefined}
                />
              ))}
              {repeating
                .filter((t) => t.section === s.name)
                .map((t) => (
                  <Button key={t.field_id} type="button" variant="outline" size="sm" onClick={() => setRows((rs) => [...rs, repeatRow(rs, t)])}>
                    Add another {t.label.toLowerCase()}
                  </Button>
                ))}
            </FieldGroup>
          </FieldSet>
        ))}
        <FieldSet>
          <FieldLegend variant="label">Organization</FieldLegend>
          <FieldGroup>
            {!editing && vaults.length > 1 && (
              <Field>
                <FieldLabel htmlFor="editor-vault">Vault</FieldLabel>
                <NativeSelect
                  id="editor-vault"
                  value={vaultId}
                  onChange={(e) => {
                    setVaultId(e.target.value)
                    setFolder("")
                  }}
                >
                  {vaults.map((v) => (
                    <NativeSelectOption key={v.id} value={v.id}>
                      {v.name}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </Field>
            )}
            {(vault?.folders.length ?? 0) > 0 && (
              <Field>
                <FieldLabel htmlFor="editor-folder">Folder</FieldLabel>
                <NativeSelect id="editor-folder" value={folder} onChange={(e) => setFolder(e.target.value)}>
                  <NativeSelectOption value="">No folder</NativeSelectOption>
                  {folderTree(vault?.folders ?? []).map((f) => (
                    <NativeSelectOption key={f.id} value={f.id}>
                      {f.path}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </Field>
            )}
            <Field>
              <FieldLabel htmlFor="editor-tags">Tags</FieldLabel>
              <TagsEditor id="editor-tags" tags={tags} onChange={setTags} />
            </Field>
            <Field orientation="horizontal">
              <Switch id="editor-favourite" checked={favourite} onCheckedChange={setFavourite} />
              <FieldContent>
                <FieldLabel htmlFor="editor-favourite">Favourite</FieldLabel>
                <FieldDescription>Listed first in quick access.</FieldDescription>
              </FieldContent>
            </Field>
          </FieldGroup>
        </FieldSet>
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
