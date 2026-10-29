// The in-page frames against axe (docs/plan/32-a11y-structure.md): the fill
// menu, the save prompt and the passkey sheet, drawn by the real build over
// the real native host and desktop endpoint, checked in light and dark.
// Serious and critical violations fail (design review R4: contrast; R7: a
// list's children are its items), and the structure the review asked for
// is checked by role: one page heading, the choices as a list of items.
//
// axe runs inside each frame's own document: the frame sits in a closed
// shadow root on the page, out of reach of an axe run from the page.
import { chromium, expect, test, type BrowserContext, type Frame, type Page } from "@playwright/test";
import axe from "axe-core";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { Desktop } from "./desktop";
import { serve, type Served } from "./server";

const DEV_ID = "dbdblnolpiehhlabpmhobmkbjipmebpe";
const dist = fileURLToPath(new URL("../dist/chromium", import.meta.url));
const TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"];
const SCHEMES = ["light", "dark"] as const;

let served: Served;
let desktop: Desktop;
let ctx: BrowserContext;
let scratch: string;
let port: number;
const at = (host: string, path: string) => `http://${host}:${port}${path}`;

test.describe.configure({ mode: "serial" });
test.setTimeout(90_000);

test.beforeAll(async () => {
  served = await serve();
  port = served.port;
  scratch = await mkdtemp(join(tmpdir(), "lockdown-a11y-"));
  const profile = join(scratch, "profile");
  desktop = Desktop.start(join(scratch, "vault"), profile, ["--platform-check"]);
  const ready = await desktop.ready;
  expect(ready.listening, String(ready.problem)).toBe(true);
  ctx = await chromium.launchPersistentContext(profile, {
    channel: "chromium",
    headless: true,
    viewport: { width: 1000, height: 760 },
    env: { ...process.env, LOCKDOWN_DATA: String(ready.data) },
    args: [
      `--disable-extensions-except=${dist}`,
      `--load-extension=${dist}`,
      "--no-proxy-server",
      "--host-resolver-rules=MAP *.test 127.0.0.1",
    ],
  });
  if (ctx.serviceWorkers().length === 0) await ctx.waitForEvent("serviceworker");
  const p = await ctx.newPage();
  await p.goto(`chrome-extension://${DEV_ID}/popup.html`);
  await p.getByRole("button", { name: "Pair with Lockdown" }).click({ timeout: 30_000 });
  const request = await desktop.next("pairing", 30_000);
  await desktop.ask(`approve ${request.request_id}`, "answered");
  await desktop.next("platform_check");
  await expect(p.getByRole("searchbox", { name: "Search Lockdown" })).toBeVisible({ timeout: 10_000 });
  await p.close();
});

test.afterAll(async () => {
  await ctx?.close();
  desktop?.stop();
  await served?.close();
  if (scratch) await rm(scratch, { recursive: true, force: true });
});

async function lockdownFrame(page: Page, kind: "menu" | "prompt" | "passkey"): Promise<Frame> {
  const deadline = Date.now() + 15_000;
  for (;;) {
    const f = page.frames().find((x) => x.url().startsWith(`chrome-extension://${DEV_ID}/frame.html#${kind}:`));
    if (f) {
      await f.waitForLoadState();
      return f;
    }
    if (Date.now() > deadline) throw new Error(`no Lockdown ${kind} frame on ${page.url()}`);
    await page.waitForTimeout(50);
  }
}

interface Violation {
  id: string;
  impact: string | null;
  nodes: string[];
}

/** axe-core in the frame's own document, with the capture's rule tags. */
async function axeIn(frame: Frame): Promise<Violation[]> {
  if (!(await frame.evaluate(() => "axe" in window))) await frame.evaluate(axe.source);
  return frame.evaluate(async (tags) => {
    const run = (window as unknown as { axe: typeof axe }).axe.run;
    const r = await run(document, { runOnly: { type: "tag", values: tags }, resultTypes: ["violations"] });
    return r.violations.map((v) => ({ id: v.id, impact: v.impact ?? null, nodes: v.nodes.map((n) => `${n.target.join(" ")}: ${n.failureSummary ?? ""}`) }));
  }, TAGS);
}

/** What fails the suite: serious and critical. */
const blocking = (vs: Violation[]) => vs.filter((v) => v.impact === "serious" || v.impact === "critical");

/** The frame's colours really are the scheme asked for (the frame reads `prefers-color-scheme`). */
async function inScheme(frame: Frame, scheme: (typeof SCHEMES)[number]) {
  const dark = await frame.evaluate(() => matchMedia("(prefers-color-scheme: dark)").matches);
  expect(dark).toBe(scheme === "dark");
}

/** One page heading, the brand; the frame's main is named, apart from the page's. */
async function frameStructure(frame: Frame) {
  await expect(frame.getByRole("heading", { level: 1 })).toHaveText("Lockdown");
  await expect(frame.getByRole("main", { name: "Lockdown" })).toBeVisible();
}

for (const scheme of SCHEMES) {
  test(`the fill menu has no serious or critical axe violation, its logins a list of items (${scheme})`, async () => {
    const page = await ctx.newPage();
    await page.emulateMedia({ colorScheme: scheme });
    await page.goto(at("localhost", "/ext/login.html"));
    await page.focus("#user");
    const menu = await lockdownFrame(page, "menu");
    const pick = menu.getByRole("button", { name: /Local site/ });
    await expect(pick).toBeEnabled();
    await inScheme(menu, scheme);
    await frameStructure(menu);
    const list = menu.getByRole("list");
    await expect(list).toHaveAccessibleName(`Lockdown's choices for localhost:${port}`);
    await expect(list.getByRole("listitem")).toHaveCount(await list.getByRole("button").count());
    expect(blocking(await axeIn(menu))).toEqual([]);
    await page.close();
  });

  test(`the save prompt has no serious or critical axe violation (${scheme})`, async () => {
    const page = await ctx.newPage();
    await page.emulateMedia({ colorScheme: scheme });
    await page.goto(at(`a11y-${scheme}.test`, "/ext/signup.html"));
    await page.fill("#user", `reader-${scheme}`);
    await page.fill("#pass", "brand-new-pass-7");
    await page.fill("#again", "brand-new-pass-7");
    await page.click("#go");
    await page.waitForURL(/welcome\.html/);
    const prompt = await lockdownFrame(page, "prompt");
    await expect(prompt.getByRole("button", { name: "Save" })).toBeEnabled();
    await inScheme(prompt, scheme);
    await frameStructure(prompt);
    // The username under the question is secondary text: it must still read (R4).
    await expect(prompt.locator(".sub")).toHaveText(`reader-${scheme}`);
    expect(blocking(await axeIn(prompt))).toEqual([]);
    await page.close();
  });
}

test("the passkey sheet, saving and then signing in, has no serious or critical axe violation in light or dark", async () => {
  const rp = (query: string) => at("localhost", `/ext/webauthn.html${query}`);
  // Dark first: saving in light makes the passkey the second loop signs in with
  // (and after it, the save sheet would say the account already holds one).
  for (const scheme of ["dark", "light"] as const) {
    const page = await ctx.newPage();
    await page.emulateMedia({ colorScheme: scheme });
    await page.goto(rp("?uv=preferred"));
    await page.click("#register");
    const sheet = await lockdownFrame(page, "passkey");
    const save = sheet.getByRole("button", { name: "Save passkey" });
    await expect(save).toBeEnabled();
    await inScheme(sheet, scheme);
    await frameStructure(sheet);
    expect(blocking(await axeIn(sheet))).toEqual([]);
    if (scheme === "light") {
      // One passkey to sign in with, for the sheet's list below.
      await save.click();
      await desktop.next("platform_check");
      await expect(page.locator("#status")).toHaveText("registered", { timeout: 15_000 });
    }
    await page.close();
  }
  for (const scheme of SCHEMES) {
    const page = await ctx.newPage();
    await page.emulateMedia({ colorScheme: scheme });
    await page.goto(rp("?uv=discouraged"));
    await page.click("#signin");
    const sheet = await lockdownFrame(page, "passkey");
    await expect(sheet.getByRole("button", { name: /Local site/ })).toBeEnabled();
    await inScheme(sheet, scheme);
    await frameStructure(sheet);
    const list = sheet.getByRole("list", { name: "Passkeys for localhost" });
    await expect(list.getByRole("listitem")).toHaveCount(1);
    // "Use another device" follows the list; it isn't one of its items.
    await expect(list.getByRole("button", { name: "Use another device or security key" })).toHaveCount(0);
    expect(blocking(await axeIn(sheet))).toEqual([]);
    await page.close();
  }
});
