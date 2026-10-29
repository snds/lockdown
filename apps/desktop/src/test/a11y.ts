// Structure checks the design review's accessibility rows ask of every
// screen (docs/plan/32-a11y-structure.md), written as plain DOM walks so
// they run in jsdom without a layout: list roles (R7) and landmark names
// (R18). The capture's axe run checks the same rules on the drawn screens.

const IMPLICIT: Record<string, string> = { UL: "list", OL: "list", LI: "listitem", MAIN: "main", NAV: "navigation", ASIDE: "complementary" }

function roleOf(el: Element): string | null {
  const explicit = el.getAttribute("role")?.trim().split(/\s+/)[0]
  if (explicit) return explicit
  if (el.tagName === "SECTION") return el.hasAttribute("aria-label") || el.hasAttribute("aria-labelledby") ? "region" : null
  return IMPLICIT[el.tagName] ?? null
}

/** Hidden from the accessibility tree, so a list may hold it beside its items. */
const ignored = (el: Element) => el.getAttribute("aria-hidden") === "true" || el.hasAttribute("hidden") || ["SCRIPT", "TEMPLATE", "STYLE"].includes(el.tagName)

/**
 * Every list in `root` that holds an element other than a list item, as
 * "<list name>: <child tag[role]>", empty when every list is well formed.
 */
export function malformedLists(root: ParentNode): string[] {
  const out: string[] = []
  for (const list of Array.from(root.querySelectorAll("*")).filter((e) => roleOf(e) === "list")) {
    for (const child of Array.from(list.children)) {
      if (ignored(child) || roleOf(child) === "listitem") continue
      const name = list.getAttribute("aria-label") ?? list.closest("[aria-label]")?.getAttribute("aria-label") ?? "(unnamed)"
      out.push(`${name}: ${child.tagName.toLowerCase()}${child.getAttribute("role") ? `[role=${child.getAttribute("role")}]` : ""}`)
    }
  }
  return out
}

/** The landmarks in `root` that share their role and name with another, as "role name". */
export function repeatedLandmarks(root: ParentNode): string[] {
  const seen = new Map<string, number>()
  for (const el of Array.from(root.querySelectorAll("*"))) {
    const role = roleOf(el)
    if (!role || !["region", "main", "navigation", "complementary", "form", "search"].includes(role)) continue
    const key = `${role} ${el.getAttribute("aria-label") ?? ""}`
    seen.set(key, (seen.get(key) ?? 0) + 1)
  }
  return [...seen].filter(([, n]) => n > 1).map(([k]) => k)
}
