import { useEffect, useMemo, useState } from "react"

import { Command, CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandShortcut, Icon, IconButton } from "@unicron/ui"
import { CrownJewelMark } from "@unicron/ui/security/crown-jewel"

import { useTemplates } from "@/app/templates"
import type { ItemSummary } from "@/core/types"
import { kindIcon, kindLabel } from "@/features/vault/list-model"
import { useQuickCopy } from "@/features/vault/quick-copy"

import { hostOf, isMac, matches, searchWords } from "./palette-model"

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
 * comes to this window. ⌘↵ or Ctrl+Enter copies the highlighted result;
 * the copy control on a result does the same for a pointer.
 */
export function QuickPalette({ open, onOpenChange, rows, onOpen, onNotice }: QuickPaletteProps) {
  const { forKind } = useTemplates()
  const quickCopy = useQuickCopy(onNotice)
  const [highlighted, setHighlighted] = useState("")
  const mod = isMac() ? "⌘" : "Ctrl+"

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

  const live = useMemo(() => rows.filter((r) => r.state !== "trashed"), [rows])
  const words = useMemo(() => new Map(live.map((r) => [r.id, searchWords(r, forKind(r.kind)?.name ?? "")])), [live, forKind])

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
      description={`Search your items by title, username, website or kind. Enter opens one; ${mod}Enter copies its password or code.`}
    >
      <Command
        value={highlighted}
        onValueChange={setHighlighted}
        filter={(value, search) => (matches(search, words.get(value) ?? []) ? 1 : 0)}
        onKeyDown={(e) => {
          if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
            e.preventDefault()
            const row = live.find((r) => r.id === highlighted)
            if (row?.copy) copy(row)
          }
        }}
      >
        <CommandInput placeholder="Search titles, usernames, websites, kinds" />
        <CommandList>
          <CommandEmpty>{live.length ? "No matches. Try fewer letters." : "Nothing saved yet."}</CommandEmpty>
          <CommandGroup heading="Items">
            {live.map((r) => (
              <CommandItem key={r.id} value={r.id} onSelect={() => pick(r.id)}>
                <Icon name={kindIcon(r.kind)} />
                <span className="flex min-w-0 flex-col">
                  <span className="flex items-center gap-1 truncate">
                    {r.title || hostOf(r.site) || kindLabel(r.kind)}
                    {r.crown_jewel && <CrownJewelMark size="sm" />}
                  </span>
                  <span className="truncate text-xs text-muted-foreground">{[forKind(r.kind)?.name, r.subtitle || r.username].filter(Boolean).join(" · ")}</span>
                </span>
                {r.copy && (
                  <CommandShortcut>
                    <span aria-hidden="true">{mod}↵</span>
                    <IconButton
                      label={`Copy ${r.copy.label}`}
                      icon="content_copy"
                      size="icon-xs"
                      tabIndex={-1}
                      onPointerDown={(e) => e.stopPropagation()}
                      onClick={(e) => {
                        e.stopPropagation()
                        copy(r)
                      }}
                    />
                  </CommandShortcut>
                )}
              </CommandItem>
            ))}
          </CommandGroup>
        </CommandList>
      </Command>
    </CommandDialog>
  )
}
