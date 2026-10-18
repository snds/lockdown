import { useEffect, useState, type ReactNode } from "react"

import { Badge, Button, Dialog, DialogAction, DialogActions, DialogContent, DialogDescription, DialogHeader, DialogTitle, Icon, ItemGroup, Spinner } from "@unicron/ui"

import { useTemplates } from "@/app/templates"
import { describeError, useClient } from "@/core/client"
import type { FieldView, ItemDetail, ItemSummary, TemplateView } from "@/core/types"
import { codeKey, useCodes } from "@/features/codes/use-codes"
import { whenLocal } from "@/lib/format"

import { ItemSecurity } from "@/features/security/item-security"

import { ItemCodeDisplay } from "./code-display"
import { RiskTierBadge } from "@unicron/ui/security/risk-tier-badge"

import { FilesPanel } from "@/features/organization/files-panel"
import { MarkdownView } from "@/features/organization/markdown-view"
import { matchRuleWords } from "@/features/organization/org-model"
import { PlacePanel } from "@/features/organization/place-panel"
import { useVaults } from "@/features/organization/vaults-context"
import { useGate } from "@/app/gate"

import { ItemCrownJewel } from "./crown-jewel"
import { sectionsOf } from "./detail-model"
import { conflictPair, kindLabel } from "./list-model"
import { ItemField, type SecretFormat } from "./item-field"
import { ShowLargeType } from "./large-type"

interface DetailPaneProps {
  /** The parent keys this pane by item and version, so a change remounts it. */
  id: string
  /** The list's rows, for the conflict's other half (D58). */
  rows: ItemSummary[]
  onEdit: (item: ItemDetail) => void
  onTrashed: () => void
  /** The item came back from the trash, or a conflict was resolved: reload, keep the selection. */
  onChanged: (notice: string) => void
  onNotice: (text: string) => void
  /** The item was sealed again under a new id (moved to another vault, or its class changed). */
  onReplaced?: (id: string, notice: string) => void
}

type Resolve = "keep-this" | "keep-other" 

/**
 * One item in full, laid out by kind (brief §3.1) from the core's template
 * (D110): a Login's methods block first (password, passkeys, codes), then
 * its websites, then the rest; a Code's display full width; a Note's body;
 * a Passkey's facts, view only; every other kind its template's sections
 * in order (a Card's number in groups of four, a Wi-Fi password with
 * "Show large type"), then the fields outside the template and those
 * added by hand. A kind with no template falls back to the field list. A crown
 * jewel's whole detail sits under the treatment (D98). A trashed item
 * offers Restore; a conflict (D58) offers to keep this copy or the other,
 * which trashes the loser after a confirm, so nothing is destroyed.
 */
export function DetailPane({ id, rows, onEdit, onTrashed, onChanged, onNotice, onReplaced }: DetailPaneProps) {
  const client = useClient()
  const gate = useGate()
  const { vaults } = useVaults()
  const [purging, setPurging] = useState(false)
  const { forKind, fieldOf } = useTemplates()
  const [item, setItem] = useState<ItemDetail | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [confirm, setConfirm] = useState(false)
  const [resolving, setResolving] = useState<Resolve | null>(null)
  const other = conflictPair(rows, id)

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

  const restore = async () => {
    try {
      await client.restore(id)
      onChanged("Restored.")
    } catch (e) {
      setError(describeError(e))
    }
  }

  /** Delete a trashed item for good now, after a confirm (a crown jewel's asks it's you). */
  const purge = async () => {
    setPurging(false)
    try {
      await gate.run((g) => client.purge(id, g))
      onTrashed()
    } catch (e) {
      setError(describeError(e))
    }
  }

  /**
   * Keep one copy of a conflict: the other goes to the trash, readable
   * there through its grace period; a kept conflict copy takes the item's
   * own title back (`resolve_conflict`).
   */
  const resolve = async (which: Resolve) => {
    if (!other) return
    setResolving(null)
    try {
      if (which === "keep-this") {
        await client.resolveConflict(id, other.id)
        onChanged("Kept this copy; the other is in the trash.")
      } else {
        await client.resolveConflict(other.id, id)
        onTrashed()
      }
    } catch (e) {
      setError(describeError(e))
    }
  }

  if (error) {
    return (
      <p role="alert" className="p-6 text-sm text-destructive">
        Couldn't open this item ({id}): {error}
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
  const template = forKind(item.kind)
  const field = (f: FieldView, format?: SecretFormat, largeType?: string) => {
    const t = fieldOf(item.kind, f.field_id)
    return (
      <ItemField
        key={f.field_id}
        itemId={item.id}
        field={t ? { ...f, label: t.label } : f}
        control={t?.control}
        onNotice={onNotice}
        format={format ?? (t?.grouped ? "groups-of-4" : undefined)}
        largeType={largeType}
      />
    )
  }
  const codeBlocks = (item.fields.filter((f) => f.kind === "totp") ?? []).map((f) => {
    const view = codes.codes?.find((c) => c.field_id === f.field_id)
    return view ? (
      <div key={f.field_id} className="flex flex-col items-start gap-1">
        <ItemCodeDisplay code={view} now={codes.now} expiresAt={codes.expiresAt[codeKey(view)] ?? null} />
        {item.kind === "code" && (
          <ShowLargeType
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

  const body = layoutFor(item, template, field, codeBlocks)
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
        {trashed && (
          <div className="flex shrink-0 items-center gap-1">
            <Button variant="outline" size="sm" onClick={restore}>
              <Icon name="restore_from_trash" data-icon="inline-start" />
              Restore
            </Button>
            <Button variant="ghost-destructive" size="sm" onClick={() => setPurging(true)}>
              <Icon name="delete_forever" data-icon="inline-start" />
              Delete now
            </Button>
          </div>
        )}
        {!trashed && (
          <div className="flex shrink-0 items-center gap-1">
            {template?.creatable && (
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
      <PlacePanel
        item={item}
        title={title}
        rows={rows}
        onChanged={onChanged}
        onReplaced={(newId, text) => (onReplaced ? onReplaced(newId, text) : onChanged(text))}
        onError={setError}
      />
      {!trashed && other && (
        <section aria-label="Resolve the conflict" className="flex flex-col gap-2 rounded-xl border p-3 text-sm">
          <p className="flex items-center gap-2">
            <Badge variant="outline">Needs attention</Badge>
            <span>
              Two devices changed this item at the same time, so both versions were kept (D58). Keep one; the other goes to the trash,
              where it stays readable through its grace period.
            </span>
          </p>
          <p className="text-muted-foreground">
            The other copy: "{other.title}", edited {whenLocal(other.edited_at)}.
          </p>
          <div className="flex flex-wrap gap-2">
            <Button size="sm" variant="outline" onClick={() => setResolving("keep-this")}>
              Keep this copy
            </Button>
            <Button size="sm" variant="outline" onClick={() => setResolving("keep-other")}>
              Keep the other
            </Button>
          </div>
        </section>
      )}
      {!trashed && <ItemSecurity key={`${item.id}:${item.version}`} itemId={item.id} onNotice={onNotice} />}
      {item.crown_jewel ? <ItemCrownJewel title={title}>{body}</ItemCrownJewel> : body}
      <FilesPanel itemId={item.id} files={item.attachments ?? []} editable={!trashed && Boolean(template?.creatable)} onChanged={onChanged} onNotice={onNotice} />
      <Dialog open={purging} onOpenChange={setPurging}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete "{title}" for good?</DialogTitle>
            <DialogDescription>
              Its keys are destroyed now, here and on every device at the next sync, so no copy anywhere can be read again. This can't be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogActions>
            <DialogAction kind="cancel" onClick={() => setPurging(false)}>
              Cancel
            </DialogAction>
            <DialogAction kind="destructive" onClick={() => void purge()}>
              Delete for good
            </DialogAction>
          </DialogActions>
        </DialogContent>
      </Dialog>
      <Dialog open={resolving !== null} onOpenChange={(o) => !o && setResolving(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{resolving === "keep-this" ? "Keep this copy?" : "Keep the other copy?"}</DialogTitle>
            <DialogDescription>
              {resolving === "keep-this" ? `"${other?.title}"` : `"${title}"`} moves to the trash. Nothing is destroyed: it stays readable in
              Deleted through its grace period and can be restored from there.
            </DialogDescription>
          </DialogHeader>
          <DialogActions>
            <DialogAction kind="cancel" onClick={() => setResolving(null)}>
              Cancel
            </DialogAction>
            <DialogAction kind="destructive" onClick={() => resolving && void resolve(resolving)}>
              {resolving === "keep-this" ? "Trash the other" : "Trash this copy"}
            </DialogAction>
          </DialogActions>
        </DialogContent>
      </Dialog>
      <Dialog open={confirm} onOpenChange={setConfirm}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Move to the trash?</DialogTitle>
            <DialogDescription>
              "{title}" leaves the list and stays readable in Deleted for {vaults.find((v) => v.id === item.vault_id)?.trash_days ?? 30} days, then it is purged on every
              device. Nothing is destroyed yet.
            </DialogDescription>
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
      <ItemGroup>{children}</ItemGroup>
    </section>
  )
}

const empty = <p className="text-sm text-muted-foreground">Nothing filled in yet. Edit the item to add its details.</p>

/** The layout per kind (brief §3.1). A kind without a template renders the generic field list. */
function layoutFor(item: ItemDetail, template: TemplateView | null, field: FieldRenderer, codeBlocks: ReactNode[]): ReactNode {
  const fields = item.fields.filter((f) => !isMeta(f))
  const title = item.fields.find((f) => f.field_id === "title")?.value ?? kindLabel(item.kind)
  const sections = (list: FieldView[], render: (f: FieldView) => ReactNode = (f) => field(f)) =>
    sectionsOf(list, template).map((s) => (
      <Block key={s.name} title={s.name}>
        {s.fields.map(render)}
      </Block>
    ))
  switch (item.kind) {
    case "login": {
      const password = fields.find((f) => f.field_id === "pw")
      const passkeys = fields.filter((f) => f.kind === "passkey")
      const urls = fields.filter(isUrl)
      const isSignIn = (f: FieldView) => f.field_id === "sign_in_method" || f.field_id === "sign_in_provider"
      const rest = fields.filter((f) => !isMethod(f) && !isUrl(f) && !isSignIn(f) && f.field_id !== "user")
      const user = fields.find((f) => f.field_id === "user")
      const method = fields.find((f) => f.field_id === "sign_in_method")?.value
      const provider = fields.find((f) => f.field_id === "sign_in_provider")?.value
      return (
        <>
          <Block title="Sign-in methods">
            {(method || provider) && (
              <p className="flex items-center gap-2 px-3 text-sm">
                <Badge variant="secondary">{method ?? "Another account"}</Badge>
                {provider && <span className="text-muted-foreground">through {provider}</span>}
              </p>
            )}
            {user && field(user)}
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
          {urls.length > 0 && (
            <Block title="Websites">
              {urls.map((u) => (
                <div key={u.field_id} className="flex flex-col">
                  {field(u)}
                  <p className="px-3 text-xs text-muted-foreground">Offered on: {matchRuleWords(u.match_rule).toLowerCase()}</p>
                </div>
              ))}
            </Block>
          )}
          {rest.length > 0 && sections(rest)}
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
          {rest.length > 0 && sections(rest)}
        </>
      )
    }
    case "note": {
      const body = fields.find((f) => ["body", "notes", "note", "text"].includes(f.field_id)) ?? fields.find((f) => f.kind === "text")
      const rest = fields.filter((f) => f !== body)
      return (
        <>
          {body?.value ? (
            <section aria-label="Note" className="rounded-md border p-3">
              <MarkdownView text={body.value} />
            </section>
          ) : (
            <p className="text-sm text-muted-foreground">An empty note. Edit it to write something.</p>
          )}
          {rest.length > 0 && sections(rest)}
        </>
      )
    }
    case "passkey": {
      return (
        <>
          {fields.length ? sections(fields, (f) => (f.kind === "passkey" ? <PasskeyPart key={f.field_id} label={f.label} /> : field(f))) : empty}
          <p className="text-sm text-muted-foreground">A passkey is made by a site and used from the browser or the OS; its key is never shown or typed.</p>
        </>
      )
    }
    case "wi-fi":
      // The network's password reads across a room: "Show large type" on the hidden fields (brief §3.1).
      return fields.length ? sections(fields, (f) => field(f, undefined, f.hidden ? `${f.label} · ${title}` : undefined)) : empty
    case "card":
      return (
        <>
          {fields.length ? sections(fields) : empty}
          <p className="text-sm text-muted-foreground">Filling a card into a page always takes a second press, never a hover or a timer.</p>
        </>
      )
    default:
      if (!template) return fields.length ? <Block title="Fields">{fields.map((f) => field(f))}</Block> : empty
      return fields.length ? sections(fields) : empty
  }
}

function PasskeyPart({ label }: { label: string }) {
  return (
    <p className="flex items-center gap-2 text-sm">
      <Badge variant="secondary">
        <Icon name="passkey" data-icon="inline-start" />
        passkey
      </Badge>
      <span className="text-muted-foreground">{label}: kept by the provider, never shown.</span>
    </p>
  )
}
