// Per-site fill fixes in the page (docs/plan/23-bridges.md §5): which
// field is which when the page's own markup misleads Lockdown, or "never
// fill here". The fixes come from the app (the person's own, kept on the
// site's logins, over the signed community list); the page cannot set or
// see them. A field is named by its own `id`, `name` or `autocomplete`
// attribute, compared exactly, never by a selector.

import type { FieldMatch, FieldRole, SiteFixView } from "../shared/protocol"

let view: SiteFixView | null = null
let loading: Promise<void> | null = null

/** Asks the app once per page (and again after the popup changes a fix).
 *  No answer in time, or an app that is locked or not paired, means no
 *  fixes: the page fills as it would without them. */
export function ensureFixes(): Promise<void> {
  if (!loading) {
    loading = Promise.race([
      chrome.runtime
        .sendMessage({ t: "fixes" })
        .then((v: SiteFixView | { error?: string } | null | undefined) => {
          view = v && "host" in v ? v : null
        })
        .catch(() => {
          view = null
        }),
      new Promise<void>((resolve) => setTimeout(resolve, 2_000)),
    ])
  }
  return loading
}

/** Forgets what the app said, so the next field asks again. */
export function resetFixes() {
  view = null
  loading = null
}

/** Never fill on this page. */
export function neverHere(): boolean {
  return view?.never === true
}

function attr(el: Element, name: "id" | "name" | "autocomplete"): string {
  return (el.getAttribute(name) ?? "").trim()
}

function matches(m: FieldMatch, el: Element): boolean {
  const want: [string | null | undefined, string][] = [
    [m.id, attr(el, "id")],
    [m.name, attr(el, "name")],
    [m.autocomplete, attr(el, "autocomplete")],
  ]
  // Every attribute the fix names must match; one it leaves out is free.
  return want.some(([w]) => !!w) && want.every(([w, have]) => !w || w === have)
}

/** The role a fix gives this field, if any. */
export function fixedRole(el: Element): FieldRole | null {
  if (!view) return null
  return view.fields.find((f) => matches(f.match, el))?.role ?? null
}
