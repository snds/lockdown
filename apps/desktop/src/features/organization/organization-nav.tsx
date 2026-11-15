import { useMemo, useState, type KeyboardEvent, type ReactNode } from "react"

import {
  AdaptiveMenu,
  AdaptiveMenuItem,
  AdaptiveMenuRadioGroup,
  AdaptiveMenuRadioItem,
  AdaptiveMenuSeparator,
  Banner,
  IconButton,
  ListRow,
  NavItem,
  NavSection,
  SectionHeader,
  useAdaptiveSheet,
  type IconName,
  type NavItemProps,
  type NavMarkProp,
} from "@unicron/ui"

import { useRowMenuKeys } from "@/app/keys"
import { notify } from "@/app/notify"
import { currentRow, folderPlace, membersPlace, navFor, tagPlace, vaultPlace, type Nav } from "@/app/places"
import { describeError, useClient } from "@/core/client"
import type { FolderView, ItemSummary, VaultView } from "@/core/types"
import { countName } from "@/lib/format"

import { ConfirmPrompt, NamePrompt, type ConfirmAsk, type NameAsk } from "./name-prompt"
import { TRASH_DAYS, canFileInVault, folderMenuOffers, folderTree, tagTree, trashDaysWords, vaultMenuOffers, type Scope } from "./org-model"
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

/** A vault's glyph, as drawn: yours alone, or shared. */
const vaultIcon = (v: VaultView): IconName => (v.sharing ? "user--multiple" : "user--avatar")

/**
 * The vaults, their folders and the tags (brief V2; X1 for the sidebar).
 * Each vault opens its folder tree in place; Tags is one more row with the
 * tag tree inside (`work/clients` under `work`); Sharing is the last row.
 * A row narrows the list to it. Every vault, folder and tag row has a ⋯
 * that opens its menu: a menu beside the sidebar and in the medium width's
 * popover, a sheet over the Switch vault sheet on a phone. The Menu key,
 * Shift+F10 and a right click open it too. Every name is sealed in the
 * vault (docs/plan/15-organization.md): this only draws what the core sends.
 *
 * In the sidebar the group is a tree: Up and Down move, Right and Left open
 * and close. In the switcher the rows are buttons, each branch with its own
 * Expand button (a finger has no arrow keys), and the same arrow keys move
 * between them.
 */
export function OrganizationNav({ rows, nav, onNavigate, variant = "sidebar", sharingMark = 0, sharing = true, onPicked }: OrganizationNavProps) {
  const client = useClient()
  const { vaults, reload } = useVaults()
  const [name, setName] = useState<NameAsk | null>(null)
  const [confirm, setConfirm] = useState<ConfirmAsk | null>(null)
  // A change from a menu that asks nothing and failed: one critical line here, in the core's words (cross-app §9).
  const [failed, setFailed] = useState<string | null>(null)
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
  /** A change is saved: the vaults and rows load again, and a toast names it (channel 2). */
  const done = (text: string) => {
    setFailed(null)
    reload()
    notify(text)
  }
  const run = (change: () => Promise<unknown>) => {
    setFailed(null)
    change().catch((e: unknown) => setFailed(describeError(e)))
  }

  const newVault = () =>
    setName({
      overline: "Vaults",
      title: "New vault",
      description: "A vault is its own locked box with its own key. Items move between vaults whenever you like.",
      label: "Name",
      confirm: "Create vault",
      onSave: (n) => client.createVault(n).then(() => done(`Created ${n}`)),
    })
  const renameVault = (v: VaultView) =>
    setName({
      overline: v.name,
      title: "Rename vault",
      description: "The name is encrypted with the vault, like everything in it.",
      label: "Name",
      initial: v.name,
      confirm: "Rename",
      onSave: (n) => client.renameVault(v.id, n).then(() => done(`Renamed ${v.name} to ${n}`)),
    })
  const newFolder = (v: VaultView, parent: FolderView | null) =>
    setName({
      overline: parent ? `${v.name} // ${parent.path}` : v.name,
      title: parent ? "New folder inside" : `New folder in ${v.name}`,
      description: "Folders belong to one vault and can sit inside each other.",
      label: "Name",
      confirm: "Create folder",
      onSave: (n) => client.addFolder(v.id, n, parent?.id ?? null).then(() => done(`Created ${n}`)),
    })
  const renameFolder = (v: VaultView, f: FolderView) =>
    setName({
      overline: `${v.name} // ${f.path}`,
      title: "Rename folder",
      description: "Everything filed in it stays filed in it.",
      label: "Name",
      initial: f.name,
      confirm: "Rename",
      onSave: (n) => client.changeFolder(v.id, f.id, n, f.parent).then(() => done(`Renamed ${f.name} to ${n}`)),
    })
  const moveToTop = (v: VaultView, f: FolderView) => run(() => client.changeFolder(v.id, f.id, f.name, null).then(() => done(`Moved ${f.name} to the top`)))
  const removeFolder = (v: VaultView, f: FolderView) =>
    setConfirm({
      overline: `${v.name} // ${f.path}`,
      title: `Remove "${f.name}"?`,
      description: "The folders and items inside it move up one level. No item is deleted.",
      confirm: "Remove folder",
      destructive: true,
      onConfirm: () =>
        client.removeFolder(v.id, f.id).then((moved) => {
          if (nav.scope?.folder === f.id) go({ vault: v.id })
          done(moved === 0 ? `Removed ${f.name}` : `Removed ${f.name}, ${moved === 1 ? "one item" : countName(moved, "item")} moved up`)
        }),
    })
  const renameTag = (tag: string) =>
    setName({
      overline: `#${tag}`,
      title: "Rename tag",
      description: "Every item with this tag, and the tags under it, takes the new name.",
      label: "Tag",
      initial: tag,
      confirm: "Rename",
      hint: "A slash nests one tag under another.",
      onSave: (n) =>
        client.renameTag(tag, n).then((count) => {
          if (nav.scope?.tag === tag) go({ tag: n })
          done(`Renamed #${tag} to #${n} on ${count === 1 ? "one item" : countName(count, "item")}`)
        }),
    })
  const removeTag = (tag: string) =>
    setConfirm({
      overline: "Tags",
      title: `Take #${tag} off every item?`,
      description: "The tag (and the tags under it) comes off every item. The items stay.",
      confirm: "Take off",
      destructive: true,
      onConfirm: () =>
        client.renameTag(tag, null).then((count) => {
          if (nav.scope?.tag === tag) go({})
          done(`Took #${tag} off ${count === 1 ? "one item" : countName(count, "item")}`)
        }),
    })
  const setTrashDays = (v: VaultView, days: number) => run(() => client.setTrashDays(v.id, days).then(() => done(`${v.name} keeps deleted items ${trashDaysWords(days).toLowerCase()}`)))

  const current = currentRow(nav)
  // A branch is open when the person opened it, or when the place showing is it or inside it.
  const [opened, setOpened] = useState<ReadonlySet<string>>(new Set())
  const [closed, setClosed] = useState<ReadonlySet<string>>(new Set())
  const holds = (branch: string) =>
    branch === "tags"
      ? Boolean(nav.area === "vault" && nav.scope?.tag)
      : nav.area === "vault" && (nav.scope?.vault === branch || (nav.scope?.folder !== undefined && vaults.some((v) => v.id === branch && v.folders.some((f) => f.id === nav.scope?.folder))))
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

  const vaultMenu = (v: VaultView) => {
    const offers = vaultMenuOffers(v, sharing)
    return (page: MenuPage, toPage: (p: MenuPage) => void) =>
      page === "trash-days" ? (
        <AdaptiveMenuRadioGroup value={String(v.trash_days)} onValueChange={(d) => setTrashDays(v, Number(d))}>
          {TRASH_DAYS.map((d) => (
            <AdaptiveMenuRadioItem key={d} value={String(d)}>
              {trashDaysWords(d)}
            </AdaptiveMenuRadioItem>
          ))}
        </AdaptiveMenuRadioGroup>
      ) : (
        <>
          {offers.includes("new-folder") && (
            <AdaptiveMenuItem icon="folder--add" onSelect={() => newFolder(v, null)}>
              New folder
            </AdaptiveMenuItem>
          )}
          {offers.includes("rename") && (
            <AdaptiveMenuItem icon="edit" onSelect={() => renameVault(v)}>
              Rename
            </AdaptiveMenuItem>
          )}
          {offers.includes("trash-days") && (
            <AdaptiveMenuItem icon="trash-can" keepOpen onSelect={() => toPage("trash-days")}>
              Keep deleted items for…
            </AdaptiveMenuItem>
          )}
          {offers.includes("share") && (
            <AdaptiveMenuItem icon="share" onSelect={() => goPlace(membersPlace(v.id))}>
              Share…
            </AdaptiveMenuItem>
          )}
          {offers.includes("members") && (
            <AdaptiveMenuItem icon="user--multiple" onSelect={() => goPlace(membersPlace(v.id))}>
              Members
            </AdaptiveMenuItem>
          )}
          {offers.includes("who-has-it") && (
            <AdaptiveMenuItem icon="user--multiple" onSelect={() => goPlace(membersPlace(v.id))}>
              Who has it
            </AdaptiveMenuItem>
          )}
        </>
      )
  }
  const folderMenu = (v: VaultView, f: FolderView) => {
    const offers = folderMenuOffers(f)
    return () => (
      <>
        <AdaptiveMenuItem icon="folder--add" onSelect={() => newFolder(v, f)}>
          New folder inside
        </AdaptiveMenuItem>
        <AdaptiveMenuItem icon="edit" onSelect={() => renameFolder(v, f)}>
          Rename
        </AdaptiveMenuItem>
        {offers.includes("move-to-top") && (
          <AdaptiveMenuItem icon="folder--move-to" onSelect={() => moveToTop(v, f)}>
            Move to the top
          </AdaptiveMenuItem>
        )}
        <AdaptiveMenuSeparator />
        <AdaptiveMenuItem icon="trash-can" variant="destructive" onSelect={() => removeFolder(v, f)}>
          Remove folder
        </AdaptiveMenuItem>
      </>
    )
  }
  const tagMenu = (tag: string) => () => (
    <>
      <AdaptiveMenuItem icon="edit" onSelect={() => renameTag(tag)}>
        Rename
      </AdaptiveMenuItem>
      <AdaptiveMenuSeparator />
      <AdaptiveMenuItem icon="trash-can" variant="destructive" onSelect={() => removeTag(tag)}>
        Take off every item
      </AdaptiveMenuItem>
    </>
  )

  // In the switcher each row is a list item of the Vaults list; in the sidebar the tree's NavSection makes its own.
  const li = (key: string, node: ReactNode) =>
    switcher ? (
      <div role="listitem" key={key}>
        {node}
      </div>
    ) : (
      node
    )
  const tree = (
    <>
      {switcher &&
        li(
          "all",
          <NavItem
            label="All vaults"
            icon="folder"
            data-level={0}
            current={nav.area === "vault" && current === nav.view && !nav.scope}
            mark={count(rows.filter((r) => r.state === "live").length)}
            onClick={() => go({})}
          />,
        )}
      {vaults.map((v) => {
        const open = isOpen(v.id)
        const offers = vaultMenuOffers(v, sharing)
        return [
          li(
            v.id,
            <OrgEntry
              key={v.id}
              label={v.name}
              icon={vaultIcon(v)}
              description={v.sharing && !v.sharing.owned && v.sharing.owner ? v.sharing.owner : undefined}
              mark={count(counts.byVault.get(v.id))}
              current={current === vaultPlace(v.id)}
              expanded={v.folders.length > 0 ? open : undefined}
              onExpandedChange={(next) => setOpen(v.id, next)}
              onClick={() => go({ vault: v.id })}
              menuOverline="Vault"
              menuTitle={v.name}
              pageTitles={{ "trash-days": "Keep deleted items for" }}
              menu={offers.length > 0 ? vaultMenu(v) : undefined}
            />,
          ),
          ...(open
            ? folderTree(v.folders).map((f) =>
                li(
                  f.id,
                  <OrgEntry
                    key={f.id}
                    label={f.name}
                    icon={nav.scope?.folder === f.id ? "folder--open" : "folder"}
                    level={f.depth + 1}
                    mark={count(counts.byFolder.get(f.id))}
                    current={current === folderPlace(f.id)}
                    onClick={() => go({ folder: f.id })}
                    menuOverline={`Folder // ${v.name}`}
                    menuTitle={f.name}
                    menu={canFileInVault(v) ? folderMenu(v, f) : undefined}
                  />,
                ),
              )
            : []),
        ]
      })}
      {tags.length > 0 && li("tags", <NavItem label="Tags" icon="tag" data-level={0} expanded={isOpen("tags")} onExpandedChange={(next) => setOpen("tags", next)} />)}
      {tags.length > 0 &&
        isOpen("tags") &&
        tags.map((t) =>
          li(
            `tag:${t.tag}`,
            <OrgEntry
              key={t.tag}
              label={t.name}
              icon="tag"
              level={t.depth + 1}
              mark={count(t.count)}
              current={current === tagPlace(t.tag)}
              onClick={() => go({ tag: t.tag })}
              menuLabel={`#${t.tag} options`}
              menuOverline="Tag"
              menuTitle={`#${t.tag}`}
              menu={tagMenu(t.tag)}
            />,
          ),
        )}
      {switcher && li("new", <NavItem label="New vault" icon="add" data-level={0} onClick={newVault} />)}
      {sharing && !switcher && (
        <NavItem label="Sharing" icon="share" current={current === "sharing"} mark={sharingMark > 0 ? { count: sharingMark, tone: "needs-you" } : undefined} onClick={() => goPlace("sharing")} />
      )}
    </>
  )
  const failure = failed ? <Banner tone="critical" size="compact" title={failed} onDismiss={() => setFailed(null)} /> : null
  return (
    <>
      {switcher ? (
        <>
          {failure}
          {/* In the sheet the header already says VAULTS // NN, so the rows stand under it without a second heading.
              They are plain rows there, each branch with a real button to open it: a finger has no arrow keys. */}
          <div role="list" aria-label="Vaults" className="flex flex-col gap-1" onKeyDown={moveInList}>
            {tree}
          </div>
        </>
      ) : (
        <>
          <NavSection title="Vaults" count={vaults.length} tree action={<IconButton label="New vault" icon="add" size="icon-sm" variant="ghost" onClick={newVault} />}>
            {tree}
          </NavSection>
          {failure}
        </>
      )}
      {switcher && sharedVaults.length > 0 && (
        <div className="flex flex-col">
          <SectionHeader title="Shared with" count={sharedVaults.length} />
          <div role="list" aria-label="Shared with">
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
      {switcher && sharing && (
        <NavItem label="Sharing" icon="share" current={current === "sharing"} mark={sharingMark > 0 ? { count: sharingMark, tone: "needs-you" } : undefined} onClick={() => goPlace("sharing")} />
      )}
      <NamePrompt ask={name} onClose={() => setName(null)} />
      <ConfirmPrompt ask={confirm} onClose={() => setConfirm(null)} />
    </>
  )
}

/** A menu shows its rows (`main`) or, after a row that leads on (Keep deleted items for…), that row's choices. */
type MenuPage = "main" | "trash-days"

type OrgEntryProps = Pick<NavItemProps, "label" | "icon" | "description" | "mark" | "current" | "expanded" | "onExpandedChange" | "onClick" | "level"> & {
  /** The ⋯ button's name: "Family options" by default. */
  menuLabel?: string
  /** The menu's overline (`VAULT`, `FOLDER // PERSONAL`) and its title, the row's name: a sheet's header on a phone. */
  menuOverline: string
  menuTitle: string
  /** A later page's title (the choices of Keep deleted items for…). */
  pageTitles?: Partial<Record<MenuPage, string>>
  /** The menu's rows for a page; none when the person's right offers nothing. */
  menu?: (page: MenuPage, toPage: (page: MenuPage) => void) => ReactNode
}

/**
 * One vault, folder or tag row with its ⋯ (brief V2): an AdaptiveMenu (a
 * menu from the medium width up, a sheet on a phone), shown by the NavItem
 * on hover and focus with a pointer and always on touch. The Menu key,
 * Shift+F10 and a right click on the row open the same menu (cross-app §10).
 */
function OrgEntry({ label, icon, description, mark, current, expanded, onExpandedChange, onClick, level = 0, menuLabel, menuOverline, menuTitle, pageTitles, menu }: OrgEntryProps) {
  const [open, setOpen] = useState(false)
  const [page, setPage] = useState<MenuPage>("main")
  const keys = useRowMenuKeys(() => setOpen(true))
  // A desktop menu shows its overline and not its title, so a later page names itself in the overline there.
  const sheet = useAdaptiveSheet()
  const pageTitle = page === "main" ? menuTitle : (pageTitles?.[page] ?? menuTitle)
  const change = (next: boolean) => {
    setOpen(next)
    if (!next) setPage("main")
  }
  return (
    <NavItem
      label={label}
      icon={icon}
      description={description}
      mark={mark}
      current={current}
      expanded={expanded}
      onExpandedChange={onExpandedChange}
      onClick={onClick}
      level={level}
      data-level={level}
      onKeyDown={menu ? keys.onKeyDown : undefined}
      onContextMenu={menu ? keys.onContextMenu : undefined}
      action={
        menu ? (
          <AdaptiveMenu
            open={open}
            onOpenChange={change}
            overline={page === "main" ? menuOverline : sheet ? menuTitle : `${menuTitle} // ${pageTitle}`}
            title={pageTitle}
            align="end"
            trigger={<IconButton label={menuLabel ?? `${label} options`} icon="overflow-menu--horizontal" size="icon-sm" variant="ghost" />}
          >
            {menu(page, setPage)}
          </AdaptiveMenu>
        ) : undefined
      }
    />
  )
}

/**
 * The switcher's arrow keys, as the sidebar's tree has them: Up and Down
 * move between rows, Home and End jump, Right opens a closed branch or goes
 * to its first row, Left closes an open one or goes to the row above it.
 */
function moveInList(event: KeyboardEvent<HTMLElement>) {
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return
  const list = event.currentTarget
  const rows = Array.from(list.querySelectorAll<HTMLElement>('[data-slot="nav-item-main"]'))
  const at = rows.indexOf(document.activeElement as HTMLElement)
  if (at < 0) return
  const row = rows[at]!
  const levelOf = (el: HTMLElement) => Number(el.dataset.level ?? 0)
  const disclosure = row.parentElement?.querySelector<HTMLElement>(':scope > [data-slot="nav-item-disclosure"]')
  const expanded = disclosure?.getAttribute("aria-expanded") ?? row.getAttribute("aria-expanded")
  let target: HTMLElement | undefined
  switch (event.key) {
    case "ArrowDown":
      target = rows[at + 1]
      break
    case "ArrowUp":
      target = rows[at - 1]
      break
    case "Home":
      target = rows[0]
      break
    case "End":
      target = rows[rows.length - 1]
      break
    case "ArrowRight":
      if (expanded === "false") {
        event.preventDefault()
        ;(disclosure ?? row).click()
        return
      }
      if (expanded === "true" && rows[at + 1] && levelOf(rows[at + 1]!) > levelOf(row)) target = rows[at + 1]
      break
    case "ArrowLeft":
      if (expanded === "true") {
        event.preventDefault()
        ;(disclosure ?? row).click()
        return
      }
      for (let i = at - 1; i >= 0; i--) {
        if (levelOf(rows[i]!) < levelOf(row)) {
          target = rows[i]
          break
        }
      }
      break
    default:
      return
  }
  event.preventDefault()
  target?.focus()
}
