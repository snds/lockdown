// The extension in Firefox (docs/plan/24-release.md §6): the dist/firefox
// build loaded into Playwright's Firefox, the real native host Firefox
// launches from its own manifest folder, the real desktop endpoint and core
// over a throwaway vault. The inline menu on a sign-in page reaches the app
// through the host and answers that this browser is not paired yet: the
// content script, the event page, native messaging, the host's and the
// app's process checks and the core, all in Firefox.
//
// Playwright has no option to load an add-on into Firefox, so the suite
// starts Firefox's remote debugging server and installs dist/firefox as a
// temporary add-on over that protocol (the `installTemporaryAddon` request
// web-ext uses). The moz-extension origin is fixed by the
// `extensions.webextensions.uuids` preference so the menu frame is known.
//
// Pairing and a fill are not driven here: pairing starts in the popup, and
// Playwright's Firefox does not load a moz-extension page as a page (a
// navigation hangs, and a tab the add-on opens with tabs.create never
// reaches Playwright). The Chromium suite drives both on the same code.
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
  // The browser's own (persistent) context, as the Chromium suite uses.
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
  const rdp = await Rdp.open(rdpPort);
  const root = await rdp.ask("root", "getRoot");
  const answer = await rdp.ask(String(root.addonsActor), "installTemporaryAddon", { addonPath: dist, openDevTools: false });
  rdp.close();
  expect((answer.addon as { id?: string } | undefined)?.id, JSON.stringify(answer)).toBe(ADDON_ID);
});

test.afterAll(async () => {
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
