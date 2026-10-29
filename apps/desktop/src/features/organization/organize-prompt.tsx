import { useMemo, useState, type FormEvent } from "react"

import {
  Dialog,
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
  NativeSelect,
  NativeSelectOption,
  Switch,
} from "@unicron/ui"

import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
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
 * Files, tags and stars one item: the folder from its own vault's tree,
 * tags as chips, the favourite switch. Saving writes one new version in
 * the core with the three organization fields replaced and every other
 * field kept as it was (`Vault::organize`).
 */
export function OrganizePrompt({ target, rows, onClose, onSaved }: OrganizePromptProps) {
  return (
    <Dialog open={target !== null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>{target && <OrganizeForm key={target.id} target={target} rows={rows} onClose={onClose} onSaved={onSaved} />}</DialogContent>
    </Dialog>
  )
}

function OrganizeForm({ target, rows, onClose, onSaved }: { target: OrganizeTarget; rows: ItemSummary[]; onClose: () => void; onSaved: () => void }) {
  const client = useClient()
  const { vaults } = useVaults()
  const vault = vaults.find((v) => v.id === target.vaultId)
  const [folder, setFolder] = useState(target.organization.folder ?? "")
  const [tags, setTags] = useState(target.organization.tags)
  const [favourite, setFavourite] = useState(target.organization.favourite)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const known = useMemo(() => [...new Set(rows.flatMap((r) => r.tags ?? []))].sort(), [rows])

  const submit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      await client.organize(target.id, { folder, tags, favourite })
      onSaved()
    } catch (err) {
      setError(describeError(err))
    } finally {
      setBusy(false)
    }
  }
  return (
    <form onSubmit={submit} className="grid gap-4">
      <DialogHeader>
        <DialogTitle>Organize "{target.title}"</DialogTitle>
        <DialogDescription>Folders belong to the item's vault{vault ? ` (${vault.name})` : ""}; tags work across every vault.</DialogDescription>
      </DialogHeader>
      <FieldGroup>
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
          {!vault?.folders.length && <FieldDescription>This vault has no folders yet; make one from its menu in the sidebar.</FieldDescription>}
        </Field>
        <Field>
          <FieldLabel htmlFor="organize-tags">Tags</FieldLabel>
          <TagsEditor id="organize-tags" tags={tags} onChange={setTags} known={known} />
          <FieldDescription>A slash nests a tag under another: work/clients.</FieldDescription>
        </Field>
        <Field orientation="horizontal">
          <Switch id="organize-favourite" checked={favourite} onCheckedChange={setFavourite} />
          <FieldContent>
            <FieldLabel htmlFor="organize-favourite">Favourite</FieldLabel>
            <FieldDescription>Listed first in quick access and in Favourites.</FieldDescription>
          </FieldContent>
        </Field>
      </FieldGroup>
      {error && <FieldError role="alert">{error}</FieldError>}
      <LabelledActions>
        <LabelledAction kind="cancel" type="button" onClick={onClose} disabled={busy}>
          Cancel
        </LabelledAction>
        <LabelledAction kind="confirm" type="submit" disabled={busy}>
          Save
        </LabelledAction>
      </LabelledActions>
    </form>
  )
}
