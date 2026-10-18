// The visibility rule from 04-security §12 as measured (D67), the same rule
// the red-team harness runs (extension/test/fixtures/harness.js): a
// non-empty box on screen, no ancestor hidden or nearly see-through, the
// element on top at the centre and four inset corners is the field itself,
// and no painted `pointer-events: none` box overlaps it. The page is
// hostile, so everything here reads layout the browser computed, never a
// property the page could have redefined on the element.

function describe(el: Element | null): string {
  if (!el) return "nothing"
  const id = el.id ? `#${el.id}` : ""
  return `${el.tagName.toLowerCase()}${id}`
}

function overlaps(a: DOMRect, b: DOMRect): boolean {
  return a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

function painted(cs: CSSStyleDeclaration): boolean {
  const bg = cs.backgroundColor
  const transparentBg = bg === "transparent" || bg === "rgba(0, 0, 0, 0)"
  const noImage = cs.backgroundImage === "none"
  const noBorder = ["Top", "Right", "Bottom", "Left"].every((s) => parseFloat(cs.getPropertyValue(`border-${s.toLowerCase()}-width`)) === 0)
  return !(transparentBg && noImage && noBorder)
}

export interface Visibility {
  visible: boolean
  why: string
}

function ancestorsShown(el: Element): Visibility {
  for (let n: Element | null = el; n; n = n.parentElement) {
    const cs = getComputedStyle(n)
    if (cs.display === "none") return { visible: false, why: `${describe(n)} is display:none` }
    if (cs.visibility === "hidden" || cs.visibility === "collapse") return { visible: false, why: `${describe(n)} is visibility:hidden` }
    if (parseFloat(cs.opacity) < 0.1) return { visible: false, why: `${describe(n)} is see-through (opacity ${cs.opacity})` }
  }
  return { visible: true, why: "" }
}

function onScreen(rect: DOMRect): Visibility {
  if (rect.width === 0 || rect.height === 0) return { visible: false, why: "empty box" }
  if (rect.bottom <= 0 || rect.right <= 0 || rect.top >= window.innerHeight || rect.left >= window.innerWidth) return { visible: false, why: "off screen" }
  return { visible: true, why: "" }
}

function points(rect: DOMRect): [number, number][] {
  const inset = Math.min(2, rect.width / 4, rect.height / 4)
  return [
    [rect.left + rect.width / 2, rect.top + rect.height / 2],
    [rect.left + inset, rect.top + inset],
    [rect.right - inset, rect.top + inset],
    [rect.left + inset, rect.bottom - inset],
    [rect.right - inset, rect.bottom - inset],
  ]
}

/** Whether `el`, a field on the page, may be filled by the visibility rule. */
export function fieldVisibility(el: Element, ignore?: Element | null): Visibility {
  const rect = el.getBoundingClientRect()
  const box = onScreen(rect)
  if (!box.visible) return box
  const shown = ancestorsShown(el)
  if (!shown.visible) return shown
  for (const [x, y] of points(rect)) {
    let hit = document.elementFromPoint(x, y)
    // Lockdown's own menu may sit beside the field; it never counts as cover.
    if (ignore && hit === ignore) hit = el
    if (!hit || !(hit === el || el.contains(hit))) return { visible: false, why: `covered by ${describe(hit)}` }
  }
  for (const other of Array.from(document.querySelectorAll("body *"))) {
    if (other === el || other === ignore || el.contains(other) || other.contains(el)) continue
    const cs = getComputedStyle(other)
    if (cs.pointerEvents !== "none") continue
    if (cs.display === "none" || cs.visibility === "hidden" || parseFloat(cs.opacity) < 0.1) continue
    if (!painted(cs)) continue
    const r = other.getBoundingClientRect()
    if (r.width > 0 && r.height > 0 && overlaps(r, rect)) return { visible: false, why: `covered by ${describe(other)} (pointer-events:none)` }
  }
  return { visible: true, why: "" }
}

/** Whether Lockdown's own menu host is what the person sees at its own
 *  points: the stand-in for IntersectionObserver v2 where the browser has
 *  none (Firefox). The host's shadow root is closed, so a hit inside it
 *  reads as the host itself. */
export function hostVisible(host: Element): boolean {
  const rect = host.getBoundingClientRect()
  if (!onScreen(rect).visible || !ancestorsShown(host).visible) return false
  return points(rect).every(([x, y]) => document.elementFromPoint(x, y) === host)
}
