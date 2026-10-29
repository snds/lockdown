// Open Lockdown with no app running (docs/plan/37-cli-popup.md §2, design
// review R16): the popup asks the real native host, which says `no_app` and
// then starts the app installed beside it. Here that "app" is a stand-in
// script beside a private copy of the host, which only leaves a mark, so
// the suite never starts a real desktop app. Without one, the popup says
// how to open Lockdown by hand.
import { chromium, expect, test, type BrowserContext } from "@playwright/test";
import { chmod, copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { Desktop } from "./desktop";

const DEV_ID = "dbdblnolpiehhlabpmhobmkbjipmebpe";
const dist = fileURLToPath(new URL("../dist/chromium", import.meta.url));
const HOW = "Lockdown couldn't open the app from the browser. Open Lockdown from your applications, then unlock it.";

let ctx: BrowserContext;
let scratch: string;
let bin: string;
let mark: string;

test.describe.configure({ mode: "serial" });
test.setTimeout(60_000);

test.beforeAll(async () => {
  scratch = await mkdtemp(join(tmpdir(), "lockdown-open-"));
  bin = join(scratch, "bin");
  mark = join(scratch, "launched");
  const profile = join(scratch, "profile");
  await mkdir(bin, { recursive: true });
  await mkdir(join(profile, "NativeMessagingHosts"), { recursive: true });
  // A private copy of the host, so the app "beside it" is this folder's.
  const host = join(bin, "lockdown-native-host");
  await copyFile(Desktop.binaries().host, host);
  await chmod(host, 0o755);
  await writeFile(
    join(profile, "NativeMessagingHosts", "design.snds.lockdown.json"),
    JSON.stringify({ name: "design.snds.lockdown", description: "Lockdown", path: host, type: "stdio", allowed_origins: [`chrome-extension://${DEV_ID}/`] }),
  );
  ctx = await chromium.launchPersistentContext(profile, {
    channel: "chromium",
    headless: true,
    // An empty data directory: no app is running, and none ever wrote an
    // endpoint file there.
    env: { ...process.env, LOCKDOWN_DATA: join(scratch, "data"), LOCKDOWN_TEST_MARK: mark },
    args: [`--disable-extensions-except=${dist}`, `--load-extension=${dist}`],
  });
  if (ctx.serviceWorkers().length === 0) await ctx.waitForEvent("serviceworker");
});

test.afterAll(async () => {
  await ctx?.close();
  if (scratch) await rm(scratch, { recursive: true, force: true });
});

async function popup() {
  const p = await ctx.newPage();
  await p.goto(`chrome-extension://${DEV_ID}/popup.html`);
  await expect(p.getByRole("button", { name: "Try again" })).toBeVisible({ timeout: 30_000 });
  return p;
}

test("with nothing to open, Open Lockdown says how to open it by hand", async () => {
  const p = await popup();
  await p.getByRole("button", { name: "Open Lockdown" }).click();
  await expect(p.getByRole("status")).toHaveText(HOW, { timeout: 15_000 });
  expect(existsSync(mark)).toBe(false);
  await p.close();
});

test("with no app running, Open Lockdown starts the app installed beside the host", async () => {
  const app = join(bin, "lockdown-desktop");
  await writeFile(app, '#!/bin/sh\necho "started with no arguments: $#" > "$LOCKDOWN_TEST_MARK"\n');
  await chmod(app, 0o755);
  const p = await popup();
  await p.getByRole("button", { name: "Open Lockdown" }).click();
  await expect(p.getByRole("status")).toHaveText("Opening Lockdown…", { timeout: 15_000 });
  await expect.poll(() => existsSync(mark), { timeout: 10_000 }).toBe(true);
  expect((await readFile(mark, "utf8")).trim()).toBe("started with no arguments: 0");
  // The stand-in never answers on the socket: after its wait the popup says how.
  await expect(p.getByRole("status")).toHaveText(HOW, { timeout: 30_000 });
  await p.close();
});
