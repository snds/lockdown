import { useEffect, useState, type ReactNode } from "react"

import { ConfirmDialog, Spinner, TextField } from "@unicron/ui"

import { describeError } from "@/core/client"

import type { DevtoolsClient, ResetPlan } from "./devtools-client"

/** The word that arms the button. */
export const CONFIRM_WORD = "reset"

/** Never settles: the yes stays busy while the app restarts, and nothing can close the question. */
const restarting = () => new Promise<never>(() => {})

/**
 * Developer → Reset Lockdown… (docs/plan/40-devtools.md §2, brief T10): a
 * Question, question size, with what will be erased on this device and
 * what stays, in the shell's own list, worked out when it opens. Typing
 * `reset` arms ERASE AND RESTART (contract 5.1c C11); the shell checks the
 * word again. Focus starts on Cancel. On success the app relaunches into
 * the first run. Mount it afresh for each opening (a new `key`), so nothing
 * typed before is still there.
 */
export function ResetDialog({ client, open, onOpenChange }: { client: DevtoolsClient; open: boolean; onOpenChange: (open: boolean) => void }) {
  const [plan, setPlan] = useState<ResetPlan | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [word, setWord] = useState("")
  const [state, setState] = useState<"idle" | "restarting">("idle")

  useEffect(() => {
    if (!open) return
    let live = true
    client
      .resetPlan()
      .then((p) => live && setPlan(p))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [client, open])

  const run = async () => {
    setError(null)
    try {
      await client.resetRun(word.trim())
    } catch (e) {
      setError(describeError(e))
      throw e
    }
    setState("restarting")
    return restarting()
  }

  const armed = word.trim() === CONFIRM_WORD && plan !== null
  const entries = plan?.keystore.entries.length ?? 0

  return (
    <ConfirmDialog
      open={open}
      onOpenChange={(o) => state === "idle" && onOpenChange(o)}
      overline="Developer"
      title="Reset Lockdown on this computer?"
      description="This erases everything Lockdown keeps on this computer and starts it again as if it had never run. It can't be undone. An account on a server is not deleted: sign in again afterwards to get its vaults back."
      confirmLabel="Erase and restart"
      tone="destructive"
      confirmDisabled={!armed}
      disabledReason={plan ? `Type ${CONFIRM_WORD} to turn this on.` : undefined}
      error={error}
      onConfirm={run}
    >
      {state === "restarting" ? (
        <p role="status" className="type-body text-text-body flex items-center gap-2">
          {/* The words are the status; the spinner beside them is only a picture. */}
          <Spinner aria-hidden /> Erased. Lockdown is restarting…
        </p>
      ) : (
        <div className="flex flex-col gap-6">
          {!plan && !error && (
            <p role="status" className="type-body text-text-secondary flex items-center gap-2">
              <Spinner aria-hidden /> Finding everything Lockdown keeps…
            </p>
          )}
          {plan && (
            <>
              <PlanList title="Erased">
                {plan.erase.map((p) => (
                  <li key={p.path}>
                    {p.what}: <Path>{p.path}</Path>
                  </li>
                ))}
                <li>
                  {entries === 0
                    ? "No saved sign-ins in this computer's keychain."
                    : `${entries === 1 ? "One entry" : `${entries} entries`} in this computer's keychain under ${plan.keystore.service}: `}
                  {plan.keystore.entries.map((e, i) => (
                    <Path key={e}>
                      {i > 0 ? ", " : ""}
                      {e}
                    </Path>
                  ))}
                  {plan.keystore.problem && <> The keychain couldn't be searched: {plan.keystore.problem}</>}
                </li>
                {plan.web_storage && <li>Everything this app's web view has stored.</li>}
              </PlanList>
              <PlanList title="Your identity">
                <li>
                  {plan.identity.why} <Path>{plan.identity.root}</Path>
                </li>
              </PlanList>
              <PlanList title="Kept">
                {plan.keeps.map((k) => (
                  <li key={k}>{k}</li>
                ))}
                {plan.refused.map((p) => (
                  <li key={p.path}>
                    {p.what}: <Path>{p.path}</Path>
                  </li>
                ))}
              </PlanList>
              <TextField
                label={`Type ${CONFIRM_WORD} to confirm`}
                value={word}
                onChange={(e) => setWord(e.currentTarget.value)}
                autoComplete="off"
                spellCheck={false}
                helper="Lockdown restarts at its first screen."
              />
            </>
          )}
        </div>
      )}
    </ConfirmDialog>
  )
}

/** One group of the plan: an overline and its lines. */
function PlanList({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <h3 className="type-overline text-text-label">{title}</h3>
      <ul className="type-caption text-text-body flex list-disc flex-col gap-1 pl-5">{children}</ul>
    </div>
  )
}

/** A path or a keychain entry, in mono, broken anywhere so a long one never widens the question. */
function Path({ children }: { children: ReactNode }) {
  return <code className="type-code-inline break-all">{children}</code>
}
