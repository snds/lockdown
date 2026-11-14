import { render } from "@testing-library/react"
import { Icon, isIconName } from "@unicron/ui"

import { templateIcon } from "./template-model"

/*
 * A template's icon arrives as data, and the core names Material Symbols
 * (crates/vault/src/template.rs). unicron draws IBM Carbon from 1.0, so
 * `templateIcon` translates the name where it is read
 * (docs/plan/62-unicron-1-0.md §2).
 *
 * CORE_ICONS is every name that Rust file uses today (its default, and one
 * per kind and variant), copied here: the desktop's tests can't read a file
 * outside this package. The review harness does read the Rust source, and
 * its engine check (design/capture/scripts/engine-check.ts) opens the "New
 * item" menu drawn from it and fails on any icon unicron has no drawing
 * for, so a template added with a new icon name is caught there.
 */
const CORE_ICONS = [
  "account_balance",
  "account_balance_wallet",
  "api",
  "badge",
  "business",
  "card_membership",
  "category",
  "contact_page",
  "credit_card",
  "database",
  "description",
  "directions_car",
  "dns",
  "encrypted",
  "fingerprint",
  "flight",
  "forest",
  "health_and_safety",
  "home",
  "id_card",
  "key",
  "license",
  "lock_reset",
  "mail",
  "medical_information",
  "passkey",
  "router",
  "sticky_note_2",
  "terminal",
  "timer",
  "wifi",
]

describe("a template's icon, from the core's name to one unicron draws", () => {
  it("translates every icon the core names, none by the fallback", () => {
    for (const name of CORE_ICONS) {
      // A fallback no template uses, so a name that fell through shows.
      const drawn = templateIcon(name, "unknown")
      expect(drawn, name).not.toBe("unknown")
      expect(isIconName(drawn), `${name} → ${drawn}`).toBe(true)
    }
  })

  it("reads the core's names as Material, also where Carbon has the same word for another drawing", () => {
    expect(templateIcon("key", "unknown")).toBe("password")
    expect(templateIcon("flight", "unknown")).toBe("plane")
    expect(templateIcon("credit_card", "unknown")).toBe("wallet")
    // `category` and `badge` are names in both sets; the core's are Material's.
    expect(templateIcon("category", "unknown")).toBe("categories")
    expect(templateIcon("badge", "unknown")).toBe("user--identification")
  })

  it("keeps a Carbon name, and falls back for an empty or unknown one", () => {
    expect(templateIcon("bicycle", "categories")).toBe("bicycle")
    expect(templateIcon("", "categories")).toBe("categories")
    expect(templateIcon(undefined, "categories")).toBe("categories")
    expect(templateIcon("not_an_icon_anywhere", "categories")).toBe("categories")
  })

  it("draws the translated name: an svg with a drawing, not the missing-icon box", () => {
    for (const name of CORE_ICONS) {
      const { container, unmount } = render(<Icon name={templateIcon(name, "unknown")} />)
      const svg = container.querySelector("svg[data-slot=icon]")!
      expect(svg, name).toHaveAttribute("viewBox", "0 0 32 32")
      expect(svg, name).not.toHaveAttribute("data-icon-missing")
      expect(svg.querySelector("path, circle, rect"), name).not.toBeNull()
      unmount()
    }
  })
})
