import { useState } from "react"

import { Badge, Button, Icon, Item, ItemActions, ItemContent, ItemDescription, ItemTitle } from "@unicron/ui"

import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"
import type { CodeView } from "@/core/types"

/** The last seconds of a code, when the next one is worth showing too. */
const EXPIRING_SECS = 5

interface CodeDisplayProps {
  code: CodeView
  /** Seconds left now, ticked by the host (`useCodes`). */
  secondsLeft: number | null
  density?: "detail" | "list"
  onNotice: (text: string) => void
}

/** `482917` → `482 917`; eight digits in two fours; Steam's five as one group. */
export function groupDigits(code: string): string {
  if (code.length === 6) return `${code.slice(0, 3)} ${code.slice(3)}`
  if (code.length === 8) return `${code.slice(0, 4)} ${code.slice(4)}`
  return code
}

/** Spoken one character at a time, never as a number (brief §7). */
export function spellOut(code: string): string {
  return code.split("").join(", ")
}

/** The plain words for a code's warnings (02-model §4). */
const WARNINGS: Record<string, string> = {
  "issuer-equals-login": "This code protects your Lockdown account itself. Keep it in another authenticator, not in this vault.",
}

/**
 * The code valid now, its countdown and, in the last seconds, the next
 * one. The core computes both; this component never sees the set-up.
 * Copy goes through the core's clipboard path (`copy_code`, same timer as
 * a secret). Reduced motion: the countdown is text, not animation.
 *
 * Placeholder for unicron's CodeDisplay (docs/specs/security/
 * code-display.md): the props follow that spec's names where they exist
 * (`variant`, `digits`, `code`, `next`, `density`), with no styling of
 * its own beyond the primitives.
 */
export function CodeDisplay({ code, secondsLeft, density = "detail", onNotice }: CodeDisplayProps) {
  const client = useClient()
  const { settings } = useSettings()
  const [error, setError] = useState<string | null>(null)
  const expiring = secondsLeft !== null && secondsLeft <= EXPIRING_SECS
  const label = [code.issuer, code.account].filter(Boolean).join(" · ") || code.title

  const copy = async () => {
    setError(null)
    try {
      const secs = await client.copyCode(code.item_id, code.field_id, settings.clipboard_clear_secs)
      onNotice(`Copied. The clipboard clears itself in ${secs} s.`)
    } catch (e) {
      setError(describeError(e))
    }
  }

  return (
    <Item variant={density === "detail" ? "outline" : "default"} size="sm" data-row="" data-area="codes">
      <ItemContent>
        <ItemDescription>{density === "detail" ? "one-time code" : label}</ItemDescription>
        <ItemTitle className="font-mono text-lg tracking-widest" aria-label={`Code: ${spellOut(code.current)}`}>
          {groupDigits(code.current)}
        </ItemTitle>
        <ItemDescription aria-live={expiring ? "polite" : "off"}>
          {secondsLeft === null
            ? "Counter-based: the next code comes when you ask"
            : expiring
              ? `Expires in ${secondsLeft} s${code.next ? `; next ${groupDigits(code.next)}` : ""}`
              : `Valid for ${secondsLeft} s`}
          {code.variant === "steam" ? " · Steam" : ""}
        </ItemDescription>
        {code.warnings.map((w) => (
          <ItemDescription key={w} className="text-attention">
            <Badge variant="outline">Needs attention</Badge> {WARNINGS[w] ?? w}
          </ItemDescription>
        ))}
        {error && (
          <ItemDescription role="alert" className="text-destructive">
            {error}
          </ItemDescription>
        )}
      </ItemContent>
      <ItemActions>
        <Button variant="ghost" size="sm" onClick={copy}>
          <Icon name="content_copy" data-icon="inline-start" />
          Copy
        </Button>
      </ItemActions>
    </Item>
  )
}
