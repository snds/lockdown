import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react"

import {
  Badge,
  Button,
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandItemAction,
  CommandList,
  CommandShortcut,
  Highlight,
  Icon,
  Kbd,
  ListRow,
  commandItemActionShortcut,
  useViewport,
} from "@unicron/ui"
import { CodeDisplay } from "@unicron/ui/security/code-display"
import { CrownJewelMark } from "@unicron/ui/security/crown-jewel"

import { useGate } from "@/app/gate"
import type { Surface } from "@/app/places"
import { useSettings } from "@/app/settings"
import { useTemplates } from "@/app/templates"
import { decodeAndWipe } from "@/core/secret-bytes"
import { describeError, isAppError, useClient } from "@/core/client"
import type { ItemSummary } from "@/core/types"
import { useCaptureProtect } from "@/features/hygiene/hygiene-client"
import { useVaults } from "@/features/organization/vaults-context"
import { useQuickClient, type QuickHit } from "@/features/quick/quick-client"
import { useQuickCopy } from "@/features/vault/quick-copy"
import { searchWith } from "@/features/vault/search-words"

import { LOCK_ROW, SHOWN_CODE_MS, actionKeys, goToRows, hostOf, isMac, type GoToRow } from "./palette-model"

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
  /**
   * GO TO: goes to a place by its id (app/places.ts `go`). Left out (the
   * quick-access window), GO TO offers Lock alone.
   */
  onGo?: (placeId: string) => void
  /** The Lock command: the app's own lock. Left out, the palette asks the core to lock. */
  onLock?: () => void
  /** Which build draws it, for GO TO's places. Left out: the desktop. */
  surface?: Surface
}

/** The last row's value: "Search everything for …". */
const SEARCH_ROW = "search-everything"
/** GO TO's rows carry this before their place id, so a place can never take an item's id. */
const GO = "go:"

/**
 * Quick access (brief X4; D111; docs/plan/26-desktop-finish.md §1): ⌘K or
 * Ctrl+K inside the app, and the system-wide shortcut's own window. Not
 * offered on a phone, where the rows swipe and the sheets do its jobs.
 *
 * ITEMS: the core's one index, the same the web vault, the browser
 * extension and the command line ask, ranked (favorites, then what this
 * device uses), forgiving a typo, over the summaries only, never a secret.
 * An exact match always ranks above a forgiven one, so Ctrl+Enter copies a
 * typo's result only when nothing matched exactly (D118's worry), and such
 * results say CLOSE MATCH. GO TO: this build's places whose name matches,
 * and Lock. The last row opens Search with the words.
 *
 * Keys on the highlighted item: Enter opens it; ⌘↵ / Ctrl+Enter copies its
 * password or code through the core's copy commands and gates; Alt+Enter
 * shows a code (the code valid now, through the `Use` gate, for thirty
 * seconds, kept out of screen captures); Shift+Enter fills a login in the
 * browser (the login's site opens and Lockdown's extension offers it first
 * there). The search field has focus whenever the palette opens.
 */
export function QuickPalette(props: QuickPaletteProps) {
  const { open, onOpenChange, inline = false } = props
  const phone = useViewport().widthClass === "compact"
  useEffect(() => {
    if (inline || phone) return
    const onKey = (e: globalThis.KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "k") {
        e.preventDefault()
        onOpenChange(!open)
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [open, onOpenChange, inline, phone])
  // A window narrowed to a phone's width while it is open closes it.
  useEffect(() => {
    if (phone && open && !inline) onOpenChange(false)
  }, [phone, open, inline, onOpenChange])
  const shortcut = commandItemActionShortcut().label
  const keys = actionKeys()
  const description = `Search your items by title, username, website or kind, and go to a place by its name. Enter opens one; ${shortcut} copies its password or code; ${keys.code} shows a code; ${keys.fill} fills a login in the browser.`
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
    <CommandDialog open={open && !phone} onOpenChange={onOpenChange} title="Quick access" description={description}>
      {open && !phone && <PaletteBody {...props} />}
    </CommandDialog>
  )
}

/** A result's one mono line: who and where (`kestrel04 // github.com`); nothing for a crown jewel (D113). */
function hitLine(r: QuickHit, kindName: string | undefined): string {
  if (r.crown_jewel) return ""
  const who = r.subtitle || r.username
  const host = r.site ? hostOf(r.site) : ""
  const words = [who, host && host !== who ? host : ""].filter(Boolean)
  return words.length ? words.join(" // ") : (kindName ?? "")
}

function PaletteBody({ onOpenChange, rows, onOpen, onNotice, inline = false, onGo, onLock, surface }: QuickPaletteProps) {
  const client = useClient()
  const quick = useQuickClient()
  const gate = useGate()
  const { settings } = useSettings()
  const { forKind } = useTemplates()
  const { vaults } = useVaults()
  const quickCopy = useQuickCopy(onNotice)
  const mac = isMac()
  const keys = actionKeys()
  const copyKey = commandItemActionShortcut().label
  const [query, setQuery] = useState("")
  const [hits, setHits] = useState<QuickHit[]>([])
  const [searched, setSearched] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [active, setActive] = useState("")
  const [shown, setShown] = useState<{ id: string; field: string; title: string; code: string } | null>(null)
  const latest = useRef(0)
  const input = useRef<HTMLInputElement>(null)
  const lastCopy = useRef<number | null>(null)
  useCaptureProtect(shown !== null)

  // Where GO TO goes: the app's own `go`. The quick-access window has no way into the main
  // window's places (only an item), so it offers Lock alone.
  const go = inline ? undefined : onGo
  const where = surface ?? "desktop"
  const q = query.trim()
  const places = useMemo(() => goToRows(q, where, vaults, mac).filter((r) => r.id === LOCK_ROW || go), [q, where, vaults, mac, go])
  const everything = Boolean(q && go)

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
            setSearched(query)
            setError(null)
          })
          .catch((e) => {
            if (mine === latest.current) setError(describeError(e))
          })
      },
      query ? 60 : 0,
    )
    return () => clearTimeout(t)
  }, [query, quick, rows])

  // The highlight starts on the first row, and moves there again when its row goes.
  const values = useMemo(() => [...hits.map((h) => h.id), ...places.map((p) => GO + p.id), ...(everything ? [SEARCH_ROW] : [])], [hits, places, everything])
  // The index's answer for these words puts the highlight on its first result (the core ranks exact matches first, D118).
  const first = useRef("")
  first.current = values[0] ?? ""
  useEffect(() => {
    if (searched !== null) setActive(first.current)
  }, [searched])
  useEffect(() => {
    if (!values.includes(active)) setActive(values[0] ?? "")
  }, [values, active])

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
  const goTo = (row: GoToRow) => {
    close()
    if (row.id === LOCK_ROW) {
      if (onLock && !inline) onLock()
      else
        void client
          .lock()
          // The lock watch hears it now rather than at its next look (it asks again on a visibility change).
          .then(() => document.dispatchEvent(new Event("visibilitychange")))
          .catch((e) => onNotice(describeError(e)))
      return
    }
    go?.(row.id)
  }
  const searchEverything = () => {
    close()
    searchWith(q)
    go?.("search")
  }
  const showCode = async (row: QuickHit) => {
    if (row.copy?.how !== "code") return
    const field = row.copy.field_id
    try {
      const bytes = await gate.run((g) => quick.quickCode(row.id, field, g))
      setShown({ id: row.id, field, title: row.title, code: decodeAndWipe(bytes) })
    } catch (e) {
      onNotice(isAppError(e) && e.message.startsWith("Cancelled") ? "Nothing was shown" : describeError(e))
    }
  }
  // The shown code's own copy: by id, on the Rust clipboard path, with the clear at the countdown's end (as ItemCodeDisplay).
  const copyShown = async () => {
    if (!shown) return
    const { copy: n } = await gate.run((g) => client.copyCode(shown.id, shown.field, settings.clipboard_clear_secs, "code", g))
    lastCopy.current = n
    return { hash: String(n) }
  }
  const clearShown = (ref?: { hash?: string }) => {
    const n = ref?.hash !== undefined ? Number(ref.hash) : lastCopy.current
    if (n === null || !Number.isSafeInteger(n)) return Promise.resolve(false)
    return client.clipboardClear(n).catch(() => false)
  }
  const fill = async (row: QuickHit) => {
    try {
      const started = await quick.quickFill(row.id)
      close()
      onNotice(`Opened ${started.host}. Lockdown's browser extension offers ${row.title || started.host} first there for the next ${Math.round(started.waits_secs / 60)} minutes`)
    } catch (e) {
      onNotice(describeError(e))
    }
  }
  const canFill = (row: QuickHit) => quick.canFill && row.kind === "login" && !row.crown_jewel && Boolean(row.site)
  const activeRow = hits.find((h) => h.id === active)
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Escape" && inline) {
      e.preventDefault()
      close()
      return
    }
    if (e.key !== "Enter") return
    // ⌘↵ copies an item; on a place or the last row it does nothing (never a second Enter).
    if ((e.metaKey || e.ctrlKey) && !activeRow) {
      e.preventDefault()
      return
    }
    if (!activeRow || e.metaKey || e.ctrlKey) return
    if (e.altKey && activeRow.copy?.how === "code") {
      e.preventDefault()
      void showCode(activeRow)
    } else if (e.shiftKey && canFill(activeRow)) {
      e.preventDefault()
      void fill(activeRow)
    }
  }

  const empty = rows.filter((r) => r.state === "live").length === 0 && query === ""
  const settled = searched === query
  const activePlace = places.find((p) => GO + p.id === active)
  return (
    <Command shouldFilter={false} value={active} onValueChange={setActive} onKeyDown={onKeyDown}>
      <CommandInput ref={input} autoFocus value={query} onValueChange={setQuery} placeholder="Search your items and places" aria-label="Search Lockdown" />
      <CommandList>
        <CommandEmpty>{error ?? (empty ? "Nothing saved yet." : "No matches. Try fewer letters.")}</CommandEmpty>
        {hits.length > 0 && (
          <CommandGroup heading="Items">
            {hits.map((r) => {
              const title = r.title || hostOf(r.site) || forKind(r.kind)?.name || r.kind
              const line = hitLine(r, forKind(r.kind)?.name)
              return (
                <ListRow
                  key={r.id}
                  render={<CommandItem value={r.id} onSelect={() => pick(r.id)} />}
                  layout="inline"
                  size="compact"
                  metaStyle="plain"
                  title={<Highlight text={title} query={q} />}
                  marks={
                    r.crown_jewel || r.favourite ? (
                      <>
                        {r.crown_jewel && <CrownJewelMark size="sm" />}
                        {r.favourite && <Icon name="star--filled" label="Favorite" />}
                      </>
                    ) : undefined
                  }
                  meta={line ? <Highlight text={line} query={q} /> : undefined}
                  badge={r.exact ? undefined : <Badge>Close match</Badge>}
                  action={r.copy ? <CommandItemAction label={`Copy ${r.copy.label}`} icon="copy" onAction={() => copy(r)} /> : undefined}
                />
              )
            })}
          </CommandGroup>
        )}
        {/* GO TO and the last row show whatever ITEMS found, so CommandEmpty never says it: this line does. */}
        {q && settled && hits.length === 0 && (places.length > 0 || everything) && <p className="type-body text-text-secondary px-2.5 py-3">{error ?? "No matches. Try fewer letters."}</p>}
        {places.length > 0 && (
          <CommandGroup heading="Go to">
            {places.map((p) => (
              <CommandItem key={p.id} value={GO + p.id} onSelect={() => goTo(p)}>
                <Icon name={p.icon} />
                <span className="flex min-w-0 flex-1 items-baseline gap-3">
                  <span className="truncate">
                    <Highlight text={p.label} query={q} />
                  </span>
                  {p.within && <span className="type-meta text-muted-foreground">{p.within}</span>}
                </span>
                {p.key && <CommandShortcut>{p.key}</CommandShortcut>}
              </CommandItem>
            ))}
          </CommandGroup>
        )}
        {everything && (
          <CommandGroup>
            <CommandItem value={SEARCH_ROW} onSelect={searchEverything}>
              <Icon name="search" />
              <span className="truncate">Search everything for “{q}”</span>
            </CommandItem>
          </CommandGroup>
        )}
      </CommandList>
      {shown && (
        // The code valid now, for thirty seconds (brief X4): outside the list, so its copy is a real button.
        <div aria-live="polite">
          <CodeDisplay
            layout="compact"
            code={shown.code}
            digits={shown.code.length === 5 || shown.code.length === 6 || shown.code.length === 8 ? shown.code.length : undefined}
            variant={shown.code.length === 5 ? "steam" : "totp"}
            issuer={shown.title}
            clipboardTimer={settings.clipboard_clear_secs}
            onCopy={copyShown}
            onClipboardClear={clearShown}
          />
        </div>
      )}
      {(activeRow || activePlace || active === SEARCH_ROW) && (
        // The keys for the highlighted row, in words (brief X4); the two that are not on the row are also buttons.
        <div className="rule-t flex flex-wrap items-center gap-x-4 gap-y-1 px-2.5 pt-2" role="group" aria-label={activeRow ? `Keys for ${activeRow.title || "this item"}` : "Keys"}>
          <span className="type-caption text-muted-foreground flex items-center gap-1.5">
            <Kbd>{mac ? "↵" : "Enter"}</Kbd>
            {activeRow ? "Open" : activePlace?.id === LOCK_ROW ? "Lock" : activePlace ? "Go" : "Search"}
          </span>
          {activeRow?.copy && (
            <span className="type-caption text-muted-foreground flex items-center gap-1.5">
              <Kbd>{copyKey}</Kbd>
              Copy {activeRow.copy.label}
            </span>
          )}
          {activeRow?.copy?.how === "code" && (
            <Button variant="ghost" size="sm" onClick={() => void showCode(activeRow)}>
              Show code <Kbd>{keys.code}</Kbd>
            </Button>
          )}
          {activeRow && canFill(activeRow) && (
            <Button variant="ghost" size="sm" onClick={() => void fill(activeRow)}>
              Fill in the browser <Kbd>{keys.fill}</Kbd>
            </Button>
          )}
        </div>
      )}
    </Command>
  )
}
