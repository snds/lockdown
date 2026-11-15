import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react"

import {
  AdaptiveMenu,
  AdaptiveMenuGroup,
  AdaptiveMenuItem,
  AdaptiveMenuSeparator,
  AppFrameInset,
  Banner,
  Button,
  CodeBlock,
  ConfirmDialog,
  EmptyState,
  FieldRow,
  FieldRowAction,
  FloatButton,
  IconButton,
  Icon,
  SectionHeader,
  Skeleton,
  Tile,
  useAppFrame,
} from "@unicron/ui"
import { CodeDisplay } from "@unicron/ui/security/code-display"
import { CrownJewelMark } from "@unicron/ui/security/crown-jewel"
import { LargeTypeView } from "@unicron/ui/security/large-type-view"
import { RiskTierBadge } from "@unicron/ui/security/risk-tier-badge"
import { SecretFieldRemoteButton, useSecretFieldRemote } from "@unicron/ui/security/secret-field"

import { useDetailBottomRow } from "@/app/detail-bottom-row"
import { useGate } from "@/app/gate"
import { deletedWords, notify } from "@/app/notify"
import { useTemplates } from "@/app/templates"
import { useDetailTopBar } from "@/components/master-detail"
import { describeError, useClient } from "@/core/client"
import type { CodeView, FieldView, ItemDetail, ItemSummary, TemplateView } from "@/core/types"
import { codeKey, useCodes } from "@/features/codes/use-codes"
import { fullDate } from "@/lib/format"

import { FillFixPanel } from "@/features/bridges/fill-fix"
import { TIER_WORDS } from "@/features/depth/depth-client"
import { EmergencyAccess, ItemProtection, useExposure } from "@/features/depth/item-protection"
import { ExportOptIn } from "@/features/export/export-opt-in"
import { useCaptureProtect } from "@/features/hygiene/hygiene-client"
import { FilesPanel } from "@/features/organization/files-panel"
import { HistoryView } from "@/features/organization/history-view"
import { MarkdownView } from "@/features/organization/markdown-view"
import type { OrganizeTarget } from "@/features/organization/org-model"
import { matchRuleWords } from "@/features/organization/org-model"
import { OrganizePrompt } from "@/features/organization/organize-prompt"
import { PlacePanel } from "@/features/organization/place-panel"
import { useVaults } from "@/features/organization/vaults-context"
import { PasskeysBlock } from "@/features/passkeys/passkeys-block"
import { ItemSecurity } from "@/features/security/item-security"
import { AccessNote, ItemShare } from "@/features/sharing/item-share"

import { LogAwayDialog, AwayBlock } from "./away-block"
import { ItemCodeDisplay } from "./code-display"
import { ItemCrownJewel } from "./crown-jewel"
import { CODE_BLOCK_FIELDS, DEFAULT_MATCH_RULE, bottomRowOf, cardFaceOf, filled, hostOf, overlineOf, sectionsOf } from "./detail-model"
import { ItemField, type RowSize, type SecretFormat } from "./item-field"
import { LARGE_TYPE_LIFETIME_S, ShowLargeType } from "./large-type"
import { LinkedField } from "./linked-field"
import { conflictPair } from "./list-model"
import { useQuickCopy } from "./quick-copy"
import { WifiCard } from "./wifi-card"

interface DetailPaneProps {
  /** The parent keys this pane by item and version, so a change remounts it. */
  id: string
  /** The list's rows: the conflict's other half (D58), and this item's quick copy and star. */
  rows: ItemSummary[]
  onEdit: (item: ItemDetail) => void
  /** The item left the list (moved to Recently deleted, deleted for good, the other conflict copy kept). The pane has said so itself. */
  onTrashed: () => void
  /** The item came back from Recently deleted, or a conflict was resolved: reload, keep the selection. */
  onChanged: (notice: string) => void
  onNotice: (text: string) => void
  /** The item was sealed again under a new id (moved to another vault, or its class changed). */
  onReplaced?: (id: string, notice: string) => void
}

type Resolve = "keep-this" | "keep-other"

/** A pane narrower than this lays its Field Rows out stacked, on any device (brief 4.0). */
const STACKED_BELOW_PX = 480

/** Whether the person keeps Details open, on this device (brief I4). Never a secret. */
const DETAILS_OPEN_KEY = "lockdown.detail.details-open"
function readDetailsOpen(): boolean {
  try {
    return globalThis.localStorage?.getItem(DETAILS_OPEN_KEY) === "1"
  } catch {
    return false
  }
}
function writeDetailsOpen(open: boolean) {
  try {
    globalThis.localStorage?.setItem(DETAILS_OPEN_KEY, open ? "1" : "0")
  } catch {
    // A private window or blocked storage: Details opens closed next time, as by default.
  }
}

/**
 * One item in full (brief I1): the overline (the kind, what tells this one
 * apart, its state), the title in the X6 role with its crown-jewel mark and
 * risk tier, a mono line (the first website's host and when it was edited),
 * one notice, then the Use zone (the template's fields by one rule, I2;
 * websites, passkeys and codes, I3; the note; the files), then Details,
 * shut by default (I4; the panels it holds today). A crown jewel's Use zone
 * sits in the crown-jewel surface (D98).
 *
 * The actions: on a desktop at the end of the overline's line (the star,
 * EDIT, the item's ⋯ menu with only the rows its access allows); on a phone
 * in the top bar (edit, ⋯) and the bottom row, chosen by the row's quick
 * copy: a secret's eye (following the Reveal setting) and its copy, a
 * code's copy, nothing for an item with nothing to copy (Q17), Restore in
 * Recently deleted. Moving an item to Recently deleted asks nothing and
 * offers UNDO in its toast (Q14).
 */
export function DetailPane(props: DetailPaneProps) {
  const client = useClient()
  const [item, setItem] = useState<ItemDetail | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [attempt, setAttempt] = useState(0)

  useEffect(() => {
    let live = true
    client
      .show(props.id)
      .then((d) => live && setItem(d))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client, props.id, attempt])

  if (error) {
    return (
      <AppFrameInset>
        <div role="alert">
          <EmptyState
            tone="critical"
            overline="Item // not opened"
            title="Couldn't open this item."
            body={
              <>
                {error} <span className="type-meta">{props.id}</span>
              </>
            }
            actionLabel="Try again"
            onAction={() => {
              setError(null)
              setAttempt((n) => n + 1)
            }}
          />
        </div>
      </AppFrameInset>
    )
  }
  if (!item) {
    return (
      <AppFrameInset>
        <div aria-busy="true" aria-label="Opening the item" role="status" className="flex flex-col pt-2">
          {[0, 1, 2, 3, 4].map((n) => (
            <Skeleton key={n} variant="row" />
          ))}
        </div>
      </AppFrameInset>
    )
  }
  return <ItemOpen {...props} item={item} setItem={setItem} />
}

interface ItemOpenProps extends DetailPaneProps {
  item: ItemDetail
  setItem: (item: ItemDetail) => void
}

/** The opened item. Split from the pane so its hooks run only once the item has come. */
function ItemOpen({ id, rows, item, setItem, onEdit, onTrashed, onChanged, onNotice, onReplaced }: ItemOpenProps) {
  const client = useClient()
  const gate = useGate()
  const frame = useAppFrame()
  const { vaults } = useVaults()
  const { forTemplate, fieldOf } = useTemplates()
  const [resolving, setResolving] = useState<Resolve | null>(null)
  const [purging, setPurging] = useState(false)
  const [actionError, setActionError] = useState<string | null>(null)
  const other = conflictPair(rows, id)
  const summary = rows.find((r) => r.id === id) ?? null

  const hasCodes = item.fields.some((f) => f.kind === "totp")
  const codes = useCodes(hasCodes, (c) => c.item_id === id)

  // Field Rows stack on a phone and in any pane under 480 px (brief 4.0).
  const paneRef = useRef<HTMLElement | null>(null)
  const [narrow, setNarrow] = useState(false)
  useLayoutEffect(() => {
    const el = paneRef.current
    if (!el || typeof ResizeObserver === "undefined") return
    const watch = new ResizeObserver(([entry]) => setNarrow((entry?.contentRect.width ?? STACKED_BELOW_PX) < STACKED_BELOW_PX))
    watch.observe(el)
    return () => watch.disconnect()
  }, [])
  const size: RowSize = frame.phone || narrow ? "stacked" : "default"

  const title = item.fields.find((f) => f.field_id === "title")?.value ?? ""
  const trashed = item.state === "trashed"
  const archived = item.state === "archived"
  // Drawn from its own template: a variant (a Passport) or the person's own (D231, D232).
  const template = forTemplate(item.template ?? item.kind, item.kind)
  // Shared with this person (docs/plan/18-sharing.md): offer only what the right allows.
  const access = item.access
  const canEdit = !access || access === "edit" || access === "manage"
  const fillOnly = access === "fill-only"
  const editable = canEdit && !trashed && Boolean(template?.creatable)
  const vault = vaults.find((v) => v.id === item.vault_id)
  const passkeys = item.fields.some((f) => f.field_id.startsWith("passkey.") || f.kind === "passkey")
  const firstCode = codes.codes?.find((c) => c.item_id === id) ?? null

  const overline = overlineOf(item, template, {
    passkeys,
    codes: hasCodes,
    code: item.kind === "code" && firstCode ? { variant: firstCode.variant, digits: firstCode.digits } : null,
    conflict: summary?.conflict_copy,
  })
  const site = item.fields.find((f) => f.kind === "url" && f.value)?.value
  // A note's overline already says when it was edited: the line then holds only a site, if any (a state is said once).
  const mono = [site ? hostOf(site) : null, item.kind === "note" ? null : `edited ${fullDate(item.edited_at)}`].filter(Boolean).join(" // ")

  const fail = (e: unknown) => setActionError(describeError(e))

  // --- what the item's actions do ---------------------------------------------------------------
  const trash = async () => {
    setActionError(null)
    try {
      await client.trash(id)
      notify(deletedWords(title), {
        action: {
          label: "Undo",
          onClick: () =>
            void client
              .restore(id)
              .then(() => (onReplaced ? onReplaced(id, `Restored ${title}`) : onChanged(`Restored ${title}`)))
              .catch((e) => onNotice(describeError(e))),
        },
      })
      onTrashed()
    } catch (e) {
      fail(e)
    }
  }
  const restore = async () => {
    setActionError(null)
    try {
      await client.restore(id)
      onChanged(`Restored ${title}`)
    } catch (e) {
      fail(e)
    }
  }
  /** Delete an item in Recently deleted for good, after a Question (a crown jewel's asks it's you). */
  const purge = async () => {
    try {
      await gate.run((g) => client.purge(id, g))
      setPurging(false)
      notify(`Deleted ${title} for good`)
      onTrashed()
    } catch (e) {
      setPurging(false)
      fail(e)
    }
  }
  /**
   * Keep one copy of a conflict: the other goes to Recently deleted,
   * readable there through its grace period; a kept conflict copy takes the
   * item's own title back (`resolve_conflict`).
   */
  const resolve = async (which: Resolve) => {
    if (!other) return
    setResolving(null)
    try {
      if (which === "keep-this") {
        await client.resolveConflict(id, other.id)
        onChanged("Kept this copy; the other is in Recently deleted.")
      } else {
        await client.resolveConflict(other.id, id)
        notify("Kept the other copy; this one is in Recently deleted")
        onTrashed()
      }
    } catch (e) {
      fail(e)
    }
  }

  // --- the phone's bottom row, chosen by the row's quick copy (brief I1) ------------------------
  const remote = useSecretFieldRemote()
  const quickCopy = useQuickCopy(onNotice)
  const copy = summary?.copy ?? null
  const plan = bottomRowOf(item, copy, canEdit)
  const remoteField = plan.start === "reveal" && copy ? copy.field_id : null
  // The elements are drawn again with the pane (the codes tick each second); the shell keeps their state.
  const bottomStart = plan.start === "reveal" ? <SecretFieldRemoteButton remote={remote} action="reveal" /> : undefined
  const bottomEnd =
    plan.end === "restore" ? (
      <FloatButton kind="primary" size="l" icon="undo" label={`Restore ${title}`} onClick={() => void restore()} />
    ) : plan.end === "copy-secret" ? (
      <SecretFieldRemoteButton remote={remote} action="copy" />
    ) : plan.end === "copy-code" && copy ? (
      <FloatButton kind="primary" size="l" icon="copy" label={`Copy ${copy.label}`} onClick={() => summary && void quickCopy(summary)} />
    ) : undefined
  useDetailBottomRow({ start: bottomStart, end: bottomEnd })

  // --- the ⋯ menu and the phone's top bar ------------------------------------------------------
  const menu = useItemMenu({ item, title, rows, summary, canEdit, onChanged, onReplaced, onError: fail, onTrash: () => void trash(), onRestore: () => void restore(), onPurge: () => setPurging(true) })
  const topBar =
    menu.rows || editable ? (
      <>
        {editable && <FloatButton size="l" icon="edit" label="Edit" onClick={() => onEdit(item)} />}
        {menu.rows && (
          <AdaptiveMenu title={`More for ${title}`} trigger={<FloatButton size="l" icon="overflow-menu--horizontal" label="More for this item" />}>
            {menu.rows}
          </AdaptiveMenu>
        )}
      </>
    ) : null
  const inTopBar = useDetailTopBar(topBar)

  // --- the Use zone (brief I2, I3) ---------------------------------------------------------------
  const field = (f: FieldView, format?: SecretFormat) => {
    const t = template?.fields.find((x) => x.field_id === f.field_id) ?? fieldOf(item.kind, f.field_id)
    if (f.kind === "linked") return <LinkedField key={f.field_id} itemId={item.id} field={f} onNotice={onNotice} fillOnly={fillOnly} size={size} />
    const label = t?.label ?? f.label
    if (CODE_BLOCK_FIELDS.has(f.field_id) && !f.hidden && f.value) {
      return <CodeBlock key={f.field_id} code={f.value} label={label} copyLabel={`Copy ${label}`} />
    }
    // Any hidden value reads across a room or types into a TV: large type on every hidden value but a
    // code's set-up and passkey material (matrix `large-type`; D242), the first row of its ⋯.
    const big = f.hidden && f.kind !== "totp" && f.kind !== "passkey" && !fillOnly ? `${label} · ${title}` : undefined
    return (
      <ItemField
        key={f.field_id}
        itemId={item.id}
        field={t ? { ...f, label: t.label } : f}
        control={t?.control}
        onNotice={onNotice}
        format={format ?? (t?.grouped ? "groups-of-4" : undefined)}
        largeType={big}
        largeTypeAction={item.kind === "wi-fi" && Boolean(big)}
        fillOnly={fillOnly}
        size={size}
        remote={f.field_id === remoteField ? remote : undefined}
      />
    )
  }
  const codeRows = item.fields
    .filter((f) => f.kind === "totp")
    .map((f) => {
      if (fillOnly) {
        return (
          <FieldRow key={f.field_id} size={size} label="One-time code">
            Filled in the browser only.
          </FieldRow>
        )
      }
      const view = codes.codes?.find((c) => c.field_id === f.field_id)
      if (!view) {
        return codes.error ? (
          <CodeDisplay key={f.field_id} layout="field" label="One-time code" error={codes.error} />
        ) : (
          <CodeDisplay key={f.field_id} layout="field" label="One-time code" loading aria-label="Computing the code…" />
        )
      }
      return <CodeOnItem key={f.field_id} view={view} now={codes.now} expiresAt={codes.expiresAt[codeKey(view)] ?? null} itemTitle={title} showAction={item.kind === "code"} />
    })
  const passkeyRows = <PasskeysBlock itemId={item.id} title={title} fields={item.fields} onChanged={onChanged} canEdit={!trashed && canEdit} size={size} />
  const use = drawUse(item, template, field, codeRows, passkeyRows, {
    size,
    vaultName: vault?.name ?? "",
    empty: (
      <p className="type-body text-support-foreground">
        {item.kind === "note" ? "An empty note." : "Nothing filled in yet."}{" "}
        {editable && (
          <Button variant="link" size="inline" onClick={() => onEdit(item)}>
            Edit
          </Button>
        )}
      </p>
    ),
    wifiCard: item.kind === "wi-fi" && !fillOnly && !trashed && (
      <div className="pt-4">
        <WifiCard itemId={item.id} ssid={item.fields.find((f) => f.field_id === "wifi.ssid")?.value ?? title} hasPassword={item.fields.some((f) => f.field_id === "wifi.passphrase")} />
      </div>
    ),
  })

  // --- the title, in the role X6 names ----------------------------------------------------------
  const titleRef = useRef<HTMLSpanElement | null>(null)
  const [titleFallback, setTitleFallback] = useState(false)
  const art = item.kind === "card"
  useLayoutEffect(() => {
    // On a phone the title is one line of display-compact; a title that will not fit takes the
    // title role on up to two lines instead.
    const el = titleRef.current
    if (!el || art || !frame.phone) return setTitleFallback(false)
    setTitleFallback(el.scrollWidth > el.clientWidth + 1)
  }, [art, frame.phone, title])
  const oneLine = frame.phone && !art && !titleFallback
  const titleClass = `min-w-0 text-foreground ${art || titleFallback ? "type-title" : frame.phone ? "type-display-compact" : "type-display"}`
  // The type roles balance their lines (text-wrap), so the cut sits on an inner span that sets its own wrap.
  const titleLines = oneLine ? "block truncate" : "line-clamp-2 break-words"

  // --- Details (brief I4): shut by default; its one line names where it is and who can get its values ---
  const [detailsOpen, setDetailsOpen] = useState(readDetailsOpen)
  const folder = vault?.folders.find((f) => f.id === item.organization?.folder)
  const guarded = !trashed && !fillOnly
  const [exposure, setExposure] = useExposure(item.id, guarded && canEdit)
  const detailsSummary =
    [vault?.name, folder?.path, guarded && canEdit && exposure ? TIER_WORDS[exposure.tier].name : null].filter(Boolean).join(" // ") || `Version ${item.version}`
  const live = canEdit && !trashed

  return (
    <article ref={paneRef} aria-label={title} className="flex flex-col pb-12">
      <AppFrameInset>
        <header className="flex flex-col gap-1 pb-6">
          <div className="flex min-h-8 items-center justify-between gap-3">
            <p className="type-overline truncate text-muted-foreground">{overline}</p>
            {!inTopBar && (
              <div className="flex shrink-0 items-center gap-1">
                {!trashed && canEdit && summary && (
                  <IconButton
                    variant="ghost"
                    size="icon-sm"
                    icon={summary.favourite ? "star--filled" : "star"}
                    label={summary.favourite ? "Remove from favorites" : "Add to favorites"}
                    aria-pressed={Boolean(summary.favourite)}
                    onClick={() => menu.toggleFavorite()}
                  />
                )}
                <ItemShare item={item} title={title} onNotice={onNotice} />
                {editable && (
                  <Button variant="ghost" size="sm" onClick={() => onEdit(item)}>
                    <Icon name="edit" data-icon="inline-start" />
                    Edit
                  </Button>
                )}
                {menu.rows && (
                  <AdaptiveMenu title={`More for ${title}`} align="end" trigger={<IconButton variant="ghost" size="icon-sm" icon="overflow-menu--horizontal" label="More for this item" />}>
                    {menu.rows}
                  </AdaptiveMenu>
                )}
              </div>
            )}
          </div>
          <div className="flex min-w-0 items-center gap-3">
            <h2 className={titleClass} title={title}>
              <span ref={titleRef} className={titleLines}>
                {title}
              </span>
            </h2>
            {item.crown_jewel && <CrownJewelMark size="md" />}
            {item.tier && <RiskTierBadge tier={item.tier} verified={item.tier_verified} size="md" explain />}
          </div>
          {mono && <p className="type-meta truncate text-muted-foreground">{mono}</p>}
          {actionError && (
            <p role="alert" className="type-caption text-critical-text">
              {actionError}
            </p>
          )}
        </header>

        {/* One notice (brief I8; vault-7 fills the slot): what changes what the person should do, before the fields. */}
        <div className="flex flex-col gap-4 empty:hidden">
          {trashed && (
            <Banner
              tone="info"
              title={`In Recently deleted. Deleted for good after ${vault?.trash_days ?? 30} days.`}
              action={
                canEdit && !frame.phone ? (
                  <Button variant="secondary" size="sm" onClick={() => void restore()}>
                    Restore
                  </Button>
                ) : undefined
              }
            />
          )}
          {archived && !trashed && (
            <Banner
              tone="info"
              title="Archived. It still syncs and exports."
              action={
                canEdit ? (
                  <Button variant="secondary" size="sm" onClick={() => void menu.archive()}>
                    Unarchive
                  </Button>
                ) : undefined
              }
            />
          )}
          {!trashed && other && canEdit && (
            <section aria-label="Resolve the conflict" className="flex flex-col gap-2">
              <Banner tone="warning" title="Two devices changed this item at the same time." body={<span className="type-meta">The other copy: edited {fullDate(other.edited_at)}</span>} />
              <div className="flex flex-wrap gap-2">
                <Button size="sm" variant="secondary" onClick={() => setResolving("keep-this")}>
                  Keep this copy
                </Button>
                <Button size="sm" variant="secondary" onClick={() => setResolving("keep-other")}>
                  Keep the other
                </Button>
              </div>
            </section>
          )}
          {!trashed && <ItemSecurity key={`security:${item.id}:${item.version}`} itemId={item.id} onNotice={onNotice} />}
        </div>

        {/* Use it (brief I1 #5). */}
        <div className="flex flex-col pt-2">{item.crown_jewel ? <ItemCrownJewel title={title}>{use}</ItemCrownJewel> : use}</div>
        {item.reprompt && <p className="type-caption pt-3 text-support-foreground">This card asks it's you before its security code is shown, copied or filled.</p>}
        {!fillOnly && (
          <div className="pt-6 empty:hidden">
            <FilesPanel itemId={item.id} files={item.attachments ?? []} editable={editable} onChanged={onChanged} onNotice={onNotice} />
          </div>
        )}

        {/* Details (brief I1 #6, I4): shut by default; the app remembers on this device. vault-6 makes its rows. */}
        <div className="pt-8">
          <SectionHeader
            title="Details"
            collapsible
            summary={detailsSummary}
            open={detailsOpen}
            onOpenChange={(o) => {
              setDetailsOpen(o)
              writeDetailsOpen(o)
            }}
          >
            <div className="flex flex-col pt-2">
              {access && (
                <div className="pb-3">
                  <AccessNote access={access} />
                </div>
              )}
              {/* One list of Field Rows; each shows only when it applies and the person may change it. */}
              <PlacePanel item={item} title={title} rows={rows} size={size} canEdit={canEdit} onChanged={onChanged} onReplaced={(newId, notice) => (onReplaced ? onReplaced(newId, notice) : onChanged(notice))} onError={setActionError} />
              {guarded && <ItemProtection itemId={item.id} itemTitle={title} size={size} canEdit={canEdit} exposure={exposure} onExposure={setExposure} />}
              {item.kind === "login" && guarded && <FillFixPanel key={item.id} itemId={item.id} crownJewel={item.crown_jewel} canEdit={canEdit} host={site ? hostOf(site) : null} size={size} />}
              {guarded && (item.kind === "login" || (item.away?.length ?? 0) > 0) && <AwayBlock itemId={item.id} title={title} away={item.away ?? []} canEdit={canEdit} onChanged={onChanged} size={size} />}
              {live && (
                <FieldRow
                  size={size}
                  label="Crown jewel"
                  action={item.crown_jewel_fixed ? undefined : <FieldRowAction onClick={() => menu.askJewel(!item.crown_jewel)}>Change</FieldRowAction>}
                  description={item.crown_jewel_fixed ? "Its kind or a field keeps it a crown jewel." : undefined}
                >
                  {item.crown_jewel_fixed ? "Always" : item.crown_jewel ? "Yes" : "No"}
                </FieldRow>
              )}
              {item.kind === "card" && (
                <FieldRow size={size} label="Ask before the security code">
                  {item.reprompt ? "Yes" : "No"}
                </FieldRow>
              )}
              <FieldRow size={size} label="Version" action={canEdit ? <FieldRowAction onClick={() => menu.openHistory()}>History</FieldRowAction> : undefined}>
                {`${item.version} // edited ${fullDate(item.edited_at)}`}
              </FieldRow>
              {item.crown_jewel && live && <EmergencyAccess itemId={item.id} canEdit={canEdit} />}
              {item.crown_jewel && !trashed && canEdit && <ExportOptIn item={item} onChanged={setItem} />}
              {/* On a phone the Share entry waits here until the ⋯ can open it (edges-8). */}
              {inTopBar && (
                <div className="pt-4">
                  <ItemShare item={item} title={title} onNotice={onNotice} />
                </div>
              )}
            </div>
          </SectionHeader>
        </div>
      </AppFrameInset>

      {menu.layers}
      <ConfirmDialog
        open={purging}
        onOpenChange={setPurging}
        tone="destructive"
        title={`Delete "${title}" for good?`}
        description="Its keys are destroyed now, here and on every device at the next sync, so no copy anywhere can be read again. This can't be undone."
        confirmLabel="Delete for good"
        onConfirm={() => purge()}
      />
      <ConfirmDialog
        open={resolving !== null}
        onOpenChange={(o) => !o && setResolving(null)}
        tone="destructive"
        title={resolving === "keep-this" ? "Keep this copy?" : "Keep the other copy?"}
        description={`${resolving === "keep-this" ? `"${other?.title}"` : `"${title}"`} moves to Recently deleted. Nothing is destroyed: it stays readable there through its grace period and can be restored from there.`}
        confirmLabel={resolving === "keep-this" ? "Keep this copy" : "Keep the other"}
        onConfirm={() => (resolving ? resolve(resolving) : undefined)}
      />
    </article>
  )
}

// --- the Use zone -------------------------------------------------------------------------------

type FieldRenderer = (f: FieldView, format?: SecretFormat) => ReactNode

const isMeta = (f: FieldView) => f.field_id === "title"
const isUrl = (f: FieldView) => f.kind === "url"
const isPasskey = (f: FieldView) => f.field_id.startsWith("passkey.") || f.kind === "passkey"
const isMethod = (f: FieldView) => f.field_id === "pw" || isPasskey(f) || f.kind === "totp"
const isNoteBody = (f: FieldView) => ["body", "notes", "note", "text"].includes(f.field_id)

/** One run of rows: a list, each row a list item (design review R7); the first section has no header. */
function Rows({ name, header = true, count, action, children }: { name: string; header?: boolean; count?: number; action?: ReactNode; children: ReactNode[] }) {
  const kids = children.filter(Boolean)
  if (kids.length === 0) return null
  return (
    <section aria-label={name} className="flex flex-col pt-6 first:pt-0">
      {header && <SectionHeader title={name} count={count} action={action} level={3} />}
      <div role="list" className="flex flex-col">
        {kids.map((k, i) => (
          <div role="listitem" key={i}>
            {k}
          </div>
        ))}
      </div>
    </section>
  )
}

/** Websites past three show three, then SHOW ALL n (brief I3, X6). */
const SITES_SHOWN = 3

function Websites({ urls, field, size }: { urls: FieldView[]; field: FieldRenderer; size: RowSize }) {
  const [all, setAll] = useState(false)
  const shown = all ? urls : urls.slice(0, SITES_SHOWN)
  return (
    <Rows
      name="Websites"
      count={urls.length > 1 ? urls.length : undefined}
      action={
        urls.length > SITES_SHOWN && !all ? (
          <Button variant="ghost" size="sm" onClick={() => setAll(true)}>
            Show all {urls.length}
          </Button>
        ) : undefined
      }
    >
      {shown.map((u) =>
        u.match_rule && u.match_rule !== DEFAULT_MATCH_RULE ? (
          <div key={u.field_id} className="flex flex-col">
            {field(u)}
            <p className={`type-caption pb-3 text-support-foreground ${size === "stacked" ? "" : "pl-39"}`}>{matchRuleWords(u.match_rule)}</p>
          </div>
        ) : (
          field(u)
        ),
      )}
    </Rows>
  )
}

interface UseZoneParts {
  size: RowSize
  vaultName: string
  empty: ReactNode
  wifiCard: ReactNode
}

/** The template's fields by one rule (brief I2), with what each kind adds. */
function drawUse(item: ItemDetail, template: TemplateView | null, field: FieldRenderer, codeRows: ReactNode[], passkeyRows: ReactNode, parts: UseZoneParts): ReactNode {
  const fields = item.fields.filter((f) => !isMeta(f) && filled(f))
  const sections = (list: FieldView[], firstHeader = false) =>
    sectionsOf(list, template).map((s, i) => {
      const notes = s.fields.filter((f) => !f.hidden && isNoteBody(f))
      const rows = s.fields.filter((f) => !notes.includes(f))
      return (
        <div key={s.name} className="flex flex-col">
          <Rows name={s.name} header={firstHeader || i > 0}>
            {rows.map((f) => field(f))}
          </Rows>
          {notes.map((n) => field(n))}
        </div>
      )
    })

  switch (item.kind) {
    case "login": {
      const password = fields.find((f) => f.field_id === "pw")
      const urls = fields.filter(isUrl)
      const isSignIn = (f: FieldView) => f.field_id === "sign_in_method" || f.field_id === "sign_in_provider"
      const user = fields.find((f) => f.field_id === "user")
      const signIn = fields.filter(isSignIn)
      const rest = fields.filter((f) => !isMethod(f) && !isUrl(f) && !isSignIn(f) && f.field_id !== "user")
      const hasPasskeys = item.fields.some(isPasskey)
      return (
        <>
          <Rows name="Sign-in" header={false}>
            {[
              user && field(user),
              password && field(password),
              hasPasskeys && passkeyRows,
              ...codeRows,
              ...signIn.map((f) => field(f)),
            ]}
          </Rows>
          {!password && <p className="type-caption pt-2 text-support-foreground">No password; this login uses other methods.</p>}
          {urls.length > 0 && <Websites urls={urls} field={field} size={parts.size} />}
          {rest.length > 0 && sections(rest, true)}
        </>
      )
    }
    case "code": {
      const rest = fields.filter((f) => f.kind !== "totp")
      return (
        <>
          <Rows name="One-time code" header={false}>
            {codeRows}
          </Rows>
          {rest.length > 0 && sections(rest, true)}
        </>
      )
    }
    case "note": {
      const body = fields.find((f) => !f.hidden && isNoteBody(f)) ?? fields.find((f) => f.kind === "text" && !f.hidden)
      const rest = fields.filter((f) => f !== body)
      return (
        <>
          {rest.length > 0 && sections(rest)}
          {body?.value ? (
            <section aria-label="Note" className={`flex max-w-162 flex-col gap-1 ${rest.length ? "pt-6" : ""}`}>
              <h3 className="type-overline text-muted-foreground">Note</h3>
              <MarkdownView text={body.value} />
            </section>
          ) : (
            rest.length === 0 && parts.empty
          )}
        </>
      )
    }
    case "passkey": {
      const rest = fields.filter((f) => !isPasskey(f))
      return (
        <>
          <Rows name="Passkey" header={false}>
            {[passkeyRows]}
          </Rows>
          {rest.length > 0 && sections(rest, true)}
          <p className="type-caption pt-3 text-support-foreground">A passkey is made by a site and used from the browser.</p>
        </>
      )
    }
    case "card": {
      const face = cardFaceOf(item)
      return (
        <>
          {(face.hasNumber || face.brand) && (
            <div className="max-w-105 pb-6" aria-hidden="true">
              <Tile size={parts.size === "stacked" ? "compact" : "default"}>
                <span className="flex flex-col gap-6">
                  <span className="type-overline text-muted-foreground">{[face.brand || "Card", parts.vaultName].filter(Boolean).join(" // ")}</span>
                  <span className="type-number-count text-foreground">•••• •••• •••• ••••</span>
                  <span className="flex gap-6 type-folio text-muted-foreground">
                    {face.expires && <span>Expires {face.expires}</span>}
                    {face.hasCvc && <span>CVC •••</span>}
                  </span>
                </span>
              </Tile>
            </div>
          )}
          {fields.length ? sections(fields) : parts.empty}
          <p className="type-caption pt-3 text-support-foreground">Filling a card into a page always takes a second press.</p>
        </>
      )
    }
    case "wi-fi":
      return fields.length ? (
        <>
          {sections(fields)}
          {parts.wifiCard}
        </>
      ) : (
        parts.empty
      )
    default: {
      const rest = fields.filter((f) => f.kind !== "totp")
      const withCodes = codeRows.length > 0
      if (!rest.length && !withCodes) return parts.empty
      if (!template) {
        return (
          <>
            {withCodes && <Rows name="One-time codes" header={false}>{codeRows}</Rows>}
            <Rows name="Other fields" header={withCodes}>
              {rest.map((f) => field(f))}
            </Rows>
          </>
        )
      }
      return (
        <>
          {withCodes && <Rows name="One-time codes" header={false}>{codeRows}</Rows>}
          {sections(rest, withCodes)}
        </>
      )
    }
  }
}

// --- a code on an item ----------------------------------------------------------------------------

/**
 * A code on an item (brief I3): unicron's CodeDisplay in the field layout
 * (the label, the code, the seconds, the 30-segment rule), its trailing ⋯
 * holding "Show large type". A one-time code item also keeps the visible
 * SHOW LARGE TYPE text action.
 */
function CodeOnItem({ view, now, expiresAt, itemTitle, showAction }: { view: CodeView; now: number; expiresAt: number | null; itemTitle: string; showAction: boolean }) {
  const [big, setBig] = useState(false)
  const { asking } = useGate()
  useCaptureProtect(big)
  const named = [view.issuer, view.account].filter(Boolean).join(" · ") || itemTitle
  const current = view.current
  const getSecret = useCallback(() => Promise.resolve(new TextEncoder().encode(current)), [current])
  return (
    <div className="flex flex-col">
      <ItemCodeDisplay
        code={view}
        now={now}
        expiresAt={expiresAt}
        layout="field"
        label={view.issuer ?? "One-time code"}
        menu={
          <AdaptiveMenuItem icon="text--scale" disabled={asking} onSelect={() => setBig(true)}>
            Show large type
          </AdaptiveMenuItem>
        }
      />
      <LargeTypeView open={big} onOpenChange={setBig} getSecret={getSecret} title={`Code · ${named}`} groupSize={3} lifetime={LARGE_TYPE_LIFETIME_S} />
      {showAction && (
        <div>
          <ShowLargeType title={`Code · ${named}`} getSecret={getSecret} />
        </div>
      )}
    </div>
  )
}

// --- the item's ⋯ menu (brief I1) -------------------------------------------------------------

interface MenuArgs {
  item: ItemDetail
  title: string
  rows: ItemSummary[]
  summary: ItemSummary | null
  canEdit: boolean
  onChanged: (notice: string) => void
  onReplaced?: (id: string, notice: string) => void
  onError: (e: unknown) => void
  onTrash: () => void
  onRestore: () => void
  onPurge: () => void
}

type Ask = "jewel-on" | "jewel-off" | null

/**
 * The item's ⋯ menu (brief I1): only the rows that apply and that the
 * person's access allows. Share… waits on edges-8 (ItemShare keeps its own
 * button until then). Folder and tags…, Move to vault, Archive, History,
 * Signing in on a TV? (logins), crown jewel on or off, and Move to Recently
 * deleted last, in critical words. In Recently deleted: Restore and Delete
 * for good. A view-only share has no menu.
 */
function useItemMenu({ item, title, rows, summary, canEdit, onChanged, onReplaced, onError, onTrash, onRestore, onPurge }: MenuArgs) {
  const client = useClient()
  const gate = useGate()
  const { vaults } = useVaults()
  const [organizing, setOrganizing] = useState<OrganizeTarget | null>(null)
  const [history, setHistory] = useState(false)
  const [away, setAway] = useState(false)
  const [ask, setAsk] = useState<Ask>(null)
  const trashed = item.state === "trashed"
  const archived = item.state === "archived"
  const org = item.organization ?? { folder: null, tags: [], favourite: Boolean(summary?.favourite) }
  const favourite = summary?.favourite ?? org.favourite
  const others = vaults.filter((v) => v.id !== item.vault_id)
  const replaced = (id: string, notice: string) => (onReplaced ? onReplaced(id, notice) : onChanged(notice))

  const toggleFavorite = () =>
    void client
      .organize(item.id, { favourite: !favourite })
      .then(() => onChanged(favourite ? `Removed ${title} from favorites` : `Added ${title} to favorites`))
      .catch(onError)
  const archive = () =>
    void (archived ? client.unarchive(item.id) : client.archive(item.id))
      .then(() => onChanged(archived ? `Unarchived ${title}` : `Archived ${title}`))
      .catch(onError)
  const move = (to: string, name: string) =>
    void gate
      .run((g) => client.moveItem(item.id, to, g))
      .then((id) => replaced(id, `Moved ${title} to ${name}`))
      .catch(onError)
  const jewel = (on: boolean) =>
    gate
      .run((g) => client.setCrownJewel(item.id, on, g))
      .then((id) => {
        setAsk(null)
        replaced(id, on ? `${title} is now a crown jewel` : `${title} is no longer a crown jewel`)
      })
      .catch((e) => {
        setAsk(null)
        onError(e)
      })

  let menuRows: ReactNode = null
  if (canEdit && trashed) {
    menuRows = (
      <>
        <AdaptiveMenuItem icon="undo" onSelect={onRestore}>
          Restore
        </AdaptiveMenuItem>
        <AdaptiveMenuItem icon="recently-viewed" onSelect={() => setHistory(true)}>
          History
        </AdaptiveMenuItem>
        <AdaptiveMenuSeparator />
        <AdaptiveMenuItem icon="trash-can" variant="destructive" onSelect={onPurge}>
          Delete for good
        </AdaptiveMenuItem>
      </>
    )
  } else if (canEdit) {
    menuRows = (
      <>
        <AdaptiveMenuItem icon={favourite ? "star--filled" : "star"} onSelect={toggleFavorite}>
          {favourite ? "Remove from favorites" : "Add to favorites"}
        </AdaptiveMenuItem>
        {!archived && (
          <AdaptiveMenuItem icon="tag" onSelect={() => setOrganizing({ id: item.id, title, vaultId: item.vault_id ?? "", organization: { folder: org.folder, tags: org.tags, favourite } })}>
            Folder and tags…
          </AdaptiveMenuItem>
        )}
        {others.length > 0 && (
          <AdaptiveMenuGroup label="Move to vault">
            {others.map((v) => (
              <AdaptiveMenuItem key={v.id} icon="folder--move-to" onSelect={() => move(v.id, v.name)}>
                {v.name}
              </AdaptiveMenuItem>
            ))}
          </AdaptiveMenuGroup>
        )}
        <AdaptiveMenuItem icon={archived ? "reset" : "archive"} onSelect={archive}>
          {archived ? "Unarchive" : "Archive"}
        </AdaptiveMenuItem>
        <AdaptiveMenuItem icon="recently-viewed" onSelect={() => setHistory(true)}>
          History
        </AdaptiveMenuItem>
        {item.kind === "login" && (
          <AdaptiveMenuItem icon="launch" onSelect={() => setAway(true)}>
            Signing in on a TV?
          </AdaptiveMenuItem>
        )}
        {!item.crown_jewel && (
          <AdaptiveMenuItem icon="locked" onSelect={() => setAsk("jewel-on")}>
            Keep as a crown jewel
          </AdaptiveMenuItem>
        )}
        {item.crown_jewel && !item.crown_jewel_fixed && (
          <AdaptiveMenuItem icon="unlocked" onSelect={() => setAsk("jewel-off")}>
            Stop keeping it as a crown jewel
          </AdaptiveMenuItem>
        )}
        <AdaptiveMenuSeparator />
        <AdaptiveMenuItem icon="trash-can" variant="destructive" onSelect={onTrash}>
          Move to Recently deleted
        </AdaptiveMenuItem>
      </>
    )
  }

  const layers = (
    <>
      <OrganizePrompt
        target={organizing}
        rows={rows}
        onClose={() => setOrganizing(null)}
        onSaved={() => {
          setOrganizing(null)
          onChanged(`Saved ${title}`)
        }}
      />
      <HistoryView
        itemId={history ? item.id : null}
        title={title}
        editable={!trashed && canEdit}
        onClose={() => setHistory(false)}
        onRestored={(notice) => {
          setHistory(false)
          onChanged(notice)
        }}
      />
      <LogAwayDialog itemId={item.id} title={title} open={away} onOpenChange={setAway} onLogged={onChanged} />
      <ConfirmDialog
        open={ask !== null}
        onOpenChange={(o) => !o && setAsk(null)}
        tone={ask === "jewel-off" ? "destructive" : "default"}
        title={ask === "jewel-on" ? "Keep as a crown jewel?" : "Stop keeping it as a crown jewel?"}
        description={
          ask === "jewel-on"
            ? "Showing or using it will ask it's you first, it is never filled into a page, and lists show only its name. It is sealed again under the crown-jewel key. You can stop later from this menu, with your master password."
            : "It will be shown, copied and filled like any other item. Your master password is asked for first, and it is sealed again under the vault's key."
        }
        confirmLabel={ask === "jewel-on" ? "Keep as a crown jewel" : "Stop keeping it"}
        onConfirm={() => jewel(ask === "jewel-on")}
      />
    </>
  )

  return {
    /** The menu's rows, or null when the access allows none. */
    rows: menuRows,
    layers,
    toggleFavorite,
    archive,
    openHistory: () => setHistory(true),
    /** Details' CROWN JEWEL row asks the menu's own Question (brief I4): today's words, one dialog. */
    askJewel: (on: boolean) => setAsk(on ? "jewel-on" : "jewel-off"),
  }
}

