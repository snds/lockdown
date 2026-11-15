import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type ReactNode } from "react"

import {
  AdaptiveMenu,
  AdaptiveMenuRadioGroup,
  AdaptiveMenuRadioItem,
  Banner,
  Button,
  ConfirmDialog,
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogOverline,
  DialogTitle,
  Field,
  FieldLabel,
  FieldRow,
  FieldRowAction,
  FieldRowValue,
  FloatButton,
  Icon,
  NativeSelect,
  NativeSelectOption,
  SectionHeader,
  Sheet,
  SheetBody,
  SheetContent,
  Skeleton,
  TextField,
  ToggleRow,
  TopBar,
  useAdaptiveSheet,
  useViewport,
} from "@unicron/ui"
import { CrownJewelMark } from "@unicron/ui/security/crown-jewel"

import { useGate } from "@/app/gate"
import { useTemplates } from "@/app/templates"
import { describeError, useClient } from "@/core/client"
import type { ItemDetail, TemplateView } from "@/core/types"

import { folderTree } from "@/features/organization/org-model"
import { TagsEditor } from "@/features/organization/tags-editor"
import { useVaults } from "@/features/organization/vaults-context"
import { AliasField } from "@/features/bridges/alias-field"

import { DERIVED_TITLE, ESSENTIALS, buildChanges, customRow, holdsValue, hostOf, inSentence, initialRows, isEssential, repeatRow, wipeRows, type EditorRow } from "./editor-model"
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
  /** A new login's first website, already typed (Search's "New login" passes the query). Read only when making one. */
  website?: string
}

interface ItemEditorProps {
  /** `null` when closed. */
  target: EditorTarget | null
  onClose: () => void
  /** Saved: the item's id, and whether the core generated a value the person should copy from the item. */
  onSaved: (id: string, generated: boolean) => void
}

const FORM_ID = "item-editor-form"

/** What the form tells the layer around it: its header's words, whether anything was typed, and how to save. */
interface FormHead {
  overline: string
  title: string
  /** The phone's top bar: the screen's own name ("New login"). */
  screen: string
  primary: string
  busy: boolean
}

/**
 * The one editor for every kind (D110; brief E2), drawn from the core's
 * template: a Form. On a desktop (and at the medium width) a dialog, form
 * 640 (Desktop / 05): the overline (`NEW LOGIN // PERSONAL VAULT`), the
 * title following what is typed, CANCEL and SAVE {KIND}. On a phone a
 * full-screen layer (Mobile / 07): close, the screen's name, and the
 * checkmark as the one primary in the top bar, where it stays while the
 * keyboard is up. The essentials come first; More options holds the rest
 * of the template, the organization block, fields of your own and the
 * crown-jewel switch. Closing with anything typed asks "Discard changes?",
 * focus on KEEP EDITING. Saving sends each field once: a secret as bytes
 * when retyped, `keep` when not (its bytes never cross back), `generate`
 * for the core to make.
 */
export function ItemEditor({ target, onClose, onSaved }: ItemEditorProps) {
  const { forTemplate, error: templatesError, templates } = useTemplates()
  const template = target ? forTemplate(target.kind, target.item?.kind) : null
  // A phone, or a phone turned sideways (a short screen): the full-height form, its primary in the top bar.
  const sheet = useAdaptiveSheet()
  const { short } = useViewport()
  const phone = sheet || short
  const dirty = useRef(false)
  const markDirty = useCallback((on = true) => {
    dirty.current = on
  }, [])
  const [asking, setAsking] = useState(false)
  const [head, setHead] = useState<FormHead | null>(null)
  const open = target !== null

  const close = () => {
    dirty.current = false
    setAsking(false)
    setHead(null)
    onClose()
  }
  const requestClose = () => {
    if (dirty.current) setAsking(true)
    else close()
  }
  const formKey = target ? `${target.kind}:${target.item?.id ?? "new"}:${target.item?.version ?? 0}` : "closed"
  const fallbackTitle = target?.item ? "Edit" : "New item"

  if (phone) {
    return (
      <Sheet open={open} onOpenChange={(o) => !o && requestClose()}>
        <SheetContent side="bottom" height="full" showCloseButton={false} aria-label={head?.screen ?? fallbackTitle}>
          <div className="px-4">
            <TopBar
              title={head?.screen ?? fallbackTitle}
              titleAs="h1"
              start={<FloatButton label="Close" icon="close" size="l" tooltip={false} onClick={requestClose} />}
              actions={head ? <FloatButton label={head.primary} icon="checkmark" kind="primary" size="l" type="submit" form={FORM_ID} disabled={head.busy} /> : undefined}
            />
          </div>
          <SheetBody>{body()}</SheetBody>
          {question()}
        </SheetContent>
      </Sheet>
    )
  }
  return (
    <Dialog open={open} onOpenChange={(o) => !o && requestClose()}>
      <DialogContent size="form">
        <DialogHeader>
          <DialogOverline>{head?.overline ?? (target?.item ? "Edit" : "New item")}</DialogOverline>
          <DialogTitle>{head?.title ?? fallbackTitle}</DialogTitle>
        </DialogHeader>
        {body()}
        {question()}
      </DialogContent>
    </Dialog>
  )

  // Declared after the returns, so the layer is the first thing this component draws.
  function body() {
    if (!target || !template) return <NotReady error={templatesError} loaded={templates !== null} onClose={close} />
    return (
      <EditorForm
        key={formKey}
        template={template}
        item={target.item}
        target={target}
        phone={phone}
        onCancel={requestClose}
        onSaved={(id, generated) => {
          dirty.current = false
          onSaved(id, generated)
        }}
        onDirty={markDirty}
        onHead={setHead}
      />
    )
  }

  function question() {
    return (
      <ConfirmDialog
        open={asking}
        onOpenChange={(o) => !o && setAsking(false)}
        title="Discard changes?"
        description="What you typed here isn't saved."
        confirmLabel="Discard"
        cancelLabel="Keep editing"
        tone="destructive"
        onConfirm={close}
      />
    )
  }
}

/** Before the form: the kinds still loading (skeleton fields), they failed, or a kind this build can't edit. */
function NotReady({ error, loaded, onClose }: { error: string | null; loaded: boolean; onClose: () => void }) {
  if (!error && !loaded)
    return (
      <div role="status" aria-label="Loading the item kinds…" className="flex flex-col gap-4">
        <Skeleton variant="row" />
        <Skeleton variant="row" />
        <Skeleton variant="row" />
        <span className="sr-only">Loading the item kinds…</span>
      </div>
    )
  return (
    <div className="flex flex-col gap-4">
      <p className="type-body text-text-secondary">{error ?? "This kind of item can't be edited here."}</p>
      <div>
        <Button variant="secondary" onClick={onClose}>
          Close
        </Button>
      </div>
    </div>
  )
}

/** A login's first website and its own password rules, from the rows being edited (D236). */
function siteOf(rows: EditorRow[]): { url?: string; rule?: string } {
  return { url: rows.find((r) => r.fieldId === "url.0")?.text || undefined, rule: rows.find((r) => r.fieldId === "pw.rule")?.text || undefined }
}

function plainTitle(item: ItemDetail | null): string {
  return item?.fields.find((f) => f.field_id === "title")?.value ?? ""
}

/** The title the dialog shows while a new item is typed: the website's host, the network's name, else the title. */
function liveTitle(template: TemplateView, rows: EditorRow[], title: string): string {
  const of = (id: string) => rows.find((r) => r.fieldId === id)?.text.trim() ?? ""
  if (template.kind === "login") return hostOf(of("url.0")) || title.trim()
  if (template.kind === "wi-fi") return of("wifi.ssid") || title.trim()
  return title.trim()
}

/** Plain words for a group of fields: each section is a group named by its header. */
function Group({ title, children, count }: { title: string; children: ReactNode; count?: number }) {
  return (
    <section aria-label={title} className="flex flex-col gap-4">
      <SectionHeader title={title} level={3} count={count} />
      {children}
    </section>
  )
}

function EditorForm({
  template,
  item,
  target,
  phone,
  onCancel,
  onSaved,
  onDirty,
  onHead,
}: {
  template: TemplateView
  item: ItemDetail | null
  target: EditorTarget
  phone: boolean
  onCancel: () => void
  onSaved: ItemEditorProps["onSaved"]
  onDirty: (on?: boolean) => void
  onHead: (head: FormHead) => void
}) {
  const client = useClient()
  const { vaults } = useVaults()
  const gate = useGate()
  const { templates } = useTemplates()
  const editing = item !== null
  const [vaultId, setVaultIdRaw] = useState(item?.vault_id ?? target.vaultId ?? vaults.find((v) => v.primary)?.id ?? vaults[0]?.id ?? "")
  const startOrg = item?.organization ?? { folder: target.folder ?? null, tags: target.tags ?? [], favourite: false }
  const [folder, setFolderRaw] = useState(startOrg.folder ?? "")
  const [tags, setTagsRaw] = useState(startOrg.tags)
  const [favourite, setFavouriteRaw] = useState(startOrg.favourite)
  const vault = vaults.find((v) => v.id === vaultId)
  const repeating = template.fields.filter((t) => t.repeats)
  const [title, setTitleRaw] = useState(plainTitle(item))
  const [startRows] = useState<EditorRow[]>(() => {
    const start = initialRows(template, item)
    // A new item made from a search that found nothing starts with what was searched for as its website.
    return item === null && target.website ? start.map((r) => (r.fieldId === "url.0" && !r.text ? { ...r, text: target.website ?? "" } : r)) : start
  })
  const [rows, setRows] = useState<EditorRow[]>(startRows)
  const [crownJewel, setCrownJewelRaw] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [aliasError, setAliasError] = useState<string | null>(null)
  const [adding, setAdding] = useState<{ label: string; kind: string } | null>(null)
  const [added, setAdded] = useState(0)
  const customKinds = templates?.custom_kinds ?? []
  const loginCode = template.kind === "login" ? rows.find((r) => r.fieldId === "code.0") : undefined
  const [withCode, setWithCodeRaw] = useState(Boolean(loginCode?.exists))

  const setTitle = setTitleRaw
  const setFolder = setFolderRaw
  const setTags = setTagsRaw
  const setFavourite = setFavouriteRaw
  const setCrownJewel = setCrownJewelRaw
  const patch = (key: string, p: Partial<EditorRow>) => setRows((rs) => rs.map((r) => (r.key === key ? { ...r, ...p } : r)))
  const setVaultId = (id: string) => {
    setVaultIdRaw(id)
    // Folders belong to a vault: another vault starts unfiled.
    setFolderRaw("")
  }
  const setWithCode = (on: boolean) => {
    setWithCodeRaw(on)
    if (!on && loginCode) setRows((rs) => rs.map((r) => (r.key === loginCode.key ? { ...r, text: "", secret: "type" } : r)))
  }

  // Whether anything differs from how the form opened, so closing asks first (brief E2). Read from
  // what is there now: a secret the window cleared while away (D315) no longer counts as typed.
  const changed =
    title !== plainTitle(item) ||
    folder !== (startOrg.folder ?? "") ||
    tags.join("\n") !== startOrg.tags.join("\n") ||
    favourite !== startOrg.favourite ||
    crownJewel ||
    vaultId !== (item?.vault_id ?? target.vaultId ?? vaults.find((v) => v.primary)?.id ?? vaults[0]?.id ?? "") ||
    rows.length !== startRows.length ||
    rows.some((r) => {
      const was = startRows.find((x) => x.key === r.key)
      return !was || r.text !== was.text || r.secret !== was.secret || r.flag !== was.flag || r.removed !== was.removed || r.matchRule !== was.matchRule || r.recipe !== was.recipe
    })
  useEffect(() => onDirty(changed), [changed, onDirty])

  const titleHint = DERIVED_TITLE[template.kind] ?? (template.id?.startsWith("user:") ? "Left empty, the template's name is used." : undefined)
  const shown = rows.filter((r) => !r.quiet)
  const order = ESSENTIALS[template.kind]
  const essential = shown
    .filter((r) => isEssential(template, r) && r.key !== loginCode?.key)
    .sort((a, b) => (order ? order.indexOf(a.fieldId ?? "") - order.indexOf(b.fieldId ?? "") : 0))
  const later = shown.filter((r) => !essential.includes(r) && r.key !== loginCode?.key)
  const sections: { name: string; rows: EditorRow[] }[] = []
  for (const r of later) {
    const section = r.template?.section ?? (r.fieldId === null || r.fieldId.startsWith("custom.") ? "Your fields" : "Other fields")
    const s = sections.find((x) => x.name === section)
    if (s) s.rows.push(r)
    else sections.push({ name: section, rows: [r] })
  }
  const own = sections.find((s) => s.name === "Your fields")
  const templateSections = sections.filter((s) => s !== own)
  // More options is open on an edit when anything inside holds a value; shut on a new item.
  const [moreOpen, setMoreOpen] = useState(() => editing && (later.some(holdsValue) || (Boolean(titleHint) && plainTitle(item) !== "") || (item?.organization?.tags.length ?? 0) > 0 || Boolean(item?.organization?.folder) || Boolean(item?.organization?.favourite)))

  // Plain words from this form that a password shouldn't lean on (the strength estimate's context).
  const context = useMemo(
    () => [title, ...rows.filter((r) => r.control === "text" || r.control === "email" || r.control === "url").map((r) => r.text)].map((w) => w.trim()).filter(Boolean),
    [title, rows],
  )

  const name = inSentence(template.name)
  const where = vault ? `${vault.name} vault` : ""
  const typed = liveTitle(template, rows, title)
  const head: FormHead = {
    overline: [editing ? `Edit ${name}` : `New ${name}`, where].filter(Boolean).join(" // "),
    title: editing ? plainTitle(item) || template.name : typed || `New ${name}`,
    screen: editing ? `Edit ${name}` : `New ${name}`,
    primary: editing ? "Save" : `Save ${name}`,
    busy,
  }
  const headKey = JSON.stringify(head)
  useEffect(() => {
    onHead(JSON.parse(headKey) as FormHead)
  }, [headKey, onHead])

  const addField = () => {
    const label = adding?.label.trim()
    if (!adding || !label) return
    setRows((rs) => [...rs, customRow(added, label, adding.kind)])
    setAdded((n) => n + 1)
    setAdding(null)
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

  const control = (r: EditorRow, rule: "inline" | "none" | "only" = "inline") => (
    <FieldControl
      key={rule === "only" ? `${r.key}:rule` : r.key}
      row={r}
      rule={rule}
      onChange={(p) => patch(r.key, p)}
      context={context}
      linkScope={{ vaultId, itemId: item?.id }}
      site={r.fieldId === "pw" && template.kind === "login" ? siteOf(rows) : undefined}
      onDiscard={
        r.fieldId === null || (!r.exists && r.template?.repeats && r.fieldId !== r.template.field_id)
          ? () => setRows((rs) => rs.filter((x) => x.key !== r.key))
          : undefined
      }
      trailing={
        r.fieldId === "user" && template.kind === "login" ? (
          <AliasField site={rows.find((x) => x.fieldId === "url.0")?.text ?? ""} onAddress={(address) => patch(r.key, { text: address })} onError={setAliasError} />
        ) : undefined
      }
      error={r.fieldId === "user" ? aliasError : undefined}
    />
  )
  const titleField = <TextField id="editor-title" label="Title" value={title} onChange={(e) => setTitle(e.target.value)} required={!titleHint} autoComplete="off" helper={titleHint} />
  const firstSite = essential.find((r) => r.fieldId === "url.0")

  const saveError = error && <Banner tone="critical" size="compact" title="Not saved" body={error} />
  const vaultRow = (
    <FieldRow
      label="Vault"
      size={phone ? "stacked" : "default"}
      rule="none"
      action={
        !editing && vaults.length > 1 ? (
          <AdaptiveMenu title="Save in which vault?" overline="Vault" trigger={<FieldRowAction aria-label="Change vault">Change</FieldRowAction>}>
            <AdaptiveMenuRadioGroup value={vaultId} onValueChange={setVaultId}>
              {vaults
                .filter((v) => v.sharing?.right !== "view")
                .map((v) => (
                  <AdaptiveMenuRadioItem key={v.id} value={v.id} icon="locked">
                    {v.name}
                  </AdaptiveMenuRadioItem>
                ))}
            </AdaptiveMenuRadioGroup>
          </AdaptiveMenu>
        ) : undefined
      }
    >
      <FieldRowValue>{vault?.name ?? "This vault"}</FieldRowValue>
    </FieldRow>
  )

  return (
    <form id={FORM_ID} onSubmit={(e) => void submit(e)} aria-busy={busy} className="contents">
      {/* While it saves the fields are read-only; the primary keeps its words and shows its spinner. */}
      <fieldset disabled={busy} className="contents">
        <EditorFields phone={phone}>
          {phone && saveError}
          {!titleHint && titleField}
          {essential.map((r) => control(r, r === firstSite ? "none" : "inline"))}
          {loginCode && !loginCode.exists && (
            <ToggleRow id="editor-add-code" label="Add a one-time code" description="Paste the set-up link the site gives you when you turn on two-step sign-in." checked={withCode} onCheckedChange={setWithCode} rule="bottom" />
          )}
          {loginCode && (withCode || loginCode.exists) && control(loginCode)}
          {vaultRow}

          <SectionHeader title="More options" level={2} collapsible open={moreOpen} onOpenChange={setMoreOpen}>
            <div className="flex flex-col gap-6 pt-2">
              <p className="type-caption text-text-secondary">A field left empty is taken off the item.</p>
              {titleHint && titleField}
              {firstSite && !templateSections.some((s) => s.name === firstSite.template?.section) && <Group title={firstSite.template?.section ?? "Websites"}>{control(firstSite, "only")}</Group>}
              {templateSections.map((s) => (
                <Group key={s.name} title={s.name}>
                  {/* The first website's box is among the essentials; its match rule leads its section here. */}
                  {firstSite && s.name === firstSite.template?.section && control(firstSite, "only")}
                  {s.rows.map((r) => control(r))}
                  {repeating
                    .filter((t) => t.section === s.name)
                    .map((t) => (
                      <div key={t.field_id}>
                        <Button type="button" variant="ghost" size="sm" onClick={() => setRows((rs) => [...rs, repeatRow(rs, t)])}>
                          Add {t.label.toLowerCase()}
                        </Button>
                      </div>
                    ))}
                </Group>
              ))}
              {/* A repeating field whose section has no other row yet (a login's websites are all in zone 1). */}
              {repeating
                .filter((t) => !templateSections.some((s) => s.name === t.section))
                .map((t) => (
                  <div key={t.field_id}>
                    <Button type="button" variant="ghost" size="sm" onClick={() => setRows((rs) => [...rs, repeatRow(rs, t)])}>
                      Add {t.label.toLowerCase()}
                    </Button>
                  </div>
                ))}
              <Group title="Organize">
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
                <ToggleRow id="editor-favourite" label="Favorite" description="Listed first in quick access." checked={favourite} onCheckedChange={setFavourite} rule="none" />
              </Group>
              <Group title="Your fields" count={own?.rows.length}>
                {own?.rows.map((r) => control(r))}
                {adding ? (
                  <div className="flex flex-wrap items-end gap-2">
                    <div className="min-w-0 flex-1">
                      <TextField id="editor-new-field" label="New field's label" value={adding.label} onChange={(e) => setAdding({ ...adding, label: e.target.value })} autoComplete="off" />
                    </div>
                    <NativeSelect aria-label="New field's kind" value={adding.kind} onChange={(e) => setAdding({ ...adding, kind: e.target.value })}>
                      {customKinds.map((k) => (
                        <NativeSelectOption key={k.kind} value={k.kind}>
                          {k.label}
                        </NativeSelectOption>
                      ))}
                    </NativeSelect>
                    <Button type="button" variant="secondary" onClick={addField} disabled={!adding.label.trim()}>
                      Add
                    </Button>
                  </div>
                ) : (
                  <div>
                    <Button type="button" variant="ghost" size="sm" onClick={() => setAdding({ label: "", kind: customKinds[0]?.kind ?? "text" })}>
                      Add field
                    </Button>
                  </div>
                )}
              </Group>
              {!editing &&
                (template.crown_jewel ? (
                  <p className="type-caption text-text-secondary flex items-center gap-2">
                    <CrownJewelMark size="sm" />
                    Always kept as a crown jewel: showing or using it asks it's you first, and it is never filled into a page.
                  </p>
                ) : (
                  <ToggleRow
                    id="editor-crown-jewel"
                    label="Keep as a crown jewel"
                    description="Showing or using it will ask it's you first, and it is never filled into a page. To undo it later, choose Stop keeping as a crown jewel in the item's menu; it asks for your master password."
                    checked={crownJewel}
                    onCheckedChange={setCrownJewel}
                    rule="none"
                  />
                ))}
            </div>
          </SectionHeader>
          {!phone && saveError}
        </EditorFields>
      </fieldset>
      {!phone && (
        <DialogFooter sticky>
          {editing && <p className="type-caption text-text-secondary me-auto">Saves a new version. The old one stays in History.</p>}
          <Button type="button" variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="submit" loading={busy}>
            <Icon name="checkmark" data-icon="inline-start" />
            {head.primary}
          </Button>
        </DialogFooter>
      )}
    </form>
  )
}

/** The dialog's scrolling body on a desktop; on a phone the sheet's body already holds the form. */
function EditorFields({ phone, children }: { phone: boolean; children: ReactNode }) {
  if (phone) return <div className="flex flex-col gap-4 pb-6">{children}</div>
  return (
    <DialogBody>
      <div className="flex flex-col gap-4">{children}</div>
    </DialogBody>
  )
}
