import { useCallback, useEffect, useState } from "react"

import {
  Badge,
  Button,
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Icon,
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"
import type { FieldView } from "@/core/types"
import { dateLocal, whenLocal } from "@/lib/format"

import { usePasskeyClient, type PasskeyView } from "./passkey-client"

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

function facts(p: PasskeyView): string {
  const parts = [p.provider, p.algorithm, p.backed_up ? "synced with your vault" : "on this device only"]
  if (p.created) parts.push(`made ${dateLocal(p.created)}`)
  parts.push(`last used on this computer: ${whenLocal(p.last_used)}`)
  return parts.join(" · ")
}

/**
 * The passkeys on one item (docs/plan/19-passkeys.md §6): who made each
 * (its provider ID, the AAGUID), its backup flags (BE, BS), when this
 * computer last used it, a site's Signal hide with Undo, and Delete,
 * which warns first when the passkey is the account's only way in that
 * Lockdown holds. The key is never drawn: it signs inside the core.
 */
export function PasskeysBlock({ itemId, title, fields, onChanged }: { itemId: string; title: string; fields: FieldView[]; onChanged: (notice: string) => void }) {
  const client = usePasskeyClient()
  const gate = useGate()
  const [views, setViews] = useState<PasskeyView[] | null>(null)
  const [confirm, setConfirm] = useState<PasskeyView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(() => {
    client
      .list(itemId)
      .then(setViews)
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
    <div className="flex flex-col gap-2" role="list" aria-label="Passkeys">
      {views === null
        ? fallback.map((g, i) => (
            <Item key={i} role="listitem" size="sm" variant="outline">
              <ItemContent>
                <ItemTitle>
                  <Badge variant="secondary">
                    <Icon name="passkey" data-icon="inline-start" />
                    passkey
                  </Badge>
                  {g.rp}
                </ItemTitle>
                <ItemDescription>{g.user}</ItemDescription>
              </ItemContent>
            </Item>
          ))
        : views.map((p) => (
            <Item key={p.credential_id} role="listitem" size="sm" variant="outline" aria-label={`Passkey for ${p.rp_id}`}>
              <ItemContent>
                <ItemTitle>
                  <Badge variant="secondary">
                    <Icon name="passkey" data-icon="inline-start" />
                    passkey
                  </Badge>
                  {p.rp_id}
                  {p.hidden && <Badge variant="destructive">hidden by the site</Badge>}
                </ItemTitle>
                <ItemDescription>{p.user_display_name && p.user_display_name !== p.user_name ? `${p.user_name} (${p.user_display_name})` : p.user_name}</ItemDescription>
                <ItemDescription>{facts(p)}</ItemDescription>
                {p.aaguid && <ItemDescription>Provider ID {p.aaguid}</ItemDescription>}
                {p.hidden && (
                  <ItemDescription>
                    {p.hidden.reason} ({whenLocal(p.hidden.at)}). Lockdown hid it rather than deleting it, in case the site was wrong.
                  </ItemDescription>
                )}
              </ItemContent>
              <ItemActions>
                {p.hidden && (
                  <Button size="sm" variant="outline" disabled={busy} onClick={() => void run(() => client.restore(itemId, p.credential_id), `The passkey for ${p.rp_id} is offered again.`)}>
                    Undo
                  </Button>
                )}
                <Button size="sm" variant="ghost" disabled={busy} onClick={() => setConfirm(p)}>
                  Delete
                </Button>
              </ItemActions>
            </Item>
          ))}
      <p className="text-sm text-muted-foreground">A passkey's private key is kept by the provider, never shown or typed; Lockdown signs with it inside the app.</p>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <Dialog open={confirm !== null} onOpenChange={(open) => !open && !busy && setConfirm(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete this passkey?</DialogTitle>
            <DialogDescription>
              {confirm?.only_sign_in
                ? `This is the only way into "${title}" that Lockdown holds: no password and no other passkey. Without it you may not be able to sign in to ${confirm.rp_id}. Add a password or another passkey first, or make sure the site has another way in.`
                : `Lockdown won't sign in to ${confirm?.rp_id ?? "the site"} with this passkey any more. The site still lists it until you remove it there. The item's history keeps the version that had it.`}
            </DialogDescription>
          </DialogHeader>
          <DialogActions>
            <DialogAction kind="cancel" onClick={() => setConfirm(null)} disabled={busy}>
              Cancel
            </DialogAction>
            <DialogAction
              kind="destructive"
              disabled={busy}
              onClick={() => confirm && void run(() => gate.run((g) => client.remove(itemId, confirm.credential_id, g)), `The passkey for ${confirm.rp_id} was deleted.`)}
            >
              {confirm?.only_sign_in ? "Delete anyway" : "Delete"}
            </DialogAction>
          </DialogActions>
        </DialogContent>
      </Dialog>
    </div>
  )
}
