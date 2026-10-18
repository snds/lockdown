// The extension in Firefox (docs/plan/24-release.md §6): the dist/firefox
// build loaded into Playwright's Firefox, the real native host Firefox
// launches from its own manifest folder, the real desktop endpoint and core
// over a throwaway vault. Pairing, then a login filled through the inline
// menu, whose buttons arm on Firefox by the menu host's hit test (there is
// no IntersectionObserver v2).
//
// Playwright has no option to load an add-on into Firefox, so the suite
// starts Firefox's remote debugging server and installs dist/firefox as a
// temporary add-on over that protocol (the `installTemporaryAddon` request
// web-ext uses). The moz-extension origin is fixed by the
// `extensions.webextensions.uuids` preference so the popup has a known URL,
// and the popup is opened by the add-on itself over the same protocol.
//
// Skipped where Playwright's Firefox isn't installed, unless
// LOCKDOWN_REQUIRE_FIREFOX=1 (CI) makes that a failure.
import { expect, firefox, test, type BrowserContext, type Frame, type Page } from "@playwright/test";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { connect } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { Desktop } from "./desktop";
import { serve, type Served } from "./server";

const ADDON_ID = "lockdown@snds.design";
const UUID = "6f1d3c2e-4b5a-4e8f-9a7b-1c2d3e4f5a6b";
const ORIGIN = `moz-extension://${UUID}`;
const dist = fileURLToPath(new URL("../dist/firefox", import.meta.url));

const installed = (() => {
  try {
    return existsSync(firefox.executablePath());
  } catch {
    return false;
  }
})();

let served: Served;
let desktop: Desktop;
let ctx: BrowserContext;
let scratch: string;
let port: number;
let rdp: Rdp;

test.describe.configure({ mode: "serial" });
test.setTimeout(90_000);
test.skip(!installed && process.env.LOCKDOWN_REQUIRE_FIREFOX !== "1", "Playwright's Firefox isn't installed here");

/** One request over Firefox's remote debugging protocol: `<length>:<json>` frames. */
class Rdp {
  private buf = Buffer.alloc(0);
  private waiting: ((m: Record<string, unknown>) => void)[] = [];
  private queue: Record<string, unknown>[] = [];

  private constructor(private sock: ReturnType<typeof connect>) {
    sock.on("data", (d) => {
      this.buf = Buffer.concat([this.buf, d]);
      for (;;) {
        const colon = this.buf.indexOf(":");
        if (colon < 0) return;
        const len = Number(this.buf.subarray(0, colon).toString());
        if (this.buf.length < colon + 1 + len) return;
        const msg = JSON.parse(this.buf.subarray(colon + 1, colon + 1 + len).toString("utf8"));
        this.buf = this.buf.subarray(colon + 1 + len);
        const w = this.waiting.shift();
        if (w) w(msg);
        else this.queue.push(msg);
      }
    });
  }

  static async open(rdpPort: number): Promise<Rdp> {
    const deadline = Date.now() + 20_000;
    for (;;) {
      try {
        const sock = await new Promise<ReturnType<typeof connect>>((resolve, reject) => {
          const s = connect(rdpPort, "127.0.0.1", () => resolve(s));
          s.once("error", reject);
        });
        const rdp = new Rdp(sock);
        await rdp.next(); // the greeting
        return rdp;
      } catch (e) {
        if (Date.now() > deadline) throw e;
        await new Promise((r) => setTimeout(r, 200));
      }
    }
  }

  next(): Promise<Record<string, unknown>> {
    const q = this.queue.shift();
    if (q) return Promise.resolve(q);
    return new Promise((resolve) => this.waiting.push(resolve));
  }

  send(to: string, type: string, extra: Record<string, unknown> = {}) {
    const body = Buffer.from(JSON.stringify({ to, type, ...extra }), "utf8");
    this.sock.write(`${body.length}:`);
    this.sock.write(body);
  }

  /** Messages from `from` until `done` says stop or `ms` pass. */
  async collect(from: string, done: (m: Record<string, unknown>) => boolean, ms: number): Promise<Record<string, unknown>[]> {
    const seen: Record<string, unknown>[] = [];
    const deadline = Date.now() + ms;
    while (Date.now() < deadline) {
      const m = await Promise.race([this.next(), new Promise<null>((r) => setTimeout(() => r(null), deadline - Date.now()))]);
      if (!m) break;
      if (m.from !== from) continue;
      seen.push(m);
      if (done(m)) break;
    }
    return seen;
  }

  async ask(to: string, type: string, extra: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
    const body = Buffer.from(JSON.stringify({ to, type, ...extra }), "utf8");
    this.sock.write(`${body.length}:`);
    this.sock.write(body);
    // Unsolicited events from other actors can arrive in between.
    for (;;) {
      const m = await this.next();
      if (m.from === to) return m;
    }
  }

  close() {
    this.sock.destroy();
  }
}

async function lockdownFrame(page: Page, kind: "menu" | "prompt" = "menu"): Promise<Frame> {
  const deadline = Date.now() + 15_000;
  for (;;) {
    const f = page.frames().find((x) => x.url().startsWith(`${ORIGIN}/frame.html#${kind}:`));
    if (f) {
      await f.waitForLoadState();
      return f;
    }
    if (Date.now() > deadline) throw new Error(`no Lockdown ${kind} frame on ${page.url()}`);
    await page.waitForTimeout(100);
  }
}

/** Opens the popup in a tab. A page navigated to a moz-extension URL from
 *  Playwright never loads in Firefox (the navigation hangs), so the add-on
 *  opens it itself: `tabs.create`, run in its background page over the
 *  remote protocol, the way a person's toolbar click would open it. */
async function openPopup(): Promise<Page> {
  const { addons } = (await rdp.ask("root", "listAddons")) as { addons: { actor: string; id: string }[] };
  const addon = addons.find((a) => a.id === ADDON_ID);
  expect(addon, JSON.stringify(addons)).toBeTruthy();
  // The descriptor's request for its target has been `getTarget`, earlier
  // `connect`, and now a watcher; whichever this Firefox answers is used.
  let consoleActor: string | undefined;
  const tried: unknown[] = [addon];
  for (const type of ["getTarget", "connect"]) {
    const answer = (await rdp.ask(addon!.actor, type)) as { form?: { consoleActor?: string }; consoleActor?: string };
    tried.push(answer);
    consoleActor = answer.form?.consoleActor ?? answer.consoleActor;
    if (consoleActor) break;
  }
  if (!consoleActor) {
    // Current Firefox: the descriptor hands out a watcher, and the
    // background page arrives as a frame target the watcher reports.
    const watcher = (await rdp.ask(addon!.actor, "getWatcher", { isServerTargetSwitchingEnabled: true })) as { actor?: string };
    tried.push(watcher);
    if (watcher.actor) {
      rdp.send(watcher.actor, "watchTargets", { targetType: "frame" });
      const seen = await rdp.collect(
        watcher.actor,
        (m) => m.type === "target-available-form" && String((m.target as { url?: string }).url).startsWith(ORIGIN),
        15_000,
      );
      tried.push(...seen);
      const form = seen.find((m) => m.type === "target-available-form" && String((m.target as { url?: string }).url).startsWith(ORIGIN));
      consoleActor = (form?.target as { consoleActor?: string } | undefined)?.consoleActor;
    }
  }
  expect(consoleActor, JSON.stringify(tried)).toBeTruthy();
  const opened = ctx.waitForEvent("page", { timeout: 30_000 });
  await rdp.ask(consoleActor!, "evaluateJSAsync", { text: `browser.tabs.create({ url: "/popup.html" })` });
  const page = await opened;
  await page.waitForURL(`${ORIGIN}/popup.html`, { timeout: 30_000 });
  await page.waitForLoadState();
  return page;
}

test.beforeAll(async () => {
  served = await serve();
  port = served.port;
  scratch = await mkdtemp(join(tmpdir(), "lockdown-ff-"));
  desktop = Desktop.start(join(scratch, "vault"), join(scratch, "unused-chromium-profile"));
  const ready = await desktop.ready;
  expect(ready.listening, String(ready.problem)).toBe(true);
  // Firefox on Linux reads host manifests from ~/.mozilla/native-messaging-hosts.
  const home = join(scratch, "home");
  const manifests = join(home, ".mozilla", "native-messaging-hosts");
  await mkdir(manifests, { recursive: true });
  await writeFile(
    join(manifests, "design.snds.lockdown.json"),
    JSON.stringify({
      name: "design.snds.lockdown",
      description: "Lockdown",
      path: Desktop.binaries().host,
      type: "stdio",
      allowed_extensions: [ADDON_ID],
    }),
  );
  const rdpPort = 6000 + Math.floor(Math.random() * 2000);
  // The browser's own (persistent) context, as the Chromium suite uses, so
  // a tab the add-on opens (the popup, below) is a page of this context.
  ctx = await firefox.launchPersistentContext(join(scratch, "ff-profile"), {
    headless: true,
    viewport: { width: 1000, height: 760 },
    args: ["-start-debugger-server", String(rdpPort)],
    env: { ...process.env, HOME: home, LOCKDOWN_DATA: String(ready.data) },
    firefoxUserPrefs: {
      "devtools.debugger.remote-enabled": true,
      "devtools.debugger.prompt-connection": false,
      "devtools.chrome.enabled": true,
      "extensions.webextensions.uuids": JSON.stringify({ [ADDON_ID]: UUID }),
      // MV3 host access granted at install, as a person accepting the
      // install prompt would.
      "extensions.originControls.grantByDefault": true,
    },
  });
  rdp = await Rdp.open(rdpPort);
  const root = await rdp.ask("root", "getRoot");
  const answer = await rdp.ask(String(root.addonsActor), "installTemporaryAddon", { addonPath: dist, openDevTools: false });
  expect((answer.addon as { id?: string } | undefined)?.id, JSON.stringify(answer)).toBe(ADDON_ID);
});

test.afterAll(async () => {
  rdp?.close();
  await ctx?.close();
  desktop?.stop();
  await served?.close();
  if (scratch) await rm(scratch, { recursive: true, force: true });
});

test("Firefox: before pairing, the inline menu reaches the app through the host and asks to pair", async () => {
  const page = await ctx.newPage();
  await page.goto(`http://localhost:${port}/ext/login.html`);
  await page.focus("#user");
  const menu = await lockdownFrame(page);
  await expect(menu.locator("body")).toContainText("Pair this browser", { timeout: 45_000 });
  await expect(page.locator("#pass")).toHaveValue("");
  await page.close();
});

test("Firefox: pairing by code, then a login fills through the menu, decided in the core", async () => {
  const p = await openPopup();
  await p.getByRole("button", { name: "Pair with Lockdown" }).click();
  const shown = (await p.locator(".code").textContent())?.replace(/\s/g, "");
  const request = await desktop.next("pairing", 30_000);
  expect(shown).toBe(request.code);
  expect(String(request.name)).toContain("Firefox");
  await desktop.ask(`approve ${request.request_id}`, "answered");
  await expect(p.getByRole("searchbox", { name: "Search Lockdown" })).toBeVisible({ timeout: 15_000 });
  await p.close();

  const page = await ctx.newPage();
  await page.goto(`http://localhost:${port}/ext/login.html`);
  await page.focus("#user");
  const menu = await lockdownFrame(page);
  const item = menu.getByRole("button", { name: /Local site/ });
  await expect(item).toBeEnabled({ timeout: 15_000 });
  await item.click();
  await expect(page.locator("#user")).toHaveValue("sean@example.com");
  await expect(page.locator("#pass")).toHaveValue("correct-horse-1");
  await page.close();
});
