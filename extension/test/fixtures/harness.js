// The content-script side of the fill gate, as the harness runs it: measure
// the page, hand the measurement to the wasm core, show nothing of our own.
//
// This file is loaded as a module in every fixture document (top pages and
// frames alike). It loads the wasm from the document's own origin, so a
// cross-origin frame has its own instance, exactly as an extension's
// content script would.
//
// What the core needs from the page (extension/schema/fill-request.schema.json):
//   top            the top-level page's origin
//   frame          this document's origin
//   field_visible  the field and every ancestor are visible, on screen and
//                  not covered by anything, whatever the coverer's opacity
//                  or pointer-events
//   user_confirmed whether the person already took a deliberate action
//   kind           what is being filled
//   decision       what `decide` said for the item on this frame
import init, { decide, fill_check, schema_version } from "/pkg/lockdown_wasm.js";

const ready = init();

/** The top-level origin as this document can see it. In Chromium a frame
 *  reads `location.ancestorOrigins`; the last entry is the top. A real
 *  extension gets the top from the background page's `sender.tab`, which
 *  is why the fill request carries it as data instead of trusting the
 *  frame's own word. */
function topOrigin() {
  const anc = location.ancestorOrigins;
  if (anc && anc.length > 0) return anc[anc.length - 1];
  if (window.top === window) return location.origin;
  try {
    return window.top.location.origin;
  } catch {
    return document.referrer ? new URL(document.referrer).origin : "";
  }
}

function describe(el) {
  if (!el) return "nothing";
  const id = el.id ? `#${el.id}` : "";
  const cls = el.className && typeof el.className === "string" ? `.${el.className.split(/\s+/).join(".")}` : "";
  return `${el.tagName.toLowerCase()}${id}${cls}`;
}

function overlaps(a, b) {
  return a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;
}

function painted(cs) {
  // A box that draws nothing (no background, no border, no image) cannot
  // hide the field; one that draws anything can.
  const bg = cs.backgroundColor;
  const transparentBg = bg === "transparent" || bg === "rgba(0, 0, 0, 0)";
  const noImage = cs.backgroundImage === "none";
  const noBorder = ["Top", "Right", "Bottom", "Left"].every((s) => parseFloat(cs[`border${s}Width`]) === 0);
  return !(transparentBg && noImage && noBorder);
}

/** The visibility rule from 04-security §12, measured. Returns
 *  `{visible, why}` so a refusal can say what covered the field. */
export function fieldVisibility(el) {
  const rect = el.getBoundingClientRect();
  if (rect.width === 0 || rect.height === 0) return { visible: false, why: "empty box" };
  if (rect.bottom <= 0 || rect.right <= 0 || rect.top >= window.innerHeight || rect.left >= window.innerWidth) {
    return { visible: false, why: "off screen" };
  }
  for (let n = el; n; n = n.parentElement) {
    const cs = getComputedStyle(n);
    if (cs.display === "none") return { visible: false, why: `${describe(n)} is display:none` };
    if (cs.visibility === "hidden" || cs.visibility === "collapse") return { visible: false, why: `${describe(n)} is visibility:hidden` };
    if (parseFloat(cs.opacity) < 0.1) return { visible: false, why: `${describe(n)} is see-through (opacity ${cs.opacity})` };
  }
  // Hit test: whatever is on top at five points must be the field itself.
  // This catches any hit-testable coverer, including an opacity:0 one.
  const inset = Math.min(2, rect.width / 4, rect.height / 4);
  const points = [
    [rect.left + rect.width / 2, rect.top + rect.height / 2],
    [rect.left + inset, rect.top + inset],
    [rect.right - inset, rect.top + inset],
    [rect.left + inset, rect.bottom - inset],
    [rect.right - inset, rect.bottom - inset],
  ];
  for (const [x, y] of points) {
    const hit = document.elementFromPoint(x, y);
    if (!hit || !(hit === el || el.contains(hit))) {
      return { visible: false, why: `covered by ${describe(hit)} at (${Math.round(x)}, ${Math.round(y)})` };
    }
  }
  // A pointer-events:none coverer is invisible to hit testing but not to
  // the eye. Any painted box with pointer-events:none that overlaps the
  // field, and is neither an ancestor nor a descendant, counts as cover.
  for (const other of document.querySelectorAll("body *")) {
    if (other === el || el.contains(other) || other.contains(el)) continue;
    const cs = getComputedStyle(other);
    if (cs.pointerEvents !== "none") continue;
    if (cs.display === "none" || cs.visibility === "hidden" || parseFloat(cs.opacity) < 0.1) continue;
    if (!painted(cs)) continue;
    const r = other.getBoundingClientRect();
    if (r.width > 0 && r.height > 0 && overlaps(r, rect)) {
      return { visible: false, why: `covered by ${describe(other)} (pointer-events:none)` };
    }
  }
  return { visible: true, why: "" };
}

/** Builds the fill request for `el`, as a content script would. */
export function measure(el, { kind = "login", userConfirmed = false } = {}) {
  const vis = fieldVisibility(el);
  return {
    request: {
      top: topOrigin(),
      frame: location.origin,
      field_visible: vis.visible,
      user_confirmed: userConfirmed,
      kind,
      decision: "exact",
    },
    why: vis.why,
  };
}

/** Measures `selector`, asks the core, returns everything for the test. */
export async function check(selector, { saved = location.href, kind = "login", userConfirmed = false } = {}) {
  await ready;
  const el = document.querySelector(selector);
  if (!el) throw new Error(`no element matches ${selector}`);
  const d = decide({ saved, current: location.href });
  const { request, why } = measure(el, { kind, userConfirmed });
  request.decision = d.decision;
  const response = fill_check(request);
  return { v: schema_version(), decision: d, request, why, response };
}

// Expose for Playwright's `evaluate`.
window.lockdownHarness = { check, measure, fieldVisibility, ready };
