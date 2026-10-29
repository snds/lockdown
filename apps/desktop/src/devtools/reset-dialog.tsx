import { useEffect, useId, useState } from "react"

import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, Field, FieldDescription, FieldLabel, Input, Spinner } from "@unicron/ui"

import { LabelledAction, LabelledActions } from "@/components/labelled-actions"
import { describeError } from "@/core/client"

import type { DevtoolsClient, ResetPlan } from "./devtools-client"

/** The word that arms the button. */
export const CONFIRM_WORD = "reset"

/**
 * Developer → Reset Lockdown… (docs/plan/40-devtools.md §2): what will be
 * erased on this device and what stays, in the shell's own list, worked
 * out when the dialog opens. Typing `reset` arms the button; the shell
 * checks the word again. On success the app relaunches into the first run.
 * Mount it afresh for each opening (a new `key`), so nothing typed before
 * is still there.
 */
export function ResetDialog({ client, open, onOpenChange }: { client: DevtoolsClient; open: boolean; onOpenChange: (open: boolean) => void }) {
  const [plan, setPlan] = useState<ResetPlan | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [word, setWord] = useState("")
  const [state, setState] = useState<"idle" | "running" | "restarting">("idle")
  const inputId = useId()

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
    setState("running")
    setError(null)
    try {
      await client.resetRun(word.trim())
      setState("restarting")
    } catch (e) {
      setError(describeError(e))
      setState("idle")
    }
  }

  const armed = word.trim() === CONFIRM_WORD && plan !== null && state === "idle"
  const entries = plan?.keystore.entries.length ?? 0

  return (
    <Dialog open={open} onOpenChange={(o) => state === "idle" && onOpenChange(o)}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Reset Lockdown on this computer?</DialogTitle>
          <DialogDescription>
            This erases everything Lockdown keeps on this computer and starts it again as if it had never run. It can't be undone. An account on a server is
            not deleted: sign in again afterwards to get its vaults back.
          </DialogDescription>
        </DialogHeader>
        {state === "restarting" ? (
          <p role="status" className="flex items-center gap-2 text-sm">
            <Spinner /> Erased. Lockdown is restarting…
          </p>
        ) : (
          <div className="flex max-h-[50vh] flex-col gap-4 overflow-y-auto text-sm">
            {!plan && !error && (
              <p role="status" className="flex items-center gap-2 text-muted-foreground">
                <Spinner /> Finding everything Lockdown keeps…
              </p>
            )}
            {plan && (
              <>
                <section aria-labelledby={`${inputId}-erase`} className="flex flex-col gap-2">
                  <h2 id={`${inputId}-erase`} className="font-medium">
                    Erased
                  </h2>
                  <ul className="flex list-disc flex-col gap-1 pl-5">
                    {plan.erase.map((p) => (
                      <li key={p.path}>
                        {p.what}: <code className="font-mono text-xs break-all">{p.path}</code>
                      </li>
                    ))}
                    <li>
                      {entries === 0
                        ? "No saved sign-ins in this computer's keychain."
                        : `${entries === 1 ? "One entry" : `${entries} entries`} in this computer's keychain under ${plan.keystore.service}: ${plan.keystore.entries.join(", ")}.`}
                      {plan.keystore.problem && <> The keychain couldn't be searched: {plan.keystore.problem}</>}
                    </li>
                    {plan.web_storage && <li>Everything this app's web view has stored.</li>}
                  </ul>
                </section>
                <section aria-labelledby={`${inputId}-identity`} className="flex flex-col gap-2">
                  <h2 id={`${inputId}-identity`} className="font-medium">
                    Your identity
                  </h2>
                  <p>
                    {plan.identity.why} <code className="font-mono text-xs break-all">{plan.identity.root}</code>
                  </p>
                </section>
                <section aria-labelledby={`${inputId}-keeps`} className="flex flex-col gap-2">
                  <h2 id={`${inputId}-keeps`} className="font-medium">
                    Kept
                  </h2>
                  <ul className="flex list-disc flex-col gap-1 pl-5">
                    {plan.keeps.map((k) => (
                      <li key={k}>{k}</li>
                    ))}
                    {plan.refused.map((p) => (
                      <li key={p.path}>
                        {p.what}: <code className="font-mono text-xs break-all">{p.path}</code>
                      </li>
                    ))}
                  </ul>
                </section>
                <Field>
                  <FieldLabel htmlFor={inputId}>Type {CONFIRM_WORD} to confirm</FieldLabel>
                  <Input id={inputId} value={word} onChange={(e) => setWord(e.currentTarget.value)} autoComplete="off" spellCheck={false} autoFocus />
                  <FieldDescription>Lockdown restarts at its first screen.</FieldDescription>
                </Field>
              </>
            )}
            {error && (
              <p role="alert" className="text-destructive">
                {error}
              </p>
            )}
          </div>
        )}
        {state !== "restarting" && (
          <LabelledActions>
            <LabelledAction kind="cancel" onClick={() => onOpenChange(false)} disabled={state !== "idle"}>
              Cancel
            </LabelledAction>
            <LabelledAction kind="destructive" onClick={() => void run()} disabled={!armed}>
              {state === "running" ? "Erasing…" : "Erase and restart"}
            </LabelledAction>
          </LabelledActions>
        )}
      </DialogContent>
    </Dialog>
  )
}
