import { useCallback, useEffect, useState } from "react"

import { Button, Icon, Item, ItemActions, ItemContent, ItemDescription, ItemGroup, ItemTitle } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"
import { dateLocal } from "@/lib/format"

import { useDepthClient, type SecurityKeyList } from "./depth-client"

/**
 * Security keys and passkeys on the account (docs/plan/22-security-depth.md
 * §4). With one added, a sign-in from a device the account has never seen
 * asks for it after the password; a device approved from another one, or
 * one that signed in before, doesn't. Adding one opens the server's page
 * (the system browser on the desktop, this tab on the web), where the
 * browser's own WebAuthn talks to the key; removing one asks first.
 */
export function SecurityKeysSection({ onNotice }: { onNotice: (text: string) => void }) {
  const depth = useDepthClient()
  const gate = useGate()
  const [list, setList] = useState<SecurityKeyList | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

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
      onNotice("Use your key on the page that opened, then come back here.")
    } catch (e) {
      setError(describeError(e))
    }
  }

  const remove = async (id: string) => {
    setBusy(true)
    setError(null)
    try {
      setList(await gate.run((g) => depth.securityKeyRemove(id, g)))
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <section aria-labelledby="security-keys-title" className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <h2 id="security-keys-title" className="text-sm font-medium">
          Security keys
        </h2>
        <Button size="sm" variant="outline" onClick={add}>
          <Icon name="key" data-icon="inline-start" />
          Add a key
        </Button>
      </div>
      <p className="text-sm text-muted-foreground">
        {list && list.keys.length > 0
          ? "A new device asks for one of these after your password. Devices you approve from here don't."
          : "Add a security key or a passkey so a stolen password alone can't sign a new device in."}
      </p>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <ItemGroup role="list" aria-label="Security keys">
        {list?.keys.map((k) => (
          <Item key={k.id} role="listitem" size="sm" variant="outline" data-row="">
            <ItemContent>
              <ItemTitle>{k.name}</ItemTitle>
              <ItemDescription>
                added {dateLocal(k.created_at)}
                {k.last_used ? ` · last used ${dateLocal(k.last_used)}` : " · not used yet"}
              </ItemDescription>
            </ItemContent>
            <ItemActions>
              <Button size="sm" variant="ghost-destructive" onClick={() => void remove(k.id)} disabled={busy}>
                Remove
              </Button>
            </ItemActions>
          </Item>
        ))}
      </ItemGroup>
    </section>
  )
}
