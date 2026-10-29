// The extension end to end (docs/plan/16-browser-extension.md §8): the
// unpacked MV3 build in a real Chromium, the real native host the browser
// launches, the real desktop endpoint and core over a throwaway vault, and
// local fixture pages. Pairing, every fill row, the red team against the
// real fill path, save and update, the shortcut, Basic Auth and the popup,
// and a strong password suggested under a site's own rules. Bridges (fill
// fixes, the sign-in map, change-password links, aliases) have their own
// suite: bridges.spec.ts.
import { chromium, expect, test, type BrowserContext, type Frame, type Page, type Worker } from "@playwright/test";
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
let worker: Worker;
let scratch: string;
let items: Record<string, string>;
let port: number;
const at = (host: string, path: string) => `http://${host}:${port}${path}`;

test.describe.configure({ mode: "serial" });
test.setTimeout(60_000);

test.beforeAll(async () => {
  served = await serve();
  port = served.port;
  scratch = await mkdtemp(join(tmpdir(), "lockdown-ext-"));
  const profile = join(scratch, "profile");
  desktop = Desktop.start(join(scratch, "vault"), profile);
  const ready = await desktop.ready;
  expect(ready.listening, String(ready.problem)).toBe(true);
  items = ready.items as Record<string, string>;
  ctx = await chromium.launchPersistentContext(profile, {
    channel: "chromium",
    headless: true,
    viewport: { width: 1000, height: 760 },
    env: { ...process.env, LOCKDOWN_DATA: String(ready.data) },
    args: [
      `--disable-extensions-except=${dist}`,
      `--load-extension=${dist}`,
      // No proxy: the mapped names below must land on this machine.
      "--no-proxy-server",
      "--host-resolver-rules=MAP *.test 127.0.0.1, MAP *.evil.net 127.0.0.1, MAP www.paypal.com 127.0.0.1, MAP www.1800flowers.com 127.0.0.1",
    ],
  });
  worker = ctx.serviceWorkers()[0] ?? (await ctx.waitForEvent("serviceworker"));
});

test.afterAll(async () => {
  await ctx?.close();
  desktop?.stop();
  await served?.close();
  if (scratch) await rm(scratch, { recursive: true, force: true });
});

/** Lockdown's frame inside `where` (the menu or the save prompt). The host
 *  is in a closed shadow root, so it is found as a frame, not a node. */
async function lockdownFrame(page: Page, kind: "menu" | "prompt" = "menu"): Promise<Frame> {
  const deadline = Date.now() + 10_000;
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

async function popup(query = ""): Promise<Page> {
  const p = await ctx.newPage();
  await p.goto(`chrome-extension://${DEV_ID}/popup.html${query}`);
  return p;
}

async function tabId(urlPattern: string): Promise<number> {
  return worker.evaluate(async (pattern) => {
    const [tab] = await chrome.tabs.query({ url: pattern });
    return tab.id as number;
  }, urlPattern);
}

test("the unpacked build loads with its fixed id", async () => {
  expect(new URL(worker.url()).host).toBe(DEV_ID);
});

test("nothing fills before the browser is paired", async () => {
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/login.html"));
  await page.focus("#user");
  const menu = await lockdownFrame(page);
  // The first connection waits for the host to hash the app's executable
  // (a large debug build on CI), so it gets longer than the default.
  await expect(menu.locator("body")).toContainText("Pair this browser", { timeout: 30_000 });
  await expect(page.locator("#pass")).toHaveValue("");
  await page.close();
});

test("pairing shows the same code in the browser and the app, and the app approves it", async () => {
  const p = await popup();
  await p.getByRole("button", { name: "Pair with Lockdown" }).click();
  const shown = (await p.getByLabel(/^Pairing code/).textContent())?.replace(/\s/g, "");
  const request = await desktop.next("pairing");
  expect(shown).toBe(request.code);
  expect(String(request.name)).toMatch(/on Linux|on macOS|on Windows/);
  await desktop.ask(`approve ${request.request_id}`, "answered");
  await expect(p.getByRole("searchbox", { name: "Search Lockdown" })).toBeVisible({ timeout: 10_000 });
  await p.close();
});

test("a login fills through the inline menu, decided in the core", async () => {
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/login.html"));
  await page.focus("#user");
  const menu = await lockdownFrame(page);
  const item = menu.getByRole("button", { name: /Local site/ });
  await expect(item).toBeEnabled();
  await item.click();
  await expect(page.locator("#user")).toHaveValue("sean@example.com");
  await expect(page.locator("#pass")).toHaveValue("correct-horse-1");
  // The page never reached into the menu: its document is the extension's.
  const pageSees = await page.evaluate(() => document.querySelectorAll("iframe").length);
  expect(pageSees).toBe(0);
  await page.close();
});

test("red team: a field in a cross-origin frame asks before it fills", async () => {
  const page = await ctx.newPage();
  await page.goto(at("localhost", `/ext/framed.html?frame=${encodeURIComponent(`http://127.0.0.1:${port}`)}`));
  const child = page.frame({ url: /127\.0\.0\.1.*login\.html/ })!;
  await child.waitForLoadState();
  await child.focus("#user");
  const menu = await lockdownFrame(page);
  await menu.getByRole("button", { name: /Loopback/ }).click();
  await expect(menu.locator("body")).toContainText("inside a page from localhost");
  await expect(child.locator("#pass")).toHaveValue("");
  await menu.getByRole("button", { name: "Fill", exact: true }).click();
  await expect(child.locator("#pass")).toHaveValue("framed-pass-2");
  await page.close();
});

for (const kase of ["opacity-overlay", "pointer-events-none-overlay"]) {
  test(`red team: a field under a ${kase} is refused on the real fill path`, async () => {
    const page = await ctx.newPage();
    await page.goto(at("localhost", `/ext/login.html?case=${kase}`));
    await page.focus("#user");
    const menu = await lockdownFrame(page);
    await menu.getByRole("button", { name: /Local site/ }).click();
    await expect(menu.locator("body")).toContainText("clickjacking");
    await expect(page.locator("#user")).toHaveValue("");
    await expect(page.locator("#pass")).toHaveValue("");
    await page.close();
  });
}

test("red team: a page that covers Lockdown's menu cannot click through it", async () => {
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/login.html?case=cover-menu"));
  await page.focus("#user");
  const menu = await lockdownFrame(page);
  await page.waitForSelector("#decoy");
  const item = menu.getByRole("button", { name: /Local site/ });
  // IntersectionObserver v2 sees the decoy over the frame: the choice stays
  // disabled, and nothing reaches the fields.
  await expect(item).toBeDisabled({ timeout: 5_000 });
  await page.mouse.click(10, 10);
  await expect(page.locator("#pass")).toHaveValue("");
  await page.close();
});

test("red team: a lookalike page gets a hard stop, in the menu and the popup", async () => {
  const page = await ctx.newPage();
  await page.goto(at("paypal.com.evil.net", "/ext/login.html"));
  await page.focus("#user");
  const menu = await lockdownFrame(page);
  const stop = menu.getByRole("alert");
  await expect(stop).toHaveText(/^Stop: \S+ isn't www\.paypal\.com\. It looks like a site you have a login for, so Lockdown won't fill here\.$/);
  // The whole sentence shows (design review R3): the frame takes its larger
  // height for a stop, and the stop's box ends inside it with nothing cut.
  await expect
    .poll(() =>
      stop.evaluate((el) => {
        const r = el.getBoundingClientRect();
        return r.top >= 0 && r.bottom <= innerHeight && r.right <= innerWidth && el.scrollHeight <= el.clientHeight && el.scrollWidth <= el.clientWidth;
      }),
    )
    .toBe(true);
  await expect(menu.getByRole("button", { name: /PayPal/ })).toHaveCount(0);
  const tab = await tabId("*://paypal.com.evil.net/*");
  const p = await popup(`?tab=${tab}`);
  await expect(p.getByRole("alert")).toContainText("www.paypal.com");
  await p.getByRole("searchbox").fill("paypal");
  const copy = p.getByRole("button", { name: "Copy password" });
  await expect(copy).toHaveCount(1);
  await copy.click();
  await expect(p.getByRole("button", { name: "Copy anyway" })).toBeVisible();
  const copied = desktop.next("copied");
  await p.getByRole("button", { name: "Copy anyway" }).click();
  expect((await copied).len).toBe("paypal-pass-3".length);
  await p.close();
  await page.close();
});

test("a one-time code fills after the second action, single field and split boxes", async () => {
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/code.html"));
  await page.focus("#code");
  let menu = await lockdownFrame(page);
  await menu.getByRole("button", { name: /Local site/ }).click();
  await expect(menu.locator("body")).toContainText("deliberate");
  await expect(page.locator("#code")).toHaveValue("");
  await menu.getByRole("button", { name: "Fill code" }).click();
  const code = await desktop.ask(`code ${items.local}`, "code");
  const filled = await page.locator("#code").inputValue();
  expect([code.code, code.next]).toContain(filled);

  await page.goto(at("localhost", "/ext/code.html?split=1"));
  await page.focus("#d0");
  menu = await lockdownFrame(page);
  await menu.getByRole("button", { name: /Local site/ }).click();
  await menu.getByRole("button", { name: "Fill code" }).click();
  await expect(page.locator("#d5")).not.toHaveValue("");
  const digits = await page.$$eval("#fields input", (els) => els.map((e) => (e as HTMLInputElement).value).join(""));
  const again = await desktop.ask(`code ${items.local}`, "code");
  expect([again.code, again.next, code.code, code.next]).toContain(digits);
  await page.close();
});

test("a card fills only after the second action", async () => {
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/checkout.html"));
  await page.focus("#number");
  const menu = await lockdownFrame(page);
  await menu.getByRole("button", { name: /Everyday card/ }).click();
  await expect(menu.locator("body")).toContainText("deliberate");
  await expect(page.locator("#number")).toHaveValue("");
  await menu.getByRole("button", { name: "Fill card" }).click();
  await expect(page.locator("#number")).toHaveValue("4242424242424242");
  await expect(page.locator("#name")).toHaveValue("Sean Sands");
  await expect(page.locator("#exp")).toHaveValue("08/30");
  await expect(page.locator("#cvc")).toHaveValue("123");
  await page.close();
});

test("an identity fills an address form", async () => {
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/address.html"));
  await page.focus("#given");
  const menu = await lockdownFrame(page);
  await menu.getByRole("button", { name: /Home/ }).click();
  await expect(page.locator("#given")).toHaveValue("Sean");
  await expect(page.locator("#family")).toHaveValue("Sands");
  await expect(page.locator("#email")).toHaveValue("sean@example.com");
  await expect(page.locator("#city")).toHaveValue("Springfield");
  await expect(page.locator("#zip")).toHaveValue("12345");
  await page.close();
});

test("the fill shortcut fills the best login for the focused field", async () => {
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/login.html"));
  await page.bringToFront();
  await page.focus("#pass");
  const answer = await worker.evaluate(() => (globalThis as unknown as { lockdownCommand(c: string): Promise<unknown> }).lockdownCommand("fill-login"));
  expect(answer).toEqual({ done: true });
  await expect(page.locator("#pass")).toHaveValue("correct-horse-1");
  await expect(page.locator("#user")).toHaveValue("sean@example.com");
  await page.close();
});

test("HTTP Basic Auth is answered for the one login saved for that host", async () => {
  const page = await ctx.newPage();
  await page.goto(at("basic.test", "/basic/"));
  await expect(page.locator("#in")).toHaveText("Welcome, basic-user");
  await page.close();
});

test("the popup searches summaries and copies through the app", async () => {
  const p = await popup();
  await p.getByRole("searchbox").fill("local");
  const results = p.getByRole("list", { name: "Results" });
  await expect(results.getByRole("listitem")).toHaveCount(1);
  await expect(results).toContainText("Local site");
  await expect(results).not.toContainText("correct-horse-1");
  const copied = desktop.next("copied");
  await results.getByRole("button", { name: "Copy password" }).click();
  expect((await copied).len).toBe("correct-horse-1".length);
  await expect(p.getByRole("status")).toContainText("clears it in");
  await p.close();
});

test("the popup fills the page in front", async () => {
  // Its change-password link is the security center's checked one:
  // bridges.spec.ts.
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/login.html"));
  const tab = await tabId(`*://localhost/*`);
  const p = await popup(`?tab=${tab}`);
  await expect(p.getByRole("heading", { name: /For localhost/ })).toBeVisible();
  await p.getByRole("button", { name: "Fill" }).click();
  await expect(page.locator("#pass")).toHaveValue("correct-horse-1");
  await p.close();
  await page.close();
});

// docs/plan/26-desktop-finish.md §1: the popup asks the core's one index,
// forgives a typo and says so, and shows a code in place through the app.
test("the popup's quick access forgives a typo and shows a code, never its set-up", async () => {
  const made = await desktop.ask("add-code otpauth://totp/Shop:me?secret=JBSWY3DPEHPK3PXP&issuer=Shop Shop code", "code_added");
  expect(made.id).toBeTruthy();
  const p = await popup();
  await expect(p.getByRole("searchbox", { name: "Search Lockdown" })).toBeFocused();
  await p.getByRole("searchbox").fill("shop cdoe");
  const results = p.getByRole("list", { name: "Results" });
  await expect(results.getByRole("listitem")).toHaveCount(1);
  await expect(results).toContainText("Close match");
  await results.getByRole("button", { name: "Show code for Shop code" }).click();
  await expect(p.getByLabel(/^Code \d \d \d \d \d \d$/)).toBeVisible();
  await expect(p.locator("body")).not.toContainText("JBSWY3DP");
  await p.close();
});

// §1: "Fill in the browser" from the desktop's quick access puts that login
// first on its site, here and in the popup.
test("a login quick access asked to fill comes first on its site", async () => {
  const started = await desktop.ask(`quick-fill ${items.local}`, "quick_fill");
  expect((started.started as { host: string }).host).toBe("localhost");
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/login.html"));
  const tab = await tabId(`*://localhost/*`);
  const p = await popup(`?tab=${tab}`);
  const row = p.getByRole("list", { name: "Logins for localhost" }).getByRole("listitem").first();
  await expect(row).toContainText("Local site");
  await expect(row).toContainText("From quick access");
  await p.close();
  await page.close();
});

// §3: a TV's code read by the camera, the person's yes in the app, then the
// real activation page: the login picked is offered first and fills there.
test("a TV sign-in opens the real page and the extension fills the login picked", async () => {
  await ctx.route("https://www.youtube.com/**", (r) =>
    r.fulfill({
      contentType: "text/html",
      body: '<!doctype html><title>Activate</title><h1>Enter the code</h1><form><input id="user" name="email" autocomplete="username"><input id="pass" type="password" name="password"></form>',
    }),
  );
  const other = await desktop.ask("login https://www.youtube.com other@example.com other-pass-8 YouTube other", "login");
  const tv = await desktop.ask("login https://www.youtube.com tv@example.com tv-pass-9 YouTube TV", "login");
  const scanned = await desktop.ask("scan-camera https://www.youtube.com/activate?user_code=WDJB-MJHT", "scanned");
  const outcome = scanned.outcome as { decision: string; activation_id: string; accounts: { item_id: string }[]; host: string };
  expect(outcome.decision).toBe("approve");
  expect(outcome.host).toBe("www.youtube.com");
  expect(outcome.accounts.map((a) => a.item_id).sort()).toEqual([String(other.id), String(tv.id)].sort());
  const approved = await desktop.ask(`activation-approve ${outcome.activation_id} ${tv.id}`, "activation");
  const url = (approved.started as { url: string }).url;
  expect(url).toBe("https://www.youtube.com/activate?user_code=WDJB-MJHT");
  const page = await ctx.newPage();
  await page.goto(url);
  await page.focus("#user");
  const menu = await lockdownFrame(page);
  const first = menu.getByRole("button", { name: /YouTube/ }).first();
  await expect(first).toContainText("YouTube TV");
  await expect(first).toBeEnabled();
  await first.click();
  await expect(page.locator("#user")).toHaveValue("tv@example.com");
  await expect(page.locator("#pass")).toHaveValue("tv-pass-9");
  await page.close();
});

test("a locked app says so instead of filling", async () => {
  await desktop.ask("lock", "locked");
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/login.html"));
  await page.focus("#user");
  const menu = await lockdownFrame(page);
  await expect(menu.locator("body")).toContainText("locked");
  await desktop.ask("unlock", "unlocked");
  await page.close();
});

test("a new sign-up is offered for saving, and saved through the app", async () => {
  const page = await ctx.newPage();
  await page.goto(at("newsite.test", "/ext/signup.html"));
  await page.fill("#user", "newbie");
  await page.fill("#pass", "brand-new-pass-5");
  await page.fill("#again", "brand-new-pass-5");
  await page.click("#go");
  await page.waitForURL(/welcome\.html/);
  const prompt = await lockdownFrame(page, "prompt");
  await expect(prompt.locator("body")).toContainText("Save this login for newsite.test");
  await prompt.getByRole("button", { name: "Save" }).click();
  await expect.poll(async () => {
    const list = await desktop.ask("list", "list");
    return (list.items as { username: string; site: string }[]).find((i) => i.username === "newbie")?.site;
  }).toBe(`http://newsite.test:${port}`);
  await page.close();
});

test("a changed password is offered as an update of the saved login", async () => {
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/change.html"));
  await page.fill("#old", "correct-horse-1");
  await page.fill("#new", "correct-horse-2");
  await page.click("#go");
  await page.waitForURL(/welcome\.html/);
  const prompt = await lockdownFrame(page, "prompt");
  await expect(prompt.locator("body")).toContainText("Update the password for Local site");
  await prompt.getByRole("button", { name: "Update" }).click();
  await expect.poll(async () => (await desktop.ask(`password ${items.local}`, "password")).value).toBe("correct-horse-2");
  await page.close();
});

test("a sign-up field is offered a strong password under the site's published rules, then saved", async () => {
  const page = await ctx.newPage();
  await page.goto(at("www.1800flowers.com", "/ext/signup.html"));
  await page.fill("#user", "flower-fan");
  await page.focus("#pass");
  const menu = await lockdownFrame(page);
  const suggest = menu.getByRole("button", { name: /Use a strong password/ });
  await expect(suggest).toContainText("Follows 1800flowers.com's password rules");
  await expect(suggest).toBeEnabled();
  // Nothing is made until the pick: the fields are still empty.
  await expect(page.locator("#pass")).toHaveValue("");
  await suggest.click();
  await expect.poll(async () => (await page.locator("#pass").inputValue()).length).toBe(20);
  const made = await page.locator("#pass").inputValue();
  await expect(page.locator("#again")).toHaveValue(made);
  expect(made).toMatch(/[0-9]/);
  await page.click("#go");
  await page.waitForURL(/welcome\.html/);
  const prompt = await lockdownFrame(page, "prompt");
  await expect(prompt.locator("body")).toContainText("Save this login for www.1800flowers.com");
  await prompt.getByRole("button", { name: "Save" }).click();
  let id = "";
  await expect.poll(async () => {
    const list = await desktop.ask("list", "list");
    id = (list.items as { id: string; username: string }[]).find((i) => i.username === "flower-fan")?.id ?? "";
    return id;
  }).not.toBe("");
  expect((await desktop.ask(`password ${id}`, "password")).value).toBe(made);
  await page.close();
});

test("a sign-in field is not offered a new password", async () => {
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/login.html"));
  await page.focus("#pass");
  const menu = await lockdownFrame(page);
  await expect(menu.getByRole("button", { name: /Local site/ })).toBeVisible();
  await expect(menu.getByRole("button", { name: /Use a strong password/ })).toHaveCount(0);
  await page.close();
});
