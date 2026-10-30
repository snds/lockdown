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

// docs/plan/37-cli-popup.md §2 (review R16): where the popup can't act it
// offers Open Lockdown; with the app running, its window comes forward,
// paired or not.
test("an unpaired popup opens Lockdown, which brings the app's window forward", async () => {
  const p = await popup();
  await expect(p.getByRole("button", { name: "Pair with Lockdown" })).toBeVisible({ timeout: 30_000 });
  const shown = desktop.next("shown");
  await p.getByRole("button", { name: "Open Lockdown" }).click();
  await shown;
  await expect(p.getByRole("status")).toHaveText("Lockdown is open on this computer.");
  await p.close();
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

// Review R17 (D159, D206): while a choice waits out its 400 ms it says it is
// about to work, an underline that fills, at full colour; covered, it looks
// disabled; armed, plainly enabled. Under reduced motion the underline
// doesn't move.
for (const motion of ["no-preference", "reduce"] as const) {
  test(`a menu choice shows it is arming, then looks plainly enabled (motion: ${motion})`, async () => {
    const page = await ctx.newPage();
    await page.emulateMedia({ reducedMotion: motion });
    await page.goto(at("localhost", "/ext/login.html?case=cover-menu-briefly"));
    await page.focus("#user");
    const menu = await lockdownFrame(page);
    // Watched from inside the frame, every animation frame, until armed.
    const seen = await menu.evaluate(
      () =>
        new Promise<{ covered: boolean; arming: string[]; armed: { opacity: string; arming: boolean } | null }>((resolve) => {
          const out = { covered: false, arming: [] as string[], armed: null as { opacity: string; arming: boolean } | null }
          const end = Date.now() + 8000
          const look = () => {
            const b = document.querySelector<HTMLButtonElement>("button.item")
            if (b) {
              const opacity = getComputedStyle(b).opacity
              const after = getComputedStyle(b, "::after")
              if (b.disabled && !b.dataset.arming && opacity === "0.6") out.covered = true
              if (b.disabled && b.dataset.arming && out.covered) out.arming.push(`${opacity} ${after.animationName} ${after.content}`)
              if (!b.disabled && out.arming.length > 0) {
                out.armed = { opacity, arming: "arming" in b.dataset }
                return resolve(out)
              }
            }
            if (Date.now() > end) return resolve(out)
            requestAnimationFrame(look)
          }
          look()
        }),
    );
    expect(seen.covered).toBe(true);
    expect(seen.arming.length).toBeGreaterThan(0);
    expect(new Set(seen.arming)).toEqual(new Set([`1 ${motion === "reduce" ? "none" : "ld-arm"} ""`]));
    expect(seen.armed).toEqual({ opacity: "1", arming: false });
    await page.close();
  });
}

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
  // The fill lands after the endpoint answers; wait for it rather than read at once.
  await expect(page.locator("#code")).not.toHaveValue("");
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
  // One action per row (review R16): while the code shows, that same place
  // copies it, through the app.
  await expect(results.getByRole("button", { name: "Show code for Shop code" })).toHaveCount(0);
  const copied = desktop.next("copied");
  await results.getByRole("button", { name: "Copy code" }).click();
  expect((await copied).len).toBe(6);
  await p.close();
});

// Review R16: every result is one row, the kind's glyph, the title, one line
// under it and one action at the trailing edge, which never wraps under the
// words however long they run; text is 13 px or more.
test("every popup result has one row anatomy with its action trailing, at 13 px or more", async () => {
  await desktop.ask(
    "login http://long.test someone.with.a.long.address@example.com long-pass-6 A login whose title runs well past the width of the popup",
    "login",
  );
  const p = await popup();
  await p.getByRole("searchbox").fill("login whose title runs");
  const results = p.getByRole("list", { name: "Results" });
  const row = results.getByRole("listitem").filter({ hasText: "A login whose title runs" });
  await expect(row).toHaveCount(1);
  await expect(row.getByRole("button")).toHaveCount(1);
  const geometry = await row.evaluate((el) => {
    const box = (sel: string) => el.querySelector(sel)!.getBoundingClientRect();
    const glyph = box('[data-slot="item-media"]');
    const title = box('[data-slot="item-title"]');
    const sub = box('[data-slot="item-description"]');
    const action = box('[data-slot="item-actions"]');
    const row = el.getBoundingClientRect();
    return { glyph, title, sub, action, row };
  });
  // Glyph first, words, then the action at the trailing edge, beside the
  // title and the subtitle rather than under them, inside the row.
  expect(geometry.glyph.right).toBeLessThanOrEqual(geometry.title.left + 1);
  expect(geometry.action.left).toBeGreaterThanOrEqual(geometry.title.right - 1);
  expect(geometry.action.left).toBeGreaterThanOrEqual(geometry.sub.right - 1);
  expect(geometry.action.top).toBeLessThan(geometry.sub.bottom);
  expect(geometry.action.right).toBeLessThanOrEqual(geometry.row.right);
  // The subtitle is one line, cut short rather than wrapped.
  expect(geometry.sub.height).toBeLessThan(28);
  // Every word the popup draws, but the badges, is 13 px or more (the
  // screen-reader-only heading and the glyphs' ligatures are not drawn).
  await p.getByRole("searchbox").fill("local");
  await expect(results.getByRole("listitem").first()).toBeVisible();
  const small = await p.evaluate(() =>
    Array.from(document.querySelectorAll("main *"))
      .filter((el) => !el.closest('[data-slot="badge"], [aria-hidden="true"], .sr-only') && Array.from(el.childNodes).some((n) => n.nodeType === 3 && n.textContent!.trim()))
      .map((el) => [el.textContent!.trim(), parseFloat(getComputedStyle(el).fontSize)] as const)
      .filter(([, px]) => px < 13),
  );
  expect(small).toEqual([]);
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
  // The popup can't search while locked; it opens Lockdown to unlock there.
  const p = await popup();
  await expect(p.getByText("Lockdown is locked. Unlock it in the app to fill and search.")).toBeVisible();
  const shown = desktop.next("shown");
  await p.getByRole("button", { name: "Open Lockdown" }).click();
  await shown;
  await expect(p.getByRole("status")).toHaveText("Lockdown is open on this computer. Unlock it there to fill and search.");
  await p.close();
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

// --- one frame at a time, and covers that come and go (docs/plan/42-frame-cover.md)
//
// A page sees Lockdown's hosts arrive (top-level children at the top z-index)
// but never what is inside them. The page script below counts them, and the
// hostile ones lay a decoy over each.

/** Counts the Lockdown hosts this page is given and how many are still on it. */
async function countHosts(page: Page) {
  await page.evaluate(() => {
    const w = window as unknown as { __hosts: Element[] };
    w.__hosts = [];
    new MutationObserver((records) => {
      for (const r of records) for (const n of r.addedNodes) if (n instanceof HTMLElement && n.style.zIndex === "2147483647") w.__hosts.push(n);
    }).observe(document.documentElement, { childList: true });
  });
  return () => page.evaluate(() => {
    const hosts = (window as unknown as { __hosts: Element[] }).__hosts;
    return { added: hosts.length, onPage: hosts.filter((h) => h.isConnected).length };
  });
}

/** A hostile page (an init script, so it runs before the content script
 *  and watches the whole document): every Lockdown host it sees is covered by a see-through
 *  decoy of its own, raised over the host, that lets clicks through to it.
 *  With `uncoverAfter`, the decoy goes after that long and the page notes
 *  when (`__uncoveredAt`, wall clock). Nothing about the host's place or
 *  size changes either way. */
async function coverHosts(page: Page, uncoverAfter?: number) {
  await page.addInitScript((after) => {
    const w = window as unknown as { __uncoveredAt?: number };
    new MutationObserver((records) => {
      for (const r of records) for (const n of r.addedNodes) {
        if (!(n instanceof HTMLElement) || n.style.zIndex !== "2147483647") continue;
        const rect = n.getBoundingClientRect();
        const d = document.createElement("div");
        d.setAttribute("popover", "manual");
        d.style.cssText = `position:fixed; margin:0; inset:auto; left:${rect.left}px; top:${rect.top}px; width:${rect.width}px; height:${rect.height}px; background:#fff; opacity:0; pointer-events:none; border:0; padding:0;`;
        document.documentElement.append(d);
        d.showPopover();
        if (after !== null) {
          setTimeout(() => {
            d.remove();
            w.__uncoveredAt = Date.now();
          }, after);
        }
      }
    }).observe(document, { childList: true, subtree: true });
  }, uncoverAfter ?? null);
}

/** Signs up on `host` and lands on its welcome page, where the save prompt comes. */
async function signUp(page: Page, host: string, user: string) {
  await page.goto(at(host, "/ext/signup.html"));
  await page.fill("#user", user);
  await page.fill("#pass", "brand-new-pass-9");
  await page.fill("#again", "brand-new-pass-9");
  await page.click("#go");
  await page.waitForURL(/welcome\.html/);
}

async function savedAs(user: string): Promise<boolean> {
  const list = await desktop.ask("list", "list");
  return (list.items as { username: string }[]).some((i) => i.username === user);
}

test("two requests for the save prompt at once draw one prompt, and it leaves no copy behind", async () => {
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/welcome.html"));
  const hosts = await countHosts(page);
  const tab = await tabId(`*://localhost/ext/welcome.html`);
  // The background asks once when the offer is made and the page asks once
  // when it loads; here both land together.
  await worker.evaluate(async (id) => {
    await Promise.all([chrome.tabs.sendMessage(id, { t: "show_prompt" }, { frameId: 0 }), chrome.tabs.sendMessage(id, { t: "show_prompt" }, { frameId: 0 })]);
  }, tab);
  await expect.poll(async () => (await hosts()).added).toBeGreaterThan(0);
  // This tab has no offer, so the prompt closes itself.
  await expect.poll(async () => (await hosts()).onPage, { timeout: 5_000 }).toBe(0);
  expect((await hosts()).added).toBe(1);
  await page.close();
});

test("a field focused twice in a row gets one menu, and Escape leaves none", async () => {
  const page = await ctx.newPage();
  await page.goto(at("localhost", "/ext/login.html"));
  const hosts = await countHosts(page);
  await page.evaluate(() => {
    const u = document.getElementById("user") as HTMLInputElement;
    u.focus();
    u.blur();
    u.focus();
  });
  const menu = await lockdownFrame(page);
  await expect(menu.getByRole("button", { name: /Local site/ })).toBeEnabled();
  expect(await hosts()).toEqual({ added: 1, onPage: 1 });
  await page.keyboard.press("Escape");
  await expect.poll(async () => (await hosts()).onPage).toBe(0);
  expect((await hosts()).added).toBe(1);
  await page.close();
});

test("after a sign-up the welcome page holds one save prompt, and its Save arms", async () => {
  const page = await ctx.newPage();
  await page.addInitScript(() => {
    const w = window as unknown as { __hosts: number };
    w.__hosts = 0;
    new MutationObserver((records) => {
      for (const r of records) for (const n of r.addedNodes) if (n instanceof HTMLElement && n.style.zIndex === "2147483647") w.__hosts++;
    }).observe(document, { childList: true, subtree: true });
  });
  await signUp(page, "one-prompt.test", "single");
  const prompt = await lockdownFrame(page, "prompt");
  await expect(prompt.getByRole("button", { name: "Save" })).toBeEnabled();
  // Long enough for a second ask (the background's, the page's) to land.
  await page.waitForTimeout(1_000);
  expect(await page.evaluate(() => (window as unknown as { __hosts: number }).__hosts)).toBe(1);
  expect(page.frames().filter((f) => f.url().includes("frame.html#prompt:"))).toHaveLength(1);
  await page.close();
});

for (const [name, effect] of [
  ["fades in", `h.style.opacity = "0"; h.style.transition = "opacity 300ms"; requestAnimationFrame(() => requestAnimationFrame(() => { h.style.opacity = "1"; }));`],
  ["slides in", `h.style.transform = "translateY(12px) scale(0.96)"; h.style.transition = "transform 300ms"; requestAnimationFrame(() => requestAnimationFrame(() => { h.style.transform = "none"; }));`],
  ["is held see-through", `h.style.opacity = "0.99"; setTimeout(() => { h.style.opacity = ""; }, 700);`],
  ["is covered, then uncovered in place", `const r = h.getBoundingClientRect(); const d = document.createElement("div"); d.setAttribute("popover", "manual"); d.style.cssText = "position:fixed; margin:0; inset:auto; left:" + r.left + "px; top:" + r.top + "px; width:" + r.width + "px; height:" + r.height + "px; background:#fff; opacity:.5; border:0; padding:0;"; document.documentElement.append(d); d.showPopover(); setTimeout(() => { d.style.visibility = "hidden"; }, 700);`],
] as const) {
  test(`a menu whose host ${name} as it appears arms once it settles`, async () => {
    const page = await ctx.newPage();
    await page.goto(at("localhost", "/ext/login.html"));
    await page.evaluate((code) => {
      new MutationObserver((records) => {
        for (const r of records) for (const n of r.addedNodes) {
          if (n instanceof HTMLElement && n.style.zIndex === "2147483647") new Function("h", code)(n);
        }
      }).observe(document.documentElement, { childList: true });
    }, effect);
    await page.focus("#user");
    const menu = await lockdownFrame(page);
    await expect(menu.getByRole("button", { name: /Local site/ })).toBeEnabled({ timeout: 5_000 });
    await page.close();
  });
}

test("red team: a save prompt the page keeps covered never arms, and a click through the cover saves nothing", async () => {
  const page = await ctx.newPage();
  await coverHosts(page);
  await signUp(page, "covered-prompt.test", "covered");
  const prompt = await lockdownFrame(page, "prompt");
  const save = prompt.getByRole("button", { name: "Save" });
  await expect(save).toBeVisible();
  // Once IntersectionObserver v2 has spoken (its first report comes within
  // about 100 ms), the button looks disabled, with no arming cue...
  await expect.poll(() => save.getAttribute("data-arming")).toBeNull();
  // ...and stays that way while the cover does.
  for (let i = 0; i < 6; i++) {
    await expect(save).toBeDisabled();
    expect(await save.getAttribute("data-arming")).toBeNull();
    await page.waitForTimeout(500);
  }
  // The decoy lets the pointer through: a real click lands on the frame.
  const box = await save.boundingBox();
  if (!box) throw new Error("Save has no box");
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await page.waitForTimeout(500);
  expect(await savedAs("covered")).toBe(false);
  await expect(save).toBeDisabled();
  await page.close();
});

test("a save prompt uncovered in place re-arms after the full 400 ms, then saves", async () => {
  const page = await ctx.newPage();
  await coverHosts(page, 1_500);
  await signUp(page, "uncovered-prompt.test", "uncovered");
  const prompt = await lockdownFrame(page, "prompt");
  const save = prompt.getByRole("button", { name: "Save" });
  await expect(save).toBeVisible();
  // Watched from inside the frame, every animation frame, until it arms.
  const armedAt = await save.evaluate(
    (b: HTMLButtonElement) =>
      new Promise<number>((resolve) => {
        const look = () => (b.disabled ? requestAnimationFrame(look) : resolve(Date.now()));
        look();
      }),
  );
  const uncoveredAt = await page.evaluate(() => (window as unknown as { __uncoveredAt?: number }).__uncoveredAt);
  expect(uncoveredAt, "the page never uncovered the prompt").toBeDefined();
  expect(armedAt - (uncoveredAt as number)).toBeGreaterThanOrEqual(400);
  await save.click();
  await expect.poll(() => savedAs("uncovered")).toBe(true);
  await page.close();
});
