// Passkeys through the extension end to end (docs/plan/19-passkeys.md §8):
// the unpacked build in a real Chromium, the real native host, the real
// desktop endpoint and core over a throwaway vault, and a relying party
// that is a real one (test/fixtures/ext/webauthn.html): it parses the
// attestation, keeps the COSE key and verifies every assertion with
// WebCrypto. The desktop runs with a stand-in platform check, so user
// verification is seen happening.
import { chromium, expect, test, type BrowserContext, type CDPSession, type Frame, type Page, type Worker } from "@playwright/test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { Desktop } from "./desktop";
import { serve, type Served } from "./server";

const DEV_ID = "dbdblnolpiehhlabpmhobmkbjipmebpe";
const LOCKDOWN_AAGUID = "07017dfd27174c9792c2b3d6513f46ce";
const dist = fileURLToPath(new URL("../dist/chromium", import.meta.url));

let served: Served;
let desktop: Desktop;
let ctx: BrowserContext;
let worker: Worker;
let scratch: string;
let items: Record<string, string>;
let port: number;
const rp = (query = "") => `http://localhost:${port}/ext/webauthn.html${query}`;

test.describe.configure({ mode: "serial" });
test.setTimeout(60_000);

test.beforeAll(async () => {
  served = await serve();
  port = served.port;
  scratch = await mkdtemp(join(tmpdir(), "lockdown-pk-"));
  const profile = join(scratch, "profile");
  desktop = Desktop.start(join(scratch, "vault"), profile, ["--platform-check"]);
  const ready = await desktop.ready;
  expect(ready.listening, String(ready.problem)).toBe(true);
  items = ready.items as Record<string, string>;
  ctx = await chromium.launchPersistentContext(profile, {
    channel: "chromium",
    headless: true,
    viewport: { width: 1000, height: 760 },
    env: { ...process.env, LOCKDOWN_DATA: String(ready.data) },
    args: [`--disable-extensions-except=${dist}`, `--load-extension=${dist}`],
  });
  worker = ctx.serviceWorkers()[0] ?? (await ctx.waitForEvent("serviceworker"));
  // Pair this browser, as a person would: compare the codes, approve.
  const p = await ctx.newPage();
  await p.goto(`chrome-extension://${DEV_ID}/popup.html`);
  await p.getByRole("button", { name: "Pair with Lockdown" }).click({ timeout: 30_000 });
  const request = await desktop.next("pairing", 30_000);
  await desktop.ask(`approve ${request.request_id}`, "answered");
  // Pairing went through the Approve gate, which here is the stand-in check.
  expect(String((await desktop.next("platform_check")).reason)).toContain("connect this browser");
  await expect(p.getByRole("searchbox", { name: "Search Lockdown" })).toBeVisible({ timeout: 10_000 });
  await p.close();
});

test.afterAll(async () => {
  await ctx?.close();
  desktop?.stop();
  await served?.close();
  if (scratch) await rm(scratch, { recursive: true, force: true });
});

async function lockdownFrame(page: Page, kind: "passkey" | "menu"): Promise<Frame> {
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

async function last(page: Page): Promise<Record<string, unknown>> {
  return page.evaluate(() => (window as unknown as { rp: { last: Record<string, unknown> } }).rp.last);
}

async function passkeysOn(item: string): Promise<{ user_name: string; hidden: unknown; provider: string; last_used: number | null; credential_id: string }[]> {
  return (await desktop.ask(`passkeys ${item}`, "passkeys")).passkeys as never;
}

test("a passkey is made in Lockdown from the sheet, on the login it belongs to, and the RP verifies it", async () => {
  const page = await ctx.newPage();
  await page.goto(rp("?uv=preferred"));
  // The wrapper is installed and marked for other managers to chain onto.
  const mark = await page.evaluate(() => {
    const f = navigator.credentials.get as unknown as Record<symbol, { provider: string; next: unknown }>;
    const m = f[Symbol.for("webauthn.provider.override")];
    return m ? { provider: m.provider, chained: typeof m.next === "function" } : null;
  });
  expect(mark).toEqual({ provider: "Lockdown", chained: true });
  await page.click("#register");
  const sheet = await lockdownFrame(page, "passkey");
  await expect(sheet.locator("body")).toContainText("Save a passkey for Fixture RP in Lockdown?");
  await expect(sheet.locator("body")).toContainText("in your login “Local site”");
  const checked = desktop.next("platform_check");
  await sheet.getByRole("button", { name: "Save passkey" }).click();
  // "preferred", and this machine has a platform check: it is asked.
  expect(String((await checked).reason)).toContain("save a passkey for localhost");
  await expect(page.locator("#status")).toHaveText("registered", { timeout: 15_000 });
  const made = await last(page);
  expect(made).toMatchObject({ aaguid: LOCKDOWN_AAGUID, up: true, uv: true, be: true, bs: true, count: 0, alg: -7, rk: true, attachment: "platform" });
  const views = await passkeysOn(items.local);
  expect(views).toHaveLength(1);
  expect(views[0]).toMatchObject({ user_name: "sean@example.com", provider: "Lockdown", hidden: null });
  // The page never reached into the sheet.
  expect(await page.evaluate(() => document.querySelectorAll("iframe").length)).toBe(0);
  await page.close();
});

test("a sign-in takes the second deliberate action, and the RP verifies the signature", async () => {
  const page = await ctx.newPage();
  await page.goto(rp("?uv=discouraged"));
  await page.click("#signin");
  const sheet = await lockdownFrame(page, "passkey");
  await expect(sheet.locator("body")).toContainText("Sign in to localhost with a passkey");
  await sheet.getByRole("button", { name: /Local site/ }).click();
  await expect(sheet.locator("body")).toContainText("Sign in to localhost as sean@example.com?");
  // One click signs nothing.
  await expect(page.locator("#status")).toHaveText("signing in");
  await sheet.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.locator("#status")).toHaveText("signed in", { timeout: 15_000 });
  expect(await last(page)).toMatchObject({ up: true, uv: false, be: true, bs: true, count: 0 });
  const views = await passkeysOn(items.local);
  expect(views[0].last_used).not.toBeNull();
  await page.close();
});

test("conditional mediation: the passkey is listed with the logins in the inline menu", async () => {
  const page = await ctx.newPage();
  await page.goto(rp("?conditional=1&uv=discouraged"));
  await expect(page.locator("#status")).toHaveText("waiting for a passkey");
  expect(await page.evaluate(() => (window as unknown as { rp: { conditional: boolean } }).rp.conditional)).toBe(true);
  await page.focus("#user");
  const menu = await lockdownFrame(page, "menu");
  await expect(menu.getByRole("button", { name: /Passkey · sean@example.com/ })).toBeVisible();
  await expect(menu.getByRole("button", { name: /Local site/ })).toHaveCount(2);
  await menu.getByRole("button", { name: /Passkey · sean@example.com/ }).click();
  await expect(menu.locator("body")).toContainText("Sign in to localhost as sean@example.com?");
  await menu.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(page.locator("#status")).toHaveText("signed in", { timeout: 15_000 });
  expect(await last(page)).toMatchObject({ up: true, be: true, count: 0 });
  await page.close();
});

test("user verification required: the platform check runs, and a no from it signs nothing", async () => {
  const page = await ctx.newPage();
  await page.goto(rp("?uv=required"));
  await page.click("#signin");
  let sheet = await lockdownFrame(page, "passkey");
  await sheet.getByRole("button", { name: /Local site/ }).click();
  const checked = desktop.next("platform_check");
  await sheet.getByRole("button", { name: "Sign in", exact: true }).click();
  expect(String((await checked).reason)).toContain("sign in to localhost with your passkey");
  await expect(page.locator("#status")).toHaveText("signed in", { timeout: 15_000 });
  expect(await last(page)).toMatchObject({ uv: true });

  await desktop.ask("platform no", "platform");
  await page.click("#signin");
  sheet = await lockdownFrame(page, "passkey");
  await sheet.getByRole("button", { name: /Local site/ }).click();
  await sheet.getByRole("button", { name: "Sign in", exact: true }).click();
  await expect(sheet.getByRole("alert")).toContainText("didn't confirm");
  await sheet.getByRole("button", { name: "OK", exact: true }).click();
  await expect(page.locator("#status")).toHaveText("error: NotAllowedError", { timeout: 10_000 });
  await desktop.ask("platform yes", "platform");
  await page.close();
});

test("red team: a page that covers the sheet cannot click through it", async () => {
  const page = await ctx.newPage();
  await page.goto(rp("?cover=1&uv=discouraged"));
  await page.click("#signin");
  const sheet = await lockdownFrame(page, "passkey");
  await page.waitForSelector("#decoy");
  // IntersectionObserver v2 sees the decoy over the sheet: the choice stays
  // disabled, and nothing is signed.
  await expect(sheet.getByRole("button", { name: /Local site/ })).toBeDisabled({ timeout: 5_000 });
  await page.mouse.click(500, 40);
  await expect(page.locator("#status")).toHaveText("signing in");
  await page.close();
});

test("an excluded credential is refused after the person sees it: InvalidStateError", async () => {
  const page = await ctx.newPage();
  await page.goto(rp("?uv=discouraged"));
  await page.click("#register");
  const sheet = await lockdownFrame(page, "passkey");
  await expect(sheet.getByRole("alert")).toContainText("already holds a passkey for this account");
  await sheet.getByRole("button", { name: "OK", exact: true }).click();
  await expect(page.locator("#status")).toHaveText("error: InvalidStateError", { timeout: 10_000 });
  await page.close();
});

test("Signals hide the passkey without deleting it, and rename it; Undo brings it back", async () => {
  const page = await ctx.newPage();
  await page.goto(rp());
  const credential = (await desktop.ask(`passkeys ${items.local}`, "passkeys")).passkeys[0].credential_id as string;
  // The RP's user id is "user-sean-1", base64url.
  await page.evaluate(() =>
    (PublicKeyCredential as unknown as { signalCurrentUserDetails(o: unknown): Promise<void> }).signalCurrentUserDetails({
      rpId: "localhost",
      userId: "dXNlci1zZWFuLTE",
      name: "sean.renamed@example.com",
      displayName: "Sean R",
    }),
  );
  await expect.poll(async () => (await passkeysOn(items.local))[0].user_name).toBe("sean.renamed@example.com");
  await page.evaluate(
    (cred) => (PublicKeyCredential as unknown as { signalUnknownCredential(o: unknown): Promise<void> }).signalUnknownCredential({ rpId: "localhost", credentialId: cred }),
    credential,
  );
  await expect.poll(async () => (await passkeysOn(items.local))[0].hidden).not.toBeNull();
  expect(await passkeysOn(items.local)).toHaveLength(1);
  // Hidden: Lockdown offers nothing for the site, so the request goes to
  // the browser's own (which, here, has no authenticator to offer).
  await page.evaluate(() => {
    const c = new AbortController();
    (window as unknown as { abortGet: AbortController }).abortGet = c;
    void navigator.credentials
      .get({ signal: c.signal, publicKey: { challenge: new Uint8Array(16), rpId: "localhost", timeout: 30_000 } })
      .catch(() => undefined);
  });
  await page.waitForTimeout(800);
  expect(page.frames().some((f) => f.url().includes("frame.html#passkey:"))).toBe(false);
  await page.evaluate(() => (window as unknown as { abortGet: AbortController }).abortGet.abort());
  expect((await desktop.ask(`restore ${items.local} ${credential}`, "restored")).ok).toBe(true);
  expect((await passkeysOn(items.local))[0].hidden).toBeNull();
  await page.close();
});

test("'Use another device or security key' hands the request to the browser's own authenticator", async () => {
  const page = await ctx.newPage();
  const cdp: CDPSession = await ctx.newCDPSession(page);
  await cdp.send("WebAuthn.enable");
  const { authenticatorId } = await cdp.send("WebAuthn.addVirtualAuthenticator", {
    options: { protocol: "ctap2", transport: "usb", hasResidentKey: true, hasUserVerification: true, isUserVerified: true, automaticPresenceSimulation: true },
  });
  await page.goto(rp("?uv=discouraged"));
  await page.evaluate(() => localStorage.removeItem("rp"));
  await page.reload();
  await page.click("#register");
  const sheet = await lockdownFrame(page, "passkey");
  await sheet.getByRole("button", { name: "Use another device or security key" }).click();
  await expect(page.locator("#status")).toHaveText(/registered|error/, { timeout: 15_000 });
  const made = await last(page);
  expect(made.error, String(made.message)).toBeUndefined();
  expect(made.aaguid).not.toBe(LOCKDOWN_AAGUID);
  expect(made.attachment).not.toBe("platform");
  await cdp.send("WebAuthn.removeVirtualAuthenticator", { authenticatorId });
  await page.close();
});

test("turned off in the app's Settings, the extension steps aside entirely", async () => {
  const page = await ctx.newPage();
  await page.goto(rp("?uv=discouraged"));
  const tryGet = () =>
    page.evaluate(() => {
      const c = new AbortController();
      (window as unknown as { abortGet: AbortController }).abortGet = c;
      void navigator.credentials
        .get({ signal: c.signal, publicKey: { challenge: new Uint8Array(16), rpId: "localhost", timeout: 30_000 } })
        .catch(() => undefined);
    });
  const sheetShown = () => page.frames().some((f) => f.url().includes("frame.html#passkey:"));
  await desktop.ask("browser-passkeys off", "browser_passkeys");
  await tryGet();
  await page.waitForTimeout(800);
  expect(sheetShown(), "the browser's own authenticator, not Lockdown").toBe(false);
  await page.evaluate(() => (window as unknown as { abortGet: AbortController }).abortGet.abort());
  // Back on, the same request reaches Lockdown again.
  await desktop.ask("browser-passkeys on", "browser_passkeys");
  await tryGet();
  await expect.poll(sheetShown).toBe(true);
  await page.evaluate(() => (window as unknown as { abortGet: AbortController }).abortGet.abort());
  // The page aborted: the sheet goes away with the request.
  await expect.poll(sheetShown).toBe(false);
  await page.close();
});

test("the automatic upgrade right after a password fill: silent in the page, told to the app", async () => {
  // D210 and D312 as changed by D375 (docs/plan/34-small-calls.md §1).
  await desktop.ask("upgrades", "upgrades");
  const page = await ctx.newPage();
  await page.goto(rp("?uv=discouraged"));
  // Lockdown fills the password here first: the upgrade rests on that.
  await page.focus("#user");
  const menu = await lockdownFrame(page, "menu");
  await menu.getByRole("button", { name: /Local site/ }).first().click();
  await expect(page.locator("#pass")).toHaveValue("correct-horse-1");
  // The site asks for a passkey with conditional mediation: no sheet, no
  // approval, no prompt anywhere; the RP still verifies what it got.
  await page.click("#upgrade");
  await expect(page.locator("#status")).toHaveText("registered", { timeout: 15_000 });
  expect(page.frames().some((f) => f.url().includes("frame.html#passkey:"))).toBe(false);
  const made = await last(page);
  expect(made).toMatchObject({ aaguid: LOCKDOWN_AAGUID, up: false, uv: false, be: true, bs: true });
  // The app hears of it for its toast, once, and the login says so.
  const told = (await desktop.ask("upgrades", "upgrades")).upgrades as { item_id: string; title: string; site: string }[];
  expect(told).toHaveLength(1);
  expect(told[0]).toMatchObject({ item_id: items.local, title: "Local site", site: "localhost" });
  expect((await desktop.ask("upgrades", "upgrades")).upgrades).toEqual([]);
  const views = (await passkeysOn(items.local)) as unknown as { credential_id: string; upgrade: { site: string } | null }[];
  expect(views.find((v) => v.credential_id === made.id)?.upgrade).toMatchObject({ site: "localhost" });
  await page.close();
});
