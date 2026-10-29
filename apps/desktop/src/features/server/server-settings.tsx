import { useEffect, useState } from "react"

import { Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { describeError } from "@/core/client"
import type { Status } from "@/core/types"
import { hostOf, withCandidate, type Candidate, type FirstRunClient } from "@/features/unlock/first-run-client"
import { ServerFacts } from "@/features/unlock/join"

import { ServerPicker } from "./server-picker"

/**
 * Settings → Server (docs/plan/39-first-run.md §1.11): what discovery found,
 * with "Look again" and a field to type an address. For a local account it
 * says the vault is kept on this computer only. Choosing a server confirms
 * its key (and pins it) and makes it the server the join and sign-in forms
 * start from. Moving a local vault onto a server is the next piece (doc 39
 * §3, gap 1); until it lands, a server is used by joining it.
 */
export function ServerSettings({ frc, status, onNotice }: { frc: FirstRunClient; status: Status; onNotice: (text: string) => void }) {
  const { settings, update } = useSettings()
  const local = !status.account || status.account.local === true
  const [candidates, setCandidates] = useState<Candidate[]>([])
  const [looking, setLooking] = useState(true)
  const [confirming, setConfirming] = useState<Candidate | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [learn, setLearn] = useState(false)
  const merge = (list: Candidate[]) => setCandidates((old) => list.reduce(withCandidate, old))
  const look = (network: boolean) => {
    setLooking(true)
    frc
      .candidates(network)
      .then(merge, () => {})
      .finally(() => setLooking(false))
  }
  useEffect(() => {
    let live = true
    frc
      .candidates(true)
      .then((l) => live && merge(l), () => {})
      .finally(() => live && setLooking(false))
    return () => {
      live = false
    }
  }, [frc])
  const chosen = settings.server ? (candidates.find((c) => c.address.toLowerCase() === settings.server?.toLowerCase())?.address ?? null) : null
  const use = async () => {
    const c = confirming
    if (!c) return
    setConfirming(null)
    setError(null)
    try {
      await frc.confirm(c.address)
      await update({ server: c.address })
      onNotice(`${hostOf(c.address)} is the server Lockdown starts from.`)
    } catch (e) {
      setError(describeError(e))
    }
  }
  return (
    <section className="flex flex-col gap-3 rounded-xl border bg-card p-4" aria-labelledby="server-settings-title">
      <h2 id="server-settings-title" className="font-medium">
        Server
      </h2>
      <p className="text-sm text-muted-foreground">
        {local
          ? "Your vault is kept on this computer only. A server keeps your devices in step and lets you share."
          : `Your vault syncs with ${hostOf(status.account?.server ?? "")}.`}{" "}
        <Button variant="link" size="inline" onClick={() => setLearn(true)}>
          Learn more
        </Button>
      </p>
      <ServerPicker
        name="settings-server"
        label="Found"
        candidates={candidates}
        chosen={chosen}
        onChoose={(address) => setConfirming(candidates.find((c) => c.address === address) ?? null)}
        looking={looking}
        onLookAgain={() => look(true)}
        onCheck={async (address) => {
          const c = await frc.check(address)
          merge([c])
          if (c.state !== "ready" && c.reason) throw new Error(c.reason)
        }}
      />
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      <Dialog open={confirming !== null} onOpenChange={(o) => !o && setConfirming(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Use {confirming ? hostOf(confirming.address) : ""}?</DialogTitle>
            <DialogDescription>Only continue if this is a server you or someone you trust runs. Lockdown checks this key every time it syncs.</DialogDescription>
          </DialogHeader>
          {confirming && <ServerFacts candidate={confirming} />}
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirming(null)}>
              Cancel
            </Button>
            <Button onClick={() => void use()}>Use this server</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={learn} onOpenChange={setLearn}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Your own server</DialogTitle>
            <DialogDescription>A Lockdown server stores only locked data it can't read, and Lockdown checks its work every time it syncs.</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3 text-sm">
            <p>You can run one yourself (the self-host guide shows how) or use one run by someone you trust.</p>
            <p>Lockdown looks on this computer and your network for one, and never connects until you've seen and confirmed its key.</p>
          </div>
          <DialogFooter>
            <Button onClick={() => setLearn(false)}>Got it</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  )
}
