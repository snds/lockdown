import { useRef, useState } from "react"
import { toast } from "sonner"

import { CodeDisplay, type CodeDisplayDensity, type CodeDisplayWarning } from "@unicron/ui/security/code-display"

import { useGate } from "@/app/gate"
import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"
import type { CodeView } from "@/core/types"

interface ItemCodeDisplayProps {
  /** What the core computed (`codes`, D102): values and timing, never the set-up. */
  code: CodeView
  /** The host clock and this code's end, from `useCodes`. */
  now: number
  expiresAt: number | null
  density?: CodeDisplayDensity
}

const KNOWN_WARNINGS: readonly CodeDisplayWarning[] = ["issuer-equals-login"]

const digitsOf = (n: number): 5 | 6 | 8 | undefined => (n === 5 || n === 6 || n === 8 ? n : undefined)

/**
 * unicron's CodeDisplay fed by the `codes` command: current, next, the
 * period and the end of the current code on the host's clock. Copy goes
 * through `copy_code` on the Rust clipboard path (the component says
 * which of the two it means; in a code's last seconds that is the next
 * one), and the clear at the countdown's end names the shell's number for
 * that copy, so it runs only while that copy is still the shell's last
 * write (`clipboard_clear`; D105, D115). An HOTP code's "Next code" asks
 * the core to move the counter on (`advance_code`, a new item version, so
 * it syncs; a crown jewel's asks first) and shows the code it answers
 * with until the next `codes` read carries it. "Fix clock" opens the OS
 * date settings where a URL for them exists, else says where to look.
 * There is no haptic on desktop, so `onHaptic` is left out. The core does
 * not measure clock skew yet, so the skew notice stays dormant.
 */
export function ItemCodeDisplay({ code, now, expiresAt, density = "detail" }: ItemCodeDisplayProps) {
  const client = useClient()
  const gate = useGate()
  const { settings } = useSettings()
  const timed = code.variant !== "hotp"
  const lastCopy = useRef<number | null>(null)
  // The step the core answered with, for the code view it moved on from; a newer view replaces it.
  const base = `${code.current}/${code.counter ?? ""}`
  const [step, setStep] = useState<{ base: string; code: string; counter: number } | null>(null)
  const advanced = step?.base === base ? step : null

  const copy = async (_value: string, which: "code" | "next") => {
    try {
      const { copy: n } = await gate.run((g) => client.copyCode(code.item_id, code.field_id, settings.clipboard_clear_secs, which, g))
      lastCopy.current = n
    } catch (e) {
      toast.error(describeError(e))
      throw e
    }
  }

  const clear = () => {
    const n = lastCopy.current
    if (n !== null) void client.clipboardClear(n).catch(() => {})
  }

  const advance = async () => {
    try {
      const next = await gate.run((g) => client.advanceCode(code.item_id, code.field_id, g))
      setStep({ base, code: next.code, counter: next.counter })
    } catch (e) {
      toast.error(describeError(e))
    }
  }

  const fixClock = () => {
    client
      .openClockSettings()
      .then((opened) => {
        if (!opened) toast("Set this computer's clock in its system settings; codes are made from the time.")
      })
      .catch((e) => toast.error(describeError(e)))
  }

  return (
    <CodeDisplay
      variant={code.variant}
      digits={digitsOf(code.digits)}
      code={advanced?.code ?? code.current}
      next={code.next ?? undefined}
      period={code.period}
      now={timed ? now : undefined}
      expiresAt={timed && expiresAt !== null ? expiresAt : undefined}
      density={density}
      issuer={code.issuer ?? undefined}
      account={code.account ?? (code.issuer ? undefined : code.title)}
      warnings={code.warnings.filter((w): w is CodeDisplayWarning => (KNOWN_WARNINGS as readonly string[]).includes(w))}
      clipboardTimer={settings.clipboard_clear_secs}
      onCopy={copy}
      onAdvance={timed ? undefined : advance}
      counter={timed ? undefined : (advanced?.counter ?? code.counter ?? undefined)}
      onClipboardClear={clear}
      onFixClock={fixClock}
      data-area="codes"
    />
  )
}
