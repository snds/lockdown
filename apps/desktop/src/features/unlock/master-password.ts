import { useState, type KeyboardEvent } from "react"

import type { SecretStrength } from "@unicron/ui/security/secret-field"

import { decodeAndWipe } from "@/core/secret-bytes"
import type { OncePage } from "@/core/types"
import type { ShownPage } from "@/features/status/once-page"
import { useStrength } from "@/features/vault/use-strength"

/** What the fields hold while the screen is up; cleared when it goes. */
export interface MasterPasswordValues {
  name: string
  password: string
  again: string
}

export const EMPTY_VALUES: MasterPasswordValues = { name: "", password: "", again: "" }

/** The screen's checks before Continue, in its words (doc 39 §1.9 screen 1). */
export function checkValues(v: MasterPasswordValues, withName: boolean, strength: SecretStrength | undefined): Partial<Record<"name" | "password" | "again", string>> {
  const errors: Partial<Record<"name" | "password" | "again", string>> = {}
  if (withName) {
    const name = v.name.trim()
    if (name === "") errors.name = "Type a name. It's what the lock screen shows."
    else if ([...name].length > 64) errors.name = "Keep the name to 64 characters or fewer."
  }
  if (v.password === "") errors.password = "Type a master password."
  else if (strength === "weak") errors.password = "This password protects everything else, so it needs to be at least fair. Add another word or two."
  if (!errors.password && v.password !== v.again) errors.again = "The two passwords don't match yet."
  return errors
}

/** The core's strength word for the password being chosen, with the name as a guessable word. */
export function useMasterStrength(values: MasterPasswordValues, withName: boolean): SecretStrength | undefined {
  const name = withName ? values.name.trim() : ""
  return useStrength(values.password, name ? [name] : [], true)
}

/** Tracks Caps Lock from the key events of a password field. */
export function useCapsLock(): [boolean, (e: KeyboardEvent<HTMLInputElement>) => void] {
  const [on, setOn] = useState(false)
  return [on, (e) => setOn(e.getModifierState?.("CapsLock") ?? false)]
}

/** A kit page from the core, decoded once and its bytes wiped. */
export function decodePage(page: OncePage, title = "Your recovery kit"): ShownPage {
  return { title, lines: page.lines, text: page.page.length ? decodeAndWipe(page.page) : null }
}
