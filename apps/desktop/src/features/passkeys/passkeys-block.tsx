import { useCallback, useEffect, useState } from "react"

import { AdaptiveDialog, Badge, ConfirmDialog, FieldRow, FieldRowAction } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"
import type { FieldView } from "@/core/types"
import { fullDate, whenLocal } from "@/lib/format"

import { usePasskeyClient, type PasskeyView } from "./passkey-client"
import { upgradeLine } from "./use-passkey-upgrades"

/** What the item's own fields say about its passkeys, while the full view loads or when it can't. */
function fromFields(fields: FieldView[]): { rp: string; user: string }[] {
  const groups = new Map<string, { rp: string; user: string }>()
  for (const f of fields) {
    const m = /^passkey\.(\d+)\.(rp_id|user_name)$/.exec(f.field_id)
    if (!m) continue
    const [, n = "", part] = m
    const g = groups.get(n) ?? { rp: "", user: "" }
    if (part === "rp_id") g.rp = f.value ?? ""
    else g.user = f.value ?? ""
    groups.set(n, g)
  }
  return [...groups.entries()].sort((a, b) => Number(a[0]) - Number(b[0])).map(([, g]) => g)
}

/** The row's value, as Mobile / 02 draws it: who made it, then when ("Lockdown // 2026.10.02"). */
const madeWords = (p: PasskeyView) => (p.created ? `${p.provider} // ${fullDate(p.created)}` : p.provider)

/** "With your vault" when it syncs (BS), otherwise "This device only" (brief I3; no device names, W9). */
const syncWords = (p: PasskeyView) => (p.backed_up ? "With your vault" : "This device only")

const nameOf = (p: PasskeyView) => (p.user_display_name && p.user_display_name !== p.user_name ? `${p.user_name} (${p.user_display_name})` : p.user_name)

type Size = "default" | "stacked"

/**
 * The passkeys on one item (docs/plan/19-passkeys.md §6; brief I3): one
 * Field Row each, as the frames draw the first ("Lockdown // 2026.10.02"),
 * its account under it, and DETAILS for the rest: who made it (the
 * provider and its ID, the AAGUID), whether it syncs (BS), when it was
 * made, when this computer last used it, its algorithm. A site's Signal
 * hide shows as HIDDEN BY THE SITE with UNDO; a passkey the site added by
 * itself after a password sign-in says so until the item is opened once on
 * this computer (opening it tells the core, and the line stays while the
 * item stays open). DELETE PASSKEY sits in the details and asks first,
 * warning when the passkey is the account's only way in that Lockdown
 * holds. The key is never drawn: it signs inside the core. Undo and Delete
 * change the item, so a shared item without edit (docs/plan/18-sharing.md)
 * offers neither; the core refuses them too.
 */
export function PasskeysBlock({
  itemId,
  title,
  fields,
  onChanged,
  canEdit = true,
  size = "default",
}: {
  itemId: string
  title: string
  fields: FieldView[]
  onChanged: (notice: string) => void
  canEdit?: boolean
  /** Field Row's layout: `stacked` on a phone and in a pane under 480 px. */
  size?: Size
}) {
  const client = usePasskeyClient()
  const gate = useGate()
  const [views, setViews] = useState<PasskeyView[] | null>(null)
  const [open, setOpen] = useState<PasskeyView | null>(null)
  const [confirm, setConfirm] = useState<PasskeyView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [noticed, setNoticed] = useState<Record<string, { site: string; at: number }>>({})

  const load = useCallback(() => {
    client
      .list(itemId)
      .then((list) => {
        setViews(list)
        const upgraded = list.filter((p) => p.upgrade)
        if (upgraded.length === 0) return
        setNoticed((cur) => {
          const next = { ...cur }
          for (const p of upgraded) if (p.upgrade) next[p.credential_id] = p.upgrade
          return next
        })
        client.upgradeSeen(itemId).catch(() => {})
      })
      .catch(() => setViews(null))
  }, [client, itemId])
  useEffect(() => {
    load()
  }, [load])

  const run = async (what: () => Promise<unknown>, notice: string) => {
    setBusy(true)
    setError(null)
    try {
      await what()
      setConfirm(null)
      setOpen(null)
      onChanged(notice)
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const fallback = fromFields(fields)
  if (views === null && fallback.length === 0) return null
  return (
    <div role="list" aria-label="Passkeys" className="flex flex-col">
      {views === null
        ? fallback.map((g, i) => (
            <div key={i} role="listitem" aria-label={`Passkey for ${g.rp}`}>
              <FieldRow size={size} label="Passkey" description={g.user || undefined}>
                {g.rp}
              </FieldRow>
            </div>
          ))
        : views.map((p) => {
            const added = p.upgrade ?? noticed[p.credential_id]
            const description = p.hidden ? `${p.hidden.reason} (${whenLocal(p.hidden.at)}). Lockdown hid it rather than deleting it, in case the site was wrong.` : added ? upgradeLine(added) : nameOf(p)
            return (
              <div key={p.credential_id} role="listitem" aria-label={`Passkey for ${p.rp_id}`}>
                <FieldRow
                  size={size}
                  label="Passkey"
                  description={description}
                  badge={p.hidden ? <Badge tone="warning">Hidden by the site</Badge> : added ? <Badge tone="neutral">Added by the site</Badge> : undefined}
                  action={
                    <>
                      {canEdit && p.hidden && (
                        <FieldRowAction disabled={busy} onClick={() => void run(() => client.restore(itemId, p.credential_id), `The passkey for ${p.rp_id} is offered again.`)}>
                          Undo
                        </FieldRowAction>
                      )}
                      <FieldRowAction onClick={() => setOpen(p)} aria-label={`Details of the passkey for ${p.rp_id}`}>
                        Details
                      </FieldRowAction>
                    </>
                  }
                >
                  {madeWords(p)}
                </FieldRow>
              </div>
            )
          })}
      {error && (
        <p role="alert" className="type-caption text-critical-text">
          {error}
        </p>
      )}
      <AdaptiveDialog
        open={open !== null}
        onOpenChange={(o) => !o && !busy && setOpen(null)}
        size="review"
        overline="Passkey"
        title={open?.rp_id ?? ""}
        description="Its private key is kept by the provider, never shown or typed; Lockdown signs with it inside the app."
      >
        {open && (
          <div className="flex flex-col">
            <FieldRow label="User name">{open.user_name}</FieldRow>
            {open.user_display_name && <FieldRow label="Display name">{open.user_display_name}</FieldRow>}
            <FieldRow label="Made by">{open.provider}</FieldRow>
            {open.aaguid && <FieldRow label="Provider ID">{open.aaguid}</FieldRow>}
            <FieldRow label="Syncs">{syncWords(open)}</FieldRow>
            {open.created && <FieldRow label="Made">{fullDate(open.created)}</FieldRow>}
            <FieldRow label="Last used">{open.last_used ? `${fullDate(open.last_used)} // on this computer` : "Never on this computer"}</FieldRow>
            <FieldRow label="Algorithm">{open.algorithm}</FieldRow>
            {canEdit && (
              <div className="pt-4">
                <FieldRowAction variant="ghost-destructive" disabled={busy} onClick={() => setConfirm(open)}>
                  Delete passkey
                </FieldRowAction>
              </div>
            )}
          </div>
        )}
      </AdaptiveDialog>
      <ConfirmDialog
        open={confirm !== null}
        onOpenChange={(o) => !o && !busy && setConfirm(null)}
        tone="destructive"
        title="Delete this passkey?"
        description={
          confirm?.only_sign_in
            ? `This is the only way into "${title}" that Lockdown holds: no password and no other passkey. Without it you may not be able to sign in to ${confirm.rp_id}. Add a password or another passkey first, or make sure the site has another way in.`
            : `Lockdown won't sign in to ${confirm?.rp_id ?? "the site"} with this passkey any more. The site still lists it until you remove it there. The item's history keeps the version that had it.`
        }
        confirmLabel={confirm?.only_sign_in ? "Delete anyway" : "Delete"}
        error={error ?? undefined}
        onConfirm={() => (confirm ? run(() => gate.run((g) => client.remove(itemId, confirm.credential_id, g)), `The passkey for ${confirm.rp_id} was deleted.`) : undefined)}
      />
    </div>
  )
}
