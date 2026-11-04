import { useState } from "react"

import { Button, Item, ItemActions, ItemContent, ItemDescription, ItemGroup, ItemTitle } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { describeError } from "@/core/client"

import { RESET_HINT, letBackInNotice } from "./key-resets-words"
import { useSharingClient, type KeyResetView } from "./sharing-client"

interface KeyResetsProps {
  resets: KeyResetView[]
  online: boolean
  onChanged: () => void
  onNotice: (text: string) => void
}

/**
 * People who reset their account and wait on you, as the owner of a vault
 * they are in or of an item shared with them, to let them back in
 * (docs/plan/55-member-key-change.md). One row each, in place, never a
 * dialog: the core's question, Let them back in (through the `Approve`
 * gate, as sharing a vault is) and Not now, which hides the row until the
 * screen is opened again; the question stays until it is answered.
 */
export function KeyResets({ resets, online, onChanged, onNotice }: KeyResetsProps) {
  const sharing = useSharingClient()
  const gate = useGate()
  const [later, setLater] = useState<string[]>([])
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const shown = resets.filter((r) => !later.includes(r.account_id))
  if (shown.length === 0) return null

  const yes = async (r: KeyResetView) => {
    setBusy(r.account_id)
    setError(null)
    try {
      const login = await gate.run((g) => sharing.letBackIn(r.account_id, g))
      onNotice(letBackInNotice(login))
      onChanged()
    } catch (e) {
      setError(describeError(e))
    } finally {
      setBusy(null)
    }
  }

  return (
    <section aria-label="Waiting on you" className="flex flex-col gap-2">
      <ItemGroup role="list" aria-label="Waiting on you">
        {shown.map((r) => (
          <Item key={r.account_id} variant="outline" size="sm" role="listitem">
            <ItemContent>
              <ItemTitle>{r.words}</ItemTitle>
              <ItemDescription>{RESET_HINT}</ItemDescription>
            </ItemContent>
            <ItemActions>
              <Button size="sm" disabled={!online || busy !== null} onClick={() => void yes(r)}>
                Let them back in
              </Button>
              <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => setLater((l) => [...l, r.account_id])}>
                Not now
              </Button>
            </ItemActions>
          </Item>
        ))}
      </ItemGroup>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </section>
  )
}
