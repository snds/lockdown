// The two manifests the build writes from one source (scripts/build.mjs):
// what differs between Chromium and Firefox, checked without a browser.
import { expect, test } from "@playwright/test";
import { readFile } from "node:fs/promises";

async function manifest(family: "chromium" | "firefox") {
  return JSON.parse(await readFile(new URL(`../dist/${family}/manifest.json`, import.meta.url), "utf8"));
}

test("Chromium: a module service worker, the fixed dev id, the Basic Auth permission", async () => {
  const m = await manifest("chromium");
  expect(m.manifest_version).toBe(3);
  expect(m.background).toEqual({ service_worker: "background.js", type: "module" });
  expect(typeof m.key).toBe("string");
  expect(m.permissions).toEqual(expect.arrayContaining(["nativeMessaging", "storage", "webRequest", "webRequestAuthProvider"]));
  expect(m.permissions).not.toContain("webRequestBlocking");
  expect(m.browser_specific_settings).toBeUndefined();
});

test("Firefox: an event page, the add-on id, blocking webRequest, no Chromium key", async () => {
  const m = await manifest("firefox");
  expect(m.manifest_version).toBe(3);
  expect(m.background).toEqual({ scripts: ["background.js"], type: "module" });
  expect(m.browser_specific_settings.gecko.id).toBe("lockdown@snds.design");
  expect(m.permissions).toEqual(expect.arrayContaining(["nativeMessaging", "storage", "webRequest", "webRequestBlocking"]));
  expect(m.permissions).not.toContain("webRequestAuthProvider");
  expect(m.key).toBeUndefined();
});

test("both: one menu frame open to pages, wasm allowed only for the extension's own pages", async () => {
  for (const family of ["chromium", "firefox"] as const) {
    const m = await manifest(family);
    expect(m.web_accessible_resources[0].resources).toEqual(["frame.html", "frame.js", "tokens.css", "ui.css"]);
    expect(m.web_accessible_resources[0].resources).not.toContain("popup.html");
    expect(m.content_security_policy.extension_pages).toBe("script-src 'self' 'wasm-unsafe-eval'; object-src 'self'");
    expect(m.commands["fill-login"].suggested_key.default).toBe("Ctrl+Shift+L");
  }
});

test("both: the passkey wrapper runs in the page's world, top frame only, before any page script", async () => {
  for (const family of ["chromium", "firefox"] as const) {
    const m = await manifest(family);
    const main = m.content_scripts.find((c: { js: string[] }) => c.js.includes("webauthn.js"));
    expect(main).toMatchObject({ world: "MAIN", run_at: "document_start", all_frames: false });
    const bridge = m.content_scripts.find((c: { js: string[] }) => c.js.includes("passkeys.js"));
    expect(bridge).toMatchObject({ run_at: "document_start", all_frames: false });
    expect(bridge.world).toBeUndefined();
    // Neither is a page-reachable resource.
    expect(m.web_accessible_resources[0].resources).not.toContain("webauthn.js");
  }
});
