import { useId, useMemo, useState, type FormEvent } from "react"

import { AdaptiveDialog, Banner, Button, Field, FieldDescription, FieldLabel, NativeSelect, NativeSelectOption, ToggleRow } from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import type { ItemSummary } from "@/core/types"

import { folderTree, type OrganizeTarget } from "./org-model"
import { TagsEditor } from "./tags-editor"
import { useVaults } from "./vaults-context"

interface OrganizePromptProps {
  target: OrganizeTarget | null
  /** Every row, for the tags already in use. */
  rows: ItemSummary[]
  onClose: () => void
  onSaved: () => void
}

/**
 * Folder and tags (brief I4's first small Form): the folder from the
 * item's own vault's tree, tags as chips (ChipField), the Favorite switch,
 * and one primary SAVE. A dialog on a desktop, a sheet on a phone. Saving
 * writes one new version in the core with the three organization fields
 * replaced and every other field kept as it was (`Vault::organize`); a
 * failure stays in the form in the core's words.
 */
export function OrganizePrompt({ target, rows, onClose, onSaved }: OrganizePromptProps) {
  const formId = useId()
  const [busy, setBusy] = useState(false)
  return (
    <AdaptiveDialog
      open={target !== null}
      onOpenChange={(o) => !o && !busy && onClose()}
      overline={target?.title}
      title="Folder and tags"
      description="Folders belong to the item's vault; tags work across every vault."
      // Typed tags would be lost to a stray press outside; Cancel, Escape and the close button still close it.
      closeOnOutsideClick={false}
      footer={
        <>
          <Button type="button" variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" form={formId} loading={busy}>
            Save
          </Button>
        </>
      }
    >
      {target && <OrganizeForm key={target.id} formId={formId} target={target} rows={rows} onBusy={setBusy} onSaved={onSaved} />}
    </AdaptiveDialog>
  )
}

function OrganizeForm({ formId, target, rows, onBusy, onSaved }: { formId: string; target: OrganizeTarget; rows: ItemSummary[]; onBusy: (busy: boolean) => void; onSaved: () => void }) {
  const client = useClient()
  const { vaults } = useVaults()
  const vault = vaults.find((v) => v.id === target.vaultId)
  const [folder, setFolder] = useState(target.organization.folder ?? "")
  const [tags, setTags] = useState(target.organization.tags)
  const [favourite, setFavourite] = useState(target.organization.favourite)
  const [error, setError] = useState<string | null>(null)
  const known = useMemo(() => [...new Set(rows.flatMap((r) => r.tags ?? []))].sort(), [rows])

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    onBusy(true)
    setError(null)
    try {
      await client.organize(target.id, { folder, tags, favourite })
      onSaved()
    } catch (err) {
      setError(describeError(err))
    } finally {
      onBusy(false)
    }
  }
  return (
    <form id={formId} onSubmit={(e) => void submit(e)} className="flex flex-col gap-6">
      {error && <Banner tone="critical" size="compact" title={error} />}
      <Field>
        <FieldLabel htmlFor="organize-folder">Folder</FieldLabel>
        <NativeSelect id="organize-folder" value={folder} onChange={(e) => setFolder(e.target.value)}>
          <NativeSelectOption value="">No folder</NativeSelectOption>
          {folderTree(vault?.folders ?? []).map((f) => (
            <NativeSelectOption key={f.id} value={f.id}>
              {f.path}
            </NativeSelectOption>
          ))}
        </NativeSelect>
        {!vault?.folders.length && <FieldDescription>{vault ? `${vault.name} has no folders yet. Make one from its menu in the sidebar.` : "This vault has no folders yet."}</FieldDescription>}
      </Field>
      <TagsEditor id="organize-tags" label="Tags" tags={tags} onChange={setTags} known={known} helper="A slash nests one tag under another: work/clients." />
      <ToggleRow label="Favorite" description="Listed first in Quick access and in Favorites." checked={favourite} onCheckedChange={setFavourite} />
    </form>
  )
}
