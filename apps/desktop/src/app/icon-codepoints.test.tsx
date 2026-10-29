import { render } from "@testing-library/react"
import { Icon, setIconCodepoints } from "@unicron/ui/icon"
import codepoints from "virtual:icon-codepoints"

/**
 * The icon fix (docs/plan/38-mac-fixes.md §2): the codepoints worked out at
 * build time from the shipped font, and unicron's Icon (as patched under
 * patches/) drawing an icon by its codepoint once it has them.
 */
describe("icons by codepoint", () => {
  afterEach(() => setIconCodepoints({}))

  it("takes every icon the sources name from the shipped font, each a private-use codepoint", () => {
    // A few the app draws everywhere, and two whose codepoints Material has never moved.
    for (const name of ["lock", "search", "check", "close", "content_copy", "print", "qr_code_2", "inventory_2"]) expect(codepoints[name], name).toBeDefined()
    expect(codepoints.search).toBe(0xe8b6)
    expect(codepoints.content_copy).toBe(0xe14d)
    for (const [name, cp] of Object.entries(codepoints)) {
      expect(name).toMatch(/^[a-z0-9_]+$/)
      expect((cp >= 0xe000 && cp <= 0xf8ff) || cp >= 0xf0000, `${name} ${cp.toString(16)}`).toBe(true)
    }
    // Only the names in use, not the font's four thousand.
    expect(Object.keys(codepoints).length).toBeLessThan(400)
  })

  it("draws the glyph by its codepoint, keeping the name beside it", () => {
    setIconCodepoints(codepoints)
    const { container } = render(<Icon name="lock" />)
    const text = container.querySelector("svg[data-slot=icon] text")!
    expect(text.textContent).toBe(String.fromCodePoint(codepoints.lock!))
    expect(text).toHaveAttribute("data-icon-name", "lock")
    // The variable axes still reach the glyph.
    expect((text as SVGTextElement).style.fontVariationSettings).toContain("'FILL' var(--c-icon-fill)")
  })

  it("draws a name it has no codepoint for as its ligature, as before", () => {
    setIconCodepoints({})
    const { container } = render(<Icon name="lock" />)
    expect(container.querySelector("svg[data-slot=icon] text")!.textContent).toBe("lock")
  })
})
