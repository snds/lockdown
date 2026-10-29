import { useEffect, useState } from "react"

import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
  Input,
  Spinner,
} from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { describeError } from "@/core/client"
import type { Status } from "@/core/types"
import { hostOf, withCandidate, type Candidate, type FirstRunClient } from "@/features/unlock/first-run-client"
import { ServerFacts } from "@/features/unlock/join"

import { ServerPicker } from "./server-picker"

/**
 * Settings → Server (docs/plan/39-first-run.md §1.11): what discovery found,
 * with "Look again" and a field to type an address. For a local account it
 * says the vault is kept on this computer only, and choosing a server
 * moves the vault there: confirm its key, a login and the master password,
 * then everything is copied and only then used. For an account already on
 * a server, choosing one confirms its key and makes it the server the join
 * and sign-in forms start from.
 */
export function ServerSettings({ frc, status, onNotice, onChanged }: { frc: FirstRunClient; status: Status; onNotice: (text: string) => void; onChanged: () => void }) {
  const { settings, update } = useSettings()
  const local = !status.account || status.account.local === true
  const [candidates, setCandidates] = useState<Candidate[]>([])
  const [looking, setLooking] = useState(true)
  const [confirming, setConfirming] = useState<Candidate | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [learn, setLearn] = useState(false)
  const [moving, setMoving] = useState<{ to: Candidate; step: "login" | "copying" | "done"; error: string | null } | null>(null)
  const [login, setLogin] = useState("")
  const [password, setPassword] = useState("")
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
      if (local) {
        setLogin(status.account?.login ?? "")
        setMoving({ to: c, step: "login", error: null })
        return
      }
      await update({ server: c.address })
      onNotice(`${hostOf(c.address)} is the server Lockdown starts from.`)
    } catch (e) {
      setError(describeError(e))
    }
  }
  const move = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!moving) return
    const to = moving.to
    setMoving({ to, step: "copying", error: null })
    try {
      await frc.moveToServer({ address: to.address, login: login.trim(), password })
      setPassword("")
      setMoving({ to, step: "done", error: null })
      onChanged()
    } catch (err) {
      setMoving({ to, step: "login", error: describeError(err) })
    }
  }
  const host = moving ? hostOf(moving.to.address) : ""
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
            <DialogTitle>
              {local ? "Move your vault to" : "Use"} {confirming ? hostOf(confirming.address) : ""}?
            </DialogTitle>
            <DialogDescription>Only continue if this is a server you or someone you trust runs. Lockdown checks this key every time it syncs.</DialogDescription>
          </DialogHeader>
          {confirming && <ServerFacts candidate={confirming} />}
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirming(null)}>
              Cancel
            </Button>
            <Button onClick={() => void use()}>{local ? "Continue" : "Use this server"}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={moving !== null} onOpenChange={(o) => !o && moving?.step !== "copying" && setMoving(null)}>
        <DialogContent>
          {moving?.step === "done" ? (
            <>
              <DialogHeader>
                <DialogTitle>Your vault is on {host} now</DialogTitle>
                <DialogDescription>
                  This computer keeps working offline and catches up when it can reach the server. You can add your other devices from Devices.
                </DialogDescription>
              </DialogHeader>
              <DialogFooter>
                <Button onClick={() => setMoving(null)}>Done</Button>
              </DialogFooter>
            </>
          ) : (
            <form onSubmit={(e) => void move(e)} className="flex flex-col gap-4">
              <DialogHeader>
                <DialogTitle>Your login on {host}</DialogTitle>
                <DialogDescription>
                  {moving?.step === "copying" ? `Copying your vault to ${host}…` : "Moving copies your vault and its signed history; nothing on this computer is lost."}
                </DialogDescription>
              </DialogHeader>
              <Field>
                <FieldLabel htmlFor="move-login">Login on this server</FieldLabel>
                <Input id="move-login" value={login} onChange={(e) => setLogin(e.target.value)} readOnly={moving?.step === "copying"} autoComplete="username" />
                <FieldDescription>Often an email address. Ask whoever runs the server if they set one up for you.</FieldDescription>
              </Field>
              <Field>
                <FieldLabel htmlFor="move-password">Master password</FieldLabel>
                <Input
                  id="move-password"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  readOnly={moving?.step === "copying"}
                  autoComplete="current-password"
                />
                <FieldDescription>Your master password stays the same; Lockdown uses it to make your login there.</FieldDescription>
              </Field>
              {moving?.error && <FieldError role="alert">{moving.error}</FieldError>}
              <DialogFooter>
                <Button type="button" variant="outline" onClick={() => setMoving(null)} disabled={moving?.step === "copying"}>
                  Cancel
                </Button>
                <Button type="submit" disabled={moving?.step === "copying" || !login.trim() || !password}>
                  {moving?.step === "copying" ? <Spinner data-icon="inline-start" /> : null}
                  Move my vault
                </Button>
              </DialogFooter>
            </form>
          )}
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
            <p>Moving copies your vault and its signed history; nothing on this computer is lost.</p>
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
