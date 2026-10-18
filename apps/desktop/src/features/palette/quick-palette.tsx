import { useEffect, useMemo } from "react"

import { Command, CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandItemAction, CommandList, Icon, commandItemActionShortcut } from "@unicron/ui"
import { CrownJewelMark } from "@unicron/ui/security/crown-jewel"

import { useTemplates } from "@/app/templates"
import type { ItemSummary } from "@/core/types"
import { kindIcon, kindLabel } from "@/features/vault/list-model"
import { folderPaths, rankRows } from "@/features/organization/org-model"
import { useVaults } from "@/features/organization/vaults-context"
import { useQuickCopy } from "@/features/vault/quick-copy"

import { hostOf, matches, searchWords } from "./palette-model"

interface QuickPaletteProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The vault's rows: masked summaries, never a secret. */
  rows: ItemSummary[]
  /** Enter: show this item in the vault. */
  onOpen: (id: string) => void
  onNotice: (text: string) => void
}

/**
 * The quick-access palette (brief §3.2; D111): ⌘K or Ctrl+K inside the
 * app, on unicron's `CommandDialog`. It searches the item summaries the
 * list already holds (title, username, website host, kind; never a
 * secret), Enter opens the item, and each result offers its one copy
 * (password, code, Wi-Fi password, card number) through the core's copy
 * commands and gates: a crown jewel asks first, and the value never
 * comes to this window.
 *
 * Each result is a `CommandItem` keyed by the item's id (`value`) and
 * found by its `keywords` alone, matched with the vault list's rule (every
 * word a plain substring, in any order) rather than cmdk's fuzzy ranking
 * (§12; D118): ⌘↵ copies the highlighted result, so a loose match ranked
 * first would copy the wrong item's password. The copy is unicron's
 * `CommandItemAction`: a pointer clicks it, ⌘↵ / Ctrl+Enter runs it for
 * the highlighted result, and the result announces it.
 */
export function QuickPalette({ open, onOpenChange, rows, onOpen, onNotice }: QuickPaletteProps) {
  const { forKind } = useTemplates()
  const quickCopy = useQuickCopy(onNotice)
  const shortcut = commandItemActionShortcut().label

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "k") {
        e.preventDefault()
        onOpenChange(!open)
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [open, onOpenChange])

  const { vaults } = useVaults()
  const paths = useMemo(() => folderPaths(vaults), [vaults])
  // Live items only (trashed and archived ones leave search), favourites first,
  // then what this device uses most lately, then A–Z: the first result is the likely one.
  const live = useMemo(() => rankRows(rows.filter((r) => r.state !== "trashed" && r.state !== "archived"), Date.now() / 1000), [rows])
  const words = (r: ItemSummary) => {
    const base = searchWords(r, forKind(r.kind)?.name ?? "")
    if (r.crown_jewel) return base
    return [...base, ...(r.tags ?? []).map((t) => `#${t}`), (r.folder && paths.get(r.folder)) || ""].filter(Boolean)
  }

  const copy = (row: ItemSummary) => {
    // Close first: a crown jewel's ask opens over the window, not over the palette.
    onOpenChange(false)
    void quickCopy(row)
  }
  const pick = (id: string) => {
    onOpenChange(false)
    onOpen(id)
  }

  return (
    <CommandDialog
      open={open}
      onOpenChange={onOpenChange}
      title="Quick access"
      description={`Search your items by title, username, website or kind. Enter opens one; ${shortcut} copies its password or code.`}
    >
      <Command filter={(_value, search, keywords) => (matches(search, keywords ?? []) ? 1 : 0)}>
        <CommandInput placeholder="Search titles, usernames, websites, kinds" />
        <CommandList>
          <CommandEmpty>{live.length ? "No matches. Try fewer letters." : "Nothing saved yet."}</CommandEmpty>
          <CommandGroup heading="Items">
            {live.map((r) => (
              <CommandItem key={r.id} value={r.id} keywords={words(r)} onSelect={() => pick(r.id)}>
                <Icon name={kindIcon(r.kind)} />
                <span className="flex min-w-0 flex-col">
                  <span className="flex items-center gap-1 truncate">
                    {r.title || hostOf(r.site) || kindLabel(r.kind)}
                    {r.favourite && <Icon name="star" fill label="favourite" />}
                    {r.crown_jewel && <CrownJewelMark size="sm" />}
                  </span>
                  <span className="truncate text-xs text-muted-foreground">{[forKind(r.kind)?.name, r.subtitle || r.username].filter(Boolean).join(" · ")}</span>
                </span>
                {r.copy && <CommandItemAction label={`Copy ${r.copy.label}`} icon="content_copy" onAction={() => copy(r)} />}
              </CommandItem>
            ))}
          </CommandGroup>
        </CommandList>
      </Command>
    </CommandDialog>
  )
}
