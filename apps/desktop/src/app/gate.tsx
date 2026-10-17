import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react"

import { Dialog, DialogAction, DialogActions, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@unicron/ui"
import { CrownJewelGate, crownJewelGateErrors, type CrownJewelProofKind } from "@unicron/ui/security/crown-jewel"

import { isAppError, useClient } from "@/core/client"
import type { ApprovalRoute, GateInput } from "@/core/types"

/**
 * A call the core may gate (D101). It is tried once with no input; when
 * the core answers `approval_needed`, the gate asks for what the core
 * named (`password`, or a plain confirm when nothing else exists on this
 * device) and calls again. The platform's own check (Touch ID, Windows
 * Hello) never comes through here: the core runs it itself.
 */
export type Gated<T> = (gate: GateInput) => Promise<T>

/** How many password tries before the vault locks (`--c-crown-jewel-gate-attempts`). */
const ATTEMPTS = 3
/** A proof collected ahead of its call is good for this long. */
const PROOF_MS = 60_000
/** The core's words when a typed password was wrong. */
const WRONG_PASSWORD = /password didn't match/i

interface GateContextValue {
  /** `run(gate => client.something(…, gate))`: the call, gated as the core decides. */
  run: <T>(call: Gated<T>) => Promise<T>
  /**
   * A crown-jewel surface's `requestProof`: ask the way the core's route
   * says (`platform` → nothing to collect, the core runs the check on the
   * call; `password` → the gate dialog; `window` → the confirm), and keep
   * what was collected for the next `run`. Resolves `false` when the person
   * backs out. The core judges the proof when the call arrives (D101).
   */
  requestProof: (kind: CrownJewelProofKind, reason: string) => Promise<boolean>
  /** Which check `use` would go to on this computer, for the biometric's name. */
  route: ApprovalRoute | null
}

const GateContext = createContext<GateContextValue>({
  run: (call) => call({}),
  requestProof: () => Promise.resolve(true),
  route: null,
})

interface Pending {
  kind: "password" | "window"
  reason: string
  attemptsLeft: number
  error?: string
  resolve: (gate: GateInput | null) => void
}

interface Proof {
  kind: CrownJewelProofKind
  gate: GateInput
  at: number
}

const cancelled = () => ({ code: "refused", message: "Cancelled; nothing happened." })

/**
 * The approval gate (D98, D101): the password is asked for in the app's
 * own window through unicron's `CrownJewelGate` at its password level,
 * for crown jewels and for everything else the core gates (export, revoke).
 * A wrong password asks again with the tries left; the last wrong one
 * locks the vault. When this device has neither a platform check nor an
 * account, the core names the `window` route and a plain confirm stands
 * in; the app says so instead of pretending to check.
 */
export function GateProvider({ children }: { children: ReactNode }) {
  const client = useClient()
  const [pending, setPending] = useState<Pending | null>(null)
  const [route, setRoute] = useState<ApprovalRoute | null>(null)
  const proof = useRef<Proof | null>(null)

  useEffect(() => {
    let live = true
    client
      .approvalRoute("use")
      .then((r) => live && setRoute(r))
      .catch(() => live && setRoute(null))
    return () => {
      live = false
    }
  }, [client])

  const ask = useCallback(
    (kind: Pending["kind"], reason: string, attemptsLeft = ATTEMPTS, error?: string) =>
      new Promise<GateInput | null>((resolve) => setPending({ kind, reason, attemptsLeft, error, resolve })),
    [],
  )

  const takeProof = (): Proof | null => {
    const p = proof.current
    proof.current = null
    return p && Date.now() - p.at <= PROOF_MS ? p : null
  }

  const run = useCallback(
    async <T,>(call: Gated<T>): Promise<T> => {
      let gate: GateInput = takeProof()?.gate ?? {}
      let reason = ""
      let attemptsLeft = ATTEMPTS
      for (;;) {
        try {
          return await call(gate)
        } catch (e) {
          if (!isAppError(e)) throw e
          if (e.code === "approval_needed") {
            reason = e.message
            const next = await ask(e.detail === "password" ? "password" : "window", reason)
            if (next === null) throw cancelled()
            gate = next
            continue
          }
          if (e.code === "refused" && WRONG_PASSWORD.test(e.message) && gate.password !== undefined) {
            attemptsLeft -= 1
            if (attemptsLeft <= 0) {
              // The last try: the vault locks, as the gate's spec says.
              await client.lock().catch(() => {})
              throw { code: "locked", message: "The password was wrong three times, so the vault locked." }
            }
            const next = await ask("password", reason || "confirm it's you", attemptsLeft, crownJewelGateErrors.wrongPassword(attemptsLeft))
            if (next === null) throw cancelled()
            gate = next
            continue
          }
          throw e
        }
      }
    },
    [ask, client],
  )

  const requestProof = useCallback(
    async (kind: CrownJewelProofKind, reason: string): Promise<boolean> => {
      proof.current = null
      let r: ApprovalRoute
      try {
        r = await client.approvalRoute(kind)
      } catch {
        return false
      }
      if (r.route === "platform") {
        // The OS asks when the call runs; nothing to collect here.
        proof.current = { kind, gate: {}, at: Date.now() }
        return true
      }
      const gate = await ask(r.route, reason)
      if (gate === null) return false
      proof.current = { kind, gate, at: Date.now() }
      return true
    },
    [ask, client],
  )

  const close = (gate: GateInput | null) => {
    pending?.resolve(gate)
    setPending(null)
  }

  const biometricName = route?.route === "platform" ? route.method : null
  const value = useMemo<GateContextValue>(
    () => ({ run, requestProof, route }),
    [run, requestProof, route],
  )
  return (
    <GateContext.Provider value={value}>
      {children}
      <CrownJewelGate
        open={pending?.kind === "password"}
        level="password"
        reason={pending?.reason ?? ""}
        attemptsLeft={pending?.attemptsLeft}
        error={pending?.error}
        biometricName={biometricName}
        onConfirm={(p) => close(p.level === "password" ? { password: p.password } : { confirmed: true })}
        onCancel={() => close(null)}
      />
      <Dialog open={pending?.kind === "window"} onOpenChange={(o) => !o && close(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Are you sure?</DialogTitle>
            <DialogDescription>{pending?.reason} This computer has no built-in check and no account yet, so this is only a confirmation.</DialogDescription>
          </DialogHeader>
          <DialogActions>
            <DialogAction kind="cancel" type="button" onClick={() => close(null)}>
              Cancel
            </DialogAction>
            <DialogAction kind="confirm" type="button" onClick={() => close({ confirmed: true })}>
              Confirm
            </DialogAction>
          </DialogActions>
        </DialogContent>
      </Dialog>
    </GateContext.Provider>
  )
}

export function useGate(): GateContextValue {
  return useContext(GateContext)
}
