import { useCallback, useRef, useState } from "react"
import { toast } from "@unicron/ui"

import type { CodeDisplayCopyRef, CodeDisplayWarning } from "@unicron/ui/security/code-display"

import { useGate } from "@/app/gate"
import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"
import type { CodeView } from "@/core/types"

const KNOWN_WARNINGS: readonly CodeDisplayWarning[] = ["issuer-equals-login"]

const digitsOf = (n: number): 5 | 6 | 8 | undefined => (n === 5 || n === 6 || n === 8 ? n : undefined)

/** What unicron's CodeDisplay takes from the host for one code on the Codes page. */
export interface CodeHost {
  variant: CodeView["variant"]
  digits: 5 | 6 | 8 | undefined
  code: string
  next: string | undefined
  period: number
  /** The host clock and the code's end; both left out for a counter code. */
  now: number | undefined
  expiresAt: number | undefined
  issuer: string | undefined
  account: string | undefined
  warnings: CodeDisplayWarning[]
  clipboardTimer: number
  counter: number | undefined
  onCopy: (value: string, which: "code" | "next") => Promise<CodeDisplayCopyRef>
  onClipboardClear: (ref?: CodeDisplayCopyRef) => Promise<boolean>
  onAdvance: (() => Promise<void>) | undefined
  onFixClock: () => void
  /** The code the display shows now, for large type (it is on the screen already). */
  shown: string
}

/**
 * The Codes page's host for one code (brief C1), as `features/vault/code-display.tsx`
 * hosts it on an item: values and timing from the core's `codes`, never the set-up
 * (D102). A copy goes through `copy_code` on the Rust clipboard path and resolves with
 * the shell's copy number as the reference; at the countdown's end that number goes to
 * `clipboard_clear`, so the clear runs only while that copy is still the shell's last
 * write, and the component says "Cleared" only when the shell says it cleared (D105,
 * D115). A counter code's NEXT CODE asks the core to move the counter on and shows the
 * code it answers with until the next `codes` read carries it.
 */
export function useCodeHost(code: CodeView, now: number, expiresAt: number | null): CodeHost {
  const client = useClient()
  const gate = useGate()
  const { settings } = useSettings()
  const timed = code.variant !== "hotp"
  const lastCopy = useRef<number | null>(null)
  // The step the core answered with, for the code view it moved on from; a newer view replaces it.
  const base = `${code.current}/${code.counter ?? ""}`
  const [step, setStep] = useState<{ base: string; code: string; counter: number } | null>(null)
  const advanced = step?.base === base ? step : null

  const onCopy = useCallback(
    async (_value: string, which: "code" | "next") => {
      try {
        const { copy: n } = await gate.run((g) => client.copyCode(code.item_id, code.field_id, settings.clipboard_clear_secs, which, g))
        lastCopy.current = n
        return { hash: String(n) }
      } catch (e) {
        toast.error(describeError(e))
        throw e
      }
    },
    [client, gate, code.item_id, code.field_id, settings.clipboard_clear_secs],
  )

  const onClipboardClear = useCallback(
    (ref?: CodeDisplayCopyRef) => {
      const n = ref?.hash !== undefined ? Number(ref.hash) : lastCopy.current
      if (n === null || !Number.isSafeInteger(n)) return Promise.resolve(false)
      return client.clipboardClear(n).catch(() => false)
    },
    [client],
  )

  const advance = useCallback(async () => {
    try {
      const next = await gate.run((g) => client.advanceCode(code.item_id, code.field_id, g))
      setStep({ base, code: next.code, counter: next.counter })
    } catch (e) {
      toast.error(describeError(e))
    }
  }, [client, gate, code.item_id, code.field_id, base])

  const onFixClock = useCallback(() => {
    client
      .openClockSettings()
      .then((opened) => {
        if (!opened) toast("Set this computer's clock in its system settings; codes are made from the time")
      })
      .catch((e) => toast.error(describeError(e)))
  }, [client])

  const shown = advanced?.code ?? code.current
  return {
    variant: code.variant,
    digits: digitsOf(code.digits),
    code: shown,
    next: code.next ?? undefined,
    period: code.period,
    now: timed ? now : undefined,
    expiresAt: timed && expiresAt !== null ? expiresAt : undefined,
    issuer: code.issuer ?? undefined,
    account: code.account ?? (code.issuer ? undefined : code.title),
    warnings: code.warnings.filter((w): w is CodeDisplayWarning => (KNOWN_WARNINGS as readonly string[]).includes(w)),
    clipboardTimer: settings.clipboard_clear_secs,
    counter: timed ? undefined : (advanced?.counter ?? code.counter ?? undefined),
    onCopy,
    onClipboardClear,
    onAdvance: timed ? undefined : advance,
    onFixClock,
    shown,
  }
}
