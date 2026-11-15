import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react"

import { ConfirmDialog } from "@unicron/ui"
import { CrownJewelGate, type CrownJewelProofKind } from "@unicron/ui/security/crown-jewel"

import { isAppError, useClient } from "@/core/client"
import type { ApprovalRoute, GateInput } from "@/core/types"

import { cancelledWords, notify } from "./notify"

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

/** A wrong try, in the gate's field (brief X5; cross-app decisions §4: "That isn't the {secret}."). */
export function wrongPasswordWords(attemptsLeft: number): string {
  return `That isn't the master password. ${attemptsLeft} ${attemptsLeft === 1 ? "try" : "tries"} left.`
}

/** What the last wrong try says: the vault locked, and why (brief X5). */
export const LOCKED_WORDS = "The password was wrong three times, so the vault locked."

/**
 * What the core asked for, as the gate says it. The core names the action
 * after "to" ("your master password is needed to export the vault"); the
 * gate's reason is "To export the vault.", and on the window route the
 * question is "Export the vault?" with the action's own verb as its yes.
 */
export function gateWords(message: string): { reason: string; question: string; verb: string; what: "copied" | "shown" | null } {
  const m = /\bto (.+?)\.?$/i.exec(message.trim())
  // The core always names the action; a message without one still asks plainly.
  if (!m) return { reason: message, question: "Go ahead?", verb: "Continue", what: null }
  const action = m[1]!
  const words = action.split(/\s+/)
  const first = words[0]!.toLowerCase()
  // A verb with its particle is the verb ("back up", "let … back in" as D634 says it).
  const verb = first === "back" && words[1] === "up" ? "Back up" : first === "let" && /back in$/.test(action) ? "Let them back in" : first
  const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)
  const what = /^(copy|use)$/.test(first) ? "copied" : /^(show|reveal|see)$/.test(first) ? "shown" : null
  return { reason: `To ${action}.`, question: `${cap(action)}?`, verb: cap(verb), what }
}

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
  /**
   * Closes the gate as if cancelled, at once: the vault locked (brief X5,
   * Lock), and no layer outlives a lock.
   */
  dismiss: () => void
  /**
   * The gate is asking now (its password or confirm dialog is open). A
   * secret field disables its own reveal and copy meanwhile, and unicron's
   * field then dims everything in its row; an action Lockdown puts in that
   * row (Show large type) goes disabled too, so it never reads as dimmed yet
   * pressable (design review R4, the crown-jewel field's contrast).
   */
  asking: boolean
  /**
   * The last lock came from three wrong passwords at the gate: the unlock
   * screen says so (LOCKED_WORDS) until the vault opens again.
   */
  triesLocked: boolean
  /** The vault opened again: the unlock screen's sentence goes back to the usual one. */
  clearTriesLocked: () => void
}

const GateContext = createContext<GateContextValue>({
  run: (call) => call({}),
  requestProof: () => Promise.resolve(true),
  route: null,
  dismiss: () => {},
  asking: false,
  triesLocked: false,
  clearTriesLocked: () => {},
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
 * Backing out of the gate: nothing happens. Where it helps, a toast says
 * what did not ("Nothing was copied", "Nothing was shown"); the error the
 * caller gets is the same as before.
 */
function cancelledFor(reason: string) {
  const what = gateWords(reason).what
  if (what) notify(cancelledWords(what))
  return cancelled()
}

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
  const [triesLocked, setTriesLocked] = useState(false)
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
            if (next === null) throw cancelledFor(reason)
            gate = next
            continue
          }
          if (e.code === "refused" && WRONG_PASSWORD.test(e.message) && gate.password !== undefined) {
            attemptsLeft -= 1
            if (attemptsLeft <= 0) {
              // The last try: the vault locks, as the gate's spec says.
              setTriesLocked(true)
              await client.lock().catch(() => {})
              throw { code: "locked", message: LOCKED_WORDS }
            }
            const next = await ask("password", reason || "confirm it's you", attemptsLeft, wrongPasswordWords(attemptsLeft))
            if (next === null) throw cancelledFor(reason)
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

  // A lock closes the gate at once, as a cancel: the call it held answers "Cancelled" and nothing is kept.
  const pendingRef = useRef(pending)
  useEffect(() => {
    pendingRef.current = pending
  }, [pending])
  const dismiss = useCallback(() => {
    proof.current = null
    const p = pendingRef.current
    if (!p) return
    setPending(null)
    p.resolve(null)
  }, [])

  const biometricName = route?.route === "platform" ? route.method : null
  const asking = pending !== null
  const clearTriesLocked = useCallback(() => setTriesLocked(false), [])
  const value = useMemo<GateContextValue>(
    () => ({ run, requestProof, route, dismiss, asking, triesLocked, clearTriesLocked }),
    [run, requestProof, route, dismiss, asking, triesLocked, clearTriesLocked],
  )
  const words = gateWords(pending?.reason ?? "")
  return (
    <GateContext.Provider value={value}>
      {children}
      {/* The password route (brief X5): unicron's gate, a question 480 dialog or a sheet, its field focused; Escape cancels. */}
      <CrownJewelGate
        open={pending?.kind === "password"}
        level="password"
        reason={words.reason}
        attemptsLeft={pending?.attemptsLeft}
        error={pending?.error}
        biometricName={biometricName}
        onConfirm={(p) => close(p.level === "password" ? { password: p.password } : { confirmed: true })}
        onCancel={() => close(null)}
      />
      {/*
        The window route: no account and no platform check on this computer, so nothing is checked. The
        question says so plainly and its yes is the action's own verb (brief X5). Focus starts on Cancel,
        and Escape cancels: it never confirms.
      */}
      <ConfirmDialog
        open={pending?.kind === "window"}
        onOpenChange={(o) => !o && close(null)}
        overline="Confirm it's you"
        title={words.question}
        description="This computer has no built-in check and no account yet, so nothing is checked here: Lockdown only asks you to be sure."
        confirmLabel={words.verb}
        onConfirm={() => close({ confirmed: true })}
        onCancel={() => close(null)}
      />
    </GateContext.Provider>
  )
}

export function useGate(): GateContextValue {
  return useContext(GateContext)
}
