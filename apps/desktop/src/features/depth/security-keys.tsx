import { useCallback, useEffect, useState } from "react"

import { Banner, Button, ConfirmDialog, FieldRow, FieldRowAction, SectionHeader, useViewport } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"
import { fullDate, rowDate } from "@/lib/format"
import { isCancelled } from "@/features/approvals/requests"

import { useDepthClient, type SecurityKeyInfo, type SecurityKeyList } from "./depth-client"

/**
 * Security keys and passkeys on the account (docs/plan/22-security-depth.md
 * §4; brief T2, doc 85). With one added, a sign-in from a device the account
 * has never seen asks for it after the password; a device approved from
 * another one, or one that signed in before, doesn't. ADD KEY opens the
 * server's page (the system browser on the desktop, this tab on the web),
 * where the browser's own WebAuthn talks to the key, and a toast says to
 * come back. REMOVE asks first (brief X5); the core then runs its own
 * `Approve` gate (D101), which the app does not add to.
 */
export function SecurityKeysSection({ onNotice }: { onNotice: (text: string) => void }) {
  const depth = useDepthClient()
  const gate = useGate()
  const [list, setList] = useState<SecurityKeyList | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [removing, setRemoving] = useState<SecurityKeyInfo | null>(null)
  // A phone puts the label over the value, so a date or a name is never cut (FieldRow `stacked`).
  const rowSize = useViewport().widthClass === "compact" ? "stacked" : "default"

  const load = useCallback(() => {
    if (!depth) return
    depth
      .securityKeys()
      .then(setList)
      .catch((e) => setError(describeError(e)))
  }, [depth])
  useEffect(load, [load])

  if (!depth) return null

  const add = async () => {
    setError(null)
    try {
      await depth.securityKeyAdd()
      onNotice("Use your key on the page that opened, then come back here")
    } catch (e) {
      setError(describeError(e))
    }
  }

  const remove = async (key: SecurityKeyInfo) => {
    setBusy(true)
    setError(null)
    try {
      setList(await gate.run((g) => depth.securityKeyRemove(key.id, g)))
      onNotice(`Removed ${key.name}`)
    } catch (e) {
      if (!isCancelled(e)) setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  const keys = list?.keys ?? []
  return (
    <section aria-label="Security keys" className="flex flex-col">
      <SectionHeader
        title="Security keys"
        count={list ? keys.length : undefined}
        action={
          <Button size="sm" variant="ghost" onClick={add}>
            Add key
          </Button>
        }
      />
      <p className="type-caption text-text-secondary pt-2 pb-3">
        {keys.length > 0
          ? "A new device asks for one of these after your password."
          : "Add a security key or a passkey so a stolen password alone can't sign a new device in."}
      </p>
      {error && <Banner size="compact" title={error} />}
      <div role="list" aria-label="Security keys">
        {keys.map((k) => (
          <div role="listitem" key={k.id}>
            <FieldRow
              size={rowSize}
              label={k.name}
              description={`added ${fullDate(k.created_at)}`}
              action={
                <FieldRowAction variant="ghost-destructive" onClick={() => setRemoving(k)} disabled={busy}>
                  Remove
                </FieldRowAction>
              }
            >
              {k.last_used ? `last used ${rowDate(k.last_used)}` : "not used yet"}
            </FieldRow>
          </div>
        ))}
      </div>
      <ConfirmDialog
        open={removing !== null}
        onOpenChange={(o) => !o && setRemoving(null)}
        overline="Security key"
        title={`Remove ${removing?.name ?? "this key"}?`}
        description="A new device can't use it to sign in from now on. You can add it again later."
        confirmLabel="Remove"
        tone="destructive"
        onConfirm={() => {
          const key = removing
          setRemoving(null)
          if (key) void remove(key)
        }}
      />
    </section>
  )
}
