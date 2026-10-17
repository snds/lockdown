import { useEffect, useState, type ReactNode } from "react"

import { Badge, Button, Dialog, DialogAction, DialogActions, DialogContent, DialogDescription, DialogHeader, DialogTitle, Icon, ItemGroup, Spinner } from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import type { FieldView, ItemDetail } from "@/core/types"
import { codeKey, useCodes } from "@/features/codes/use-codes"
import { whenLocal } from "@/lib/format"

import { ItemCodeDisplay } from "./code-display"
import { RiskTierBadge } from "@unicron/ui/security/risk-tier-badge"

import { ItemCrownJewel } from "./crown-jewel"
import { kindLabel } from "./list-model"
import { ItemField, type SecretFormat } from "./item-field"
import { LargeTypeButton } from "./large-type"

interface DetailPaneProps {
  /** The parent keys this pane by item and version, so a change remounts it. */
  id: string
  onEdit: (item: ItemDetail) => void
  onTrashed: () => void
  onNotice: (text: string) => void
}

/**
 * One item in full, laid out by kind (brief §3.1): a Login's methods block
 * first (password, passkeys, codes), then its URLs, then the rest; a Code's
 * display full width; a Note's body; a Card's number in groups of four
 * with CVC and PIN masked; every other kind as its field list. A crown
 * jewel's whole detail sits under the treatment (D98).
 */
export function DetailPane({ id, onEdit, onTrashed, onNotice }: DetailPaneProps) {
  const client = useClient()
  const [item, setItem] = useState<ItemDetail | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [confirm, setConfirm] = useState(false)

  useEffect(() => {
    let live = true
    client
      .show(id)
      .then((d) => live && setItem(d))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client, id])

  const hasCodes = item?.fields.some((f) => f.kind === "totp") ?? false
  const codes = useCodes(hasCodes, (c) => c.item_id === id)

  const trash = async () => {
    try {
      await client.trash(id)
      setConfirm(false)
      onTrashed()
    } catch (e) {
      setError(describeError(e))
    }
  }

  if (error) {
    return (
      <p role="alert" className="p-6 text-sm text-destructive">
        {error}
      </p>
    )
  }
  if (!item) {
    return (
      <div className="p-6">
        <Spinner />
      </div>
    )
  }
  const title = item.fields.find((f) => f.field_id === "title")?.value ?? kindLabel(item.kind)
  const trashed = item.state === "trashed"
  const field = (f: FieldView, format?: SecretFormat, largeType?: string) => (
    <ItemField key={f.field_id} itemId={item.id} field={f} onNotice={onNotice} format={format} largeType={largeType} />
  )
  const codeBlocks = (item.fields.filter((f) => f.kind === "totp") ?? []).map((f) => {
    const view = codes.codes?.find((c) => c.field_id === f.field_id)
    return view ? (
      <div key={f.field_id} className="flex flex-col items-start gap-1">
        <ItemCodeDisplay code={view} now={codes.now} expiresAt={codes.expiresAt[codeKey(view)] ?? null} />
        {item.kind === "code" && (
          <LargeTypeButton
            title={`Code · ${[view.issuer, view.account].filter(Boolean).join(" · ") || title}`}
            getSecret={() => Promise.resolve(new TextEncoder().encode(view.current))}
          />
        )}
      </div>
    ) : (
      <div key={f.field_id} className="text-sm text-muted-foreground">
        {codes.error ?? "Computing the code…"}
      </div>
    )
  })

  const body = layoutFor(item, field, codeBlocks)
  return (
    <article className="flex flex-col gap-4 p-4 md:p-6">
      <header className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="flex items-center gap-2 truncate text-xl font-semibold tracking-tight">
            {title}
            {item.tier && <RiskTierBadge tier={item.tier} verified={item.tier_verified} size="md" explain />}
          </h2>
          <p className="text-sm text-muted-foreground">
            {kindLabel(item.kind)} · version {item.version} · edited {whenLocal(item.edited_at)}
            {trashed ? " · in the trash" : ""}
          </p>
        </div>
        {!trashed && (
          <div className="flex shrink-0 items-center gap-1">
            {item.kind === "login" && (
              <Button variant="outline" size="sm" onClick={() => onEdit(item)}>
                <Icon name="edit" data-icon="inline-start" />
                Edit
              </Button>
            )}
            <Button variant="ghost-destructive" size="sm" onClick={() => setConfirm(true)}>
              <Icon name="delete" data-icon="inline-start" />
              Trash
            </Button>
          </div>
        )}
      </header>
      {item.crown_jewel ? <ItemCrownJewel title={title}>{body}</ItemCrownJewel> : body}
      <Dialog open={confirm} onOpenChange={setConfirm}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Move to the trash?</DialogTitle>
            <DialogDescription>"{title}" leaves the list and stays readable in Deleted through its grace period. Nothing is destroyed yet.</DialogDescription>
          </DialogHeader>
          <DialogActions>
            <DialogAction kind="cancel" onClick={() => setConfirm(false)}>
              Cancel
            </DialogAction>
            <DialogAction kind="destructive" onClick={trash}>
              Trash
            </DialogAction>
          </DialogActions>
        </DialogContent>
      </Dialog>
    </article>
  )
}

type FieldRenderer = (f: FieldView, format?: SecretFormat, largeType?: string) => ReactNode

const isMeta = (f: FieldView) => f.field_id === "title"
const isUrl = (f: FieldView) => f.kind === "url"
const isMethod = (f: FieldView) => f.field_id === "pw" || f.kind === "passkey" || f.kind === "totp"

function Block({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2" aria-label={title}>
      <h3 className="text-xs font-medium text-muted-foreground uppercase">{title}</h3>
      <ItemGroup className="gap-2">{children}</ItemGroup>
    </section>
  )
}

/** The layout per kind (brief §3.1). Unknown kinds render the generic field list. */
function layoutFor(item: ItemDetail, field: FieldRenderer, codeBlocks: ReactNode[]): ReactNode {
  const fields = item.fields.filter((f) => !isMeta(f))
  const title = item.fields.find((f) => f.field_id === "title")?.value ?? kindLabel(item.kind)
  switch (item.kind) {
    case "login": {
      const password = fields.find((f) => f.field_id === "pw")
      const passkeys = fields.filter((f) => f.kind === "passkey")
      const urls = fields.filter(isUrl)
      const rest = fields.filter((f) => !isMethod(f) && !isUrl(f))
      return (
        <>
          <Block title="Sign-in methods">
            {password ? field(password) : <p className="text-sm text-muted-foreground">No password; this login uses other methods.</p>}
            {passkeys.map((p) => (
              <p key={p.field_id} className="flex items-center gap-2 text-sm">
                <Badge variant="secondary">
                  <Icon name="passkey" data-icon="inline-start" />
                  passkey
                </Badge>
                <span className="text-muted-foreground">{p.label}; used from the browser or the OS, never shown.</span>
              </p>
            ))}
            {codeBlocks}
          </Block>
          {urls.length > 0 && <Block title="Websites">{urls.map((u) => field(u))}</Block>}
          {rest.length > 0 && <Block title="Details">{rest.map((f) => field(f))}</Block>}
        </>
      )
    }
    case "code": {
      const rest = fields.filter((f) => f.kind !== "totp")
      return (
        <>
          <Block title="One-time code">
            {codeBlocks}
            {fields.filter((f) => f.kind === "totp").map((f) => field(f))}
          </Block>
          {rest.length > 0 && <Block title="Details">{rest.map((f) => field(f))}</Block>}
        </>
      )
    }
    case "wi-fi": {
      // The network's password reads across a room: "Show large type" on the hidden fields (brief §3.1).
      return fields.length ? (
        <Block title="Network">{fields.map((f) => field(f, undefined, f.hidden ? `${f.label} · ${title}` : undefined))}</Block>
      ) : (
        <p className="text-sm text-muted-foreground">No fields of this kind yet.</p>
      )
    }
    case "note": {
      const body = fields.find((f) => ["body", "notes", "note", "text"].includes(f.field_id)) ?? fields.find((f) => f.kind === "text")
      const rest = fields.filter((f) => f !== body)
      return (
        <>
          {body ? (
            <section aria-label="Note" className="rounded-md border p-3">
              <pre className="font-sans text-sm whitespace-pre-wrap">{body.value ?? ""}</pre>
            </section>
          ) : (
            <p className="text-sm text-muted-foreground">An empty note.</p>
          )}
          {rest.length > 0 && <Block title="Details">{rest.map((f) => field(f))}</Block>}
        </>
      )
    }
    case "card": {
      const number = fields.find((f) => ["number", "card_number", "pan"].includes(f.field_id))
      const secrets = fields.filter((f) => f !== number && (f.hidden || /cvc|cvv|pin/i.test(f.field_id)))
      const rest = fields.filter((f) => f !== number && !secrets.includes(f))
      return (
        <>
          <Block title="Card">
            {number && field(number, "groups-of-4")}
            {secrets.map((f) => field(f))}
          </Block>
          {rest.length > 0 && <Block title="Details">{rest.map((f) => field(f))}</Block>}
          <p className="text-sm text-muted-foreground">Filling a card into a page always takes a second press, never a hover or a timer.</p>
        </>
      )
    }
    default:
      return fields.length ? (
        <Block title="Fields">{fields.map((f) => field(f))}</Block>
      ) : (
        <p className="text-sm text-muted-foreground">No fields of this kind yet.</p>
      )
  }
}
