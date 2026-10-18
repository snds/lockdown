import { useMemo, useState } from "react"

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
  Icon,
  IconButton,
  Separator,
} from "@unicron/ui"
import { toast } from "sonner"

import { NavRow, type Nav } from "@/app/shell"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary, VaultView } from "@/core/types"

import { ConfirmPrompt, NamePrompt, type ConfirmAsk, type NameAsk } from "./name-prompt"
import { folderTree, tagTree, type Scope } from "./org-model"
import { useVaults } from "./vaults-context"

interface OrganizationNavProps {
  rows: ItemSummary[]
  nav: Nav
  onNavigate: (nav: Nav) => void
}

/** The grace periods the vault menu offers, in days. */
const TRASH_DAYS = [7, 14, 30, 60, 90, 365]

const same = (a: Scope | undefined, b: Scope) => (a?.vault ?? "") === (b.vault ?? "") && (a?.folder ?? "") === (b.folder ?? "") && (a?.tag ?? "") === (b.tag ?? "")

/**
 * The sidebar's Vaults, Folders and Tags (brief §3 "Vaults, folders,
 * tags"). Each vault lists its folders as a tree under it; each tag lists
 * the tags nested under it (`work/clients` under `work`). A row narrows
 * the list to it; its menu makes, renames or removes. Every name is sealed
 * in the vault (docs/plan/15-organization.md): this only draws what the
 * core sends.
 */
export function OrganizationNav({ rows, nav, onNavigate }: OrganizationNavProps) {
  const client = useClient()
  const { vaults, reload } = useVaults()
  const [name, setName] = useState<NameAsk | null>(null)
  const [confirm, setConfirm] = useState<ConfirmAsk | null>(null)
  const tags = useMemo(() => tagTree(rows), [rows])
  const counts = useMemo(() => {
    const byVault = new Map<string, number>()
    const byFolder = new Map<string, number>()
    for (const r of rows) {
      if (r.state !== "live") continue
      if (r.vault_id) byVault.set(r.vault_id, (byVault.get(r.vault_id) ?? 0) + 1)
      if (r.folder) byFolder.set(r.folder, (byFolder.get(r.folder) ?? 0) + 1)
    }
    return { byVault, byFolder }
  }, [rows])

  const go = (scope: Scope) => onNavigate({ area: "vault", view: "all", scope })
  const done = (text: string) => () => {
    reload()
    toast(text)
  }
  const fail = (e: unknown) => toast.error(describeError(e))

  const newVault = () =>
    setName({
      title: "New vault",
      description: "A vault is its own locked box with its own key. Items move between vaults whenever you like.",
      label: "Name",
      confirm: "Make vault",
      onSave: (n) => client.createVault(n).then(done(`Made "${n}".`)),
    })
  const renameVault = (v: VaultView) =>
    setName({
      title: "Rename vault",
      description: "The name is encrypted with the vault, like everything in it.",
      label: "Name",
      initial: v.name,
      confirm: "Rename",
      onSave: (n) => client.renameVault(v.id, n).then(done("Renamed.")),
    })
  const newFolder = (v: VaultView, parent: string | null) =>
    setName({
      title: parent ? "New folder inside" : `New folder in ${v.name}`,
      description: "Folders belong to one vault and can sit inside each other.",
      label: "Name",
      confirm: "Make folder",
      onSave: (n) => client.addFolder(v.id, n, parent).then(done(`Made "${n}".`)),
    })
  const renameFolder = (v: VaultView, id: string, current: string, parent: string | null) =>
    setName({
      title: "Rename folder",
      description: "Everything filed in it stays filed in it.",
      label: "Name",
      initial: current,
      confirm: "Rename",
      onSave: (n) => client.changeFolder(v.id, id, n, parent).then(done("Renamed.")),
    })
  const removeFolder = (v: VaultView, id: string, folderName: string) =>
    setConfirm({
      title: `Remove "${folderName}"?`,
      description: "The folders and items inside it move up one level. No item is deleted.",
      confirm: "Remove folder",
      destructive: true,
      onConfirm: () =>
        client.removeFolder(v.id, id).then((moved) => {
          if (nav.scope?.folder === id) go({ vault: v.id })
          done(moved === 1 ? "Removed; one item moved up." : `Removed; ${moved} items moved up.`)()
        }),
    })
  const renameTag = (tag: string) =>
    setName({
      title: "Rename tag",
      description: "Every item with this tag, and the tags under it, takes the new name.",
      label: "Tag",
      initial: tag,
      confirm: "Rename",
      hint: "A slash nests one tag under another.",
      onSave: (n) =>
        client.renameTag(tag, n).then((count) => {
          if (nav.scope?.tag === tag) go({ tag: n })
          done(count === 1 ? "Renamed on one item." : `Renamed on ${count} items.`)()
        }),
    })
  const removeTag = (tag: string) =>
    setConfirm({
      title: `Take off #${tag}?`,
      description: "The tag (and the tags under it) comes off every item. The items stay.",
      confirm: "Take it off",
      destructive: true,
      onConfirm: () =>
        client.renameTag(tag, null).then((count) => {
          if (nav.scope?.tag === tag) go({})
          done(count === 1 ? "Taken off one item." : `Taken off ${count} items.`)()
        }),
    })
  const setTrashDays = (v: VaultView, days: number) => client.setTrashDays(v.id, days).then(done(`Trashed items in ${v.name} now stay ${days} days.`)).catch(fail)

  return (
    <div className="hidden md:flex md:flex-col md:gap-1">
      <Separator />
      <div className="flex items-center justify-between px-2 pt-1">
        <h2 className="text-xs font-medium text-muted-foreground uppercase">Vaults</h2>
        <IconButton label="New vault" icon="add" size="icon-sm" variant="ghost" onClick={newVault} />
      </div>
      <ul aria-label="Vaults" className="flex flex-col gap-1">
        {vaults.map((v) => (
          <li key={v.id} className="flex flex-col gap-1">
            <div className="flex items-center gap-1">
              <div className="min-w-0 flex-1">
                <NavRow icon="shelves" label={v.name} count={counts.byVault.get(v.id)} current={nav.area === "vault" && same(nav.scope, { vault: v.id })} onClick={() => go({ vault: v.id })} />
              </div>
              <DropdownMenu>
                <DropdownMenuTrigger render={<IconButton label={`${v.name} options`} icon="more_horiz" size="icon-sm" variant="ghost" />} />
                <DropdownMenuContent align="end">
                  <DropdownMenuItem onClick={() => newFolder(v, null)}>
                    <Icon name="create_new_folder" />
                    New folder
                  </DropdownMenuItem>
                  <DropdownMenuItem onClick={() => renameVault(v)}>
                    <Icon name="edit" />
                    Rename
                  </DropdownMenuItem>
                  <DropdownMenuSub>
                    <DropdownMenuSubTrigger>
                      <Icon name="delete" />
                      Keep trashed items
                    </DropdownMenuSubTrigger>
                    <DropdownMenuSubContent>
                      <DropdownMenuGroup>
                        <DropdownMenuLabel>Purged on every device after</DropdownMenuLabel>
                        <DropdownMenuRadioGroup value={String(v.trash_days)} onValueChange={(d) => void setTrashDays(v, Number(d))}>
                          {TRASH_DAYS.map((d) => (
                            <DropdownMenuRadioItem key={d} value={String(d)}>
                              {d === 365 ? "A year" : `${d} days`}
                            </DropdownMenuRadioItem>
                          ))}
                        </DropdownMenuRadioGroup>
                      </DropdownMenuGroup>
                    </DropdownMenuSubContent>
                  </DropdownMenuSub>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
            {v.folders.length > 0 && (
              <ul aria-label={`Folders in ${v.name}`} className="flex flex-col gap-1">
                {folderTree(v.folders).map((f) => (
                  <li key={f.id} className="flex items-center gap-1">
                    <div className="min-w-0 flex-1">
                      <NavRow
                        icon={nav.scope?.folder === f.id ? "folder_open" : "folder"}
                        label={f.name}
                        depth={f.depth + 1}
                        count={counts.byFolder.get(f.id)}
                        current={nav.area === "vault" && nav.scope?.folder === f.id}
                        onClick={() => go({ folder: f.id })}
                      />
                    </div>
                    <DropdownMenu>
                      <DropdownMenuTrigger render={<IconButton label={`${f.name} options`} icon="more_horiz" size="icon-sm" variant="ghost" />} />
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onClick={() => newFolder(v, f.id)}>
                          <Icon name="create_new_folder" />
                          New folder inside
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => renameFolder(v, f.id, f.name, f.parent)}>
                          <Icon name="edit" />
                          Rename
                        </DropdownMenuItem>
                        {f.parent && (
                          <DropdownMenuItem onClick={() => void client.changeFolder(v.id, f.id, f.name, null).then(done("Moved to the top.")).catch(fail)}>
                            <Icon name="drive_file_move" />
                            Move to the top
                          </DropdownMenuItem>
                        )}
                        <DropdownMenuSeparator />
                        <DropdownMenuItem variant="destructive" onClick={() => removeFolder(v, f.id, f.name)}>
                          <Icon name="delete" />
                          Remove
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>
      {tags.length > 0 && (
        <>
          <h2 className="px-2 pt-2 text-xs font-medium text-muted-foreground uppercase">Tags</h2>
          <ul aria-label="Tags" className="flex flex-col gap-1">
            {tags.map((t) => (
              <li key={t.tag} className="flex items-center gap-1">
                <div className="min-w-0 flex-1">
                  <NavRow icon="sell" label={t.name} depth={t.depth} count={t.count} current={nav.area === "vault" && nav.scope?.tag === t.tag} onClick={() => go({ tag: t.tag })} />
                </div>
                <DropdownMenu>
                  <DropdownMenuTrigger render={<IconButton label={`#${t.tag} options`} icon="more_horiz" size="icon-sm" variant="ghost" />} />
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem onClick={() => renameTag(t.tag)}>
                      <Icon name="edit" />
                      Rename
                    </DropdownMenuItem>
                    <DropdownMenuItem variant="destructive" onClick={() => removeTag(t.tag)}>
                      <Icon name="delete" />
                      Take off every item
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </li>
            ))}
          </ul>
        </>
      )}
      <NamePrompt ask={name} onClose={() => setName(null)} />
      <ConfirmPrompt ask={confirm} onClose={() => setConfirm(null)} />
    </div>
  )
}
