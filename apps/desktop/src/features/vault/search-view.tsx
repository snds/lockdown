import { useMemo, useState } from "react"

import { AppFrameInset, Button, EmptyState, ItemGroup, SearchField, SectionHeader } from "@unicron/ui"

import { usePlaces } from "@/app/places"
import { useShell } from "@/app/shell"
import { MasterDetail } from "@/components/master-detail"
import type { ItemSummary } from "@/core/types"
import { folderPaths } from "@/features/organization/org-model"
import { useVaults } from "@/features/organization/vaults-context"
import { count, countName } from "@/lib/format"

import { DetailPane } from "./detail-pane"
import { ItemRow } from "./item-row"
import { rowsIn, searchRows, sortRows } from "./list-model"
import { useQuickCopy } from "./quick-copy"

interface SearchViewProps {
  rows: ItemSummary[] | null
  /** The open item: kept by the app, as in the vault's list. */
  selected: string | null
  onSelect: (id: string | null) => void
  onChanged: () => void
  onNotice: (text: string) => void
}

/** What the field searches, by the vaults' names (brief X4): "Personal and Family", "your vault", "7 vaults". */
function searchLabel(names: string[]): string {
  if (names.length <= 1) return "Search your vault"
  if (names.length > 3) return `Search ${names.length} vaults`
  return `Search ${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`
}

/**
 * Search as a place (redesign brief X4; this is its basic form, edges-6
 * finishes it): the field, then the live items in every vault that match,
 * grouped by vault, each opening beside the list. Nothing found: the Empty
 * State, whose New login starts a login with the query as its website.
 * Archived and deleted items stay out (D149). Escape clears the field,
 * then leaves for the Items list.
 */
export function SearchView({ rows, selected, onSelect, onChanged, onNotice }: SearchViewProps) {
  const { vaults } = useVaults()
  const { go } = usePlaces()
  const { edit } = useShell()
  const [query, setQuery] = useState("")
  const [version, setVersion] = useState(0)
  const quickCopy = useQuickCopy(onNotice)
  const paths = useMemo(() => folderPaths(vaults), [vaults])
  const q = query.trim()
  const found = useMemo(() => (rows && q ? sortRows(searchRows(rowsIn(rows, "all"), q, paths), "title") : []), [rows, q, paths])
  // One group per vault that holds a match, in the vaults' own order; no headers with one vault.
  const groups = useMemo(() => vaults.map((v) => ({ vault: v, rows: found.filter((r) => r.vault_id === v.id) })).filter((g) => g.rows.length > 0), [vaults, found])
  const loose = found.filter((r) => !vaults.some((v) => v.id === r.vault_id))
  const bump = () => {
    setVersion((v) => v + 1)
    onChanged()
  }
  const label = searchLabel(vaults.map((v) => v.name))
  const list = (rs: ItemSummary[]) => (
    <ItemGroup role="list">
      {rs.map((r) => (
        <ItemRow key={r.id} row={r} selected={selected === r.id} onSelect={onSelect} onCopy={quickCopy} />
      ))}
    </ItemGroup>
  )
  return (
    <MasterDetail
      title="Search"
      header={
        <AppFrameInset>
          <h1 className="sr-only">Search</h1>
          <SearchField
            label={label}
            value={query}
            onValueChange={setQuery}
            autoFocus
            onKeyDown={(e) => {
              // Escape clears first (the field's own); a second one leaves.
              if (e.key === "Escape" && !query) go("all")
            }}
          />
        </AppFrameInset>
      }
      list={
        <div className="flex flex-col gap-6 pt-6" aria-live="polite">
          {q && rows && found.length === 0 && (
            <AppFrameInset>
              <EmptyState
                overline={`Search // ${count(0)}`}
                title={`No item for ${q}.`}
                body={vaults.length > 1 ? "Nothing in your vaults matches. Save it from the browser next time you sign in, or add it here." : "Nothing in your vault matches. Save it from the browser next time you sign in, or add it here."}
                action={
                  <Button variant="secondary" onClick={() => edit({ kind: "login", item: null, website: q })}>
                    New login
                  </Button>
                }
              />
            </AppFrameInset>
          )}
          {groups.map((g) => (
            <section key={g.vault.id} aria-label={`${g.vault.name}, ${countName(g.rows.length, "result")}`}>
              {vaults.length > 1 && (
                <AppFrameInset>
                  <SectionHeader title={g.vault.name} count={g.rows.length} />
                </AppFrameInset>
              )}
              {list(g.rows)}
            </section>
          ))}
          {loose.length > 0 && list(loose)}
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
              bump()
              onNotice("Moved to the trash.")
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
  )
}
