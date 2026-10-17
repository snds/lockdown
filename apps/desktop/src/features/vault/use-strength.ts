import { useEffect, useState } from "react"

import { secretStrengthFromScore, type SecretStrength } from "@unicron/ui/security/secret-field"

import { useClient } from "@/core/client"

/** How long typing must pause before the core is asked; each keystroke restarts it. */
export const STRENGTH_PAUSE_MS = 150

/**
 * The strength word for a password the person is typing (D116): the
 * core's zxcvbn score (`password_strength`), mapped to unicron's word
 * with `secretStrengthFromScore`. The candidate goes once per pause, as
 * bytes zeroed when the call returns, with the form's plain words as
 * context; a reply for an older candidate is dropped, and the last word
 * stays until the newer one lands, so the meter doesn't flicker while the
 * person types. Off (no meter) when `enabled` is false, when nothing is
 * typed, and when the core can't say. Only ever what was just typed here:
 * a stored secret is never scored. Nothing of the candidate is kept here.
 */
export function useStrength(candidate: string, context: readonly string[], enabled: boolean): SecretStrength | undefined {
  const client = useClient()
  const [strength, setStrength] = useState<SecretStrength | undefined>(undefined)
  const words = context.join("\n")

  useEffect(() => {
    if (!enabled || candidate === "") return
    let current = true
    const timer = window.setTimeout(() => {
      const bytes = Array.from(new TextEncoder().encode(candidate))
      client
        .passwordStrength(bytes, words ? words.split("\n") : [])
        .then((score) => {
          if (current) setStrength(score === null ? undefined : secretStrengthFromScore(score))
        })
        .catch(() => {})
        .finally(() => bytes.fill(0))
    }, STRENGTH_PAUSE_MS)
    return () => {
      current = false
      window.clearTimeout(timer)
    }
  }, [client, candidate, words, enabled])

  return enabled && candidate !== "" ? strength : undefined
}
