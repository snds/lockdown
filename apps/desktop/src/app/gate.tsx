import { createContext, useCallback, useContext, useMemo, useRef, useState, type FormEvent, type ReactNode } from "react"

import {
  Dialog,
  DialogAction,
  DialogActions,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Field,
  FieldLabel,
  Input,
} from "@unicron/ui"

import { isAppError } from "@/core/client"
import type { GateInput } from "@/core/types"

/**
 * A call the core may gate (D101). It is tried once with no input; when
 * the core answers `approval_needed`, the gate asks for what the core
 * named (`password`, or a plain confirm when nothing else exists on this
 * device) and calls again. The platform's own check (Touch ID, Windows
 * Hello) never comes through here: the core runs it itself.
 */
export type Gated<T> = (gate: GateInput) => Promise<T>

interface GateContextValue {
  run: <T>(call: Gated<T>) => Promise<T>
  /** The crown-jewel proof collected for the next call, if one is waiting (`use` or `reveal`). */
  pendingProof: () => "use" | "reveal" | null
}

const GateContext = createContext<GateContextValue>({ run: (call) => call({}), pendingProof: () => null })

interface Pending {
  kind: "password" | "window"
  reason: string
  resolve: (gate: GateInput | null) => void
}

/**
 * The placeholder for unicron's CrownJewel gate (docs/specs/security/
 * crown-jewel-treatment.md): one dialog, in the app's own window, never in
 * a page (D98). When the real component lands, its context drives
 * SecretField's `gate` prop and this provider goes.
 */
export function GateProvider({ children }: { children: ReactNode }) {
  const [pending, setPending] = useState<Pending | null>(null)
  const passwordRef = useRef<HTMLInputElement>(null)

  const ask = useCallback(
    (kind: Pending["kind"], reason: string) =>
      new Promise<GateInput | null>((resolve) => setPending({ kind, reason, resolve })),
    [],
  )

  const run = useCallback(
    async <T,>(call: Gated<T>): Promise<T> => {
      try {
        return await call({})
      } catch (e) {
        if (!isAppError(e) || e.code !== "approval_needed") throw e
        const kind = e.detail === "password" ? "password" : "window"
        const gate = await ask(kind, e.message)
        if (gate === null) throw { code: "refused", message: "Cancelled; nothing happened." }
        return await call(gate)
      }
    },
    [ask],
  )

  const close = (gate: GateInput | null) => {
    pending?.resolve(gate)
    setPending(null)
  }

  const submit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    const input = passwordRef.current
    const password = input?.value ?? ""
    if (input) input.value = ""
    close(pending?.kind === "password" ? { password } : { confirmed: true })
  }

  const value = useMemo(() => ({ run, pendingProof: () => null }), [run])
  return (
    <GateContext.Provider value={value}>
      {children}
      <Dialog open={pending !== null} onOpenChange={(o) => !o && close(null)}>
        <DialogContent>
          <form onSubmit={submit} className="flex flex-col gap-4">
            <DialogHeader>
              <DialogTitle>{pending?.kind === "password" ? "Your account password" : "Are you sure?"}</DialogTitle>
              <DialogDescription>{pending?.reason}</DialogDescription>
            </DialogHeader>
            {pending?.kind === "password" && (
              <Field>
                <FieldLabel htmlFor="gate-password">Account password</FieldLabel>
                <Input id="gate-password" ref={passwordRef} type="password" autoComplete="current-password" autoFocus required />
              </Field>
            )}
            <DialogActions>
              <DialogAction kind="cancel" type="button" onClick={() => close(null)}>
                Cancel
              </DialogAction>
              <DialogAction kind="confirm" type="submit">
                {pending?.kind === "password" ? "Continue" : "Confirm"}
              </DialogAction>
            </DialogActions>
          </form>
        </DialogContent>
      </Dialog>
    </GateContext.Provider>
  )
}

/** `run(gate => client.something(…, gate))`: the call, gated as the core decides. */
export function useGate(): GateContextValue {
  return useContext(GateContext)
}
