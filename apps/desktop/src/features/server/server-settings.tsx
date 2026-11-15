import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react"

import { AdaptiveDialog, Badge, Banner, Button, ConfirmDialog, FieldRow, FieldRowAction, FieldRowValue, FloatButton, TextField, useAppFrame } from "@unicron/ui"
import { SecretField } from "@unicron/ui/security/secret-field"

import { useDetailBottomRow } from "@/app/detail-bottom-row"
import { useSettings } from "@/app/settings"
import { describeError } from "@/core/client"
import type { Status } from "@/core/types"
import { useGroupLevel, useRowSize } from "@/features/settings/setting-rows"
import { hostOf, withCandidate, type Candidate, type FirstRunClient } from "@/features/unlock/first-run-client"
import { ServerFacts } from "@/features/unlock/join"

import { ServerPicker } from "./server-picker"

/** Where a vault is moving, and how far it got. */
interface Moving {
  to: Candidate
  step: "login" | "copying" | "done"
  error: string | null
}

/**
 * Settings → Server (brief T5, doc 82; docs/plan/39-first-run.md §1.11):
 *
 * 1. YOUR VAULT: "On this computer only" or "Syncs with <host>", and LEARN
 *    MORE, a question dialog with today's four paragraphs.
 * 2. SERVERS FOUND NN: one Radio Row per server discovery found, where it
 *    was found and what a check found; only a ready one can be chosen.
 *    LOOK AGAIN and TYPE ADDRESS.
 * 3. Choosing one asks first (a Review of its key: ADDRESS, FOUND, SERVER
 *    KEY in DigitGroups, POLICY): "Use <host>?" with focus on CANCEL and
 *    USE THIS SERVER. For an account on a server it becomes the one the
 *    join and sign-in forms start from.
 * 4. For a vault on this computer only, a Form follows: "Move your vault to
 *    <host>": the login there and the master password, MOVE MY VAULT (on a
 *    phone the section holds the form, and MOVE MY VAULT ends the detail
 *    bottom row). While it copies the form is read-only under a live
 *    COPYING YOUR VAULT; then one sentence and DONE.
 *
 * This server (`ServerChecks`) follows in the section for an account on a
 * server; the web vault shows only that part.
 */
export function ServerSettings({ frc, status, onNotice, onChanged }: { frc: FirstRunClient; status: Status; onNotice: (text: string) => void; onChanged: () => void }) {
  const { settings, update } = useSettings()
  const size = useRowSize()
  const level = useGroupLevel()
  const { phone } = useAppFrame()
  const local = !status.account || status.account.local === true
  const [candidates, setCandidates] = useState<Candidate[]>([])
  const [looking, setLooking] = useState(true)
  const [confirming, setConfirming] = useState<Candidate | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [learn, setLearn] = useState(false)
  const [moving, setMoving] = useState<Moving | null>(null)
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

  /** USE THIS SERVER: the key is confirmed; a local vault goes on to the move. A refusal keeps the question open with the core's words. */
  const use = async () => {
    const c = confirming
    if (!c) return
    setError(null)
    try {
      await frc.confirm(c.address)
    } catch (e) {
      setError(describeError(e))
      throw e
    }
    setConfirming(null)
    if (local) {
      setLogin(status.account?.login ?? "")
      setPassword("")
      setMoving({ to: c, step: "login", error: null })
      return
    }
    try {
      await update({ server: c.address })
      onNotice(`${hostOf(c.address)} is the server Lockdown starts from`)
    } catch (e) {
      setError(describeError(e))
    }
  }

  const closeMove = () => {
    setPassword("")
    setMoving(null)
  }
  const move = async (e?: FormEvent) => {
    e?.preventDefault()
    if (!moving || moving.step !== "login" || !login.trim() || !password) return
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
  const copying = moving?.step === "copying"
  const missing = !login.trim() ? "Type your login on this server first." : !password ? "Type your master password first." : null
  // On a phone the section holds the move form, and its yes ends the detail bottom row (brief T5, #3).
  const inline = phone && moving !== null
  const rowEnd = useMemo(
    () =>
      inline && moving?.step !== "done" ? (
        <FloatButton label="Move my vault" icon="cloud--upload" kind="primary" size="l" extended disabled={copying || missing !== null} onClick={() => void move()} />
      ) : undefined,
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `move` reads the same state these name.
    [inline, moving, copying, missing, login, password],
  )
  const inRow = useDetailBottomRow({ end: rowEnd })

  const moveFields = moving && moving.step !== "done" && (
    <form id="move-form" onSubmit={(e) => void move(e)} className="flex flex-col gap-6" aria-label={`Move your vault to ${host}`}>
      {copying && (
        <div role="status">
          <Badge tone="live">Copying your vault</Badge>
        </div>
      )}
      <TextField
        id="move-login"
        label="Login on this server"
        value={login}
        onChange={(e) => setLogin(e.target.value)}
        readOnly={copying}
        autoComplete="username"
        helper="Often an email address. Ask whoever runs the server if they set one up for you."
      />
      <SecretField
        mode="edit"
        id="move-password"
        label="Master password"
        name="password"
        value={password}
        onChange={setPassword}
        disabled={copying}
        autoComplete="current-password"
        description="It stays the same. Lockdown uses it to make your login there."
      />
      <p className="type-caption text-text-secondary">Moving copies your vault and its signed history. Nothing on this computer is lost.</p>
      {moving.error && <Banner tone="critical" size="compact" title={moving.error} />}
    </form>
  )
  const moveDone = moving?.step === "done" && (
    <p className="type-body text-text-secondary">
      This computer keeps working offline and catches up when it can reach the server. You can add your other devices from Devices.
    </p>
  )
  const moveFooter: ReactNode =
    moving?.step === "done" ? (
      <Button onClick={closeMove}>Done</Button>
    ) : (
      <>
        <Button type="button" variant="secondary" onClick={closeMove} disabled={copying}>
          Cancel
        </Button>
        <Button type="submit" form="move-form" loading={copying} disabled={missing !== null} disabledReason={copying ? undefined : (missing ?? undefined)}>
          Move my vault
        </Button>
      </>
    )

  if (inline && moving) {
    return (
      <div className="flex flex-col gap-6">
        <div className="flex flex-col gap-2">
          <p className="type-overline text-text-label">Move vault</p>
          <h2 className="type-lead">{moving.step === "done" ? `Your vault is on ${host} now` : `Move your vault to ${host}`}</h2>
        </div>
        {moveFields}
        {moveDone}
        {/* The bottom row's yes has no room for its reason: it is said here, as the dialog says it under its yes (#37). */}
        {inRow && moving.step === "login" && missing && <p className="type-caption text-text-secondary">{missing}</p>}
        {/* The bottom row is not drawn while the keyboard is up: the form keeps its buttons then (brief X6). */}
        <div className="flex flex-wrap gap-3">
          {moving.step === "done" ? (
            <Button onClick={closeMove}>Done</Button>
          ) : (
            <>
              <Button type="button" variant="secondary" onClick={closeMove} disabled={copying}>
                Cancel
              </Button>
              {!inRow && (
                <Button type="submit" form="move-form" loading={copying} disabled={missing !== null}>
                  Move my vault
                </Button>
              )}
            </>
          )}
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-col">
        <FieldRow size={size} label="Your vault" action={<FieldRowAction onClick={() => setLearn(true)}>Learn more</FieldRowAction>}>
          <FieldRowValue truncate="wrap">{local ? "On this computer only" : `Syncs with ${hostOf(status.account?.server ?? "")}`}</FieldRowValue>
        </FieldRow>
      </div>
      <ServerPicker
        name="settings-server"
        label="Servers found"
        headingLevel={level}
        candidates={candidates}
        chosen={chosen}
        onChoose={(address) => {
          setError(null)
          setConfirming(candidates.find((c) => c.address === address) ?? null)
        }}
        looking={looking}
        onLookAgain={() => look(true)}
        onCheck={async (address) => {
          const c = await frc.check(address)
          merge([c])
          if (c.state !== "ready" && c.reason) throw new Error(c.reason)
        }}
      />
      {error && confirming === null && (
        <p role="alert" className="type-caption text-critical-text">
          {error}
        </p>
      )}

      {/* The server's key, before anything is sent to it (brief T5; doc 78 D862: a question with focus on CANCEL, gap 2). */}
      <ConfirmDialog
        open={confirming !== null}
        onOpenChange={(o) => !o && setConfirming(null)}
        size="review"
        overline="Server key"
        title={`Use ${confirming ? hostOf(confirming.address) : ""}?`}
        description="Only continue if this is a server you or someone you trust runs. Lockdown checks this key every time it syncs."
        confirmLabel="Use this server"
        error={confirming ? error : null}
        onConfirm={use}
        onCancel={() => setError(null)}
      >
        {confirming && <ServerFacts candidate={confirming} />}
      </ConfirmDialog>

      <AdaptiveDialog
        open={moving !== null && !phone}
        onOpenChange={(o) => !o && !copying && closeMove()}
        size="form"
        closeOnOutsideClick={false}
        overline="Move vault"
        title={moving?.step === "done" ? `Your vault is on ${host} now` : `Move your vault to ${host}`}
        footer={moveFooter}
      >
        {moveFields}
        {moveDone}
      </AdaptiveDialog>

      <AdaptiveDialog
        open={learn}
        onOpenChange={setLearn}
        size="question"
        overline="Server"
        title="Your own server"
        description="A Lockdown server stores only locked data it can't read, and Lockdown checks its work every time it syncs."
        footer={<Button onClick={() => setLearn(false)}>Done</Button>}
      >
        <div className="flex flex-col gap-3">
          <p className="type-body">You can run one yourself (the self-host guide shows how) or use one run by someone you trust.</p>
          <p className="type-body">Moving copies your vault and its signed history. Nothing on this computer is lost.</p>
          <p className="type-body">Lockdown looks on this computer and your network for one, and never connects until you've seen and confirmed its key.</p>
        </div>
      </AdaptiveDialog>
    </div>
  )
}
