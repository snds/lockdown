import { useEffect, useRef, useState } from "react"

import { Button, Icon, Item, ItemActions, ItemContent, ItemDescription, ItemTitle } from "@unicron/ui"

import { describeError, useClient } from "@/core/client"
import { decodeAndWipe } from "@/core/secret-bytes"
import type { FieldView } from "@/core/types"

/** A reveal masks itself again after this long (design brief §4). */
const REVEAL_MS = 30_000

interface SecretFieldProps {
  itemId: string
  field: FieldView
  /** Every field's copy timer, so a second copy stands the first down. */
  onNotice: (text: string) => void
}

/**
 * One field of an item. Plain fields show their value. Hidden fields show
 * the core's mask; **Reveal** fetches the bytes, decodes them for the
 * 30 seconds they are on screen (or until the window loses focus, or the
 * field is hidden again), then drops the string and zeroes the buffer.
 * **Copy** never brings the value here: the shell copies from the core and
 * clears the clipboard after the timer if it still holds our value.
 *
 * This is a placeholder for unicron's SecretField (03-architecture §9);
 * it carries no styling of its own beyond the primitives.
 */
export function SecretField({ itemId, field, onNotice }: SecretFieldProps) {
  const client = useClient()
  const [shown, setShown] = useState<string | null>(null)
  const [clearIn, setClearIn] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const hideTimer = useRef<number | null>(null)

  const hide = () => {
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

  const reveal = async () => {
    if (shown !== null) {
      hide()
      return
    }
    setError(null)
    try {
      const bytes = await client.reveal(itemId, field.field_id)
      setShown(decodeAndWipe(bytes))
      hideTimer.current = window.setTimeout(hide, REVEAL_MS)
    } catch (e) {
      setError(describeError(e))
    }
  }

  const copy = async () => {
    setError(null)
    try {
      const secs = await client.copySecret(itemId, field.field_id)
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

  return (
    <Item variant="outline" size="sm">
      <ItemContent>
        <ItemDescription>{field.label}</ItemDescription>
        <ItemTitle className="font-mono break-all">
          {field.hidden ? (shown ?? field.mask ?? "••••••••") : (field.value ?? "")}
        </ItemTitle>
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
            <Button variant="ghost" size="sm" onClick={reveal} aria-pressed={shown !== null}>
              <Icon name={shown !== null ? "visibility_off" : "visibility"} data-icon="inline-start" />
              {shown !== null ? "Hide" : "Reveal"}
            </Button>
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
