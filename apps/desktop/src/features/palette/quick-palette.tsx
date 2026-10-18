import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react"

import { Button, Command, CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandItemAction, CommandList, Icon, commandItemActionShortcut } from "@unicron/ui"
import { CrownJewelMark } from "@unicron/ui/security/crown-jewel"

import { useGate } from "@/app/gate"
import { useTemplates } from "@/app/templates"
import { decodeAndWipe } from "@/core/secret-bytes"
import { describeError, isAppError } from "@/core/client"
import type { ItemSummary } from "@/core/types"
import { useCaptureProtect } from "@/features/hygiene/hygiene-client"
import { useQuickClient, type QuickHit } from "@/features/quick/quick-client"
import { kindIcon, kindLabel } from "@/features/vault/list-model"
import { useQuickCopy } from "@/features/vault/quick-copy"

import { SHOWN_CODE_MS, actionKeys, hostOf } from "./palette-model"

interface QuickPaletteProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The vault's rows, to tell "nothing saved" from "nothing found"; search itself is the core's. */
  rows: ItemSummary[]
  /** Enter: show this item in the vault. */
  onOpen: (id: string) => void
  onNotice: (text: string) => void
  /** Drawn in place (the quick-access window) instead of as a dialog. */
  inline?: boolean
}

/**
 * The quick-access palette (brief §3.2; D111; docs/plan/26-desktop-finish.md §1):
 * ⌘K or Ctrl+K inside the app, and the system-wide shortcut's own window.
 * The search is the core's one index, the same the web vault, the browser
 * extension and the command line ask: ranked (favourites, then what this
 * device uses), forgiving a typo, over the summaries only, never a secret.
 * An exact match always ranks above a forgiven one, so Ctrl+Enter copies a
 * typo's result only when nothing matched exactly (D118's worry), and such
 * results say "Close match".
 *
 * Actions on the highlighted result: Enter opens it; ⌘↵ / Ctrl+Enter copies
 * its password or code through the core's copy commands and gates; Alt+Enter
 * shows a code in place (the code valid now, through the `Use` gate, for
 * thirty seconds, kept out of screen captures); Shift+Enter fills a login
 * in the browser (the login's site opens and Lockdown's extension offers it
 * first there). The search field has focus whenever the palette opens.
 */
export function QuickPalette(props: QuickPaletteProps) {
  const { open, onOpenChange, inline = false } = props
  useEffect(() => {
    if (inline) return
    const onKey = (e: globalThis.KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "k") {
        e.preventDefault()
        onOpenChange(!open)
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [open, onOpenChange, inline])
  const shortcut = commandItemActionShortcut().label
  const keys = actionKeys()
  const description = `Search your items by title, username, website or kind. Enter opens one; ${shortcut} copies its password or code; ${keys.code} shows a code; ${keys.fill} fills a login in the browser.`
  if (inline) {
    return (
      <div className="flex h-full flex-col" aria-label="Quick access">
        <p className="sr-only">{description}</p>
        <PaletteBody {...props} />
      </div>
    )
  }
  // The body is drawn only while open, so each opening starts empty with the search in focus.
  return (
    <CommandDialog open={open} onOpenChange={onOpenChange} title="Quick access" description={description}>
      {open && <PaletteBody {...props} />}
    </CommandDialog>
  )
}

function PaletteBody({ onOpenChange, rows, onOpen, onNotice, inline = false }: QuickPaletteProps) {
  const quick = useQuickClient()
  const gate = useGate()
  const { forKind } = useTemplates()
  const quickCopy = useQuickCopy(onNotice)
  const keys = actionKeys()
  const [query, setQuery] = useState("")
  const [hits, setHits] = useState<QuickHit[]>([])
  const [error, setError] = useState<string | null>(null)
  const [active, setActive] = useState("")
  const [shown, setShown] = useState<{ id: string; code: string } | null>(null)
  const latest = useRef(0)
  const input = useRef<HTMLInputElement>(null)
  useCaptureProtect(shown !== null)

  useEffect(() => {
    const t = setTimeout(() => input.current?.focus(), 0)
    return () => clearTimeout(t)
  }, [])

  useEffect(() => {
    const mine = ++latest.current
    const t = setTimeout(
      () => {
        quick
          .search(query, 50)
          .then((found) => {
            if (mine !== latest.current) return
            setHits(found)
            setError(null)
            setActive(found[0]?.id ?? "")
          })
          .catch((e) => {
            if (mine === latest.current) setError(describeError(e))
          })
      },
      query ? 60 : 0,
    )
    return () => clearTimeout(t)
  }, [query, quick, rows])

  // A shown code hides itself.
  useEffect(() => {
    if (!shown) return
    const t = setTimeout(() => setShown(null), SHOWN_CODE_MS)
    return () => clearTimeout(t)
  }, [shown])

  const close = useCallback(() => {
    onOpenChange(false)
    if (inline) void quick.hide()
  }, [onOpenChange, inline, quick])

  const copy = (row: QuickHit) => {
    // Close first: a crown jewel's ask opens over the window, not over the palette.
    close()
    void quickCopy(row)
  }
  const pick = (id: string) => {
    close()
    if (inline) void quick.openMain(id)
    else onOpen(id)
  }
  const showCode = async (row: QuickHit) => {
    if (row.copy?.how !== "code") return
    try {
      const bytes = await gate.run((g) => quick.quickCode(row.id, row.copy!.field_id, g))
      setShown({ id: row.id, code: decodeAndWipe(bytes) })
    } catch (e) {
      onNotice(isAppError(e) && e.message.startsWith("Cancelled") ? "Nothing was shown." : describeError(e))
    }
  }
  const fill = async (row: QuickHit) => {
    try {
      const started = await quick.quickFill(row.id)
      close()
      onNotice(`Opened ${started.host}. Lockdown's browser extension offers ${row.title || started.host} first there for the next ${Math.round(started.waits_secs / 60)} minutes.`)
    } catch (e) {
      onNotice(describeError(e))
    }
  }
  const canFill = (row: QuickHit) => quick.canFill && row.kind === "login" && !row.crown_jewel && Boolean(row.site)
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const row = hits.find((h) => h.id === active)
    if (e.key === "Escape" && inline) {
      e.preventDefault()
      close()
      return
    }
    if (!row || e.key !== "Enter" || e.metaKey || e.ctrlKey) return
    if (e.altKey && row.copy?.how === "code") {
      e.preventDefault()
      void showCode(row)
    } else if (e.shiftKey && canFill(row)) {
      e.preventDefault()
      void fill(row)
    }
  }

  const activeRow = hits.find((h) => h.id === active)
  const empty = rows.filter((r) => r.state === "live").length === 0 && query === ""
  const body = (
    <Command shouldFilter={false} value={active} onValueChange={setActive} onKeyDown={onKeyDown}>
      <CommandInput ref={input} autoFocus value={query} onValueChange={setQuery} placeholder="Search titles, usernames, websites, kinds" aria-label="Search Lockdown" />
      <CommandList>
        <CommandEmpty>{error ?? (empty ? "Nothing saved yet." : "No matches. Try fewer letters.")}</CommandEmpty>
        {hits.length > 0 && (
          <CommandGroup heading="Items">
            {hits.map((r) => (
              <CommandItem key={r.id} value={r.id} onSelect={() => pick(r.id)}>
                <Icon name={kindIcon(r.kind)} />
                <span className="flex min-w-0 flex-col">
                  <span className="flex items-center gap-1 truncate">
                    {r.title || hostOf(r.site) || kindLabel(r.kind)}
                    {r.favourite && <Icon name="star" fill label="favourite" />}
                    {r.crown_jewel && <CrownJewelMark size="sm" />}
                    {!r.exact && <span className="text-xs text-muted-foreground">Close match</span>}
                  </span>
                  <span className="truncate text-xs text-muted-foreground">{[forKind(r.kind)?.name, r.subtitle || r.username].filter(Boolean).join(" · ")}</span>
                  {shown?.id === r.id && (
                    <span className="font-mono text-lg tracking-widest" aria-live="polite" aria-label={`Code ${shown.code.split("").join(" ")}`}>
                      {shown.code}
                    </span>
                  )}
                </span>
                {r.copy && <CommandItemAction label={`Copy ${r.copy.label}`} icon="content_copy" onAction={() => copy(r)} />}
              </CommandItem>
            ))}
          </CommandGroup>
        )}
      </CommandList>
      {activeRow && (activeRow.copy?.how === "code" || canFill(activeRow)) && (
        <div className="flex flex-wrap items-center gap-2 border-t px-2 py-1.5" role="group" aria-label={`More for ${activeRow.title || "this item"}`}>
          {activeRow.copy?.how === "code" && (
            <Button variant="ghost" size="sm" onClick={() => void showCode(activeRow)}>
              <Icon name="pin" data-icon="inline-start" />
              Show code <span className="text-muted-foreground">{keys.code}</span>
            </Button>
          )}
          {canFill(activeRow) && (
            <Button variant="ghost" size="sm" onClick={() => void fill(activeRow)}>
              <Icon name="open_in_browser" data-icon="inline-start" />
              Fill in the browser <span className="text-muted-foreground">{keys.fill}</span>
            </Button>
          )}
        </div>
      )}
    </Command>
  )
  return body
}
