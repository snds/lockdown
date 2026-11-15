import { useEffect, useMemo, useState, type ReactElement, type ReactNode } from "react"

import {
  AdaptiveMenu,
  AdaptiveMenuGroup,
  AdaptiveMenuItem,
  AdaptiveMenuSeparator,
  Button,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  Icon,
  SectionHeader,
  useAdaptiveSheet,
} from "@unicron/ui"

import { usePlaces } from "@/app/places"
import { templateIcon } from "@/app/template-model"
import { useTemplates } from "@/app/templates"
import { useClient } from "@/core/client"
import type { ItemSummary, TemplateView, UserTemplateView, VaultView } from "@/core/types"
import { useVaults } from "@/features/organization/vaults-context"
import { count } from "@/lib/format"

import { isKindView, kindIcon } from "./list-model"
import { TemplatesManager } from "./template-designer"

/** The menu's groups (D231): the everyday kinds, the identities and documents, the technical ones, the rest. */
const GROUPS: Array<[string, readonly string[]]> = [
  ["Sign-ins and money", ["login", "code", "card", "bank-account", "crypto-wallet", "note"]],
  ["Identities and documents", ["identity", "document", "membership"]],
  ["Networks and machines", ["wi-fi", "server", "database", "ssh-key", "api-credential", "software-licence"]],
]
const OTHER = "Other"

/** The five shown first, before the groups (brief E1, Q18): Wi-Fi network takes the frame's Passkey row, which no one makes by hand. */
const FIRST_KINDS = ["login", "card", "note", "code", "wi-fi"] as const

/** Which group a template sits in; the custom item and anything new go last. */
function groupOf(t: TemplateView): string {
  return GROUPS.find(([, kinds]) => kinds.includes(t.kind))?.[0] ?? OTHER
}

const idOf = (t: TemplateView) => t.id ?? t.kind

/** Items made from each template, from the rows (`template`, else the kind); trashed and archived ones are not counted. */
function templateCounts(rows: readonly ItemSummary[]): Map<string, number> {
  const out = new Map<string, number>()
  for (const r of rows) {
    if (r.state === "trashed" || r.state === "archived") continue
    const id = r.template ?? r.kind
    out.set(id, (out.get(id) ?? 0) + 1)
  }
  return out
}

/** The kind the screen is about, which goes first: One-time code on Codes, the kind's own view. */
function leadingKind(area: string, view: string): string | null {
  if (area === "codes") return "code"
  if (area === "vault" && isKindView(view as never)) return view
  return null
}

/** The vault a new item goes to (the scope's, else the primary one), for the sheet's overline. */
function vaultFor(vaults: readonly VaultView[], scope: { vault?: string; folder?: string } | undefined): VaultView | undefined {
  const scoped = vaults.find((v) => v.id === scope?.vault) ?? vaults.find((v) => v.folders.some((f) => f.id === scope?.folder))
  // A vault shared to this person to view only takes nothing new: the overline names the primary one.
  const writable = scoped && scoped.sharing?.right !== "view" ? scoped : undefined
  return writable ?? vaults.find((v) => v.primary) ?? vaults[0]
}

interface NewItemMenuProps {
  onPick: (template: string) => void
  onNotice?: (text: string) => void
  /**
   * The control that opens it. Left out, the menu draws its own "New item"
   * button. The shell passes its own (the sidebar's primary, the phone's
   * create button) and holds `open`.
   */
  trigger?: ReactElement<Record<string, unknown>>
  open?: boolean
  onOpenChange?: (open: boolean) => void
  /**
   * The item rows, for each template's count. Left out, the menu reads the
   * list once each time it opens; `null` while the host's rows load (no counts).
   */
  rows?: readonly ItemSummary[] | null
}

/**
 * "New item" (brief E1): one list, two looks. On a desktop a menu under
 * NEW ITEM; on a phone a sheet (Mobile / 06) whose overline names the
 * vault the item goes to. The five made most come first, each with its
 * count; then every other template a person makes by hand (D110) in the
 * code's four groups (D231), variants included: submenus on a desktop,
 * closed Section Headers on a phone. Then the person's own templates
 * (D232), and "Your templates" to make or change them. A passkey is made by
 * a site through a provider and is never in the list. `onPick` gets the
 * template's id (`login`, `identity/passport`, `user:<id>`).
 */
export function NewItemMenu({ onPick, onNotice = () => {}, trigger, open, onOpenChange, rows }: NewItemMenuProps) {
  const { templates, error, userTemplates } = useTemplates()
  const { nav } = usePlaces()
  const { vaults } = useVaults()
  const sheet = useAdaptiveSheet()
  const [managing, setManaging] = useState(false)
  const [own, setOwn] = useState(open ?? false)
  const isOpen = open ?? own
  const read = useReadRows(rows === undefined && isOpen)
  const counts = useMemo(() => {
    const list = rows === undefined ? read : rows
    return list ? templateCounts(list) : null
  }, [rows, read])

  const kinds = (templates?.kinds ?? []).filter((t) => t.creatable)
  const lead = leadingKind(nav.area, nav.view)
  const firstKinds = lead && !(FIRST_KINDS as readonly string[]).includes(lead) ? [lead, ...FIRST_KINDS] : lead ? [lead, ...FIRST_KINDS.filter((k) => k !== lead)] : [...FIRST_KINDS]
  const first = firstKinds.map((k) => kinds.find((t) => t.kind === k && !t.variant)).filter((t): t is TemplateView => Boolean(t))
  const rest = kinds.filter((t) => !first.includes(t))
  const groups = [...GROUPS.map(([g]) => g), OTHER].map((g) => [g, rest.filter((t) => groupOf(t) === g)] as const).filter(([, list]) => list.length > 0)
  const vault = vaultFor(vaults, nav.scope)
  const howMany = (id: string) => counts?.get(id)

  const row = (t: TemplateView) => (
    <AdaptiveMenuItem key={idOf(t)} icon={templateIcon(t.icon, kindIcon(t.kind))} count={howMany(idOf(t))} onSelect={() => onPick(idOf(t))}>
      {t.name}
    </AdaptiveMenuItem>
  )
  const userRow = (t: UserTemplateView) => (
    <AdaptiveMenuItem key={t.id} icon={templateIcon(t.icon, "categories")} count={howMany(`user:${t.id}`)} onSelect={() => onPick(`user:${t.id}`)}>
      {t.name}
    </AdaptiveMenuItem>
  )

  let body: ReactNode
  if (!templates) {
    // No way to ask the core again from here yet (doc 89 §6): the line says what happened, and closing and opening tries nothing new.
    body = <AdaptiveMenuGroup label={error ? "The item kinds didn't load." : "Loading the item kinds…"}>{null}</AdaptiveMenuGroup>
  } else {
    body = (
      <>
        <AdaptiveMenuGroup>{first.map(row)}</AdaptiveMenuGroup>
        <AdaptiveMenuSeparator />
        {groups.map(([name, list]) => (sheet ? <SheetGroup key={name} name={name}>{list.map(row)}</SheetGroup> : <MenuGroup key={name} name={name}>{list.map(row)}</MenuGroup>))}
        {userTemplates.length > 0 && (
          <>
            <AdaptiveMenuSeparator />
            <AdaptiveMenuGroup label={`Your templates ${count(userTemplates.length)}`}>{userTemplates.map(userRow)}</AdaptiveMenuGroup>
          </>
        )}
        <AdaptiveMenuSeparator />
        <AdaptiveMenuItem icon="categories" onSelect={() => setManaging(true)}>
          Your templates
        </AdaptiveMenuItem>
      </>
    )
  }

  return (
    <>
      <AdaptiveMenu
        title="New item"
        overline={sheet ? `New item // ${vault ? `${vault.name} vault` : "your vault"}` : undefined}
        align="end"
        width="auto"
        open={isOpen}
        onOpenChange={(next) => {
          setOwn(next)
          onOpenChange?.(next)
        }}
        trigger={
          trigger ?? (
            <Button size="sm">
              <Icon name="add" data-icon="inline-start" />
              New item
            </Button>
          )
        }
      >
        {body}
      </AdaptiveMenu>
      <TemplatesManager open={managing} onOpenChange={setManaging} onNotice={onNotice} />
    </>
  )
}

/** The item list, read once each time the menu opens when its host passes no rows; `null` until it arrives or when it fails (no counts). */
function useReadRows(when: boolean): ItemSummary[] | null {
  const client = useClient()
  const [rows, setRows] = useState<ItemSummary[] | null>(null)
  useEffect(() => {
    if (!when) return
    let live = true
    Promise.resolve()
      .then(() => client.list())
      .then((r) => live && setRows(Array.isArray(r) ? r : null))
      .catch(() => live && setRows(null))
    return () => {
      live = false
    }
  }, [client, when])
  return rows
}

/** A group on a phone: a closed Section Header whose title opens it (`aria-expanded`), named as a group. */
function SheetGroup({ name, children }: { name: string; children: ReactNode }) {
  return (
    <div role="group" aria-label={name}>
      <SectionHeader title={name} level={3} collapsible defaultOpen={false}>
        <div className="flex flex-col gap-1">{children}</div>
      </SectionHeader>
    </div>
  )
}

/** A group on a desktop: a submenu, its rows a group of the same name. */
function MenuGroup({ name, children }: { name: string; children: ReactNode }) {
  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>{name}</DropdownMenuSubTrigger>
      <DropdownMenuSubContent width="auto">
        <DropdownMenuGroup aria-label={name}>{children}</DropdownMenuGroup>
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}

/** The four kinds quick create offers (States / 03): the ones made most. */
const QUICK_KINDS = ["login", "card", "note", "code"] as const

/**
 * Quick create's rows (States / 03): Login, Card, Secure note and One-time
 * code, each by the name its template carries, with no counts, under the
 * overline CREATE // LONG PRESS. Menu rows: the shell puts them in the menu
 * its create button opens on a long press, and turns the button to close.
 */
export function QuickCreateItems({ onPick }: { onPick: (template: string) => void }) {
  const { templates } = useTemplates()
  // The kind's own template, not one of its variants (a Passport, a Router).
  const kinds = (templates?.kinds ?? []).filter((t) => t.creatable && !t.variant)
  const rows = QUICK_KINDS.map((k) => kinds.find((t) => t.kind === k)).filter((t): t is TemplateView => Boolean(t))
  return (
    <DropdownMenuGroup>
      <DropdownMenuLabel>Create // long press</DropdownMenuLabel>
      {rows.map((t) => (
        <DropdownMenuItem key={t.kind} onClick={() => onPick(t.kind)}>
          <Icon name={templateIcon(t.icon, kindIcon(t.kind))} />
          {t.name}
        </DropdownMenuItem>
      ))}
    </DropdownMenuGroup>
  )
}
