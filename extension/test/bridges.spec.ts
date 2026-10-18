// Bridges through the extension end to end (docs/plan/23-bridges.md): the
// unpacked build in a real Chromium, the real native host, the real
// desktop endpoint and core over a throwaway vault, and local fixture
// pages. Per-site fill fixes set from the popup and kept on the logins;
// "never fill here" holding in the menu, the popup and the core; the
// federated sign-in map from a "Sign in with …" click and the provider's
// own sign-in page; the security center's checked change-password link and
// a curated one; and an alias made for a sign-up form through the `Use`
// gate. The provider's and GitHub's pages are answered by the suite
// itself (and their names resolve here), so nothing reaches the internet.
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
const at = (host: string, path: string) => `http://${host}:${port}${path}`;

test.describe.configure({ mode: "serial" });
test.setTimeout(60_000);

const page = (title: string, body = "") => `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>${title}</title></head><body><h1>${title}</h1>${body}</body></html>`;

test.beforeAll(async () => {
  served = await serve();
  port = served.port;
  scratch = await mkdtemp(join(tmpdir(), "lockdown-br-"));
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
      // The suite answers the provider's and GitHub's pages itself (below);
      // a request that slips past (a tab the extension opens can start
      // before the suite's routes reach it) finds no such host and no
      // proxy, so nothing leaves this machine.
      "--no-proxy-server",
      "--host-resolver-rules=MAP *.test 127.0.0.1, MAP accounts.google.com ~NOTFOUND, MAP github.com ~NOTFOUND",
    ],
  });
  await ctx.route("https://accounts.google.com/**", (r) => r.fulfill({ contentType: "text/html", body: page("Sign in with Google", "<p>Choose an account</p>") }));
  await ctx.route("https://github.com/**", (r) => {
    const path = new URL(r.request().url()).pathname;
    return r.fulfill({
      contentType: "text/html",
      body: path === "/login" ? page("Sign in to GitHub", '<form><input id="user" name="login" autocomplete="username"><input id="pass" name="password" type="password"></form>') : page("Security"),
    });
  });
  const worker = ctx.serviceWorkers()[0] ?? (await ctx.waitForEvent("serviceworker"));
  expect(new URL(worker.url()).host).toBe(DEV_ID);
  // Pair this browser, as a person would: compare the codes, approve.
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

async function addLogin(url: string, user: string, pw: string, title: string): Promise<string> {
  const made = await desktop.ask(`login ${url} ${user} ${pw} ${title}`, "login");
  expect(made.id, `login for ${url}`).toBeTruthy();
  return String(made.id);
}

async function tabOf(p: Page): Promise<number> {
  const worker = ctx.serviceWorkers()[0];
  return worker.evaluate(async (url) => {
    const tabs = await chrome.tabs.query({});
    return tabs.find((t) => t.url === url)?.id as number;
  }, p.url());
}

/** Whether a tab was opened at `url` (loaded or not: GitHub's name
 *  resolves nowhere here). */
async function opened(url: string): Promise<boolean> {
  return ctx.serviceWorkers()[0].evaluate(async (want) => (await chrome.tabs.query({})).some((t) => t.url === want || t.pendingUrl === want), url);
}

async function popupFor(p: Page): Promise<Page> {
  const pop = await ctx.newPage();
  await pop.goto(`chrome-extension://${DEV_ID}/popup.html?tab=${await tabOf(p)}`);
  return pop;
}

function menuFrame(p: Page): Frame | undefined {
  return p.frames().find((x) => x.url().startsWith(`chrome-extension://${DEV_ID}/frame.html#menu:`));
}

async function lockdownMenu(p: Page): Promise<Frame> {
  const deadline = Date.now() + 10_000;
  for (;;) {
    const f = menuFrame(p);
    if (f) {
      await f.waitForLoadState();
      return f;
    }
    if (Date.now() > deadline) throw new Error(`no Lockdown menu on ${p.url()}`);
    await p.waitForTimeout(50);
  }
}

/** Focuses a field afresh and gives the extension time to show a menu. */
async function focusAgain(p: Page, selector: string) {
  await p.locator("h1").click();
  await p.focus(selector);
  await p.waitForTimeout(1_000);
}

interface MapLogin {
  item_id: string;
  title: string;
}

interface FederationMap {
  providers: { id: string; sites: MapLogin[] }[];
  pending: { provider: string; host: string; confirmed: boolean }[];
}

async function federation(): Promise<FederationMap> {
  return (await desktop.ask("federation", "federation")).map as FederationMap;
}

test("a fill fix set in the popup names the field the page gets wrong, and is kept on the login", async () => {
  const item = await addLogin("http://fixsite.test", "fixer", "fix-pass-6", "Fix site");
  const p = await ctx.newPage();
  await p.goto(at("fixsite.test", "/ext/fix.html"));
  // "Member number" says nothing of a username: no menu.
  await focusAgain(p, "#member");
  expect(menuFrame(p)).toBeUndefined();

  const pop = await popupFor(p);
  await pop.getByText("Fix this page").click();
  const fields = pop.getByRole("list", { name: "Fields on this page" });
  await expect(fields.getByRole("listitem")).toHaveCount(2);
  await expect(fields).toContainText("Member number");
  await fields.getByRole("combobox", { name: /Member number/ }).selectOption("username");
  await pop.getByRole("button", { name: "Save fix" }).click();
  await expect(pop.getByRole("status")).toContainText("Kept the fix with 1 login for fixsite.test");
  await pop.close();

  // The page now offers and fills the login into the right fields.
  await focusAgain(p, "#member");
  const menu = await lockdownMenu(p);
  await menu.getByRole("button", { name: /Fix site/ }).click();
  await expect(p.locator("#member")).toHaveValue("fixer");
  await expect(p.locator("#pin")).toHaveValue("fix-pass-6");

  // It lives on the login, where sync and history carry it.
  const kept = (await desktop.ask(`fix ${item}`, "fix")).fix as { never: boolean; fields: { match: { id?: string; name?: string }; role: string }[] };
  expect(kept.never).toBe(false);
  expect(kept.fields).toEqual([{ match: expect.objectContaining({ id: "member", name: "member" }), role: "username" }]);
  await p.close();
});

test("never fill here holds in the menu, the popup and the core", async () => {
  const p = await ctx.newPage();
  await p.goto(at("fixsite.test", "/ext/fix.html"));
  const pop = await popupFor(p);
  await pop.getByText("Fix this page").click();
  await pop.getByLabel("Never fill on fixsite.test").check();
  await pop.getByRole("button", { name: "Save fix" }).click();
  await expect(pop.getByRole("status")).toContainText("Kept the fix");
  // The popup's own Fill is refused, with the reason.
  await pop.getByRole("button", { name: "Fill" }).click();
  await expect(pop.getByRole("status")).toContainText("You told Lockdown never to fill on fixsite.test");
  await pop.close();
  // No menu on the page, even on the field the fix named.
  await focusAgain(p, "#member");
  expect(menuFrame(p)).toBeUndefined();
  await focusAgain(p, "#pin");
  expect(menuFrame(p)).toBeUndefined();
  await expect(p.locator("#pin")).toHaveValue("");
  await p.close();
});

test("a Sign in with Google click, then Google's own sign-in page, puts the login on the map", async () => {
  const item = await addLogin("http://fedsite.test", "fed@example.com", "fed-pass-7", "Fed shop");
  const p = await ctx.newPage();
  await p.goto(at("fedsite.test", "/ext/federated.html"));
  await p.click("#google");
  await p.waitForURL(/accounts\.google\.com/);
  await expect(p.locator("h1")).toHaveText("Sign in with Google");
  await expect
    .poll(async () => (await federation()).providers.find((x) => x.id === "google")?.sites.map((s) => s.item_id) ?? [])
    .toEqual([item]);
  // The click's own question was answered by the provider's page.
  expect((await federation()).pending.filter((x) => x.host.startsWith("fedsite.test"))).toEqual([]);
  await p.close();
});

test("a click alone waits for the person's yes, and a page's own click counts for nothing", async () => {
  const p = await ctx.newPage();
  await p.goto(at("fed2.test", "/ext/federated.html"));
  // The page "clicks" its Microsoft button itself: not a person.
  await p.evaluate(() => (document.getElementById("microsoft") as HTMLButtonElement).click());
  await p.click("#apple");
  await expect.poll(async () => (await federation()).pending.map((x) => `${x.provider}@${x.host.split(":")[0]}:${x.confirmed}`)).toEqual(["apple@fed2.test:false"]);
  expect((await federation()).pending.some((x) => x.provider === "microsoft")).toBe(false);
  await p.close();
});

test("a provider page that returns to another site confirms nothing", async () => {
  const p = await ctx.newPage();
  await p.goto(at("fed3.test", `/ext/federated.html?return=${encodeURIComponent("https://elsewhere.test/cb")}`));
  await addLogin("http://fed3.test", "three", "fed-pass-8", "Fed three");
  await p.click("#google");
  await p.waitForURL(/accounts\.google\.com/);
  await p.waitForTimeout(1_000);
  const map = await federation();
  expect(map.providers.find((x) => x.id === "google")?.sites.map((s) => s.title)).toEqual(["Fed shop"]);
  // The click is still a question for the person, never a confirmed one.
  expect(map.pending.filter((x) => x.provider === "google").map((x) => `${x.host.split(":")[0]}:${x.confirmed}`)).toEqual(["fed3.test:false"]);
  await p.close();
});

test("the popup's change-password link is the one the security center checked", async () => {
  await desktop.ask("change-check on", "change_check");
  await addLogin(`http://127.0.0.1:${port}`, "changer@example.com", "change-pass-9", "Changer");
  const p = await ctx.newPage();
  await p.goto(at("127.0.0.1", "/ext/login.html"));
  const pop = await popupFor(p);
  const row = pop.getByRole("listitem").filter({ hasText: "Changer" });
  const opened = ctx.waitForEvent("page");
  await row.getByRole("button", { name: "Change password" }).click();
  const change = await opened;
  // The app asked the site, found its well-known page, and opened it.
  await change.waitForURL(/\/ext\/change\.html$/);
  await expect(pop.getByRole("status")).toContainText("Opened the change-password page for Changer");
  await change.close();
  await pop.close();
  await p.close();
});

test("a site without a well-known page gets the curated one from the signed data", async () => {
  // Off, the app asks no site; GitHub's curated page comes from the data.
  await desktop.ask("change-check off", "change_check");
  await addLogin("https://github.com", "octo", "gh-pass-10", "GitHub");
  const p = await ctx.newPage();
  await p.goto("https://github.com/login");
  const pop = await popupFor(p);
  await pop.getByRole("listitem").filter({ hasText: "GitHub" }).getByRole("button", { name: "Change password" }).click();
  await expect(pop.getByRole("status")).toContainText("Opened the page where github.com changes passwords");
  // The tab the extension opened, as the browser has it.
  await expect.poll(async () => opened("https://github.com/settings/security")).toBe(true);
  await pop.close();
  await p.close();
});

test("an alias is made for a sign-up form through the Use gate and put in its email field", async () => {
  const service = await desktop.ask(`alias-service simple_login http://127.0.0.1:${port}`, "alias_service");
  expect(service.id).toBeTruthy();
  const p = await ctx.newPage();
  await p.goto(at("newshop.test", "/ext/signup-email.html"));
  const pop = await popupFor(p);
  await expect(pop.getByRole("heading", { name: "Hide my email" })).toBeVisible();
  const checked = desktop.next("platform_check");
  await pop.getByRole("button", { name: "New alias from SimpleLogin" }).click();
  // Using the service's key is a use of a crown jewel: the check is asked.
  expect(String((await checked).reason)).toContain("alias service");
  await expect(pop.getByRole("status")).toContainText("and put it in the email field");
  await expect(p.locator("#email")).toHaveValue(/^fixture\.1@aliases\.test$/);
  expect(served.aliasCalls).toEqual([{ hostname: "newshop.test", key: "fixture-alias-key" }]);
  const overview = (await desktop.ask("aliases", "aliases")).overview as { configured: { made: number }[] };
  expect(overview.configured[0].made).toBe(1);

  // A no at the check makes nothing and asks the service nothing.
  await desktop.ask("platform no", "platform");
  await pop.getByRole("button", { name: "New alias from SimpleLogin" }).click();
  await desktop.next("platform_check");
  await expect(pop.getByRole("status")).toContainText("didn't confirm, so nothing happened");
  expect(served.aliasCalls).toHaveLength(1);
  await desktop.ask("platform yes", "platform");
  await pop.close();
  await p.close();
});
