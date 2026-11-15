import { useMemo, useState } from "react"

import { useGate } from "@/app/gate"
import { useNeedsYouSource, type NeedsYouEntry } from "@/app/needs-you"
import { describeError } from "@/core/client"

import { RESET_TITLE, letBackInNotice } from "./key-resets-words"
import { useSharingClient, type KeyResetView } from "./sharing-client"

interface KeyResetsProps {
  resets: KeyResetView[]
  online: boolean
  onChanged: () => void
  onNotice: (text: string) => void
  /** A yes that failed, in the core's words; the screen shows it as a critical Banner. */
  onError?: (words: string) => void
}

/**
 * People who reset their account and wait on you, as the owner of a vault
 * they are in or of an item shared with them, to let them back in
 * (docs/plan/55-member-key-change.md). Each is needs-you row 6 of brief X3,
 * on Sharing only: WAITING ON YOU, the core's question (D634), LET THEM
 * BACK IN through the `Approve` gate, and NOT NOW, which hides it until
 * Sharing is opened again; the question stays until it is answered. It is
 * never a dialog, and it never opens one by itself. When something higher
 * waits, the shell draws it as an ordinary row under WAITING ON YOU instead.
 * Offline, the yes would fail: nothing is registered until the server is back.
 */
export function KeyResets({ resets, online, onChanged, onNotice, onError }: KeyResetsProps) {
  const [later, setLater] = useState<string[]>([])
  const shown = online ? resets.filter((r) => !later.includes(r.account_id)) : []
  return (
    <>
      {shown.map((r) => (
        <KeyReset key={r.account_id} reset={r} onChanged={onChanged} onNotice={onNotice} onError={onError} onLater={() => setLater((l) => [...l, r.account_id])} />
      ))}
    </>
  )
}

function KeyReset({ reset, onChanged, onNotice, onError, onLater }: { reset: KeyResetView; onChanged: () => void; onNotice: (text: string) => void; onError?: (words: string) => void; onLater: () => void }) {
  const sharing = useSharingClient()
  const gate = useGate()
  const [busy, setBusy] = useState(false)
  const entry = useMemo<NeedsYouEntry>(() => {
    const yes = async () => {
      setBusy(true)
      try {
        const login = await gate.run((g) => sharing.letBackIn(reset.account_id, g))
        onNotice(letBackInNotice(login))
        onChanged()
      } catch (e) {
        onError?.(describeError(e))
      } finally {
        setBusy(false)
      }
    }
    return {
      id: `reset:${reset.account_id}`,
      priority: 6,
      title: RESET_TITLE,
      line: reset.words,
      action: { label: "Let them back in", onClick: () => void yes(), busy },
      textAction: { label: "Not now", onClick: onLater },
      shows: "sharing",
    }
  }, [reset, busy, gate, sharing, onNotice, onChanged, onError, onLater])
  useNeedsYouSource(entry)
  return null
}
