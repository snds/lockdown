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
  ListRow,
  NavItem,
  NavSection,
  SectionHeader,
  toast,
  type IconName,
  type NavMarkProp,
} from "@unicron/ui"

import { currentRow, folderPlace, membersPlace, navFor, tagPlace, vaultPlace, type Nav } from "@/app/places"
import { describeError, useClient } from "@/core/client"
import type { ItemSummary, VaultView } from "@/core/types"
import { countName } from "@/lib/format"

import { ConfirmPrompt, NamePrompt, type ConfirmAsk, type NameAsk } from "./name-prompt"
import { folderTree, tagTree, type Scope } from "./org-model"
import { useVaults } from "./vaults-context"

interface OrganizationNavProps {
  rows: ItemSummary[]
  nav: Nav
  onNavigate: (nav: Nav) => void
  /**
   * `sidebar`: the VAULTS group of the desktop sidebar (brief 3.2).
   * `switcher`: the same rows in the phone's Switch vault sheet and the
   * medium width's popover (brief X1), led by All vaults, with New vault as
   * a row and the SHARED WITH rows that open a vault's members.
   */
  variant?: "sidebar" | "switcher"
  /** What waits on the person in Sharing: its row's mark, in the needs-you tone. */
  sharingMark?: number
  /** Whether Sharing has a row here (it always does; the web vault's holds People and Links). */
  sharing?: boolean
  /** A row was chosen: the sheet or popover that holds the switcher closes. */
  onPicked?: () => void
}

/** The grace periods the vault menu offers, in days. */
const TRASH_DAYS = [7, 14, 30, 60, 90, 365]

/** A vault's glyph, as drawn: yours alone, or shared. */
const vaultIcon = (v: VaultView): IconName => (v.sharing ? "user--multiple" : "user--avatar")

/**
 * The vaults, their folders and the tags (brief §3 "Vaults, folders,
 * tags"; redesign brief 3.2 and X1). Each vault opens its folder tree in
 * place; Tags is one more row with the tag tree inside (`work/clients`
 * under `work`); Sharing is the last row. A row narrows the list to it;
 * its menu makes, renames or removes. Every name is sealed in the vault
 * (docs/plan/15-organization.md): this only draws what the core sends.
 * The group is a tree: Up and Down move, Right and Left open and close.
 */
export function OrganizationNav({ rows, nav, onNavigate, variant = "sidebar", sharingMark = 0, sharing = true, onPicked }: OrganizationNavProps) {
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

  const go = (scope: Scope) => {
    onNavigate({ area: "vault", view: "all", scope })
    onPicked?.()
  }
  const goPlace = (id: string) => {
    const to = navFor(id)
    if (to) onNavigate(to)
    onPicked?.()
  }
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

  const current = currentRow(nav)
  // A branch is open when the person opened it, or when the place showing is it or inside it.
  const [opened, setOpened] = useState<ReadonlySet<string>>(new Set())
  const [closed, setClosed] = useState<ReadonlySet<string>>(new Set())
  const holds = (branch: string) => (branch === "tags" ? Boolean(nav.area === "vault" && nav.scope?.tag) : nav.area === "vault" && (nav.scope?.vault === branch || (nav.scope?.folder !== undefined && vaults.some((v) => v.id === branch && v.folders.some((f) => f.id === nav.scope?.folder)))))
  const isOpen = (branch: string) => (opened.has(branch) || holds(branch)) && !closed.has(branch)
  const setOpen = (branch: string, open: boolean) => {
    const flip = (set: ReadonlySet<string>, on: boolean) => {
      const next = new Set(set)
      if (on) next.add(branch)
      else next.delete(branch)
      return next
    }
    setOpened((set) => flip(set, open))
    setClosed((set) => flip(set, !open))
  }
  const count = (n: number | undefined): NavMarkProp | undefined => (n ? { count: n, label: countName(n, "item") } : undefined)
  const sharedVaults = vaults.filter((v) => v.sharing && v.sharing.kind !== "received")
  const switcher = variant === "switcher"
  return (
    <>
      <NavSection title="Vaults" count={vaults.length} tree action={switcher ? undefined : <IconButton label="New vault" icon="add" size="icon-sm" variant="ghost" onClick={newVault} />}>
        {switcher && <NavItem label="All vaults" icon="folder" current={nav.area === "vault" && current === nav.view && !nav.scope} mark={count(rows.filter((r) => r.state === "live").length)} onClick={() => go({})} />}
        {vaults.map((v) => {
          const open = isOpen(v.id)
          return [
            <NavItem
              key={v.id}
              label={v.name}
              icon={vaultIcon(v)}
              description={v.sharing && !v.sharing.owned && v.sharing.owner ? v.sharing.owner : undefined}
              mark={count(counts.byVault.get(v.id))}
              current={current === vaultPlace(v.id)}
              expanded={v.folders.length > 0 ? open : undefined}
              onExpandedChange={(next) => setOpen(v.id, next)}
              onClick={() => go({ vault: v.id })}
              action={
                <DropdownMenu>
                  <DropdownMenuTrigger render={<IconButton label={`${v.name} options`} icon="overflow-menu--horizontal" size="icon-sm" variant="ghost" />} />
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem onClick={() => newFolder(v, null)}>
                      <Icon name="folder--add" />
                      New folder
                    </DropdownMenuItem>
                    <DropdownMenuItem onClick={() => renameVault(v)}>
                      <Icon name="edit" />
                      Rename
                    </DropdownMenuItem>
                    <DropdownMenuSub>
                      <DropdownMenuSubTrigger>
                        <Icon name="trash-can" />
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
              }
            />,
            ...(open
              ? folderTree(v.folders).map((f) => (
                  <NavItem
                    key={f.id}
                    label={f.name}
                    icon={nav.scope?.folder === f.id ? "folder--open" : "folder"}
                    level={f.depth + 1}
                    mark={count(counts.byFolder.get(f.id))}
                    current={current === folderPlace(f.id)}
                    onClick={() => go({ folder: f.id })}
                    action={
                      <DropdownMenu>
                        <DropdownMenuTrigger render={<IconButton label={`${f.name} options`} icon="overflow-menu--horizontal" size="icon-sm" variant="ghost" />} />
                        <DropdownMenuContent align="end">
                          <DropdownMenuItem onClick={() => newFolder(v, f.id)}>
                            <Icon name="folder--add" />
                            New folder inside
                          </DropdownMenuItem>
                          <DropdownMenuItem onClick={() => renameFolder(v, f.id, f.name, f.parent)}>
                            <Icon name="edit" />
                            Rename
                          </DropdownMenuItem>
                          {f.parent && (
                            <DropdownMenuItem onClick={() => void client.changeFolder(v.id, f.id, f.name, null).then(done("Moved to the top.")).catch(fail)}>
                              <Icon name="folder--move-to" />
                              Move to the top
                            </DropdownMenuItem>
                          )}
                          <DropdownMenuSeparator />
                          <DropdownMenuItem variant="destructive" onClick={() => removeFolder(v, f.id, f.name)}>
                            <Icon name="trash-can" />
                            Remove
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    }
                  />
                ))
              : []),
          ]
        })}
        {tags.length > 0 && <NavItem label="Tags" icon="tag" expanded={isOpen("tags")} onExpandedChange={(next) => setOpen("tags", next)} />}
        {tags.length > 0 &&
          isOpen("tags") &&
          tags.map((t) => (
            <NavItem
              key={t.tag}
              label={t.name}
              icon="tag"
              level={t.depth + 1}
              mark={count(t.count)}
              current={current === tagPlace(t.tag)}
              onClick={() => go({ tag: t.tag })}
              action={
                <DropdownMenu>
                  <DropdownMenuTrigger render={<IconButton label={`#${t.tag} options`} icon="overflow-menu--horizontal" size="icon-sm" variant="ghost" />} />
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem onClick={() => renameTag(t.tag)}>
                      <Icon name="edit" />
                      Rename
                    </DropdownMenuItem>
                    <DropdownMenuItem variant="destructive" onClick={() => removeTag(t.tag)}>
                      <Icon name="trash-can" />
                      Take off every item
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              }
            />
          ))}
        {switcher && <NavItem label="New vault" icon="add" onClick={newVault} />}
        {sharing && !switcher && (
          <NavItem label="Sharing" icon="share" current={current === "sharing"} mark={sharingMark > 0 ? { count: sharingMark, tone: "needs-you" } : undefined} onClick={() => goPlace("sharing")} />
        )}
      </NavSection>
      {switcher && sharedVaults.length > 0 && (
        <div className="flex flex-col">
          <SectionHeader title="Shared with" count={sharedVaults.length} />
          <div role="list">
            {sharedVaults.map((v) => (
              <ListRow
                key={v.id}
                role="listitem"
                size="compact"
                title={v.name}
                meta={`${countName(v.sharing?.people ?? 0, "member")} // ${v.sharing?.owned ? "you own it" : (v.sharing?.owner ?? "shared with you")}`}
                metaStyle="plain"
                status={v.sharing?.owned ? "Owner" : undefined}
                onClick={() => goPlace(membersPlace(v.id))}
              />
            ))}
          </div>
        </div>
      )}
      {switcher && sharing && <NavItem label="Sharing" icon="share" current={current === "sharing"} mark={sharingMark > 0 ? { count: sharingMark, tone: "needs-you" } : undefined} onClick={() => goPlace("sharing")} />}
      <NamePrompt ask={name} onClose={() => setName(null)} />
      <ConfirmPrompt ask={confirm} onClose={() => setConfirm(null)} />
    </>
  )
}
