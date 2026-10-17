// The clickjacking red team, browser-driven (04-security §12; 07-workspace
// §4 step 5). The cases are the ones `matcher::tests::
// never_fill_cross_origin_iframes_without_confirmation` states in Rust;
// here a real Chromium measures a real page and the same wasm core
// answers. The expectations are copied from the Rust test, not derived.
import { test, expect, type Frame, type Page } from "@playwright/test";
import { serve, type Served } from "./server";

let served: Served;
let localhost: string;
let loopback: string;

test.beforeAll(async () => {
  served = await serve();
  localhost = `http://localhost:${served.port}`;
  loopback = `http://127.0.0.1:${served.port}`;
});

test.afterAll(async () => {
  await served?.close();
});

interface Outcome {
  v: number;
  decision: { decision: string };
  request: { top: string; frame: string; field_visible: boolean; user_confirmed: boolean; kind: string; decision: string };
  why: string;
  response: { v: number; verdict: "fill" | "confirm" | "refuse"; reason?: string };
}

async function check(where: Page | Frame, options: Record<string, unknown> = {}): Promise<Outcome> {
  await where.waitForFunction(() => (window as any).lockdownHarness !== undefined);
  return where.evaluate(async (opts) => {
    const h = (window as any).lockdownHarness;
    await h.ready;
    return h.check("#pass", opts);
  }, options);
}

function childFrame(page: Page): Frame {
  const frame = page.frame({ url: /frame\.html/ });
  if (!frame) throw new Error("the fixture did not create the frame");
  return frame;
}

test("a field in a cross-origin frame needs a confirmation", async ({ page }) => {
  await page.goto(`${localhost}/top.html?case=cross-origin&frame=${loopback}`);
  const frame = childFrame(page);
  await frame.waitForLoadState();
  const out = await check(frame);
  expect(out.decision.decision).toBe("exact");
  expect(out.request.top).toBe(localhost);
  expect(out.request.frame).toBe(loopback);
  expect(out.request.field_visible, out.why).toBe(true);
  expect(out.response.verdict).toBe("confirm");
  expect(out.response.reason).toContain("inside a page from localhost");
  // After the person's deliberate action, the same field fills.
  const confirmed = await check(frame, { userConfirmed: true });
  expect(confirmed.response.verdict).toBe("fill");
});

test("a field in a same-origin frame fills (control)", async ({ page }) => {
  await page.goto(`${localhost}/top.html?case=same-origin&frame=${localhost}`);
  const frame = childFrame(page);
  await frame.waitForLoadState();
  const out = await check(frame);
  expect(out.request.top).toBe(localhost);
  expect(out.request.frame).toBe(localhost);
  expect(out.request.field_visible, out.why).toBe(true);
  expect(out.response.verdict).toBe("fill");
  expect(out.response.reason).toBeUndefined();
});

test("a field under an opacity-0 overlay is refused as clickjacking", async ({ page }) => {
  await page.goto(`${localhost}/top.html?case=opacity-overlay`);
  const out = await check(page);
  expect(out.request.field_visible).toBe(false);
  expect(out.why).toContain("covered by div#overlay");
  expect(out.response.verdict).toBe("refuse");
  expect(out.response.reason).toContain("clickjacking");
  // Confirmation does not rescue a hidden field: the refusal comes first.
  const confirmed = await check(page, { userConfirmed: true });
  expect(confirmed.response.verdict).toBe("refuse");
});

test("a field under a pointer-events:none overlay is refused as clickjacking", async ({ page }) => {
  await page.goto(`${localhost}/top.html?case=pointer-events-none-overlay`);
  // The overlay really does let clicks through: a hit test at the field's
  // centre finds the field, which is exactly why hit testing alone is not
  // the rule.
  const hitIsField = await page.evaluate(() => {
    const el = document.querySelector("#pass")!;
    const r = el.getBoundingClientRect();
    return document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2) === el;
  });
  expect(hitIsField).toBe(true);
  const out = await check(page);
  expect(out.request.field_visible).toBe(false);
  expect(out.why).toContain("pointer-events:none");
  expect(out.response.verdict).toBe("refuse");
  expect(out.response.reason).toContain("clickjacking");
});

test("a plain top-level field fills, and sensitive kinds still ask", async ({ page }) => {
  await page.goto(`${localhost}/top.html?case=control`);
  const login = await check(page);
  expect(login.request.field_visible, login.why).toBe(true);
  expect(login.response.verdict).toBe("fill");
  for (const kind of ["code", "card", "passkey"]) {
    const out = await check(page, { kind });
    expect(out.response.verdict, kind).toBe("confirm");
    expect(out.response.reason).toContain("deliberate tap");
  }
  const identity = await check(page, { kind: "identity" });
  expect(identity.response.verdict).toBe("fill");
});

test("a lookalike is refused whatever else is true", async ({ page }) => {
  await page.goto(`${localhost}/top.html?case=control`);
  // `localhost` has no registrable domain, so no fixture host can imitate
  // a real site. Drive the core in the page with the Rust test's own pair
  // instead; what matters is that the browser build refuses it.
  const out = await page.evaluate(async () => {
    const h = (window as any).lockdownHarness;
    await h.ready;
    const m = await import("/pkg/lockdown_wasm.js");
    const d = m.decide({ saved: "https://www.paypal.com", current: "https://paypal.com.evil.net/login" });
    const r = m.fill_check({
      top: "https://paypal.com.evil.net",
      frame: "https://paypal.com.evil.net",
      field_visible: true,
      user_confirmed: true,
      kind: "login",
      decision: d.decision,
    });
    return { d, r };
  });
  expect(out.d.decision).toBe("lookalike");
  expect(out.r.verdict).toBe("refuse");
  expect(out.r.reason).toContain("looks like a different site");
});
