import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react"

import { Button, Icon, Item, ItemActions, ItemContent, ItemDescription, ItemTitle } from "@unicron/ui"

import { useGate } from "@/app/gate"
import { useSettings } from "@/app/settings"
import { describeError, useClient } from "@/core/client"
import { decodeAndWipe } from "@/core/secret-bytes"
import type { FieldView, RevealGesture } from "@/core/types"

/** In toggle mode a reveal masks itself again after this long (design brief §4). */
const REVEAL_MS = 30_000

/** How a value is drawn: as is, or in groups of four (cards, D-brief §3.1). */
export type SecretFormat = "plain" | "groups-of-4"

interface SecretFieldProps {
  itemId: string
  field: FieldView
  /** Every field's copy timer, so a second copy stands the first down. */
  onNotice: (text: string) => void
  format?: SecretFormat
  /** Overrides the setting (tests, and the first-run kit page). */
  gesture?: RevealGesture
}

function formatValue(text: string, format: SecretFormat): string {
  if (format !== "groups-of-4") return text
  const digits = text.replace(/\s+/g, "")
  return digits.replace(/(.{4})(?=.)/g, "$1 ")
}

/**
 * One field of an item. Plain fields show their value. Hidden fields show
 * the core's mask; a reveal fetches the bytes, decodes them for as long
 * as they are on screen, then drops the string and zeroes the buffer.
 *
 * Reveal gesture (D97): **hold** by default: the value shows while the
 * pointer is down or Space is held on the Reveal control, and masks on
 * release, on leaving the control, or on cancel. The **toggle** setting
 * shows on a tap and masks on the next, after 30 seconds, on window blur
 * and on lock. **Copy** never brings the value here: the shell copies from
 * the core and clears the clipboard after the timer if it still holds our
 * value. A crown jewel's reveal and copy go through the gate (D101).
 *
 * This is a placeholder for unicron's SecretField (docs/specs/security/
 * secret-field.md): same props in spirit (`revealMode`, `format`,
 * `clipboardTimer`, `gate`), no styling of its own beyond the primitives.
 */
export function SecretField({ itemId, field, onNotice, format = "plain", gesture }: SecretFieldProps) {
  const client = useClient()
  const gate = useGate()
  const { settings } = useSettings()
  const mode = gesture ?? settings.reveal
  const [shown, setShown] = useState<string | null>(null)
  const [clearIn, setClearIn] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const hideTimer = useRef<number | null>(null)
  /** In hold mode: whether the control is still held while the bytes arrive. */
  const holding = useRef(false)

  const hide = () => {
    holding.current = false
    if (hideTimer.current) window.clearTimeout(hideTimer.current)
    hideTimer.current = null
    setShown(null)
  }

  useEffect(() => {
    // Mask on blur and on unmount; never outlive the screen.
    const onBlur = () => hide()
    window.addEventListener("blur", onBlur)
    return () => {
      window.removeEventListener("blur", onBlur)
      hide()
    }
  }, [])

  useEffect(() => {
    if (clearIn === null || clearIn <= 0) return
    const t = window.setTimeout(() => setClearIn((n) => (n === null ? null : n - 1)), 1000)
    return () => window.clearTimeout(t)
  }, [clearIn])

  const fetchAndShow = async (keepFor: number | null) => {
    setError(null)
    try {
      const bytes = await gate.run((g) => client.reveal(itemId, field.field_id, g))
      if (keepFor === null && !holding.current) {
        // Released before the bytes arrived: never draw them.
        bytes.fill(0)
        return
      }
      setShown(decodeAndWipe(bytes))
      if (keepFor !== null) hideTimer.current = window.setTimeout(hide, keepFor)
    } catch (e) {
      holding.current = false
      setError(describeError(e))
    }
  }

  // Toggle mode.
  const toggle = () => {
    if (shown !== null) hide()
    else void fetchAndShow(REVEAL_MS)
  }

  // Hold mode.
  const holdStart = () => {
    if (holding.current || shown !== null) return
    holding.current = true
    void fetchAndShow(null)
  }
  const holdEnd = () => {
    if (holding.current || shown !== null) hide()
  }
  const onPointerDown = (e: PointerEvent<HTMLButtonElement>) => {
    if (e.button !== 0) return
    e.currentTarget.setPointerCapture?.(e.pointerId)
    holdStart()
  }
  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key !== " " && e.key !== "Enter") return
    e.preventDefault()
    if (!e.repeat) holdStart()
  }
  const onKeyUp = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === " " || e.key === "Enter") holdEnd()
  }

  const copy = async () => {
    setError(null)
    try {
      const secs = await gate.run((g) => client.copySecret(itemId, field.field_id, settings.clipboard_clear_secs, g))
      setClearIn(secs)
      onNotice(`Copied. The clipboard clears itself in ${secs} s.`)
    } catch (e) {
      setError(describeError(e))
    }
  }

  const copyPlain = async () => {
    if (field.value === null) return
    try {
      await navigator.clipboard.writeText(field.value)
      onNotice("Copied.")
    } catch (e) {
      setError(describeError(e))
    }
  }

  const value = field.hidden ? (shown !== null ? formatValue(shown, format) : (field.mask ?? "••••••••")) : formatValue(field.value ?? "", format)

  return (
    <Item variant="outline" size="sm" data-row="">
      <ItemContent>
        <ItemDescription>{field.label}</ItemDescription>
        <ItemTitle className="font-mono break-all">{value}</ItemTitle>
        {clearIn !== null && (
          <ItemDescription aria-live="polite">{clearIn > 0 ? `Clipboard clears in ${clearIn} s` : "Clipboard cleared"}</ItemDescription>
        )}
        {error && (
          <ItemDescription role="alert" className="text-destructive">
            {error}
          </ItemDescription>
        )}
      </ItemContent>
      <ItemActions>
        {field.hidden ? (
          <>
            {mode === "hold" ? (
              <Button
                variant="ghost"
                size="sm"
                aria-pressed={shown !== null}
                aria-describedby={`${field.field_id}-hold-hint`}
                onPointerDown={onPointerDown}
                onPointerUp={holdEnd}
                onPointerCancel={holdEnd}
                onPointerLeave={holdEnd}
                onKeyDown={onKeyDown}
                onKeyUp={onKeyUp}
                onBlur={holdEnd}
                onContextMenu={(e) => e.preventDefault()}
              >
                <Icon name={shown !== null ? "visibility_off" : "visibility"} data-icon="inline-start" />
                Hold to reveal
                <span id={`${field.field_id}-hold-hint`} className="sr-only">
                  Shows the value while held; release hides it.
                </span>
              </Button>
            ) : (
              <Button variant="ghost" size="sm" onClick={toggle} aria-pressed={shown !== null}>
                <Icon name={shown !== null ? "visibility_off" : "visibility"} data-icon="inline-start" />
                {shown !== null ? "Hide" : "Reveal"}
              </Button>
            )}
            <Button variant="ghost" size="sm" onClick={copy}>
              <Icon name="content_copy" data-icon="inline-start" />
              Copy
            </Button>
          </>
        ) : field.kind === "url" && field.value ? (
          <Button variant="ghost" size="sm" onClick={() => client.openSite(field.value!).catch((e) => setError(describeError(e)))}>
            <Icon name="open_in_new" data-icon="inline-start" />
            Open
          </Button>
        ) : (
          field.value && (
            <Button variant="ghost" size="sm" onClick={copyPlain}>
              <Icon name="content_copy" data-icon="inline-start" />
              Copy
            </Button>
          )
        )}
      </ItemActions>
    </Item>
  )
}
