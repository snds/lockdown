import { useEffect, useMemo, useState } from "react"

import { AppFrameInset, Banner, Button, EmptyState, FilterChip, FilterChipGroup, SearchField, SectionHeader, useAppFrame } from "@unicron/ui"

import { useCrumb } from "@/app/crumbs"
import { usePlaces, type Nav } from "@/app/places"
import { useShell } from "@/app/shell"
import { useStatusItem } from "@/app/status"
import { MasterDetail } from "@/components/master-detail"
import type { ItemSummary, VaultView } from "@/core/types"
import { OrganizePrompt } from "@/features/organization/organize-prompt"
import { folderPaths, inScope, rowMenuOffers, scopeTitle, targetOf, type OrganizeTarget } from "@/features/organization/org-model"
import { RowMenuItems } from "@/features/organization/row-menu"
import { useRowMenuLayers } from "@/features/organization/row-menu-layers"
import { useVaults } from "@/features/organization/vaults-context"
import { count, countName } from "@/lib/format"

import { DetailPane } from "./detail-pane"
import { ItemRow } from "./item-row"
import { VIEW_LABELS, rowWords, rowsIn, searchRows, sortRows } from "./list-model"
import { clearTyped, onSearchWith, openedFrom, typedWords } from "./search-words"
import { ItemCodeDisplay } from "./code-display"
import { codeKey, useCodes } from "@/features/codes/use-codes"

interface SearchViewProps {
  rows: ItemSummary[] | null
  /** The open item: kept by the app, as in the vault's list. */
  selected: string | null
  onSelect: (id: string | null) => void
  onChanged: () => void
  onNotice: (text: string) => void
}

/** Recently used before typing (brief X4): five at most. */
export const RECENT_MAX = 5

/** What the field searches, by the vaults' names (brief X4): "Personal and Family", "your vault", "7 vaults". */
export function searchLabel(names: readonly string[]): string {
  if (names.length <= 1) return "Search your vault"
  if (names.length > 3) return `Search ${names.length} vaults`
  return `Search ${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`
}

/** Where nothing matched, in the drawn words: "Nothing in Personal or Family matches." */
function nothingIn(names: readonly string[]): string {
  if (names.length === 1) return `Nothing in ${names[0]} matches.`
  if (names.length === 0 || names.length > 3) return "Nothing in your vaults matches."
  return `Nothing in ${names.slice(0, -1).join(", ")} or ${names[names.length - 1]} matches.`
}

/** The second chip: the list Search was opened from, when it is narrower than every live item. */
export interface Narrow {
  /** The chip's words, in sentence case ("Family", "#work", "Cards", "Recently deleted"). */
  label: string
  nav: Nav
  /** Archived and Recently deleted are outside Search (D149): opened from there, their chip starts chosen. */
  chosen: boolean
}

/** The chip for the list Search was opened from (brief X4), or none for All items and for places that are not lists. */
export function narrowOf(nav: Nav | null, vaults: VaultView[]): Narrow | null {
  if (!nav || nav.area !== "vault") return null
  const scopeName = nav.scope ? scopeTitle(nav.scope, vaults) : null
  if (!scopeName && nav.view === "all") return null
  const label = scopeName && nav.view !== "all" ? `${scopeName} // ${VIEW_LABELS[nav.view]}` : (scopeName ?? VIEW_LABELS[nav.view])
  return { label, nav, chosen: nav.view === "archived" || nav.view === "deleted" }
}

/** The rows a chip searches: every live item, or the list it names. */
export function searchBase(rows: ItemSummary[], narrow: Narrow | null, vaults: VaultView[]): ItemSummary[] {
  if (!narrow) return rowsIn(rows, "all")
  const inView = rowsIn(rows, narrow.nav.view)
  return narrow.nav.scope ? inScope(inView, narrow.nav.scope, vaults) : inView
}

/** Recently used on this device (`last_used`, never synced), newest first, five at most. */
export function recentlyUsed(rows: ItemSummary[]): ItemSummary[] {
  return rows
    .filter((r) => (r.last_used ?? 0) > 0)
    .sort((a, b) => (b.last_used ?? 0) - (a.last_used ?? 0) || a.title.localeCompare(b.title))
    .slice(0, RECENT_MAX)
}

/** One group per vault that holds a match, in the vaults' own order; rows of a vault the app doesn't list go last. */
export function groupByVault(found: ItemSummary[], vaults: VaultView[]): Array<{ vault: VaultView | null; rows: ItemSummary[] }> {
  const groups: Array<{ vault: VaultView | null; rows: ItemSummary[] }> = vaults.map((v) => ({ vault: v, rows: found.filter((r) => r.vault_id === v.id) })).filter((g) => g.rows.length > 0)
  const loose = found.filter((r) => !vaults.some((v) => v.id === r.vault_id))
  if (loose.length) groups.push({ vault: null, rows: loose })
  return groups
}

const EVERYWHERE = "everywhere"
const HERE = "here"

/**
 * Search as a place (brief X4; Desktop / 11, Mobile / 14 and 15). The field
 * has focus, named for the vaults it looks in. Before typing: RECENTLY
 * USED, up to five items this device used, newest first; nothing when
 * nothing was used here. Each key narrows the results at once (the rows are
 * loaded): live items in every vault, grouped by vault under a Section
 * Header with its count (none with one vault), the words marked in the
 * title and the mono line (a crown jewel's in its title only), on the item
 * lists' own rows with their ⋯ and, on a phone, their swipes. Opened from a
 * narrower list, the chips EVERYWHERE and that list (chosen when it is
 * Archived or Recently deleted, which Search leaves out, D149). Nothing
 * found: the Empty State, whose NEW LOGIN starts a login with the words as
 * its website. Escape clears the field, then leaves for where the person was.
 */
export function SearchView({ rows, selected, onSelect, onChanged, onNotice }: SearchViewProps) {
  const { vaults } = useVaults()
  const { go, navigate } = usePlaces()
  const { edit, leaveSearch } = useShell()
  const frame = useAppFrame()
  const [query, setQuery] = useState(typedWords)
  // Opened by typing in All items' field: emptying the field goes back to the list, as a search in place would.
  const [fromList] = useState(() => typedWords() !== "")
  // Read while drawing, before the list that was showing clears it (search-words.ts).
  const [from] = useState(openedFrom)
  const narrow = useMemo(() => narrowOf(from, vaults), [from, vaults])
  const [chip, setChip] = useState(() => (narrow?.chosen ? HERE : EVERYWHERE))
  const here = Boolean(narrow && chip === HERE)
  // Taken once: the next Search starts empty. Words handed over later (Quick access) fill the field.
  useEffect(() => clearTyped(), [])
  useEffect(
    () =>
      onSearchWith((words) => {
        setQuery(words)
        clearTyped()
      }),
    [],
  )
  // Escape leaves for where the person was: the list it was typed in, the list it was opened from, or
  // any other place, Codes and Settings among them (the shell's place before Search, Lockdown 19).
  const leave = () => {
    if (fromList) go("all")
    else if (from) navigate(from)
    else leaveSearch()
  }
  const type = (words: string) => {
    setQuery(words)
    if (!words && fromList) go("all")
  }
  const [version, setVersion] = useState(0)
  const [organizing, setOrganizing] = useState<OrganizeTarget | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const paths = useMemo(() => folderPaths(vaults), [vaults])
  const q = query.trim()
  const base = useMemo(() => (rows ? searchBase(rows, here ? narrow : null, vaults) : []), [rows, here, narrow, vaults])
  const found = useMemo(() => (q ? sortRows(searchRows(base, q, paths), "title") : []), [base, q, paths])
  const recent = useMemo(() => (q ? [] : recentlyUsed(base)), [base, q])
  const oneVault = vaults.length < 2 || Boolean(here && narrow?.nav.scope?.vault)
  const groups = useMemo(() => (oneVault ? [{ vault: null, rows: found }] : groupByVault(found, vaults)), [oneVault, found, vaults])
  // A found item's one-time code shows inside its own row, under its words (Lockdown 19, as the popup's
  // D957): a code under the list could be read as another item's.
  const withCodes = useMemo(() => [...found, ...recent].filter((r) => r.codes > 0 && !r.crown_jewel).map((r) => r.id), [found, recent])
  const codeIds = withCodes.join(" ")
  const codes = useCodes(withCodes.length > 0, (c) => withCodes.includes(c.item_id))
  const reloadCodes = codes.reload
  useEffect(() => {
    if (codeIds) reloadCodes()
  }, [codeIds, reloadCodes])
  const codeOf = (r: ItemSummary) => {
    const mine = (codes.codes ?? []).filter((c) => c.item_id === r.id)
    if (!mine.length) return undefined
    return (
      <div className="flex flex-col gap-1">
        {mine.map((c) => (
          <ItemCodeDisplay key={codeKey(c)} code={c} now={codes.now} expiresAt={codes.expiresAt[codeKey(c)] ?? null} layout="compact" />
        ))}
      </div>
    )
  }

  // SEARCH, or SEARCH // FAMILY while that chip is chosen; the status says how many.
  useCrumb(here && narrow ? [narrow.label] : null)
  useStatusItem(q && rows ? { text: `${count(found.length)} ${found.length === 1 ? "result" : "results"}` } : null)

  const bump = () => {
    setVersion((v) => v + 1)
    onChanged()
  }
  const changed = (text: string, moved?: string, id?: string) => {
    setActionError(null)
    if (moved && selected === id) onSelect(moved)
    bump()
    if (text) onNotice(text)
  }
  const menuLayers = useRowMenuLayers({ onChanged: (text) => changed(text), onError: setActionError })
  // A phone's rows swipe as the item lists' do (brief V1).
  const swipe = frame.phone ? { onChanged: (text: string) => changed(text), onError: setActionError, onNotice } : null
  const label = searchLabel(vaults.map((v) => v.name))
  const list = (rs: ItemSummary[], mark: string) => (
    <ul role="list" className="flex flex-col">
      {rs.map((r) => (
        <ItemRow
          key={r.id}
          row={r}
          selected={selected === r.id}
          onSelect={onSelect}
          highlight={mark}
          words={rowWords(r, { view: "all", vaults, oneVault })}
          detail={codeOf(r)}
          swipe={swipe}
          actions={
            rowMenuOffers(r) ? (
              <RowMenuItems
                row={r}
                onOrganize={(row) => setOrganizing(targetOf(row))}
                onHistory={menuLayers.onHistory}
                onPurge={menuLayers.onPurge}
                onChanged={(text, moved) => changed(text, moved, r.id)}
                onError={setActionError}
                swiped={Boolean(swipe)}
              />
            ) : undefined
          }
        />
      ))}
    </ul>
  )
  return (
    <>
      <MasterDetail
        title="Search"
        header={
          <AppFrameInset>
            <h1 className="sr-only">Search</h1>
            <div className="flex flex-col gap-4 pb-2">
              <SearchField
                label={label}
                value={query}
                onValueChange={type}
                autoFocus
                onKeyDown={(e) => {
                  // Escape clears first (the field's own); a second one leaves.
                  if (e.key === "Escape" && !query) leave()
                }}
              />
              {narrow && (
                <FilterChipGroup type="single" scroll label="Search in" value={chip} onValueChange={setChip}>
                  <FilterChip value={EVERYWHERE}>Everywhere</FilterChip>
                  <FilterChip value={HERE}>{narrow.label}</FilterChip>
                </FilterChipGroup>
              )}
            </div>
          </AppFrameInset>
        }
        list={
          <div className="flex flex-col gap-6 pb-6" aria-live="polite">
            {actionError && (
              <AppFrameInset>
                <Banner tone="critical" size="compact" title={actionError} onDismiss={() => setActionError(null)} />
              </AppFrameInset>
            )}
            {!q && recent.length > 0 && (
              <section aria-label={`Recently used, ${countName(recent.length, "item")}`}>
                <AppFrameInset>
                  <SectionHeader title="Recently used" count={recent.length} />
                </AppFrameInset>
                {list(recent, "")}
              </section>
            )}
            {q && rows && found.length === 0 && (
              // The Empty State keeps its own 16 from the edge, under the field, as Mobile / 14 draws it. Beside the
              // sidebar the pane's gutter is that edge already, so it steps back by its own 16 and lines up with the
              // field, as Desktop / 11 draws it (a Unicron gap: EmptyState has no flush form).
              <div className={frame.phone ? undefined : "-ms-4"}>
                <EmptyState
                  overline={`Search // ${count(0)}`}
                  title={`No item for ${q}.`}
                  body={`${here && narrow ? `Nothing in ${narrow.label} matches.` : nothingIn(vaults.map((v) => v.name))} Save it from the browser next time you sign in, or add it here.`}
                  action={
                    <Button variant="secondary" onClick={() => edit({ kind: "login", item: null, website: q })}>
                      New login
                    </Button>
                  }
                />
              </div>
            )}
            {found.length > 0 &&
              groups.map((g) => (
                <section key={g.vault?.id ?? "loose"} aria-label={`${g.vault?.name ?? "Results"}, ${countName(g.rows.length, "result")}`}>
                  {g.vault && (
                    <AppFrameInset>
                      <SectionHeader title={g.vault.name} count={g.rows.length} />
                    </AppFrameInset>
                  )}
                  {list(g.rows, q)}
                </section>
              ))}
          </div>
        }
        detail={
          selected ? (
            <DetailPane
              key={`${selected}:${version}`}
              id={selected}
              rows={rows ?? []}
              onEdit={(item) => edit({ kind: item.template ?? item.kind, item })}
              onTrashed={() => {
                onSelect(null)
                // The pane says it itself, with UNDO (Q14).
                bump()
              }}
              onChanged={(text) => {
                bump()
                onNotice(text)
              }}
              onReplaced={(id, text) => {
                onSelect(id)
                bump()
                onNotice(text)
              }}
              onNotice={onNotice}
            />
          ) : null
        }
        onBack={() => onSelect(null)}
        emptyLabel="Pick a result to see it here."
      />
      <OrganizePrompt
        target={organizing}
        rows={rows ?? []}
        onClose={() => setOrganizing(null)}
        onSaved={() => {
          const title = organizing?.title
          setOrganizing(null)
          bump()
          onNotice(title ? `Saved ${title}` : "Saved")
        }}
      />
      {menuLayers.layers}
    </>
  )
}
