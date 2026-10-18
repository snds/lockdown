// Pairing from the inline menu, with no popup (docs/plan/27-web-server-finish.md
// §5), in Chromium: the same route the Firefox suite drives, where the popup
// can't be opened as a page. A fresh profile and desktop fixture, so this
// browser starts unpaired: a regular sign-in page, the content script's
// menu, "Pair this browser", the same code in the menu and in the app, the
// app approves, and the menu then fills the login, decided in the core.
import { chromium, expect, test, type BrowserContext, type Frame, type Page } from "@playwright/test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { Desktop } from "./desktop";
import { serve, type Served } from "./server";

const DEV_ID = "dbdblnolpiehhlabpmhobmkbjipmebpe";
const dist = fileURLToPath(new URL("../dist/chromium", import.meta.url));

let served: Served;
let desktop: Desktop;
let ctx: BrowserContext;
let scratch: string;
let port: number;

test.describe.configure({ mode: "serial" });
test.setTimeout(60_000);

async function lockdownFrame(page: Page): Promise<Frame> {
  const deadline = Date.now() + 10_000;
  for (;;) {
    const f = page.frames().find((x) => x.url().startsWith(`chrome-extension://${DEV_ID}/frame.html#menu:`));
    if (f) {
      await f.waitForLoadState();
      return f;
    }
    if (Date.now() > deadline) throw new Error(`no Lockdown menu frame on ${page.url()}`);
    await page.waitForTimeout(50);
  }
}

test.beforeAll(async () => {
  served = await serve();
  port = served.port;
  scratch = await mkdtemp(join(tmpdir(), "lockdown-menu-pair-"));
  const profile = join(scratch, "profile");
  desktop = Desktop.start(join(scratch, "vault"), profile);
  const ready = await desktop.ready;
  expect(ready.listening, String(ready.problem)).toBe(true);
  ctx = await chromium.launchPersistentContext(profile, {
    channel: "chromium",
    headless: true,
    viewport: { width: 1000, height: 760 },
    env: { ...process.env, LOCKDOWN_DATA: String(ready.data) },
    args: [`--disable-extensions-except=${dist}`, `--load-extension=${dist}`, "--no-proxy-server"],
  });
});

test.afterAll(async () => {
  await ctx?.close();
  desktop?.stop();
  await served?.close();
  if (scratch) await rm(scratch, { recursive: true, force: true });
});

test("the inline menu pairs the browser with the app and then fills, with no popup", async () => {
  const page = await ctx.newPage();
  await page.goto(`http://localhost:${port}/ext/login.html`);
  await page.focus("#user");
  const menu = await lockdownFrame(page);
  const pair = menu.getByRole("button", { name: "Pair this browser" });
  await expect(pair).toBeEnabled({ timeout: 30_000 });
  await pair.click();
  const shown = (await menu.getByLabel(/^Pairing code/).textContent())?.replace(/\s/g, "");
  const request = await desktop.next("pairing");
  expect(shown).toBe(request.code);
  await desktop.ask(`approve ${request.request_id}`, "answered");
  const item = menu.getByRole("button", { name: /Local site/ });
  await expect(item).toBeEnabled({ timeout: 15_000 });
  await item.click();
  await expect(page.locator("#user")).toHaveValue("sean@example.com");
  await expect(page.locator("#pass")).toHaveValue("correct-horse-1");
  await page.close();
});

