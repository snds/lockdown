import { useCallback, useEffect, useMemo, useState } from "react"

import {
  Button,
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
  Icon,
  Input,
  Item,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
  cn,
} from "@unicron/ui"

import { MasterDetail, PaneHeader } from "@/components/master-detail"
import { describeError, useClient } from "@/core/client"
import type { ItemDetail, ItemSummary } from "@/core/types"

import { DetailPane } from "./detail-pane"
import { LoginEditor } from "./login-editor"

interface VaultViewProps {
  onNotice: (text: string) => void
}

/** The vault: a searchable list on the left, one item on the right. */
export function VaultView({ onNotice }: VaultViewProps) {
  const client = useClient()
  const [rows, setRows] = useState<ItemSummary[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [query, setQuery] = useState("")
  const [selected, setSelected] = useState<string | null>(null)
  const [version, setVersion] = useState(0)
  const [editor, setEditor] = useState<{ open: boolean; item: ItemDetail | null }>({ open: false, item: null })

  const reload = useCallback(() => {
    client
      .list()
      .then((r) => {
        setRows(r)
        setError(null)
      })
      .catch((e) => setError(describeError(e)))
  }, [client])

  useEffect(reload, [reload, version])

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase()
    if (!rows) return []
    if (!q) return rows
    return rows.filter((r) => [r.title, r.site, r.username].some((s) => s.toLowerCase().includes(q)))
  }, [rows, query])

  const header = (
    <PaneHeader title="Vault">
      <Button size="sm" onClick={() => setEditor({ open: true, item: null })}>
        <Icon name="add" data-icon="inline-start" />
        New login
      </Button>
    </PaneHeader>
  )

  const list = (
    <div className="flex flex-col gap-2 px-2 pb-4">
      <div className="px-2">
        <Input
          type="search"
          placeholder="Search titles, sites, usernames"
          aria-label="Search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          autoFocus
        />
      </div>
      {error && (
        <p role="alert" className="px-2 text-sm text-destructive">
          {error}
        </p>
      )}
      {rows && rows.length === 0 && (
        <Empty>
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Icon name="key" />
            </EmptyMedia>
            <EmptyTitle>Nothing saved yet</EmptyTitle>
            <EmptyDescription>Add a login to start.</EmptyDescription>
          </EmptyHeader>
        </Empty>
      )}
      <ItemGroup role="list">
        {shown.map((r) => (
          <Item
            key={r.id}
            role="listitem"
            size="sm"
            render={<button type="button" />}
            aria-current={selected === r.id ? "true" : undefined}
            className={cn("w-full text-left", selected === r.id && "bg-surface-selected")}
            onClick={() => setSelected(r.id)}
          >
            <ItemMedia variant="icon">
              <Icon name={r.kind === "login" ? "key" : "description"} />
            </ItemMedia>
            <ItemContent>
              <ItemTitle>{r.title || r.site || r.id}</ItemTitle>
              <ItemDescription>{[r.username, r.site].filter(Boolean).join(" · ")}</ItemDescription>
            </ItemContent>
          </Item>
        ))}
      </ItemGroup>
    </div>
  )

  return (
    <>
      <MasterDetail
        header={header}
        list={list}
        detail={
          selected ? (
            <DetailPane
              key={`${selected}:${version}`}
              id={selected}
              onEdit={(item) => setEditor({ open: true, item })}
              onTrashed={() => {
                setSelected(null)
                setVersion((v) => v + 1)
                onNotice("Moved to the trash.")
              }}
              onNotice={onNotice}
            />
          ) : null
        }
        onBack={() => setSelected(null)}
        emptyIcon="key"
        emptyLabel="Pick an item to see its fields. Hidden values stay masked until you reveal them."
      />
      <LoginEditor
        open={editor.open}
        item={editor.item}
        onClose={() => setEditor({ open: false, item: null })}
        onSaved={(id) => {
          setEditor({ open: false, item: null })
          setSelected(id)
          setVersion((v) => v + 1)
        }}
      />
    </>
  )
}
